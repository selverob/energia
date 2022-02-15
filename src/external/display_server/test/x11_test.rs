use crate::external::display_server::x11::{self, X11DisplayServerController, X11Interface};
use crate::external::display_server::{
    test, DPMSLevel, DPMSTimeouts, DisplayServer, DisplayServerController, SystemState,
};
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
    command.args(["-br", "-ac", "-screen", "0", "200x200x24", "-terminate"]);
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

fn with_xvfb<F>(func: F)
where
    F: FnOnce(x11::X11Interface, RustConnection, usize),
{
    let (addr, mut child) = initialize_xvfb(true).expect("Xvfb initialization failed");
    let iface = x11::X11Interface::new(Some(&addr)).expect("Couldn't create X11 interface");
    let (connection, screen_num) = connect_to_xvfb(Some(&addr));
    func(iface, connection, screen_num);
    child.wait().expect("Xvfb didn't even start");
}

fn with_system_x11<F>(func: F)
where
    F: FnOnce(x11::X11Interface, RustConnection, usize),
{
    let iface = x11::X11Interface::new(None).expect("Couldn't create X11 interface");
    let (connection, screen_num) =
        RustConnection::connect(None).expect("Couldn't create test connection to system X11");
    func(iface, connection, screen_num);
}
#[test]
fn test_xvfb_init() {
    with_xvfb(|_, connection, _| {
        assert_eq!(connection.setup().roots_len(), 1);
    });
}

#[test]
fn test_error_without_extension() {
    let (addr, mut child) = initialize_xvfb(false).expect("Xvfb initialization failed");
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
    with_xvfb(|iface, _, _| {
        iface
            .terminate_watcher()
            .expect("Error when terminating watcher");
        iface
            .uninstall_screensaver()
            .expect("Error when uninstalling screensaver");
    });
}

#[test]
fn test_setting_and_getting_timeout() {
    with_xvfb(|iface, _, _| {
        let controller = iface.get_controller();
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
    });
}

#[test]
fn test_basic_flow() {
    with_xvfb(|iface, connection, screen_num| {
        let root = connection.setup().roots[screen_num].root;
        let controller = iface.get_controller();
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
    });
}

// Since this needs to use system's X11 due to dummy X11 driver and XVfb not
// supporting DPMS, it's ingored by default.
// Additionally, since we need these tests not to run in parallel, the whole
// coverage for X11's DPMS is merged into a single test function.
// This will cause blinking on your local display.
// Do not move your mouse while running the test!
#[test]
#[ignore]
fn test_dpms() {
    with_system_x11(|iface, _, _| {
        test_dpms_state_control(iface.get_controller());
        test_dpms_levels(iface.get_controller());
        test_dpms_timeouts(iface.get_controller());
    });
}

fn test_dpms_state_control(controller: X11DisplayServerController) {
    assert!(controller.is_dpms_capable().unwrap());
    controller.set_dpms_state(false).unwrap();
    assert_eq!(controller.get_dpms_level().unwrap(), None);
    controller.set_dpms_state(true).unwrap();
    assert_eq!(controller.get_dpms_level().unwrap(), Some(DPMSLevel::On));
}

fn test_dpms_levels(controller: X11DisplayServerController) {
    controller
        .set_dpms_state(true)
        .expect("Couldn't enable DPMS");
    for level in vec![
        DPMSLevel::Standby,
        DPMSLevel::Suspend,
        DPMSLevel::Off,
        DPMSLevel::On,
    ] {
        controller
            .set_dpms_level(level)
            .expect("Failed to set DPMS level");
        assert_eq!(controller.get_dpms_level().unwrap(), Some(level));
    }
}

fn test_dpms_timeouts(controller: X11DisplayServerController) {
    let original_timeouts = controller
        .get_dpms_timeouts()
        .expect("Couldn't get current DPMS timeouts");
    let test_timeouts = DPMSTimeouts::new(10, 20, 30);
    controller
        .set_dpms_timeouts(test_timeouts)
        .expect("Couldn't set DPMS timeouts");
    assert_eq!(
        controller
            .get_dpms_timeouts()
            .expect("Couldn't get DPMS timeouts"),
        test_timeouts
    );
    controller
        .set_dpms_timeouts(original_timeouts)
        .expect("Couldn't reset DPMS timeouts");
}
