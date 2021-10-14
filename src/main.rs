mod control;
// mod external;
mod system;
mod armaf; 

use tokio;
use std::env;
use env_logger;
use std::time::Duration;

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let sensor_port = armaf::test_sensor::spawn(Duration::from_secs(5));
    let controller_port = armaf::test_controller::spawn(Duration::from_secs(10), sensor_port);
    tokio::time::sleep(Duration::from_secs(30)).await;
    controller_port.request(()).await;
}

// use anyhow::Result;
// use external::idleness;
// use external::idleness::idleness_monitor::IdlenessMonitor;
// use log::info;
// use std::time::{Duration, Instant};
// use std::env;
// // use std::thread::sleep;
// // use std::time::Duration;
// // use zbus;
// // use zvariant::OwnedObjectPath;

// fn main() -> Result<()> {
//     env::set_var("RUST_LOG", "debug");
//     env_logger::init();
//     let mut monitor = idleness::x11::X11IdlenessMonitor::new(None)?;
//     monitor.set_idleness_timeout(15)?;
//     info!("Idleness timeout set");
//     let receiver = monitor.get_idleness_channel();
//     loop {
//         let result = receiver.recv_deadline(Instant::now() + Duration::from_secs(20));
//         match result {
//             Ok(state) => info!("Got screensaver event, system is {:?}", state),
//             Err(_) => break,
//         };
//     }
//     monitor.set_idleness_timeout(-1)
// }
