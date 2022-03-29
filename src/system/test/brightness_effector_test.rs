use crate::{
    armaf::{spawn_server, EffectorMessage},
    external::{brightness as bs, brightness::BrightnessController},
    system::brightness_effector::BrightnessEffectorActor,
};
use std::time::Duration;

#[tokio::test]
async fn test_basic_flow() {
    let brightness = bs::mock::MockBrightnessController::new(80);

    let port = spawn_server(BrightnessEffectorActor::new(brightness.clone()))
        .await
        .expect("Actor initialization failed");
    let res = port
        .request(EffectorMessage::Execute)
        .await
        .expect("Failed to dim display");
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
    let port = spawn_server(BrightnessEffectorActor::new(brightness.clone()))
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
async fn test_failing_brightness_controller() {
    let brightness = bs::mock::MockBrightnessController::new(80);
    let port = spawn_server(BrightnessEffectorActor::new(brightness.clone()))
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
