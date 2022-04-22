#![warn(missing_docs)]

//! A modern power manager for Linux

mod armaf;
mod control;
mod external;
mod system;

use clap::Parser;
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

/// A modern power manager
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about=None)]
struct Args {
    /// Log verbosity. Either one of trace, debug, info, warn, error or a full Rust flexi_logger specification
    #[clap(short, long, default_value_t = String::from("info"))]
    log_level: String,

    /// Directory into which to write log files. Defaults to ~/.config/energia/log/
    #[clap(long)]
    log_directory: Option<String>,

    /// Path to the configuration file. Defaults to ~/.config/energia/config.toml
    #[clap(long, short)]
    config_file: Option<String>,
}

fn get_user_home() -> String {
    env::var("HOME").unwrap_or("".to_owned())
}

fn initialize_logging(args: &Args) -> anyhow::Result<flexi_logger::LoggerHandle> {
    let default_dir = format!("{}/.config/energia/log", get_user_home());
    let log_dir = args.log_directory.as_ref().unwrap_or(&default_dir);
    Ok(Logger::try_with_str(&args.log_level)?
        .log_to_file(FileSpec::default().directory(log_dir).basename("energia"))
        .format(flexi_logger::opt_format)
        .print_message()
        .duplicate_to_stderr(flexi_logger::Duplicate::Debug)
        .start()?)
}

async fn parse_config(args: &Args) -> anyhow::Result<toml::Value> {
    let default_path = format!("{}/.config/energia/config.toml", get_user_home());
    let config_path = args.config_file.as_ref().unwrap_or(&default_path);
    Ok(toml::from_slice(&fs::read(config_path).await?)?)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let log_handle = initialize_logging(&args);
    if let Err(e) = log_handle.as_ref() {
        println!("Failed to initialize logging system: {}", e);
    }
    log_panics::init();

    let config = parse_config(&args)
        .await
        .expect("Couldn't read configuration");
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
        .map(Some)
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
