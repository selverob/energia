mod session;

use std::time::Duration;
use dbus::blocking::{Connection};
use session::Manager;

fn main() {
    let conn = Connection::new_system().unwrap();
    let proxy = conn.with_proxy("org.freedesktop.login1", "/org/freedesktop/login1", Duration::new(5, 0));
    let first_session = &proxy.list_sessions().unwrap()[0].0;
    proxy.lock_session(first_session);
}
