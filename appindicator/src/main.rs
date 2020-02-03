mod session;

use std::env;

use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus_APP_INDICATOR_STATUS_ACTIVE};
use glib::source::{Continue, timeout_add_local};
use dbus::blocking::{Connection};
use std::time::Duration;
use session::Manager;

fn main() {
    gtk::init().unwrap();
    let mut indicator = AppIndicator::new("libappindicator test application", "");
    indicator.set_status(AppIndicatorStatus_APP_INDICATOR_STATUS_ACTIVE);
    // let mut path = env::current_dir().expect("");
    // path.push("./examples/rust-logo-64x64-blk.png");
    // indicator.set_icon_full(path.to_str().unwrap(), "icon");
    timeout_add_local(1000, move || {
        let conn = Connection::new_system().unwrap();
        let proxy = conn.with_proxy("org.freedesktop.login1", "/org/freedesktop/login1", Duration::new(5, 0));
        let inhibitors = proxy.list_inhibitors().unwrap();
        let mut m = gtk::Menu::new();
        for (_, app, _, _, _, _) in inhibitors {
            let mi = gtk::MenuItemBuilder::new().;
            builder.label
            let mi = gtk::MenuItem::new_with_label(&app);
            m.append(&mi);
        }
        indicator.set_menu(&mut m); 
        m.show_all();
        Continue(true)
    });
    gtk::main();
}
