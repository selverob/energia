use crate::external::idleness::IdlenessSetter;

use super::{mock, DisplayServerInterface, SystemState};

#[test]
fn test_setting_and_getting_timeout() {
    let interface = mock::Interface::new(10);

    let setter = interface.get_idleness_setter();
    assert_eq!(setter.get_idleness_timeout().expect("Failing even when failure mode is false"), 10);
    
    setter.set_idleness_timeout(2).expect("Failing even when failure mode is false");
    assert_eq!(setter.get_idleness_timeout().expect("Failing even when failure mode is false"), 2);
}

#[test]
fn test_failure_mode() {
    let interface = mock::Interface::new(10);
    let setter = interface.get_idleness_setter();
    interface.set_failure_mode(true);
    setter.get_idleness_timeout().expect_err("No failure even when failure mode is false");
    setter.set_idleness_timeout(10).expect_err("No failure even when failure mode is false");
}

#[test]
fn test_idleness_channel() {
    let interface = mock::Interface::new(10);
    let mut chan = interface.get_idleness_channel();
    assert_eq!(*chan.borrow_and_update(), SystemState::Awakened);
    interface.notify_state_transition(SystemState::Idle).expect("Send error");
    while !chan.has_changed().expect("Receive error") {}
    assert_eq!(*chan.borrow_and_update(), SystemState::Idle);
}
