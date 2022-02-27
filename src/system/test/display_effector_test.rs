use super::super::display_effector;
use crate::armaf::{spawn_actor, EffectorMessage};
use crate::external::brightness as bs;
use crate::external::brightness::BrightnessController;
use crate::external::display_server as ds;
use crate::external::display_server::DisplayServer;
use crate::external::display_server::DisplayServerController;
use crate::system::display_effector::DisplayEffect;
use std::time::Duration;

#[tokio::test]
async fn test_original_config_saving() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    ds_controller.set_dpms_state(true).unwrap();
    ds_controller
        .set_dpms_level(ds::DPMSLevel::Standby)
        .unwrap();
    ds_controller
        .set_dpms_timeouts(ds::DPMSTimeouts::new(42, 43, 44))
        .unwrap();
    let port = spawn_actor(display_effector::DisplayEffector::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

    tokio::time::sleep(Duration::from_millis(250)).await;

    // Test if the display effector sets its own state when it's initialized
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );
    assert_eq!(
        ds_controller.get_dpms_timeouts().unwrap(),
        ds::DPMSTimeouts::new(0, 0, 0)
    );

    // Used to test if the display controller doesn't change the brightness on
    // termination if it wasn't changed by itself
    brightness.set_brightness(45).await.unwrap();

    // Test if the display effector resets the state to original when it's initialized
    drop(port);
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );
    assert_eq!(
        ds_controller.get_dpms_timeouts().unwrap(),
        ds::DPMSTimeouts::new(0, 0, 0)
    );
    assert_eq!(brightness.get_brightness().await.unwrap(), 45);
}

#[tokio::test]
async fn test_basic_flow() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();

    let port = spawn_actor(display_effector::DisplayEffector::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");
    port.request(EffectorMessage::Execute(DisplayEffect::Dim))
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);

    port.request(EffectorMessage::Execute(DisplayEffect::TurnOff))
        .await
        .expect("Failed to turn display off");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::Off)
    );

    port.request(EffectorMessage::Rollback(DisplayEffect::TurnOff))
        .await
        .expect("Failed to turn display on");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);

    port.request(EffectorMessage::Rollback(DisplayEffect::Dim))
        .await
        .expect("Failed to undim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
}

#[tokio::test]
async fn test_undim_on_termination() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);

    let port = spawn_actor(display_effector::DisplayEffector::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");
    port.request(EffectorMessage::Execute(DisplayEffect::Dim))
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    drop(port);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
}

#[tokio::test]
async fn test_failing_display_server() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    ds_controller.set_dpms_level(ds::DPMSLevel::On).unwrap();
    let port = spawn_actor(display_effector::DisplayEffector::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

    display.set_failure_mode(true);

    port.request(EffectorMessage::Execute(DisplayEffect::Dim))
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);

    port.request(EffectorMessage::Execute(DisplayEffect::TurnOff))
        .await
        .expect_err("No error reported on failing display server controller");
    port.request(EffectorMessage::Rollback(DisplayEffect::TurnOff))
        .await
        .expect_err("No error reported on failing display server controller");

    port.request(EffectorMessage::Rollback(DisplayEffect::Dim))
        .await
        .expect("Failed to undim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
}

#[tokio::test]
async fn test_failing_brightness_controller() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    let port = spawn_actor(display_effector::DisplayEffector::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

    brightness.set_failure_mode(true);

    port.request(EffectorMessage::Execute(DisplayEffect::Dim))
        .await
        .expect_err("No error returned from failing controller");

    port.request(EffectorMessage::Execute(DisplayEffect::TurnOff))
        .await
        .expect("Failed to turn display off");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::Off)
    );

    port.request(EffectorMessage::Rollback(DisplayEffect::TurnOff))
        .await
        .expect("Failed to turn display on");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );

    port.request(EffectorMessage::Rollback(DisplayEffect::Dim))
        .await
        .expect_err("An error should be returned from undim if no dimming occurred");

    brightness.set_failure_mode(false);
    port.request(EffectorMessage::Execute(DisplayEffect::Dim))
        .await
        .expect("Dimming failed");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    brightness.set_failure_mode(true);
    port.request(EffectorMessage::Rollback(DisplayEffect::Dim))
        .await
        .expect_err("No error occurred even when undim failed");
}
