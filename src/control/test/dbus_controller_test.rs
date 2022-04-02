use crate::{
    armaf::ActorPort,
    control::{dbus_controller::DBusController, test::effects_counter::EffectsCounter},
};

#[tokio::test]
#[ignore]
async fn test_locking() {
    let path = "/org/energia/test_dbus_locking";
    let name = "org.energia.lock_test.Manager";
    let ec = EffectsCounter::new();
    let dbus_controller = DBusController::new(path, name, ec.get_port());
    let handle = dbus_controller
        .spawn()
        .await
        .expect("Couldn't start controller");

    let our_connection = zbus::Connection::session().await.unwrap();
    let result = our_connection
        .call_method(Some(name), path, Some("org.energia.Manager"), "Lock", &())
        .await;
    assert!(result.is_ok());
    assert_eq!(ec.ongoing_effect_count(), 1);
    handle.await_shutdown().await;
    let after_disconnection = our_connection
        .call_method(
            Some("org.energia.Manager"),
            path,
            Some("org.energia.Manager"),
            "Lock",
            &(),
        )
        .await;
    assert!(after_disconnection.is_err());

    // Let's make sure that the de-registration properly drops ports it owns,
    // anything else could wreak havoc on our shutdown processes.
    let ec_port = ec.get_port();
    drop(ec);
    ec_port.await_shutdown().await;
}

#[tokio::test]
#[ignore]
async fn test_errors() {
    let path = "/org/energia/test_dbus_errors";
    let name = "org.energia.errors_test.Manager";
    let (port, _) = ActorPort::make();
    let dbus_controller = DBusController::new(path, name, port);
    let handle = dbus_controller
        .spawn()
        .await
        .expect("Couldn't start controller");

    let our_connection = zbus::Connection::session().await.unwrap();
    let result = our_connection
        .call_method(Some(name), path, Some("org.energia.Manager"), "Lock", &())
        .await;
    assert!(result.is_err());
    handle.await_shutdown().await;
}
