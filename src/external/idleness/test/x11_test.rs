use crate::external::idleness::x11;
use crate::external::idleness::{DisplayServerInterface, IdlenessController, SystemState};
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
    // If you start getting errors from the display server interface saying it cannot connect to
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
    let iface = x11::X11Interface::new(Some(&addr));
    assert!(iface.is_err());
    assert!(iface
        .unwrap_err()
        .to_string()
        .contains("screensaver X11 extension unsupported"));
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_termination() {
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let iface =
        x11::X11Interface::new(Some(&addr)).expect("Failed to create display server interface");
    iface
        .terminate_watcher()
        .expect("Error when terminating watcher");
    iface
        .uninstall_screensaver()
        .expect("Error when uninstalling screensaver");
    drop(iface);
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_setting_and_getting_timeout() {
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let iface =
        x11::X11Interface::new(Some(&addr)).expect("Failed to create display server interface");
    let controller = iface.get_idleness_controller();
    let default = controller
        .get_idleness_timeout()
        .expect("Couldn't get idleness timeout");
    controller
        .set_idleness_timeout(2)
        .expect("Couldn't set idleness timeout");
    assert_eq!(
        controller
            .get_idleness_timeout()
            .expect("Couldn't get idleness timeout"),
        2
    );
    controller
        .set_idleness_timeout(-1)
        .expect("Couldn't set idleness timeout");
    assert_eq!(
        controller
            .get_idleness_timeout()
            .expect("Couldn't get idleness timeout"),
        default
    );
    drop(controller);
    drop(iface);
    child.wait().expect("Xvfb didn't even start");
}

#[test]
fn test_basic_flow() {
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb couldn't be started");
    let (connection, screen_num) = connect_to_xvfb(Some(&addr));
    let root = connection.setup().roots[screen_num].root;
    let iface =
        x11::X11Interface::new(Some(&addr)).expect("Failed to create display server interface");
    let controller = iface.get_idleness_controller();
    controller
        .set_idleness_timeout(2)
        .expect("Failed to set Idleness timeout");
    let mut receiver = iface.get_idleness_channel();
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
    controller
        .set_idleness_timeout(-1)
        .expect("Failed to reset screensaver timeout");
    drop(connection);
    drop(controller);
    drop(iface);
    child.wait().expect("Xvfb didn't even start");
}
