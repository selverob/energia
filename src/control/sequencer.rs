use crate::{
    armaf,
    external::display_server::{DisplayServerController, SystemState},
};
use anyhow::{Context, Result};
use log;
use std::time::Duration;
use thiserror::Error;
use tokio::{select, sync::watch, time::Instant};

#[derive(Debug, Copy, Clone)]
pub struct GetRunningTime;

#[derive(Debug, Copy, Clone, Error)]
#[error("Sequencer's port dropped, actor must terminate")]
struct PortDropped;

#[derive(Debug, Copy, Clone)]
enum PositionChange {
    Increment,
    Reset,
}

pub struct Sequencer<C: DisplayServerController> {
    timeout_sequence: Vec<u64>,
    current_position: usize,
    controller: C,
    state_channel: watch::Receiver<SystemState>,
    position_changed_at: Instant,
    original_timeout: Option<i16>,
    child_port: armaf::ActorPort<SystemState, (), anyhow::Error>,
    command_receiver: Option<armaf::ActorReceiver<GetRunningTime, Duration, ()>>,
    initial_position_dirty: bool,
    shorten_initial_sleep_by: Duration,
}

impl<C: DisplayServerController> Sequencer<C> {
    pub fn new(
        child_port: armaf::ActorPort<SystemState, (), anyhow::Error>,
        ds_controller: C,
        state_channel: watch::Receiver<SystemState>,
        timeout_sequence: &[u64],
        starting_position: usize,
        shorten_initial_sleep_by: Duration,
    ) -> Sequencer<C> {
        Sequencer {
            timeout_sequence: timeout_sequence.to_owned(),
            current_position: starting_position,
            controller: ds_controller,
            state_channel,
            position_changed_at: Instant::now(),
            original_timeout: None,
            child_port,
            command_receiver: None,
            initial_position_dirty: false,
            shorten_initial_sleep_by,
        }
    }

    pub async fn spawn(mut self) -> Result<armaf::ActorPort<GetRunningTime, Duration, ()>> {
        let (command_port, command_receiver) = armaf::ActorPort::make();
        self.command_receiver = Some(command_receiver);
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

        Ok(command_port)
    }

    async fn initialize(&mut self) -> Result<()> {
        self.original_timeout = match self.get_current_ds_timeout().await {
            Ok(initial_timeout) => Some(initial_timeout),
            Err(err) => {
                log::error!("Failed getting initial timeout, setting it to -1: {}", err);
                None
            }
        };
        self.initial_position_dirty =
            self.current_position != 0 && *self.state_channel.borrow() == SystemState::Awakened;
        log::debug!("Initial position dirty? {}", self.initial_position_dirty);
        let initial_timeout_index = if self.initial_position_dirty {
            self.current_position
        } else {
            0
        };
        self.set_ds_timeout(self.timeout_sequence[initial_timeout_index] as i16)
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
        // We want reuse the sleep future, so we need to set it to some initial
        // timeout. If the initial position is handled by display server, this
        // will just get ignored and eventually reset. If the initial position
        // is internally handled, this will ensure it fires.
        let sleep = tokio::time::sleep(
            Duration::from_secs(self.timeout_sequence[self.current_position])
                .saturating_sub(self.shorten_initial_sleep_by),
        );
        tokio::pin!(sleep);
        loop {
            let was_state_change = match self.loop_iteration(&mut sleep).await {
                Err(e) => {
                    if Self::is_terminating_error(e) {
                        return;
                    } else {
                        if self.current_position == 0 {
                            self.force_activity().await;
                        }
                        true
                    }
                }
                Ok(was_state_change) => was_state_change,
            };
            // We started within the sequence while the system was active, so
            // the current display server timeout is not associated with
            // position 0. Also, the last command wasn't a control command,
            // so we have actually advanced our position
            if self.initial_position_dirty && was_state_change {
                log::debug!("Undirtying initial position");
                if let Err(e) = self.set_ds_timeout(self.timeout_sequence[0] as i16).await {
                    log::error!("Couldn't set display server timeout, first effect bunch may be executed at unexpected times: {}", e);
                } else {
                    self.initial_position_dirty = false;
                }
            }
            if was_state_change && self.position_handleable_by_sleep() {
                log::debug!("Resetting the sleep future");
                sleep.as_mut().reset(
                    Instant::now()
                        .checked_add(Duration::from_secs(
                            self.timeout_sequence[self.current_position],
                        ))
                        .unwrap(),
                )
            }
        }
    }

