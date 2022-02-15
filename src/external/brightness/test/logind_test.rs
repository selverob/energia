use crate::external::brightness::BrightnessController;

use super::super::logind;

#[tokio::test]
#[ignore]
async fn test_backlight_setting() {
    let mut factory = crate::external::dbus::ConnectionFactory::new();
    let connection = factory
        .get_system()
        .await
        .expect("Couldn't create system D-Bus connection");
    let manager_proxy = logind_zbus::manager::ManagerProxy::new(&connection)
        .await
        .expect("Couldn't create manager proxy");
    let path = manager_proxy
        .get_session_by_PID(std::process::id())
        .await
        .expect("Couldn't get session");
    let controller =
        logind::LogindBrightnessController::new("intel_backlight", connection, path.as_ref())
            .await
            .expect("Couldn't create brightness controller");
    let original_brightness = controller
        .get_brightness()
        .await
        .expect("Couldn't fetch current brightness");
    controller
        .set_brightness(50)
        .await
        .expect("Couldn't set new brightness");
    assert_eq!(
        controller
            .get_brightness()
            .await
            .expect("Couldn't fetch current brightness"),
        50
    );
    controller
        .set_brightness(original_brightness)
        .await
        .expect("Couldn't restore brightness");
}
