use std::time::Duration;

use crate::{
    armaf::{self, ActorPort},
    external::display_server::{mock, DisplayServer, DisplayServerController, SystemState},
    system::sequencer::Sequencer,
};
use anyhow::{anyhow, Result};
use tokio::{self, sync::mpsc, time::sleep};

#[tokio::test]
async fn test_complete_sequence() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
    );
    let handle = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");
    assert!(receiver.try_recv().is_err());

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

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

    drop(handle);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
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
    let handle = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    sleep(Duration::from_secs(6)).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

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

    drop(handle);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
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
    let handle = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

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

    drop(handle);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
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