    async fn loop_iteration(
        &mut self,
        sleep: &mut std::pin::Pin<&mut tokio::time::Sleep>,
    ) -> Result<bool> {
        select! {
            // Sleep futures are not fused, they will reinitialize every time
            // you await them, so we need to handle the condition here
            _ = sleep.as_mut(), if self.position_handleable_by_sleep() => {
                log::debug!("Sleep future fired");
                self.change_position_and_notify(PositionChange::Increment).await?;
                Ok(true)
            }
            change_result = self.state_channel.changed() => {
                log::debug!("Display server channel fired");
                change_result?;
                let new_state = *self.state_channel.borrow_and_update();
                let ds_position = if self.initial_position_dirty {
                    self.current_position
                } else {
                    0
                };
                match (self.current_position, new_state) {
                    (position, SystemState::Awakened) if position == ds_position => {
                        log::error!("Received an unexpected awake from display server, is something else setting the timeouts?");
                        Ok(false)
                    }
                    (position, SystemState::Idle) if position == ds_position  => {
                        self.change_position_and_notify(PositionChange::Increment).await?;
                        Ok(true)
                    }
                    (_, SystemState::Awakened) => {
                        self.change_position_and_notify(PositionChange::Reset).await?;
                        Ok(true)
                    }
                    (_, SystemState::Idle) => {
                        log::error!("Received an unexpected idle from display server, is something else setting the timeouts?");
                        Ok(false)
                    }
                }
            },
            res = self.command_receiver.as_mut().unwrap().recv() => {
                log::debug!("Command receiver fired");
                match res {
                    None => return Err(anyhow::Error::new(PortDropped)),
                    Some(req) => {
                        if req.respond(Ok(self.get_running_time())).is_err() {
                            log::error!("Couldn't respond to actor request, actor is probably dead. Terminating.");
                            return Err(anyhow::Error::new(PortDropped));
                        }
                    }
                };
                Ok(false)
            }
        }
    }

    async fn tear_down(self) -> Result<()> {
        log::debug!("Tearing down");
        let reset_result = self
            .set_ds_timeout(self.original_timeout.unwrap_or(-1i16))
            .await;
        self.child_port.await_shutdown().await;
        log::debug!("Stopped");
        reset_result
    }

    fn position_handleable_by_sleep(&self) -> bool {
        self.current_position != 0
            && self.current_position < self.timeout_sequence.len()
            && !self.initial_position_dirty
    }

    async fn change_position_and_notify(&mut self, change: PositionChange) -> Result<()> {
        // This method may seem needlessly complicated - why can't we just send
        // the result to actor and if it's successful, change the position and
        // time?
        //
        // In single-threaded runtimes, like the one used in tests, the task
        // would not necessarily run further after calling ActorPort#request,
        // meaning the position and time wouldn't get updated and the tests
        // would not be able to test GetRunningTime functionality. Also, since
        // the tests use tokio's time shifting functionality, they cannot use
        // a multi-threaded runtime.
        let original_position = self.current_position;
        let message_for_actor = match change {
            PositionChange::Increment => {
                self.current_position += 1;
                SystemState::Idle
            }
            PositionChange::Reset => {
                self.current_position = 0;
                SystemState::Awakened
            }
        };
        assert!(self.current_position <= self.timeout_sequence.len());
        self.position_changed_at = Instant::now();

        if let Err(e) = self.child_port.request(message_for_actor).await {
            self.current_position = original_position;
            self.position_changed_at = Instant::now();
            Err(anyhow::Error::new(e))
        } else {
            log::debug!(
                "Changing position {} to {} (internally handled = {})",
                original_position,
                self.current_position,
                self.position_handleable_by_sleep(),
            );
            Ok(())
        }
    }

    fn get_running_time(&self) -> Duration {
        if self.current_position == 0 {
            return Duration::ZERO;
        }
        let step_times: u64 = self.timeout_sequence[0..self.current_position].iter().sum();
        log::debug!(
            "Step time sum: {}, additionally elapsed: {:?}",
            step_times,
            self.position_changed_at.elapsed()
        );
        Duration::from_secs(step_times).saturating_add(self.position_changed_at.elapsed())
    }

    async fn force_activity(&mut self) {
        log::debug!("Recovering from actor error by forcing display server to be active");
        if let Err(e) = self.controller.force_activity() {
            log::error!(
                "Couldn't force activity on display server, effects will be stopped until next awake-idle cycle: {}",
            e);
        }
        log::debug!("Waiting for display server to become active again...");
        loop {
            if let Err(e) = self.state_channel.changed().await {
                log::error!("Couldn't await idleness channel change, effects will be stopped until next awake-idle cycle: {}", e);
                return;
            }
            if *self.state_channel.borrow_and_update() == SystemState::Awakened {
                return;
            } else {
                log::warn!("Unexpected Idle state while waiting for display server to reactivate after downstream actor error.");
            }
        }
    }

    fn is_terminating_error(e: anyhow::Error) -> bool {
        if e.downcast_ref::<PortDropped>().is_some() {
            log::debug!("Port dropped - terminating actor.");
            return true;
        }
        match e.downcast_ref::<armaf::ActorRequestError<anyhow::Error>>() {
            Some(are) => match are {
                armaf::ActorRequestError::Actor(actor_error) => {
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
