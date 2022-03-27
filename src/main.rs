#![warn(missing_docs)]

//! A modern power manager for Linux

mod armaf;
mod control;
mod external;
mod system;

use control::environment_controller::EnvironmentController;
use env_logger;
use external::dependency_provider::DependencyProvider;
use std::env;
use tokio::{self, fs};

use crate::system::upower_sensor::UPowerSensor;

#[tokio::main]
async fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();
    let config_bytes = fs::read("config.toml")
        .await
        .expect("Couldn't read config file");
    let config: toml::Value = toml::from_slice(&config_bytes).expect("Config parsing failuer");
    log::info!("Parsed config is: {:?}", config);
    let mut system_dependencies = DependencyProvider::make_system()
        .await
        .expect("Couldn't construct dependency provider");
    let upower_channel = UPowerSensor::new(
        system_dependencies
            .get_dbus_system_connection()
            .await
            .expect("Couldn't get connection to system DBus"),
    )
    .await
    .expect("Couldn't start UPower sensor");
    let environment_controller =
        EnvironmentController::new(&config, system_dependencies, upower_channel);
    let handle = environment_controller
        .spawn()
        .await
        .expect("Couldn't spawn environment controller");
    tokio::signal::ctrl_c().await.expect("Signal wait failed");
    handle.await_shutdown().await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
