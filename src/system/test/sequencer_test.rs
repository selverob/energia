use std::time::Duration;

use crate::system::sequencer::Sequencer;
use crate::{
    armaf::{self, ActorPort},
    external::display_server::{mock, DisplayServer, SystemState},
};
use anyhow::{anyhow, Result};
use tokio::{self, sync::mpsc, time::sleep};

#[tokio::test]
async fn test_complete_sequence() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );
    sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");
    assert!(receiver.try_recv().is_err());

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    assert!(receiver.try_recv().is_err());
    sleep(Duration::from_secs(6)).await;

    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    assert!(receiver.try_recv().is_err());
    sleep(Duration::from_secs(3)).await;

    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(1)).await;
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn test_interruptions() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );
    sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(3)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
}

#[tokio::test]
async fn test_actor_errors() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 5, 2];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );
    sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(
        &mut receiver,
        SystemState::Idle,
        Err(anyhow!("Forced error")),
    )
    .await;

    assert!(receiver.try_recv().is_err());
    sleep(Duration::from_millis(200)).await;
    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(
        &mut receiver,
        SystemState::Idle,
        Err(anyhow!("Forced error")),
    )
    .await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(3)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert!(receiver.try_recv().is_err());
}

async fn assert_request_came(
    receiver: &mut mpsc::Receiver<armaf::Request<SystemState, (), anyhow::Error>>,
    expected_state: SystemState,
    response: Result<()>,
) {
    let req = receiver.recv().await.unwrap();
    assert_eq!(req.payload, expected_state);
    req.respond(response).unwrap();
}
