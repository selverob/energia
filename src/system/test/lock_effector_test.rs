use tokio::time::Instant;

use crate::{
    armaf::{Effector, EffectorMessage},
    external::dependency_provider::DependencyProvider,
    system::lock_effector::LockEffector,
};

#[tokio::test]
async fn test_basic_flow() {
    let config = toml::toml! {
        command = "sleep"
        args = ["5"]
    };
    let mut di = DependencyProvider::make_mock();
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
}

#[tokio::test]
async fn test_error_without_config() {
    let mut di = DependencyProvider::make_mock();
    assert!(LockEffector.spawn(None, &mut di).await.is_err());
}
