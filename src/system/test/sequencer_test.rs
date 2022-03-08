use std::time::Duration;

use crate::external::display_server::{mock, DisplayServer, SystemState};
use crate::system::sequencer::Sequencer;
use tokio::{self, sync::broadcast::error::TryRecvError, time::sleep};

#[tokio::test]
async fn test_complete_sequence() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let sequencer = Sequencer::new(
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );

    let mut state_channel = sequencer.spawn().await.expect("Sequencer initialization failure");
    assert_eq!(state_channel.try_recv(), Err(TryRecvError::Empty));

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);
    
    sleep(Duration::from_secs(6)).await;
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    sleep(Duration::from_secs(3)).await;
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    sleep(Duration::from_secs(1)).await;
    assert_eq!(state_channel.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn test_interruptions() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let sequencer = Sequencer::new(
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );

    let mut state_channel = sequencer.spawn().await.expect("Sequencer initialization failure");

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);
    
    sleep(Duration::from_secs(6)).await;
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    iface.notify_state_transition(SystemState::Awakened).unwrap();
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Awakened);

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    sleep(Duration::from_secs(6)).await;
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    sleep(Duration::from_secs(3)).await;
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Idle);

    iface.notify_state_transition(SystemState::Awakened).unwrap();
    assert_eq!(state_channel.recv().await.unwrap(), SystemState::Awakened);
}

// #[tokio::test]
// async fn test_error_handling() {
//     let iface = mock::Interface::new(600);
//     iface.set_failure_mode(true);
//     let setter = iface.get_controller();
//     let port = spawn_server(sequencer::IdlenessEffector::new(
//         iface.get_controller(),
//         &vec![20, 30],
//     ))
//     .await
//     .expect("Actor initialization failed");
//     port.request(EffectorMessage::Execute)
//         .await
//         .expect_err("Idleness effector didn't return an error on broken interface");
//     iface.set_failure_mode(false);
//     port.request(EffectorMessage::Execute)
//         .await
//         .expect("Idleness effector failed to set idleness");
//     assert_eq!(setter.get_idleness_timeout().unwrap(), 20);
//     iface.set_failure_mode(true);
//     port.request(EffectorMessage::Rollback)
//         .await
//         .expect_err("Idleness effector didn't return an error on broken interface");
//     iface.set_failure_mode(false);
//     port.request(EffectorMessage::Rollback)
//         .await
//         .expect("Idleness effector failed to roll back");
//     assert_eq!(setter.get_idleness_timeout().unwrap(), -1);
//     drop(port);
// }
