use crate::external::display_server::{
    mock, DPMSLevel, DPMSTimeouts, DisplayServer, DisplayServerController, SystemState,
};

#[test]
fn test_setting_and_getting_timeout() {
    let interface = mock::Interface::new(10);

    let controller = interface.get_controller();
    assert_eq!(
        controller
            .get_idleness_timeout()
            .expect("Failing even when failure mode is false"),
        10
    );

    controller
        .set_idleness_timeout(2)
        .expect("Failing even when failure mode is false");
    assert_eq!(
        controller
            .get_idleness_timeout()
            .expect("Failing even when failure mode is false"),
        2
    );
}

#[test]
fn test_failure_mode() {
    let interface = mock::Interface::new(10);
    let controller = interface.get_controller();
    interface.set_failure_mode(true);
    controller
        .get_idleness_timeout()
        .expect_err("No failure even when failure mode is true");
    controller
        .set_idleness_timeout(10)
        .expect_err("No failure even when failure mode is true");
    controller
        .is_dpms_capable()
        .expect_err("No failure even when failure mode is true");
    controller
        .get_dpms_level()
        .expect_err("No failure even when failure mode is true");
    controller
        .set_dpms_level(DPMSLevel::On)
        .expect_err("No failure even when failure mode is true");
    controller
        .set_dpms_state(false)
        .expect_err("No failure even when failure mode is true");
    controller
        .get_dpms_timeouts()
        .expect_err("No failure even when failure mode is true");
    controller
        .set_dpms_timeouts(DPMSTimeouts::new(1, 2, 3))
        .expect_err("No failure even when failure mode is true");
}

#[test]
fn test_idleness_channel() {
    let interface = mock::Interface::new(10);
    let mut chan = interface.get_idleness_channel();
    assert_eq!(*chan.borrow_and_update(), SystemState::Awakened);
    interface
        .notify_state_transition(SystemState::Idle)
        .expect("Send error");
    while !chan.has_changed().expect("Receive error") {}
    assert_eq!(*chan.borrow_and_update(), SystemState::Idle);
}

#[test]
fn test_dpms_state_control() {
    let interface = mock::Interface::new(10);
    let writing_controller = interface.get_controller();
    let reading_controller = interface.get_controller();

    writing_controller.set_dpms_state(false).unwrap();
    assert_eq!(reading_controller.get_dpms_level().unwrap(), None);
    writing_controller.set_dpms_state(true).unwrap();
    assert_eq!(
        reading_controller.get_dpms_level().unwrap(),
        Some(DPMSLevel::On)
    );
}

#[test]
fn test_dpms_levels() {
    let interface = mock::Interface::new(10);
    let writing_controller = interface.get_controller();
    let reading_controller = interface.get_controller();

    for level in vec![
        DPMSLevel::Standby,
        DPMSLevel::Suspend,
        DPMSLevel::Off,
        DPMSLevel::On,
    ] {
        writing_controller.set_dpms_level(level).unwrap();
        assert_eq!(reading_controller.get_dpms_level().unwrap(), Some(level));
    }
}

#[test]
fn test_dpms_timeouts() {
    let interface = mock::Interface::new(10);
    let writing_controller = interface.get_controller();
    let reading_controller = interface.get_controller();

    let test_timeouts = DPMSTimeouts::new(42, 43, 44);
    writing_controller.set_dpms_timeouts(test_timeouts).unwrap();
    assert_eq!(
        reading_controller.get_dpms_timeouts().unwrap(),
        test_timeouts
    );
}
