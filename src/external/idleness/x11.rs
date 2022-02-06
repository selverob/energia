use super::idleness_monitor::{IdlenessMonitor, SystemState};
use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::watch;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::screensaver::{self, ConnectionExt as _, State};
use x11rb::protocol::xproto::{
    AtomEnum, Blanking, ConnectionExt as _, CreateWindowAux, EventMask, Exposures, PropMode,
    Screen, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

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

pub struct X11IdlenessMonitor {
    event_receiver: watch::Receiver<SystemState>,
    command_connection: RustConnection,
    /// Stores the ID of the window on which events to stop monitoring thread can be sent
    control_window_id: Window,
    /// X11 atom representing the screensaver attached to the root window
    screensaver_atom: u32,
    screen_num: usize,
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
        let (receiver_connection, screen_num) = RustConnection::connect(display_name)?;
        let screen = receiver_connection.setup().roots[screen_num].clone();
        let screensaver_atom = Self::install_screensaver(&receiver_connection, &screen)?;
        let control_window_id = Self::install_control_window(&receiver_connection, &screen)?;
        log::debug!("Screensaver installed");
        let event_receiver = start_event_receiver(receiver_connection, screen, control_window_id)?;
        Ok(X11IdlenessMonitor {
            event_receiver,
            command_connection,
            control_window_id,
            screensaver_atom,
            screen_num,
        })
    }

    fn install_screensaver(connection: &RustConnection, screen: &Screen) -> Result<u32> {
        // Screensaver installation code from xss-lock's register_screensaver function,
        // translated to x11rb with event registration bits ripped out.
        let pixmap_id = connection.generate_id()?;
        let pixmap_create_cookie =
            connection.create_pixmap(screen.root_depth, pixmap_id, screen.root, 1, 1)?;
        let screensaver_atom_cookie =
            connection.intern_atom(false, "_MIT_SCREEN_SAVER_ID".as_bytes());
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
        Ok(atom)
    }

    fn install_control_window(connection: &RustConnection, screen: &Screen) -> Result<u32> {
        let window_id = connection.generate_id()?;
        let aux_values = CreateWindowAux::default().event_mask(EventMask::STRUCTURE_NOTIFY);
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window_id,
                screen.root,
                -1,
                -1,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                screen.root_visual,
                &aux_values,
            )?
            .check()
            .context("Couldn't install control window")?;
        connection.flush()?;
        Ok(window_id)
    }

    fn terminate_watcher(&self) -> Result<()> {
        log::info!("Terminating idleness watcher");
        self.command_connection
            .destroy_window(self.control_window_id)?
            .check()?;
        self.uninstall_screensaver()?;
        Ok(())
    }

    fn uninstall_screensaver(&self) -> Result<()> {
        log::info!("Uninstalling screensaver");
        let screen = &self.command_connection.setup().roots[self.screen_num];
        let unset_cookie = self
            .command_connection
            .screensaver_unset_attributes(screen.root)?;
        let property_delete_cookie = self
            .command_connection
            .delete_property(screen.root, self.screensaver_atom)?;
        unset_cookie.check().context("Couldn't unset screensaver")?;
        property_delete_cookie
            .check()
            .context("Couldn't delete screensaver property")
    }
}

impl IdlenessMonitor for X11IdlenessMonitor {
    fn get_idleness_channel(&self) -> watch::Receiver<SystemState> {
        self.event_receiver.clone()
    }

    fn set_idleness_timeout(&mut self, timeout: i16) -> Result<()> {
        debug!("Setting X11 idleness timeout to {}", timeout);
        Ok(self
            .command_connection
            .set_screen_saver(timeout, 0, Blanking::NOT_PREFERRED, Exposures::DEFAULT)?
            .check()?)
    }
}

impl Drop for X11IdlenessMonitor {
    fn drop(&mut self) {
        if let Err(e) = self.terminate_watcher() {
            log::error!("Couldn't terminate X11 watcher {}", e);
        }
    }
}

fn start_event_receiver(
    connection: RustConnection,
    screen: Screen,
    control_window_id: u32,
) -> Result<watch::Receiver<SystemState>> {
    connection
        .screensaver_select_input(screen.root, screensaver::Event::NOTIFY_MASK)?
        .check()
        .context("Couldn't set event mask for screensaver events")?;
    let (tx, rx) = watch::channel(SystemState::Awakened);
    std::thread::spawn(move || loop {
        let event_result = connection.wait_for_event();
        debug!("Received idleness event from X11");
        match event_result {
            Err(err) => {
                error!("Error received when waiting for idleness event: {:?}", err);
                continue;
            }
            Ok(Event::ScreensaverNotify(event)) => tx
                .send(event.state.into())
                .unwrap_or_else(|err| error!("Couldn't notify about idleness event: {}", err)),
            Ok(Event::DestroyNotify(event)) => {
                if event.window != control_window_id {
                    log::debug!("Spurious window destruction caught");
                }
                log::info!("X11 idleness control window destroyed, stopping watcher");
                return;
            }
            _ => error!("Unknown event received from X11"),
        }
    });
    Ok(rx)
}
