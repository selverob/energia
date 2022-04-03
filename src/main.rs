#![warn(missing_docs)]

//! A modern power manager for Linux

mod armaf;
mod control;
mod external;
mod system;

use control::{dbus_controller::DBusController, environment_controller::EnvironmentController};
use external::dependency_provider::DependencyProvider;
use flexi_logger::{FileSpec, Logger};
use std::env;
use tokio::{self, fs};

use crate::{
    armaf::spawn_server,
    control::{
        effector_inventory::{EffectorInventory, GetEffectorPort},
        sleep_controller::SleepController,
    },
    system::{
        inhibition_sensor::InhibitionSensor, sleep_sensor::SleepSensor, upower_sensor::UPowerSensor,
    },
};

fn initialize_logging() -> anyhow::Result<flexi_logger::LoggerHandle> {
    let user_home = env::var("HOME").unwrap_or("".to_owned());
    let log_dir =
        env::var("ENERGIA_LOG_DIR").unwrap_or(format!("{}/.config/energia/log", user_home));
    Ok(Logger::try_with_env_or_str("info")?
        .log_to_file(FileSpec::default().directory(log_dir).basename("energia"))
        .print_message()
        .duplicate_to_stderr(flexi_logger::Duplicate::Debug)
        .start()?)
}

async fn parse_config() -> anyhow::Result<toml::Value> {
    let user_home = env::var("HOME").unwrap_or("".to_owned());
    let config_path = env::var("ENERGIA_CONFIG_PATH")
        .unwrap_or(format!("{}/.config/energia/config.toml", user_home));
    Ok(toml::from_slice(&fs::read(config_path).await?)?)
}

#[tokio::main]
async fn main() {
    let log_handle = initialize_logging();
    if let Err(e) = log_handle.as_ref() {
        println!("Failed to initialize logging system: {}", e);
    }
    log_panics::init();

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

    let environment_controller = EnvironmentController::new(
        &config,
        effector_inventory.clone(),
        inhibition_sensor,
        ds_controller.clone(),
        idleness_channel,
        upower_channel,
    );

    let environment_controller_handle = environment_controller
        .spawn()
        .await
        .expect("Couldn't spawn environment controller");

    let lock_effector = effector_inventory
        .request(GetEffectorPort("lock".to_string()))
        .await
        .map(|p| Some(p))
        .unwrap_or(None);

    let dbus_controller_handle = DBusController::new(
        "/org/energia/Manager",
        "org.energia.Manager",
        lock_effector.clone(),
    )
    .spawn()
    .await
    .expect("Failed to start D-Bus controller");

    let sleep_controller_handle = SleepController::new(
        sleep_sensor_channel.subscribe(),
        lock_effector,
        ds_controller,
    )
    .spawn()
    .await;

    tokio::signal::ctrl_c().await.expect("Signal wait failed");
    environment_controller_handle.await_shutdown().await;
    sleep_controller_handle.await_shutdown().await;
    sleep_sensor_handle.await_shutdown().await;
    dbus_controller_handle.await_shutdown().await;
    effector_inventory.await_shutdown().await;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
