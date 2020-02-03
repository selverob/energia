use std::env;

use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus_APP_INDICATOR_STATUS_ACTIVE};
use glib::source::{Continue, timeout_add_local};
use std::time::Instant;

fn main() {
    let first_instant = Instant::now();
    gtk::init().unwrap();
    let mut indicator = AppIndicator::new("libappindicator test application", "");
    indicator.set_status(AppIndicatorStatus_APP_INDICATOR_STATUS_ACTIVE);
    let mut path = env::current_dir().expect("");
    path.push("./examples/rust-logo-64x64-blk.png");
    indicator.set_icon_full(path.to_str().unwrap(), "icon");
    timeout_add_local(1000, move || {
        let elapsed_secs = first_instant.elapsed().as_secs();
        let mut m = gtk::Menu::new();
        let mi = gtk::MenuItem::new_with_label(&format!("{:}", elapsed_secs));
        m.append(&mi);
        indicator.set_menu(&mut m); 
        m.show_all();
        Continue(true)
    });
    gtk::main();
}
