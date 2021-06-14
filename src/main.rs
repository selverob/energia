mod dbus;
mod idleness;

use crate::dbus::{login_manager, session};

use anyhow::{bail, Result};
use idleness::idleness_monitor::IdlenessMonitor;
use log::info;
use std::time::{Duration, Instant};
// use std::thread::sleep;
// use std::time::Duration;
// use zbus;
// use zvariant::OwnedObjectPath;

fn main() -> Result<()> {
    env_logger::init();
    let mut monitor = idleness::x11::X11IdlenessMonitor::new(None)?;
    monitor.set_idleness_timeout(15)?;
    info!("Idleness timeout set");
    let receiver = monitor.get_idleness_channel();
    let result = receiver.recv_deadline(Instant::now() + Duration::from_secs(30));
    if result.is_ok() {
        info!("Got screensaver event");
    }
    monitor.set_idleness_timeout(-1)
}

// fn main() -> Result<()> {
//     env_logger::init();
//     let conn = zbus::Connection::new_system()?;
//     let session_path = get_session_path(&conn)?;
//     monitor_session_idle_status(&conn, session_path)
// }

// fn get_session_path(conn: &zbus::Connection) -> Result<OwnedObjectPath> {
//     let manager = login_manager::ManagerProxy::new(conn)?;
//     let mut sessions = manager.list_sessions()?;
//     if let Some((_, _, _, _, path)) = sessions.pop() {
//         Ok(path)
//     } else {
//         bail!("No sessions found on the system")
//     }
// }

// fn monitor_session_idle_status(
//     conn: &zbus::Connection,
//     session_path: OwnedObjectPath,
// ) -> Result<()> {
//     let session_proxy =
//         session::SessionProxy::new_for(&conn, "org.freedesktop.login1", session_path.as_str())?;
//     loop {
//         let hint = session_proxy.idle_hint()?;
//         info!("Idle hint is: {}", hint);
//         sleep(Duration::from_secs(5));
//     }
// }
