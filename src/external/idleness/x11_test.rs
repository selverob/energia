use super::idleness_monitor::{IdlenessMonitor, SystemState};
use super::x11;
use std::io;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xtest::ConnectionExt;
use x11rb::rust_connection::RustConnection;

static DISPLAY_NUMBER: AtomicUsize = AtomicUsize::new(1);

fn initialize_xvfb(enable_extension: bool) -> io::Result<(String, Child)> {
    let mut command = Command::new("Xvfb");
    command.args(["-br", "-ac", "-terminate", "-audit", "4"]);
    if !enable_extension {
        command.args(["-extension", "MIT-SCREEN-SAVER"]);
    }
    let display_id = DISPLAY_NUMBER.fetch_add(1, Ordering::SeqCst);
    let display_addr = format!(":{}", display_id);
    command.arg(&display_addr);
    let child = command.spawn()?;
    // Xvfb forks and takes some time to initialize, so we just need to wait for a while
    // If you start getting errors from the Idleness Monitor saying it cannot connect to
    // X11, try increasing this delay.
    sleep(Duration::from_millis(800));
    Ok((display_addr, child))
}

fn connect_to_xvfb(display_addr: Option<&str>) -> (RustConnection, usize) {
    RustConnection::connect(display_addr).expect("Couldn't create test connection to Xvfb")
}

#[test]
fn test_xvfb_init() -> io::Result<()> {
    let (addr, mut child) = initialize_xvfb(true)?;
    let (connection, _) = connect_to_xvfb(Some(&addr));
    assert_eq!(connection.setup().roots_len(), 1);
    drop(connection);
    child.wait().expect("Xvfb didn't even start");
    Ok(())
}

#[test]
fn test_error_without_extension() {
    let (addr, mut child) = initialize_xvfb(false).expect("Xvfb couldn't be started");
    let monitor = x11::X11IdlenessMonitor::new(Some(&addr));
    assert!(monitor.is_err());
    assert!(monitor
        .unwrap_err()
        .to_string()
        .contains("screensaver X11 extension unsupported"));
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_termination() {
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let monitor =
        x11::X11IdlenessMonitor::new(Some(&addr)).expect("Failed to create Idleness Monitor");
    monitor
        .terminate_watcher()
        .expect("Error when terminating watcher");
    monitor
        .uninstall_screensaver()
        .expect("Error when uninstalling screensaver");
    drop(monitor);
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_basic_flow() {
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let (connection, screen_num) = connect_to_xvfb(Some(&addr));
    let root = connection.setup().roots[screen_num].root;
    let mut monitor =
        x11::X11IdlenessMonitor::new(Some(&addr)).expect("Failed to create Idleness Monitor");
    monitor
        .set_idleness_timeout(2)
        .expect("Failed to set Idleness timeout");
    let mut receiver = monitor.get_idleness_channel();
    sleep(Duration::from_secs(3));
    assert!(receiver.has_changed().expect("Failure in receive channel"));
    assert_eq!(*receiver.borrow_and_update(), SystemState::Idle);
    connection
        .xtest_fake_input(2, 12, x11rb::CURRENT_TIME, root, 0, 0, 0)
        .expect("Failed sending event")
        .check()
        .expect("X11 failed to process synthetic event");
    connection.flush().expect("Failed to flush connection");
    sleep(Duration::from_secs(2));
    assert!(receiver.has_changed().expect("Failure in receive channel"));
    assert_eq!(*receiver.borrow_and_update(), SystemState::Awakened);
    monitor
        .set_idleness_timeout(-1)
        .expect("Failed to reset screensaver timeout");
    drop(connection);
    drop(monitor);
    child.wait().expect("Xvfb didn't even start");
}
