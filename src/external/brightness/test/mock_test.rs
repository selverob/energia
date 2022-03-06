use super::super::mock;
use crate::external::brightness::BrightnessController;

#[tokio::test]
async fn test_backlight_setting() {
    let controller = mock::MockBrightnessController::new(100);
    assert_eq!(controller.get_brightness().await.unwrap(), 100);
    controller.set_brightness(45).await.unwrap();
    assert!(controller.set_brightness(101).await.is_err());
}

#[tokio::test]
async fn test_errors() {
    let controller = mock::MockBrightnessController::new(100);
    controller.set_failure_mode(true);
    assert!(controller.get_brightness().await.is_err());
    assert!(controller.set_brightness(42).await.is_err());
}
