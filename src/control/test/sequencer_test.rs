use std::time::Duration;

use crate::{
    armaf::{self, ActorPort},
    control::sequencer::{GetRunningTime, Sequencer},
    external::display_server::{mock, DisplayServer, DisplayServerController, SystemState},
};
use anyhow::{anyhow, Result};
use tokio;

#[tokio::test(start_paused = true)]
async fn test_complete_sequence() {
    let iface = mock::Interface::new(600);
    let sequence = vec![5, 5, 2];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
        0,
        Duration::ZERO,
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
        0,
        Duration::ZERO,
    );
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);
    assert_elapsed_time(&sequencer_port, 0).await;

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;

    idleness_step(6, &mut receiver, Ok(()), &sequencer_port, 10).await;

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 0).await;

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 5);

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 5).await;

    idleness_step(6, &mut receiver, Ok(()), &sequencer_port, 10).await;
    idleness_step(3, &mut receiver, Ok(()), &sequencer_port, 12).await;

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
        0,
        Duration::ZERO,
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

    idleness_step(6, &mut receiver, Ok(()), &sequencer_port, 10).await;
    idleness_step(
        6,
        &mut receiver,
        Err(anyhow!("Forced error")),
        &sequencer_port,
        10,
    )
    .await;
    idleness_step(6, &mut receiver, Ok(()), &sequencer_port, 15).await;
    idleness_step(3, &mut receiver, Ok(()), &sequencer_port, 17).await;

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

#[tokio::test(start_paused = true)]
async fn test_initial_position_from_awakened() {
    let iface = mock::Interface::new(600);
    let sequence = vec![1, 2, 3, 4];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
        1,
        Duration::ZERO,
    );
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 2);
    assert_elapsed_time(&sequencer_port, 1).await;

    iface.notify_state_transition(SystemState::Idle).unwrap();
    assert_request_came(&mut receiver, SystemState::Idle, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 3).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);

    idleness_step(4, &mut receiver, Ok(()), &sequencer_port, 6).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 0).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);
}

#[tokio::test(start_paused = true)]
async fn test_initial_position_from_idle() {
    let iface = mock::Interface::new(600);
    iface.notify_state_transition(SystemState::Idle).unwrap();
    let sequence = vec![1, 2, 3, 4];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
        1,
        Duration::ZERO,
    );
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);
    assert_elapsed_time(&sequencer_port, 1).await;

    idleness_step(3, &mut receiver, Ok(()), &sequencer_port, 3).await;
    idleness_step(4, &mut receiver, Ok(()), &sequencer_port, 6).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);

    iface
        .notify_state_transition(SystemState::Awakened)
        .unwrap();
    assert_request_came(&mut receiver, SystemState::Awakened, Ok(())).await;
    assert_elapsed_time(&sequencer_port, 0).await;
    assert_eq!(iface.get_controller().get_idleness_timeout().unwrap(), 1);
}

#[tokio::test(start_paused = true)]
async fn test_shortened_initial_sleep() {
    let iface = mock::Interface::new(600);
    iface.notify_state_transition(SystemState::Idle).unwrap();
    let sequence = vec![10];
    let (port, mut receiver) = ActorPort::make();
    let sequencer = Sequencer::new(
        port,
        iface.get_controller(),
        iface.get_idleness_channel(),
        &sequence,
        0,
        Duration::from_secs(5),
    );
    let sequencer_port = sequencer
        .spawn()
        .await
        .expect("Sequencer failed to initialize");

    idleness_step(6, &mut receiver, Ok(()), &sequencer_port, 10).await;
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

async fn idleness_step(
    advance_secs: u64,
    receiver: &mut armaf::ActorReceiver<SystemState, (), anyhow::Error>,
    response: Result<()>,
    sequencer_port: &ActorPort<GetRunningTime, Duration, ()>,
    expected_seconds: u64,
) {
    advance_by_secs(advance_secs).await;
    assert_request_came(receiver, SystemState::Idle, response).await;
    assert_elapsed_time(sequencer_port, expected_seconds).await;
}
