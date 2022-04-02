#![warn(missing_docs)]

//! A modern power manager for Linux

mod armaf;
mod control;
mod external;
mod system;

use armaf::{EffectorPort, Handle};
use control::{dbus_controller::DBusController, environment_controller::EnvironmentController};
use env_logger;
use external::dependency_provider::DependencyProvider;
use std::env;
use tokio::{self, fs};

use crate::{
    armaf::spawn_server,
    control::effector_inventory::{EffectorInventory, GetEffectorPort},
    external::display_server::x11::X11Interface,
    system::{
        inhibition_sensor::InhibitionSensor, sleep_sensor::SleepSensor, upower_sensor::UPowerSensor,
    },
};

fn initialize_logging() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();
}

async fn parse_config() -> anyhow::Result<toml::Value> {
    let config_path = env::var("ENERGIA_CONFIG_PATH").unwrap_or("config.toml".to_owned());
    Ok(toml::from_slice(&fs::read(config_path).await?)?)
}

async fn try_launch_dbus_controller(lock_effector: EffectorPort) -> Option<Handle> {
    let controller =
        DBusController::new("/org/energia/Manager", "org.energia.Manager", lock_effector);
    match controller.spawn().await {
        Ok(handle) => Some(handle),
        Err(e) => {
            log::error!("Couldn't spawn D-Bus API: {}", e);
            None
        }
    }
}

#[tokio::main]
async fn main() {
    initialize_logging();

    let config = parse_config().await.expect("Couldn't read configuration");
    log::info!("Parsed config is: {:?}", config);

    let mut system_dependencies = DependencyProvider::make_system()
        .await
        .expect("Couldn't construct dependency provider");

    let ds_controller = system_dependencies.get_display_controller();
    let idleness_channel = system_dependencies.get_idleness_channel();
    let dbus_connection = system_dependencies
        .get_dbus_system_connection()
        .await
        .expect("Couldn't get connection to system D-Bus");

    let inhibition_sensor = spawn_server(InhibitionSensor::new(dbus_connection.clone()))
        .await
        .expect("Couldn't start inhibition sensor");

    let upower_channel = UPowerSensor::new(dbus_connection.clone())
        .await
        .expect("Couldn't start UPower sensor");

    let sleep_sensor = SleepSensor::new(dbus_connection);
    let (sleep_sensor_handle, sleep_sensor_channel) = sleep_sensor
        .spawn()
        .await
        .expect("Sleep sensor failed to start");

    let effector_inventory =
        spawn_server(EffectorInventory::new(config.clone(), system_dependencies))
            .await
            .expect("Couldn't spawn EffectorInventory");

    let environment_controller: EnvironmentController<X11Interface> = EnvironmentController::new(
        &config,
        effector_inventory.clone(),
        inhibition_sensor,
        ds_controller,
        idleness_channel,
        upower_channel,
    );

    let environment_controller_handle = environment_controller
        .spawn()
        .await
        .expect("Couldn't spawn environment controller");

    let dbus_controller_handle = match effector_inventory
        .request(GetEffectorPort("lock".to_string()))
        .await
    {
        Ok(port) => try_launch_dbus_controller(port).await,
        Err(_) => None,
    };

    tokio::signal::ctrl_c().await.expect("Signal wait failed");
    if let Some(h) = dbus_controller_handle {
        h.await_shutdown().await;
    }
    environment_controller_handle.await_shutdown().await;
    sleep_sensor_handle.await_shutdown().await;
    effector_inventory.await_shutdown().await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
