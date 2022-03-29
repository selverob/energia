use tokio::time::Instant;

use crate::{
    armaf::{Effector, EffectorMessage},
    external::dependency_provider::DependencyProvider,
    system::lock_effector::LockEffector,
};

#[tokio::test]
#[cfg(not(tarpaulin))] // Cannot run Tarpaulin test with external commands, see https://github.com/xd009642/tarpaulin/issues/971
async fn test_basic_flow() {
    let config = toml::toml! {
        command = "sleep"
        args = ["5"]
    };

    let mut di =
        DependencyProvider::make_mock(Some(crate::external::dbus::ConnectionFactory::new()));
    let connection = di.get_dbus_system_connection().await.unwrap();
    let manager_proxy = logind_zbus::manager::ManagerProxy::new(&connection)
        .await
        .unwrap();
    let path = manager_proxy
        .get_session_by_PID(std::process::id())
        .await
        .unwrap();
    let session_proxy = logind_zbus::session::SessionProxy::builder(&connection)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    let port = LockEffector.spawn(Some(config), &mut di).await.unwrap();
    assert_eq!(
        port.request(EffectorMessage::CurrentlyAppliedEffects)
            .await
            .expect("Couldn't get number of effects"),
        0
    );
    assert_eq!(
        port.request(EffectorMessage::Execute)
            .await
            .expect("Couldn't lock system"),
        1
    );
    let start = Instant::now();
    assert!(session_proxy.locked_hint().await.unwrap());
    assert_eq!(
        port.request(EffectorMessage::CurrentlyAppliedEffects)
            .await
            .expect("Couldn't get number of effects"),
        1
    );
    port.request(EffectorMessage::Execute)
        .await
        .expect_err("Double locking was allowed");
    assert_eq!(
        port.request(EffectorMessage::Rollback)
            .await
            .expect("Couldn't await finish of locker"),
        0
    );
    assert!(start.elapsed() > std::time::Duration::from_secs(5));
    assert!(!session_proxy.locked_hint().await.unwrap());
}

#[tokio::test]
async fn test_error_without_config() {
    let mut di = DependencyProvider::make_mock(None);
    assert!(LockEffector.spawn(None, &mut di).await.is_err());
}
