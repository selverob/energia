use crate::{
    armaf::{spawn_server, EffectorMessage},
    external::{
        display_server as ds,
        display_server::{DisplayServer, DisplayServerController},
    },
    system::dpms_effector::DPMSEffectorActor,
};

#[tokio::test]
async fn test_original_config_saving() {
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    ds_controller.set_dpms_state(true).unwrap();
    ds_controller
        .set_dpms_level(ds::DPMSLevel::Standby)
        .unwrap();
    ds_controller
        .set_dpms_timeouts(ds::DPMSTimeouts::new(42, 43, 44))
        .unwrap();
    let port = spawn_server(DPMSEffectorActor::new(display.get_controller()))
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
}

#[tokio::test]
async fn test_basic_flow() {
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();

    let port = spawn_server(DPMSEffectorActor::new(display.get_controller()))
        .await
        .expect("Actor initialization failed");

    let res = port
        .request(EffectorMessage::Execute)
        .await
        .expect("Failed to turn display off");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::Off)
    );
    assert_eq!(res, 1);

    let res = port
        .request(EffectorMessage::Rollback)
        .await
        .expect("Failed to turn display on");
    assert_eq!(
        ds_controller.get_dpms_level().unwrap(),
        Some(ds::DPMSLevel::On)
    );
    assert_eq!(res, 0);
}

#[tokio::test]
async fn test_failing_display_server() {
    let display = ds::mock::Interface::new(-1);
    let ds_controller = display.get_controller();
    ds_controller.set_dpms_level(ds::DPMSLevel::On).unwrap();
    let port = spawn_server(DPMSEffectorActor::new(display.get_controller()))
        .await
        .expect("Actor initialization failed");

    display.set_failure_mode(true);

    port.request(EffectorMessage::Execute)
        .await
        .expect_err("No error reported on failing display server controller");

    let res = port
        .request(EffectorMessage::CurrentlyAppliedEffects)
        .await
        .expect("Failed to get applied effect count");
    assert_eq!(res, 0);
}
