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

    let res = port
        .request(EffectorMessage::CurrentlyAppliedEffects)
        .await
        .expect("Couldn't get current effect count");
    assert_eq!(res, 0);

    let res = port.request(EffectorMessage::Execute).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.idle_hint().await.unwrap(), true);
    assert_eq!(res, 1);

    let res = port
        .request(EffectorMessage::CurrentlyAppliedEffects)
        .await
        .expect("Couldn't get current effect count");
    assert_eq!(res, 1);

    let res = port.request(EffectorMessage::Rollback).await.unwrap();
    sleep(Duration::from_millis(200)); // See the comment in SessionEffector#handle_message
    assert_eq!(session_proxy.idle_hint().await.unwrap(), false);
    assert_eq!(res, 0);

    let res = port
        .request(EffectorMessage::CurrentlyAppliedEffects)
        .await
        .expect("Couldn't get current effect count");
    assert_eq!(res, 0);
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
