#![warn(missing_docs)]

//! A modern power manager for Linux

mod armaf;
mod control;
mod external;
mod system;

use env_logger;
use std::env;
use std::time::Duration;
use tokio;

// #[tokio::main]
// async fn main() {
//     env::set_var("RUST_LOG", "debug");
//     env_logger::init();
//     let idleness_controller = control::idleness_controller::spawn(vec![]);
//     tokio::time::sleep(Duration::from_secs(30)).await;
//     idleness_controller
//         .request(control::idleness_controller::Stop)
//         .await;
// }

use anyhow::Result;
use external::idleness;
use external::idleness::idleness_monitor::IdlenessMonitor;
use log::info;
use std::thread::sleep;
use std::time::Instant;
// use std::time::Duration;
// use zbus;
// use zvariant::OwnedObjectPath;

fn main() -> Result<()> {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let mut monitor = idleness::x11::X11IdlenessMonitor::new(None)?;
    monitor.set_idleness_timeout(7)?;
    info!("Idleness timeout set");
    let mut receiver = monitor.get_idleness_channel();
    for _ in 0..2 {
        while !receiver.has_changed()? {}
        info!(
            "Got screensaver event, system is {:?}",
            receiver.borrow_and_update()
        );
    }
    monitor.set_idleness_timeout(-1)
}
