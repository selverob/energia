use std::time::Duration;

use crate::external::display_server::DisplayServerController;
use crate::{external::display_server::SystemState};
use anyhow::{anyhow, Context, Result};
use log;
use tokio::{sync::{broadcast, watch}};

pub struct Sequencer<C: DisplayServerController> {
    timeout_sequence: Vec<u64>,
    current_position: usize,
    controller: C,
    state_channel: watch::Receiver<SystemState>,
    original_timeout: Option<i16>,
    sender: Option<broadcast::Sender<SystemState>>,
}

impl<C: DisplayServerController> Sequencer<C> {
    pub fn new(
        controller: C,
        state_channel: watch::Receiver<SystemState>,
        timeout_sequence: &Vec<u64>,
    ) -> Sequencer<C> {
        Sequencer {
            timeout_sequence: timeout_sequence.clone(),
            current_position: 0,
            controller,
            state_channel,
            original_timeout: None,
            sender: None,
        }
    }

    pub async fn spawn(mut self) -> Result<broadcast::Receiver<SystemState>> {
        let receiver = self.initialize().await?;

        tokio::spawn(async move {
            // We're ignoring errors here, since any error other than channel
            // closure should be handled in main_loop and channel closures mean
            // we can terminate
            let _ = self.main_loop().await;
            if let Err(e) = self.tear_down().await {
                log::error!("Error when tearing down: {}", e);
            }
        });

        Ok(receiver)
    }

    async fn initialize(&mut self) -> Result<broadcast::Receiver<SystemState>> {
        self.original_timeout = match self.get_current_ds_timeout().await {
            Ok(initial_timeout) => Some(initial_timeout),
            Err(err) => {
                log::error!("Failed getting initial timeout, setting it to -1: {}", err);
                None
            }
        };
        self.set_ds_timeout(self.timeout_sequence[0] as i16)
            .await
            .context("Failed to set initial timeout on the display server")?;
        let (sender, receiver) = broadcast::channel(8);
        self.sender = Some(sender);
        Ok(receiver)
    }

    async fn get_current_ds_timeout(&self) -> Result<i16> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.get_idleness_timeout()).await?
    }

    async fn set_ds_timeout(&self, timeout: i16) -> Result<()> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.set_idleness_timeout(timeout)).await?
    }

    async fn main_loop(&mut self) -> Result<()> {
        loop {
            if self.current_position == 0 {
                self.wait_for_ds_signal(SystemState::Idle).await?;
                self.current_position += 1;
            } else if self.current_position < self.timeout_sequence.len() {
                self.wait_for_internal_sleep().await?;
            } else {
                self.wait_for_ds_signal(SystemState::Awakened).await?;
                self.current_position = 0;
            }
        }
    }

    async fn wait_for_ds_signal(&mut self, expected_state: SystemState) -> Result<()> {
        self.state_channel
            .changed()
            .await
            .context("Display server idleness channel dropped")?;
        loop {
            let received_state = *self.state_channel.borrow_and_update();
            if received_state!= expected_state {
                log::error!("Received an unexpected state {:?} from display server, is something else setting the timeouts?", received_state);
            } else {
                break;
            }
        }
        self.sender.as_ref().unwrap().send(expected_state)?;
        Ok(())
    }

    async fn wait_for_internal_sleep(&mut self) -> Result<()> {
        let sleep = tokio::time::sleep(Duration::from_secs(
            self.timeout_sequence[self.current_position],
        ));
        tokio::pin!(sleep);
        tokio::select! {
            _ = &mut sleep => {
                self.sender.as_ref().unwrap().send(SystemState::Idle)?;
                self.current_position += 1;
            }
            _ = self.state_channel.changed() => {
                if *self.state_channel.borrow_and_update() == SystemState::Awakened {
                    self.sender.as_ref().unwrap().send(SystemState::Awakened)?;
                    self.current_position = 0;
                } else {
                    log::error!("Received an unexpected idle from display server, is something else setting the timeouts?");
                }
            }
        };
        Ok(())
    }

    async fn tear_down(&mut self) -> Result<()> {
        Ok(self
            .set_ds_timeout(self.original_timeout.unwrap_or(-1i16))
            .await?)
    }
}
