mod control;
mod external;
mod system;
use actix::prelude::*;
use control::idleness_controller::IdlenessController;
use std::env;
use system::messages::Stop;

#[actix::main]
async fn main() {
    env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let addr = IdlenessController::new(vec![]).start();

    let stop_response = addr.send(Stop).await;
    println!("{:?}", stop_response);
    //System::current().stop();
}

// use anyhow::Result;
// use external::idleness;
// use external::idleness::idleness_monitor::IdlenessMonitor;
// use log::info;
// use std::time::{Duration, Instant};
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
