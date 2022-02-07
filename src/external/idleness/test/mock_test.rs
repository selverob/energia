use crate::external::idleness::{mock, DisplayServerInterface, IdlenessController, SystemState};

#[test]
fn test_setting_and_getting_timeout() {
    let interface = mock::Interface::new(10);

    let controller = interface.get_idleness_controller();
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
    let controller = interface.get_idleness_controller();
    interface.set_failure_mode(true);
    controller
        .get_idleness_timeout()
        .expect_err("No failure even when failure mode is true");
    controller
        .set_idleness_timeout(10)
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
