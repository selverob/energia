use super::idleness_monitor::{IdlenessMonitor, SystemState};
use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::mpsc;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::screensaver::{self, ConnectionExt as _, State};
use x11rb::protocol::xproto::{
    AtomEnum, Blanking, ConnectionExt as _, Exposures, PropMode, Screen, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X11IdlenessMonitor {
    event_receiver: mpsc::Receiver<SystemState>,
    command_connection: RustConnection,
    // Stores the ID of the window on which events to stop monitoring thread can be sent
    //window_id: Window
}

impl X11IdlenessMonitor {
    pub fn new(display_name: Option<&str>) -> Result<X11IdlenessMonitor> {
        let command_connection = RustConnection::connect(display_name)?.0;
        if command_connection
            .extension_information(screensaver::X11_EXTENSION_NAME)?
            .is_none()
        {
            return Err(anyhow!("screensaver X11 extension unsupported"));
        }
        let event_receiver = start_event_receiver(display_name)?;
        Ok(X11IdlenessMonitor {
            event_receiver,
            command_connection,
        })
    }
}

impl IdlenessMonitor for X11IdlenessMonitor {
    fn get_idleness_channel(&mut self) -> &mut mpsc::Receiver<SystemState> {
        &mut self.event_receiver
    }

    fn set_idleness_timeout(&mut self, timeout: i16) -> Result<()> {
        debug!("Setting X11 idleness timeout to {}", timeout);
        Ok(self
            .command_connection
            .set_screen_saver(timeout, 0, Blanking::NOT_PREFERRED, Exposures::DEFAULT)?
            .check()?)
    }
}

fn start_event_receiver(display_name: Option<&str>) -> Result<mpsc::Receiver<SystemState>> {
    let (event_connection, screen_num) = RustConnection::connect(display_name)?;
    let screen = &event_connection.setup().roots[screen_num];
    install_screensaver(&event_connection, screen)?;
    event_connection
        .screensaver_select_input(screen.root, screensaver::Event::NOTIFY_MASK)?
        .check()
        .context("Couldn't set event mask for screensaver events")?;
    let (tx, rx) = mpsc::channel(5);
    std::thread::spawn(move || loop {
        let event_result = event_connection.wait_for_event();
        debug!("Received idleness event from X11");
        match event_result {
            Err(err) => {
                error!("Error received when waiting for idleness event: {:?}", err);
                continue;
            }
            Ok(Event::ScreensaverNotify(event)) => tx
                .blocking_send(event.state.into())
                .unwrap_or_else(|err| error!("Couldn't notify about idleness event: {}", err)),
            _ => error!("Unknown event received from X11"),
        }
    });
    Ok(rx)
}

fn install_screensaver(connection: &RustConnection, screen: &Screen) -> Result<()> {
    // Screensaver installation code from xss-lock's register_screensaver function,
    // translated to x11rb with event registration bits ripped out.
    let pixmap_id = connection.generate_id()?;
    let pixmap_create_cookie =
        connection.create_pixmap(screen.root_depth, pixmap_id, screen.root, 1, 1)?;
    let screensaver_atom_cookie = connection.intern_atom(false, "_MIT_SCREEN_SAVER_ID".as_bytes());
    let set_attributes_cookie = connection.screensaver_set_attributes(
        screen.root,
        -1,
        -1,
        1,
        1,
        0,
        WindowClass::COPY_FROM_PARENT,
        screen.root_depth,
        0,
        &Default::default(),
    );
    pixmap_create_cookie
        .check()
        .context("Couldn't create pixmap for screensaver")?;
    let atom = screensaver_atom_cookie?.reply()?.atom;
    set_attributes_cookie?.check().context(
        "Couldn't set screensaver attributes. Is another screensaver already installed?",
    )?;
    connection
        .change_property(
            PropMode::REPLACE,
            screen.root,
            atom,
            AtomEnum::PIXMAP,
            32,
            1,
            &pixmap_id.to_ne_bytes(),
        )?
        .check()?;
    Ok(())
}

impl Into<SystemState> for State {
    fn into(self) -> SystemState {
        match self {
            State::ON => SystemState::Idle,
            State::CYCLE => SystemState::Idle,
            State::OFF => SystemState::Awakened,
            State::DISABLED => SystemState::Awakened,
            _ => unreachable!(),
        }
    }
}
