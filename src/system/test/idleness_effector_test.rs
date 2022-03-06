use crate::armaf::{spawn_actor, EffectorMessage};
use crate::external::display_server::{mock, DisplayServer, DisplayServerController};
use crate::system::idleness_effector;
use tokio;

#[tokio::test]
async fn test_happy_path() {
    let iface = mock::Interface::new(600);
    let setter = iface.get_controller();
    let sequence = vec![10, 20, 30];
    let port = spawn_actor(idleness_effector::IdlenessEffector::new(
        iface.get_controller(),
        &sequence,
    ))
    .await
    .expect("Actor initialization failed");
    for timeout in sequence.iter() {
        port.request(EffectorMessage::Execute)
            .await
            .expect("Idleness effector failed to set idleness");
        assert_eq!(setter.get_idleness_timeout().unwrap(), *timeout);
    }

    port.request(EffectorMessage::Execute)
        .await
        .expect_err("Idleness effector overflowed timeout sequence");

    for i in 1..sequence.len() {
        port.request(EffectorMessage::Rollback)
            .await
            .expect("Idleness effector failed to roll back");
        assert_eq!(
            setter.get_idleness_timeout().unwrap(),
            sequence[sequence.len() - 1 - i]
        );
    }

    port.request(EffectorMessage::Rollback)
        .await
        .expect("Idleness effector failed to roll back");
    assert_eq!(setter.get_idleness_timeout().unwrap(), 600);

    port.request(EffectorMessage::Rollback)
        .await
        .expect_err("Idleness effector underflowed timeout sequence");

    drop(port);
}

#[tokio::test]
async fn test_error_handling() {
    let iface = mock::Interface::new(600);
    iface.set_failure_mode(true);
    let setter = iface.get_controller();
    let port = spawn_actor(idleness_effector::IdlenessEffector::new(
        iface.get_controller(),
        &vec![20, 30],
    ))
    .await
    .expect("Actor initialization failed");
    port.request(EffectorMessage::Execute)
        .await
        .expect_err("Idleness effector didn't return an error on broken interface");
    iface.set_failure_mode(false);
    port.request(EffectorMessage::Execute)
        .await
        .expect("Idleness effector failed to set idleness");
    assert_eq!(setter.get_idleness_timeout().unwrap(), 20);
    iface.set_failure_mode(true);
    port.request(EffectorMessage::Rollback)
        .await
        .expect_err("Idleness effector didn't return an error on broken interface");
    iface.set_failure_mode(false);
    port.request(EffectorMessage::Rollback)
        .await
        .expect("Idleness effector failed to roll back");
    assert_eq!(setter.get_idleness_timeout().unwrap(), -1);
    drop(port);
}
