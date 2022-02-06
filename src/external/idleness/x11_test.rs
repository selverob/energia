use super::x11;
use super::idleness_monitor::{IdlenessMonitor, SystemState};
use std::process::{Command, Child};
use std::io;
use std::env;
use std::thread::sleep;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xtest::ConnectionExt;
use x11rb::rust_connection::RustConnection;

const XVFB_SCREEN: Option<&'static str> = Some(":4");

fn initialize_xvfb(enable_extension: bool) -> io::Result<Child> {
    env::set_var("RUST_LOG", "debug");
    let mut command = Command::new("Xvfb");
    command.args(["-br", "-ac", "-terminate", "-audit", "4",]);
    if !enable_extension {
        command.args(["-extension", "MIT-SCREEN-SAVER"]);
    }
    command.arg(XVFB_SCREEN.unwrap());
    let child = command.spawn()?;
    // Xvfb forks and takes some time to initialize, so we just need to wait for a while
    // If you start getting errors from the Idleness Monitor saying it cannot connect to
    // X11, try increasing this delay.
    sleep(Duration::from_millis(800));
    Ok(child)
}

fn connect_to_xvfb() -> (RustConnection, usize) {
    RustConnection::connect(XVFB_SCREEN).expect("Couldn't create test connection to Xvfb")
}

#[test]
fn test_xvfb_init() -> io::Result<()>{
    let mut child = initialize_xvfb(true)?;
    let (connection, screen_num) = connect_to_xvfb();
    assert_eq!(connection.setup().roots_len(), 1);
    drop(connection);
    child.wait().expect("Xvfb didn't even start");
    Ok(())
}

#[test]
fn test_error_without_extension() {
    let mut child = initialize_xvfb(false).expect("Xvfb couldn't be started");
    let monitor = x11::X11IdlenessMonitor::new(XVFB_SCREEN);
    assert!(monitor.is_err());
    assert!(monitor.unwrap_err().to_string().contains("screensaver X11 extension unsupported"));
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_termination() {
    let mut child = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let monitor = x11::X11IdlenessMonitor::new(XVFB_SCREEN).expect("Failed to create Idleness Monitor");
    monitor.terminate_watcher().expect("Error when terminating watcher");
    monitor.uninstall_screensaver().expect("Error when uninstalling screensaver");
    drop(monitor);
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_basic_flow() {
    let mut child = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let (connection, screen_num) = connect_to_xvfb();
    let root = connection.setup().roots[screen_num].root;
    let mut monitor = x11::X11IdlenessMonitor::new(XVFB_SCREEN).expect("Failed to create Idleness Monitor");
    monitor.set_idleness_timeout(2).expect("Failed to set Idleness timeout");
    let mut receiver = monitor.get_idleness_channel();
    sleep(Duration::from_secs(3));
    assert!(receiver.has_changed().expect("Failure in receive channel"));
    assert_eq!(*receiver.borrow_and_update(), SystemState::Idle);
    connection.xtest_fake_input(2, 12, x11rb::CURRENT_TIME, root, 0, 0, 0).expect("Failed sending event").check().expect("X11 failed to process synthetic event");
    connection.flush().expect("Failed to flush connection");
    sleep(Duration::from_secs(2));
    assert!(receiver.has_changed().expect("Failure in receive channel"));
    assert_eq!(*receiver.borrow_and_update(), SystemState::Awakened);
    monitor.set_idleness_timeout(-1).expect("Failed to reset screensaver timeout");
    drop(connection);
    drop(monitor);
    child.wait().expect("Xvfb didn't even start");
}
