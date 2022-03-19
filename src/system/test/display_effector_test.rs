use super::super::display_effector;
use crate::{
    armaf::{spawn_server, EffectorMessage},
    external::{
        brightness as bs,
        brightness::BrightnessController,
        display_server as ds,
        display_server::{DisplayServer, DisplayServerController},
    },
};
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
    let port = spawn_server(display_effector::DisplayEffectorActor::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

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

    // Test if the display effector resets the state to original when it's terminated
    port.await_shutdown().await;
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::Standby)
    );
    assert_eq!(
        ds_controller.get_dpms_timeouts().unwrap(),
        ds::DPMSTimeouts::new(42, 43, 44)
    );
    assert_eq!(brightness.get_brightness().await.unwrap(), 45);
}

#[tokio::test]
async fn test_basic_flow() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();

    let port = spawn_server(display_effector::DisplayEffectorActor::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");
    let res = port
        .request(EffectorMessage::Execute)
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    assert_eq!(res, 1);

    let res = port
        .request(EffectorMessage::Execute)
        .await
        .expect("Failed to turn display off");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::Off)
    );
    assert_eq!(res, 2);

    let res = port
        .request(EffectorMessage::Rollback)
        .await
        .expect("Failed to turn display on");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    assert_eq!(res, 1);

    let res = port
        .request(EffectorMessage::Rollback)
        .await
        .expect("Failed to undim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
    assert_eq!(res, 0);
}

#[tokio::test]
async fn test_undim_on_termination() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);

    let port = spawn_server(display_effector::DisplayEffectorActor::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");
    port.request(EffectorMessage::Execute)
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    port.await_shutdown().await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
}

#[tokio::test]
async fn test_failing_display_server() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    ds_controller.set_dpms_level(ds::DPMSLevel::On).unwrap();
    let port = spawn_server(display_effector::DisplayEffectorActor::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

    display.set_failure_mode(true);

    let res = port
        .request(EffectorMessage::Execute)
        .await
        .expect("Failed to dim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    assert_eq!(res, 1);

    port.request(EffectorMessage::Execute)
        .await
        .expect_err("No error reported on failing display server controller");

    let res = port
        .request(EffectorMessage::CurrentlyAppliedEffects)
        .await
        .expect("Failed to get applied effect count");
    assert_eq!(res, 1);

    let res = port
        .request(EffectorMessage::Rollback)
        .await
        .expect("Failed to undim display");
    assert_eq!(brightness.get_brightness().await.unwrap(), 80);
    assert_eq!(res, 0);
}

#[tokio::test]
async fn test_failing_brightness_controller() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let display = ds::mock::Interface::new(-1);
    let port = spawn_server(display_effector::DisplayEffectorActor::new(
        brightness.clone(),
        display.get_controller(),
    ))
    .await
    .expect("Actor initialization failed");

    brightness.set_failure_mode(true);

    port.request(EffectorMessage::Execute)
        .await
        .expect_err("No error returned from failing controller");

    port.request(EffectorMessage::Rollback)
        .await
        .expect_err("Rolling back from initial state succeeded");

    brightness.set_failure_mode(false);
    port.request(EffectorMessage::Execute)
        .await
        .expect("Dimming failed");
    assert_eq!(brightness.get_brightness().await.unwrap(), 40);
    brightness.set_failure_mode(true);
    port.request(EffectorMessage::Rollback)
        .await
        .expect_err("No error occurred even when undim failed");
}
