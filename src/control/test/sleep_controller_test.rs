use crate::{
    control::sleep_controller::SleepController,
    external::display_server::{mock, DisplayServer, SystemState},
    system::sleep_sensor::SleepUpdate,
};

use super::effects_counter::EffectsCounter;

#[tokio::test]
async fn test_with_locker() {
    let lock_ec = EffectsCounter::new();
    let (sleep_sender, sleep_receiver) = tokio::sync::broadcast::channel(1);
    let ds = mock::Interface::new(10);
    let sleep_controller_handle = SleepController::new(
        sleep_receiver,
        Some(lock_ec.get_port()),
        ds.get_controller(),
    )
    .spawn()
    .await;

    assert_eq!(lock_ec.ongoing_effect_count(), 0);
    ds.notify_state_transition(SystemState::Idle).unwrap();

    let (confirmation_sender, mut confirmation_receiver) = tokio::sync::mpsc::channel(1);
    sleep_sender
        .send(SleepUpdate::GoingToSleep(confirmation_sender))
        .unwrap();

    confirmation_receiver.recv().await.unwrap();
    assert_eq!(lock_ec.ongoing_effect_count(), 1);

    let mut idleness_channel = ds.get_idleness_channel();
    idleness_channel.borrow_and_update();

    sleep_sender.send(SleepUpdate::WokenUp).unwrap();
    idleness_channel.changed().await.unwrap();
    assert_eq!(*idleness_channel.borrow_and_update(), SystemState::Awakened);

    sleep_controller_handle.await_shutdown().await;
}

#[tokio::test]
async fn test_without_locker() {
    let (sleep_sender, sleep_receiver) = tokio::sync::broadcast::channel(1);
    let ds = mock::Interface::new(10);
    let sleep_controller_handle = SleepController::new(sleep_receiver, None, ds.get_controller())
        .spawn()
        .await;

    ds.notify_state_transition(SystemState::Idle).unwrap();

    let (confirmation_sender, mut confirmation_receiver) = tokio::sync::mpsc::channel(1);
    sleep_sender
        .send(SleepUpdate::GoingToSleep(confirmation_sender))
        .unwrap();

    confirmation_receiver.recv().await.unwrap();

    let mut idleness_channel = ds.get_idleness_channel();
    idleness_channel.borrow_and_update();

    sleep_sender.send(SleepUpdate::WokenUp).unwrap();
    idleness_channel.changed().await.unwrap();
    assert_eq!(*idleness_channel.borrow_and_update(), SystemState::Awakened);

    sleep_controller_handle.await_shutdown().await;
}
