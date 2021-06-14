use std::thread::sleep;

use super::idleness_monitor::IdlenessMonitor;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel;
use log::{debug, error};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::screensaver::{self, ConnectionExt as _, Kind, State};
use x11rb::protocol::xproto::{Blanking, ConnectionExt as _, Exposures, WindowClass};
use x11rb::rust_connection::RustConnection;

pub struct X11IdlenessMonitor {
    event_receiver: crossbeam_channel::Receiver<()>,
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
            command_connection,
            event_receiver,
        })
    }
}

impl IdlenessMonitor for X11IdlenessMonitor {
    fn get_idleness_channel(&self) -> crossbeam_channel::Receiver<()> {
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

fn start_event_receiver(display_name: Option<&str>) -> Result<crossbeam_channel::Receiver<()>> {
    let (event_connection, screen_num) = RustConnection::connect(display_name)?;
    let window_id = create_window(&event_connection, screen_num)?;
    event_connection
        .screensaver_select_input(window_id, screensaver::Event::NOTIFY_MASK)?
        .check()
        .context("Couldn't set event mask for screensaver events")?;
    let (tx, rx) = crossbeam_channel::bounded(3);
    std::thread::spawn(move || loop {
        let event_result = event_connection.wait_for_event();
        debug!("{:?}", event_result);
        if event_result.is_err() {
            error!(
                "Error received when waiting for idleness event: {:?}",
                event_result.err()
            );
            continue;
        }
        debug!("Received idleness event from X11");
        tx.send(())
            .unwrap_or_else(|err| error!("Couldn't notify about idleness event: {}", err));
    });
    Ok(rx)
}

fn create_window(connection: &RustConnection, screen_num: usize) -> Result<u32> {
    let screen = &connection.setup().roots[screen_num];
    let window_id = connection.generate_id()?;

    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window_id,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &Default::default(),
        )?
        .check()
        .context("Couldn't create screensaver state monitoring window")?;
    connection
        .map_window(window_id)?
        .check()
        .context("Couldn't map screensaver state monitoring window")?;
    connection.flush()?;
    Ok(window_id)
}
