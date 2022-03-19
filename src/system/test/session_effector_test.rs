use crate::{
    armaf::{spawn_server, EffectorMessage},
    external::dbus,
    system::session_effector,
};
use anyhow::Result;
use logind_zbus::{manager, session};
use std::{process, thread::sleep, time::Duration};
use tokio;

#[tokio::test]
#[ignore]
async fn test_happy_path() {
    let mut factory = dbus::ConnectionFactory::new();
    let test_connection = factory.get_system().await.unwrap();
    let session_proxy = get_session_proxy(&test_connection).await.unwrap();
    let port = spawn_server(session_effector::SessionEffectorActor::new(
        factory.get_system().await.unwrap(),
    ))
    .await
    .expect("Actor initialization failed");
    port.request(EffectorMessage::Execute).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.idle_hint().await.unwrap(), true);

    port.request(EffectorMessage::Execute).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.locked_hint().await.unwrap(), true);

    port.request(EffectorMessage::Execute)
        .await
        .expect_err("Effector allowed state machine overflow");

    port.request(EffectorMessage::Rollback).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.idle_hint().await.unwrap(), true);
    assert_eq!(session_proxy.locked_hint().await.unwrap(), false);

    port.request(EffectorMessage::Rollback).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.idle_hint().await.unwrap(), false);
    assert_eq!(session_proxy.locked_hint().await.unwrap(), false);

    port.request(EffectorMessage::Rollback)
        .await
        .expect_err("Effector allowed state machine underflow");
}

async fn get_session_proxy<'c>(
    connection: &'c zbus::Connection,
) -> Result<session::SessionProxy<'c>> {
    let manager_proxy = manager::ManagerProxy::new(&connection).await?;
    let path = manager_proxy.get_session_by_PID(process::id()).await?;
    Ok(session::SessionProxy::builder(connection)
        .path(path)?
        .build()
        .await?)
}
