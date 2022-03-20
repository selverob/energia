use std::time::Duration;

use crate::{
    armaf::{self, ActorPort},
    external::display_server::{mock, DisplayServer, DisplayServerController, SystemState},
    system::sequencer::{GetRunningTime, Sequencer},
};
use anyhow::{anyhow, Result};
use tokio;

#[tokio::test(start_paused = true)]
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
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert!(receiver.request_receiver.try_recv().is_err());
    assert_elapsed_time(&sequencer_port, 0).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    advance_by_secs(4).await;
    assert!(receiver.request_receiver.try_recv().is_err());
    assert_elapsed_time(&sequencer_port, 9).await;

    advance_by_secs(2).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    // Due to us jumping in time, the time that will be seen by actor once sleep filer will be 10 and not 11
    assert_elapsed_time(&sequencer_port, 10).await;

    advance_by_secs(1).await;
    assert!(receiver.request_receiver.try_recv().is_err());
    assert_elapsed_time(&sequencer_port, 11).await;

    advance_by_secs(1).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 12).await;

    advance_by_secs(1).await;
    assert!(receiver.request_receiver.try_recv().is_err());
    assert_elapsed_time(&sequencer_port, 13).await;

    drop(receiver);
    sequencer_port.await_shutdown().await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
}

#[tokio::test(start_paused = true)]
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
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);
    assert_elapsed_time(&sequencer_port, 0).await;

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    advance_by_secs(6).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 10).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 0).await;

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 5).await;

    advance_by_secs(6).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 10).await;

    advance_by_secs(3).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 12).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 0).await;

    drop(receiver);
    sequencer_port.await_shutdown().await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
}

#[tokio::test(start_paused = true)]
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
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);
    assert_elapsed_time(&sequencer_port, 0).await;

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_eq!(*iface.get_idleness_channel().borrow(), SystemState::Idle);
    assert_request_came(
        &mut receiver,
        SystemState::Idle,
        Err(anyhow!("Forced error")),
    )
    .await;
    assert_elapsed_time(&sequencer_port, 0).await;
    assert_eq!(
        *iface.get_idleness_channel().borrow(),
        SystemState::Awakened
    );

    assert!(receiver.request_receiver.try_recv().is_err());
    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 5).await;

    advance_by_secs(6).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 10).await;

    advance_by_secs(6).await;
    assert_request_came(
        &mut receiver,
        SystemState::Idle,
        Err(anyhow!("Forced error")),
    )
    .await;
    assert_elapsed_time(&sequencer_port, 10).await;

    advance_by_secs(6).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 15).await;

    advance_by_secs(3).await;
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 17).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert!(receiver.request_receiver.try_recv().is_err());
    assert_elapsed_time(&sequencer_port, 0).await;

    drop(receiver);
    sequencer_port.await_shutdown().await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 600);
}

async fn assert_request_came(
    receiver: &mut armaf::ActorReceiver<SystemState, (), anyhow::Error>,
    expected_state: SystemState,
    response: Result<()>,
) {
    let req = receiver.recv().await.unwrap();
    assert_eq!(req.payload, expected_state);
    req.respond(response).unwrap();
}

async fn advance_by_secs(seconds: u64) {
    log::debug!("Advancing test time by {}s", seconds);
    tokio::time::advance(Duration::from_secs(seconds)).await
}

async fn assert_elapsed_time(
    port: &ActorPort<GetRunningTime, Duration, ()>,
    expected_seconds: u64,
) {
    let res = port
        .request(GetRunningTime)
        .await
        .expect("couldn't get running time from Sequencer");
    assert_eq!(res, Duration::from_secs(expected_seconds));
}
