use std::sync::Arc;

use super::{
    interface::{DPMSLevel, DPMSTimeouts, DisplayServer, SystemState},
    DisplayServerController,
};
use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::watch;
use x11rb::{
    connection::{Connection, RequestConnection},
    protocol::{
        dpms::{self, ConnectionExt as _},
        screensaver::{self, ConnectionExt as _, State},
        xproto::{
            AtomEnum, Blanking, ConnectionExt as _, CreateWindowAux, EventMask, Exposures,
            PropMode, Screen, ScreenSaver, Window, WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
    COPY_DEPTH_FROM_PARENT,
};

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

#[derive(Debug, Clone)]
pub struct X11Interface {
    event_receiver: watch::Receiver<SystemState>,
    command_connection: Arc<RustConnection>,
    /// Stores the ID of the window on which events to stop monitoring thread can be sent
    control_window_id: Window,
    /// X11 atom representing the screensaver attached to the root window
    screensaver_atom: u32,
    screen_num: usize,
}

impl X11Interface {
    pub fn new(display_name: Option<&str>) -> Result<X11Interface> {
        let command_connection = Arc::new(RustConnection::connect(display_name)?.0);
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
        let event_receiver =
            Self::start_event_receiver(receiver_connection, screen, control_window_id)?;
        Ok(X11Interface {
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

    pub fn terminate_watcher(&self) -> Result<()> {
        log::info!("Terminating idleness watcher");
        self.command_connection
            .destroy_window(self.control_window_id)?
            .check()?;
        self.uninstall_screensaver()?;
        Ok(())
    }

    pub fn uninstall_screensaver(&self) -> Result<()> {
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
}

impl DisplayServer for X11Interface {
    type Controller = X11DisplayServerController;

    fn get_idleness_channel(&self) -> watch::Receiver<SystemState> {
        self.event_receiver.clone()
    }

    fn get_controller(&self) -> Self::Controller {
        X11DisplayServerController {
            connection: self.command_connection.clone(),
        }
    }
}

impl Drop for X11Interface {
    fn drop(&mut self) {
        if let Err(e) = self.terminate_watcher() {
            log::error!("Couldn't terminate X11 watcher {}", e);
        }
    }
}

#[derive(Debug, Clone)]
pub struct X11DisplayServerController {
    connection: Arc<RustConnection>,
}

impl DisplayServerController for X11DisplayServerController {
    fn set_idleness_timeout(&self, timeout: i16) -> Result<()> {
        debug!("Setting idleness timeout to {}", timeout);
        Ok(self
            .connection
            .set_screen_saver(timeout, 0, Blanking::NOT_PREFERRED, Exposures::DEFAULT)?
            .check()?)
    }

    fn get_idleness_timeout(&self) -> Result<i16> {
        debug!("Fetching idleness timeout");
        Ok(self.connection.get_screen_saver()?.reply()?.timeout as i16)
    }

    fn force_activity(&self) -> Result<()> {
        debug!("Force resetting the screensaver timeout");
        Ok(self
            .connection
            .force_screen_saver(ScreenSaver::RESET)?
            .check()?)
    }

    fn is_dpms_capable(&self) -> Result<bool> {
        debug!("Fetching DPMS capability");
        Ok(self.connection.dpms_capable()?.reply()?.capable)
    }

    fn get_dpms_level(&self) -> Result<Option<super::DPMSLevel>> {
        debug!("Fetching DPMS level");
        let info = self.connection.dpms_info()?.reply()?;
        if info.state {
            Ok(Some(DPMSLevel::from(info.power_level)))
        } else {
            Ok(None)
        }
    }

    fn set_dpms_level(&self, level: DPMSLevel) -> Result<()> {
        debug!("Setting DPMS level");
        Ok(self
            .connection
            .dpms_force_level(dpms::DPMSMode::from(level))?
            .check()?)
    }

    fn set_dpms_state(&self, enabled: bool) -> Result<()> {
        debug!("Setting DPMS state");
        if enabled {
            Ok(self.connection.dpms_enable()?.check()?)
        } else {
            Ok(self.connection.dpms_disable()?.check()?)
        }
    }

    fn get_dpms_timeouts(&self) -> Result<super::DPMSTimeouts> {
        debug!("Fetching DPMS timeouts");
        Ok(self.connection.dpms_get_timeouts()?.reply()?.into())
    }

    fn set_dpms_timeouts(&self, timeouts: super::DPMSTimeouts) -> Result<()> {
        debug!("Setting DPMS timeouts");
        Ok(self
            .connection
            .dpms_set_timeouts(timeouts.standby, timeouts.suspend, timeouts.off)?
            .check()?)
    }
}

impl From<dpms::DPMSMode> for DPMSLevel {
    fn from(mode: dpms::DPMSMode) -> Self {
        match mode {
            dpms::DPMSMode::ON => DPMSLevel::On,
            dpms::DPMSMode::STANDBY => DPMSLevel::Standby,
            dpms::DPMSMode::SUSPEND => DPMSLevel::Suspend,
            dpms::DPMSMode::OFF => DPMSLevel::Off,
            _ => unreachable!(),
        }
    }
}

impl From<DPMSLevel> for dpms::DPMSMode {
    fn from(level: DPMSLevel) -> Self {
        match level {
            DPMSLevel::On => dpms::DPMSMode::ON,
            DPMSLevel::Standby => dpms::DPMSMode::STANDBY,
            DPMSLevel::Suspend => dpms::DPMSMode::SUSPEND,
            DPMSLevel::Off => dpms::DPMSMode::OFF,
        }
    }
}

impl From<dpms::GetTimeoutsReply> for DPMSTimeouts {
    fn from(timeouts: dpms::GetTimeoutsReply) -> Self {
        DPMSTimeouts {
            standby: timeouts.standby_timeout,
            suspend: timeouts.suspend_timeout,
            off: timeouts.off_timeout,
        }
    }
}
