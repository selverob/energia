use std::os::unix::prelude::FromRawFd;
use tokio;
use logind_zbus::{manager};
use zvariant::OwnedFd;
use crate::system::inhibition_sensor;
use crate::external::dbus::ConnectionFactory;

#[tokio::test]
async fn test_inhibition_sensor() {
    let mut factory = ConnectionFactory::new();
    let test_connection = factory.get_system().await.unwrap();
    let manager_proxy = manager::ManagerProxy::new(&test_connection).await.unwrap();
    let port = inhibition_sensor::spawn(factory.get_system().await.unwrap());
    let inhibition_fd = manager_proxy.inhibit(manager::InhibitType::Idle, "energia tests", "testing idleness manager", "block").await.unwrap();
    let inhibitors = port.request(inhibition_sensor::GetInhibitions).await.expect("inhibition sensor internal error");
    let our_inhibitor = inhibitors.iter().find(|i| i.who() == "energia tests").unwrap();
    assert_eq!(our_inhibitor.what().types(), &vec![manager::InhibitType::Idle]);
    assert_eq!(our_inhibitor.why(), "testing idleness manager");
    assert_eq!(our_inhibitor.mode(), manager::Mode::Block);
    drop(inhibition_fd);
}
