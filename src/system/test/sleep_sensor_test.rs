use std::time::Duration;

use logind_zbus::manager::{InhibitType, Mode};
use tokio::time::sleep;

use crate::{
    armaf::{spawn_server, EffectorMessage},
    external::dbus::ConnectionFactory,
    system::{
        sleep_effector::SleepEffectorActor,
        sleep_sensor::{ReadyToSleep, SleepSensor, SleepUpdate},
    },
};

#[tokio::test]
#[ignore]
async fn test_happy_path() {
    let mut connection_factory = ConnectionFactory::new();
    let connection = connection_factory.get_system().await.unwrap();
    let manager_proxy = logind_zbus::manager::ManagerProxy::new(&connection)
        .await
        .unwrap();
    let sensor = SleepSensor::new(connection_factory.get_system().await.unwrap());
    let sleep_effector = spawn_server(SleepEffectorActor::new(
        connection_factory.get_system().await.unwrap(),
    ))
    .await
    .unwrap();
    let (handle, sender) = sensor.spawn().await.expect("Sensor failed to start");
    sleep(Duration::from_secs(1)).await;
    let inhibitors = manager_proxy.list_inhibitors().await.unwrap();
    println!("{:?}", inhibitors);
    assert!(inhibitors.into_iter().any(|inhibitor| inhibitor
        .what()
        .types()
        .contains(&InhibitType::Sleep)
        && inhibitor.who() == "Energia Power Manager"
        && inhibitor.mode() == Mode::Delay));
    let mut receivers = vec![sender.subscribe(), sender.subscribe()];
    sleep_effector
        .request(EffectorMessage::Execute)
        .await
        .unwrap();
    for receiver in receivers.iter_mut() {
        let message = receiver.recv().await.unwrap();
        if let SleepUpdate::GoingToSleep(c) = message {
            c.send(ReadyToSleep).await.unwrap();
        } else {
            unreachable!();
        }
    }
    sleep_effector
        .request(EffectorMessage::Rollback)
        .await
        .unwrap();
    for ref mut receiver in receivers.iter_mut() {
        let message = receiver.recv().await.unwrap();
        if let SleepUpdate::WokenUp = message {
        } else {
            unreachable!();
        }
    }
    handle.await_shutdown().await;
}
