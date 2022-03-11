use std::time::Duration;

use crate::external::display_server::SystemState;
use crate::{
    armaf::{self, ActorRequestError},
    external::display_server::DisplayServerController,
};
use anyhow::{Context, Result};
use log;
use tokio::sync::{broadcast, watch};

pub struct Sequencer<C: DisplayServerController> {
    timeout_sequence: Vec<u64>,
    current_position: usize,
    controller: C,
    state_channel: watch::Receiver<SystemState>,
    original_timeout: Option<i16>,
    port: armaf::ActorPort<SystemState, (), anyhow::Error>,
}

impl<C: DisplayServerController> Sequencer<C> {
    pub fn new(
        receiver_port: armaf::ActorPort<SystemState, (), anyhow::Error>,
        ds_controller: C,
        state_channel: watch::Receiver<SystemState>,
        timeout_sequence: &Vec<u64>,
    ) -> Sequencer<C> {
        Sequencer {
            timeout_sequence: timeout_sequence.clone(),
            current_position: 0,
            controller: ds_controller,
            state_channel,
            original_timeout: None,
            port: receiver_port,
        }
    }

    pub async fn spawn(mut self) -> Result<()> {
        self.initialize().await?;

        tokio::spawn(async move {
            // We're ignoring errors here, since any error other than channel
            // closure should be handled in main_loop and channel closures mean
            // we can terminate
            let _ = self.main_loop().await;
            if let Err(e) = self.tear_down().await {
                log::error!("Error when tearing down: {}", e);
            }
        });

        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
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
        Ok(())
    }

    async fn get_current_ds_timeout(&self) -> Result<i16> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.get_idleness_timeout()).await?
    }

    async fn set_ds_timeout(&self, timeout: i16) -> Result<()> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.set_idleness_timeout(timeout)).await?
    }

    async fn main_loop(&mut self) {
        loop {
            log::debug!("Waiting on timeout no. {}", self.current_position);
            if self.current_position == 0 {
                if let Err(e) = self.wait_for_ds_signal(SystemState::Idle, false).await {
                    if Self::is_terminating_error(e) {
                        return;
                    } else {
                        self.force_activity().await;
                        continue;
                    }
                }
                self.current_position += 1;
            } else if self.current_position < self.timeout_sequence.len() {
                if let Err(e) = self.wait_for_internal_sleep().await {
                    if Self::is_terminating_error(e) {
                        return;
                    }
                }
            } else {
                if let Err(e) = self.wait_for_ds_signal(SystemState::Awakened, false).await {
                    if Self::is_terminating_error(e) {
                        return;
                    } else {
                        continue;
                    }
                }
                self.current_position = 0;
            }
        }
    }

    async fn wait_for_ds_signal(
        &mut self,
        expected_state: SystemState,
        no_propagate: bool,
    ) -> Result<()> {
        log::debug!("Waiting for display server signal");
        self.state_channel
            .changed()
            .await
            .context("Display server idleness channel dropped")?;
        loop {
            let received_state = *self.state_channel.borrow_and_update();
            if received_state != expected_state {
                log::error!("Received an unexpected state {:?} from display server, is something else setting the timeouts?", received_state);
            } else {
                break;
            }
        }
        if !no_propagate {
            self.port.request(expected_state).await?;
        }
        Ok(())
    }

    async fn wait_for_internal_sleep(&mut self) -> Result<()> {
        log::debug!("Waiting for internal sleep or display server activity");
        let sleep = tokio::time::sleep(Duration::from_secs(
            self.timeout_sequence[self.current_position],
        ));
        tokio::pin!(sleep);
        tokio::select! {
            _ = &mut sleep => {
                self.port.request(SystemState::Idle).await?;
                self.current_position += 1;
            }
            _ = self.state_channel.changed() => {
                if *self.state_channel.borrow_and_update() == SystemState::Awakened {
                    self.port.request(SystemState::Awakened).await?;
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

    async fn force_activity(&mut self) {
        log::debug!("Recovering from actor error by forcing display server to be active");
        if let Err(e) = self.controller.force_activity() {
            log::error!(
                "Couldn't force activity on display server, effects will be stopped until next idleness. {}",
            e);
        }
        log::debug!("Waiting for display server to become active again...");
        if let Err(e) = self.wait_for_ds_signal(SystemState::Awakened, true).await {
            log::error!("Failure while waiting for filtered activity signal: {}", e);
        } else {
            log::debug!("Display server active");
        }
    }

    fn is_terminating_error(e: anyhow::Error) -> bool {
        match e.downcast_ref::<ActorRequestError<anyhow::Error>>() {
            Some(are) => match are {
                ActorRequestError::ActorError(actor_error) => {
                    log::error!("Internal error in downstream actor: {}", actor_error);
                    false
                }
                _ => true,
            },
            None => {
                log::error!("Internal error: {}", e);
                false
            }
        }
    }
}
