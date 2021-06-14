use super::idleness_monitor::{IdlenessMonitor, SystemState};
use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::screensaver::{self, ConnectionExt as _, State};
use x11rb::protocol::xproto::{Blanking, ConnectionExt as _, Exposures};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub struct X11IdlenessMonitor {
    event_receiver: crossbeam_channel::Receiver<SystemState>,
    command_connection: RustConnection,
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
    fn get_idleness_channel(&self) -> crossbeam_channel::Receiver<SystemState> {
        self.event_receiver.clone()
    }

    fn set_idleness_timeout(&mut self, timeout: i16) -> Result<()> {
        debug!("Setting X11 idleness timeout to {}", timeout);
        Ok(self
            .command_connection
            .set_screen_saver(timeout, 0, Blanking::DEFAULT, Exposures::DEFAULT)?
            .check()?)
    }
}

fn start_event_receiver(
    display_name: Option<&str>,
) -> Result<crossbeam_channel::Receiver<SystemState>> {
    let (event_connection, screen_num) = RustConnection::connect(display_name)?;
    let screen = &event_connection.setup().roots[screen_num];
    // This is simple and will possibly break the moment user has an actual screensaver installed,
    // we may need to replace this with screensaver installation, look into xss-lock (register_screensaver function)
    // for inspiration
    event_connection
        .screensaver_select_input(screen.root, screensaver::Event::NOTIFY_MASK)?
        .check()
        .context("Couldn't set event mask for screensaver events")?;
    let (tx, rx) = crossbeam_channel::bounded::<SystemState>(3);
    std::thread::spawn(move || loop {
        let event_result = event_connection.wait_for_event();
        debug!("Received idleness event from X11");
        match event_result {
            Err(err) => {
                error!("Error received when waiting for idleness event: {:?}", err);
                continue;
            }
            Ok(Event::ScreensaverNotify(event)) => tx
                .send(event.state.into())
                .unwrap_or_else(|err| error!("Couldn't notify about idleness event: {}", err)),
            _ => error!("Unknown event received from X11"),
        }
    });
    Ok(rx)
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
