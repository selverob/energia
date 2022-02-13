use crate::armaf::EffectorMessage;
use crate::external::idleness::{mock, DisplayServerInterface, IdlenessController};
use crate::system::idleness_effector;
use tokio;

#[tokio::test]
async fn test_happy_path() {
    let iface = mock::Interface::new(600);
    let setter = iface.get_idleness_controller();
    let port = idleness_effector::spawn(iface.get_idleness_controller());
    port.request(EffectorMessage::Execute(10))
        .await
        .expect("Idleness effector failed to set idleness");
    assert_eq!(setter.get_idleness_timeout().unwrap(), 10);
    port.request(EffectorMessage::Rollback(1))
        .await
        .expect("Idleness effector failed to roll back");
    assert_eq!(setter.get_idleness_timeout().unwrap(), 600);
    drop(port);
}

#[tokio::test]
async fn test_error_handling() {
    let iface = mock::Interface::new(600);
    iface.set_failure_mode(true);
    let setter = iface.get_idleness_controller();
    let port = idleness_effector::spawn(iface.get_idleness_controller());
    port.request(EffectorMessage::Execute(10))
        .await
        .expect_err("Idleness effector didn't return an error on broken interface");
    port.request(EffectorMessage::Rollback(1))
        .await
        .expect_err("Idleness effector didn't return an error on broken interface");
    iface.set_failure_mode(false);
    port.request(EffectorMessage::Rollback(1))
        .await
        .expect("Idleness effector failed to roll back");
    assert_eq!(setter.get_idleness_timeout().unwrap(), -1);
    drop(port);
}
