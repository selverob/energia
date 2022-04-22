use std::time::Duration;

use crate::armaf::{Handle, HandleChild};
use anyhow::Result;
use logind_zbus::manager::{InhibitType, ManagerProxy, PrepareForSleepStream};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Copy)]
pub struct ReadyToSleep;

#[derive(Debug, Clone)]
pub enum SleepUpdate {
    GoingToSleep(mpsc::Sender<ReadyToSleep>),
    WokenUp,
}

#[derive(Debug, Error)]
#[non_exhaustive]
enum SleepSensorError {
    #[error("handle to sleep sensor closed")]
    HandleClosed,

    #[error("error on subscriber notification: {0}")]
    BroadcastFailed(#[from] broadcast::error::SendError<SleepUpdate>),

    #[error("downstream actors didn't confirm sleep preparation")]
    DownstreamTimeout,

    #[error("system is sleeping without preparing for sleep")]
    StateError,

    #[error("couldn't create sleep inhibitor: {0}")]
    InhibitorCreationError(#[from] zbus::Error),
}

pub struct SleepSensor {
    connection: zbus::Connection,
    sender: Option<broadcast::Sender<SleepUpdate>>,
    manager_proxy: Option<ManagerProxy<'static>>,
    handle: Option<HandleChild>,
    max_delay_time: Duration,
    sleep_signal_stream: Option<PrepareForSleepStream<'static>>,
}

impl SleepSensor {
    pub fn new(connection: zbus::Connection) -> SleepSensor {
        SleepSensor {
            connection,
            sender: None,
            manager_proxy: None,
            sleep_signal_stream: None,
            max_delay_time: Duration::ZERO,
            handle: None,
        }
    }

    pub async fn spawn(mut self) -> Result<(Handle, broadcast::Sender<SleepUpdate>)> {
        let (sender, _) = broadcast::channel(3);
        let returned_sender = sender.clone();
        let manager_proxy = logind_zbus::manager::ManagerProxy::new(&self.connection).await?;
        self.max_delay_time = Duration::from_micros(manager_proxy.inhibit_delay_max_USec().await?);
        let (handle, handle_child) = Handle::new();
        self.handle = Some(handle_child);
        self.sender = Some(sender);
        self.sleep_signal_stream = Some(manager_proxy.receive_prepare_for_sleep().await?);
        self.manager_proxy = Some(manager_proxy);
        tokio::spawn(async move {
            self.main_loop().await;
        });
        Ok((handle, returned_sender))
    }

    async fn main_loop(mut self) {
        loop {
            match self.wait_for_sleep().await {
                Ok(()) => {}
                Err(SleepSensorError::HandleClosed) => {
                    log::info!("Terminating SleepSensor");
                    return;
                }
                Err(SleepSensorError::StateError) => {
                    log::error!("{}", SleepSensorError::StateError);
                    continue;
                }
                Err(e) => {
                    log::error!("{}", e);
                }
            }
            match self.wait_for_wake_up().await {
                Ok(()) => {}
                Err(SleepSensorError::HandleClosed) => {
                    log::info!("Terminating SleepSensor");
                    return;
                }
                Err(e) => {
                    log::error!("{}", e);
                }
            }
        }
    }

    async fn set_up_delay_inhibitor(&mut self) -> zbus::Result<zbus::zvariant::OwnedFd> {
        log::debug!("Setting up delay inhibitor");
        self.manager_proxy
            .as_ref()
            .unwrap()
            .inhibit(
                InhibitType::Sleep,
                "Energia Power Manager",
                "Handle pre-sleep tasks",
                "delay",
            )
            .await
    }

    async fn wait_for_sleep(&mut self) -> Result<(), SleepSensorError> {
        // Once we finish, we don't need to delay sleep one way or another, so we're OK.
        let _delay_handle = self.set_up_delay_inhibitor().await?;
        tokio::select! {
            _ = self.handle.as_mut().unwrap().should_terminate() => Err(SleepSensorError::HandleClosed),
            Some(stream_value) = self.sleep_signal_stream.as_mut().unwrap().next() => {
                if !stream_value.args()?.start {
                    return Err(SleepSensorError::StateError)
                }
                log::info!("System is preparing to go to sleep, notifying actors");
                let subscriber_count = self.sender.as_ref().unwrap().receiver_count();
                let (confirmation_sender, confirmation_receiver) = mpsc::channel(subscriber_count);
                self.sender.as_ref().unwrap().send(SleepUpdate::GoingToSleep(confirmation_sender))?;
                self.wait_for_confirmations(confirmation_receiver, subscriber_count).await
            }
        }
    }

    async fn wait_for_confirmations(
        &mut self,
        mut receiver: mpsc::Receiver<ReadyToSleep>,
        expected_confirmations: usize,
    ) -> Result<(), SleepSensorError> {
        let mut received_confirmations = 0;
        let timeout = tokio::time::sleep(self.max_delay_time);
        tokio::pin!(timeout);
        while received_confirmations < expected_confirmations {
            tokio::select! {
                _ = &mut timeout => {
                    log::warn!("{} actors subscribed to sleep notifications did not respond to notification", expected_confirmations - received_confirmations);
                    return Err(SleepSensorError::DownstreamTimeout);
                }
                res = receiver.recv() => {
                    if res.is_none() {
                        // Channel is closed - all living subscribers responded
                        // and some of them were terminated between the call to
                        // receiver_count() and this method's execution - we can
                        // safely sleep.
                        return Ok(())
                    }
                    received_confirmations += 1;
                    log::debug!("{} out of {} confirmations about sleep readiness received", received_confirmations, expected_confirmations);
                }
                _ = self.handle.as_mut().unwrap().should_terminate() => return Err(SleepSensorError::HandleClosed),
            }
        }
        Ok(())
    }

    async fn wait_for_wake_up(&mut self) -> Result<(), SleepSensorError> {
        tokio::select! {
            stream_val = self.sleep_signal_stream.as_mut().unwrap().next() => {
                match stream_val {
                    None => Err(SleepSensorError::StateError),
                    Some(signal) => {
                        if !signal.args()?.start {
                            log::debug!("System is going to sleep NOW");
                            // The signal is sent as the computer is preparing to go to
                            // sleep We want it to actually go to sleep, thus the wait.
                            tokio::time::sleep(Duration::from_millis(1000)).await;
                            self.sender.as_ref().unwrap().send(SleepUpdate::WokenUp)?;
                            Ok(())
                        } else {
                            Err(SleepSensorError::StateError)
                        }
                    }
                }
            }
            _ = self.handle.as_mut().unwrap().should_terminate() => Err(SleepSensorError::HandleClosed),
        }
    }
}
