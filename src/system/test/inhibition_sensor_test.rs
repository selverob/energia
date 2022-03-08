use crate::armaf::spawn_server;
use crate::external::dbus::ConnectionFactory;
use crate::system::inhibition_sensor;
use logind_zbus::manager;
use tokio;

#[tokio::test]
async fn test_inhibition_sensor() {
    let mut factory = ConnectionFactory::new();
    let test_connection = factory.get_system().await.unwrap();
    let manager_proxy = manager::ManagerProxy::new(&test_connection).await.unwrap();
    let port = spawn_server(inhibition_sensor::InhibitionSensor::new(
        factory.get_system().await.unwrap(),
    ))
    .await
    .expect("Actor initialization failed");
    let inhibition_fd = manager_proxy
        .inhibit(
            manager::InhibitType::Idle,
            "energia tests",
            "testing idleness manager",
            "block",
        )
        .await
        .unwrap();
    let inhibitors = port
        .request(inhibition_sensor::GetInhibitions)
        .await
        .expect("inhibition sensor internal error");
    let inhibitor_count = inhibitors.len();
    let our_inhibitor = inhibitors
        .iter()
        .find(|i| i.who() == "energia tests")
        .unwrap();
    assert_eq!(
        our_inhibitor.what().types(),
        &vec![manager::InhibitType::Idle]
    );
    assert_eq!(our_inhibitor.why(), "testing idleness manager");
    assert_eq!(our_inhibitor.mode(), manager::Mode::Block);
    drop(inhibition_fd);
    let new_inhibitors = port
        .request(inhibition_sensor::GetInhibitions)
        .await
        .expect("inhibition sensor internal error");
    assert_eq!(new_inhibitors.len(), inhibitor_count - 1);
}
