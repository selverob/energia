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

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "trace");
    env_logger::init();
    let config_bytes = fs::read("config.toml")
        .await
        .expect("Couldn't read config file");
    let config: toml::Value = toml::from_slice(&config_bytes).expect("Config parsing failuer");
    log::info!("Parsed config is: {:?}", config);
    let system_dependencies = DependencyProvider::make_system()
        .await
        .expect("Couldn't construct dependency provider");
    let environment_controller = EnvironmentController::new(&config, system_dependencies)
        .expect("Couldn't construct environment controller");
    let handle = environment_controller.spawn().await;
    tokio::signal::ctrl_c().await.expect("Signal wait failed");
    drop(handle);
}
