use std::time::{Duration};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::{ManagerProxy, PrepareForSleepStream};
use tokio_stream::StreamExt;

use crate::armaf::{Actor, EffectorMessage};

pub struct SleepEffector {
    connection: zbus::Connection,
    manager_proxy: Option<ManagerProxy<'static>>,
    sleep_signal_stream: Option<PrepareForSleepStream<'static>>,
}

impl SleepEffector {
    pub fn new(connection: zbus::Connection) -> SleepEffector {
        SleepEffector {
            connection,
            manager_proxy: None,
            sleep_signal_stream: None,
        }
    }
}

#[async_trait]
impl Actor<EffectorMessage, ()> for SleepEffector {
    fn get_name(&self) -> String {
        "SleepEffector".to_owned()
    }

    async fn initialize(&mut self) -> Result<()> {
        let manager_proxy = logind_zbus::manager::ManagerProxy::new(&self.connection).await?;
        self.sleep_signal_stream = Some(manager_proxy.receive_prepare_for_sleep().await?);
        self.manager_proxy = Some(manager_proxy);
        Ok(())
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<()> {
        match payload {
            EffectorMessage::Execute => {
                log::info!("Putting system to sleep");
                self.manager_proxy.as_ref().unwrap().suspend(false).await?;
            }
            EffectorMessage::Rollback => {
                loop {
                    let stream_val = self.sleep_signal_stream.as_mut().unwrap().next().await;
                    match stream_val {
                        None => return Err(anyhow!("Wakeup notification stream exhausted. Rollback called without suspending computer first?")),
                        Some(signal) => {
                            // The stream may still contain notifications about going to sleep (start = true)
                            // we want to see if we have woken up from sleep.
                            if !signal.args()?.start {
                                // The signal is sent as the computer is preparing to go to sleep (maybe?)
                                // We want it to actually go to sleep, thus the wait.
                                tokio::time::sleep(Duration::from_millis(1000)).await;
                                return Ok(());
                            } else {
                                log::debug!("Dropping PrepareForSleep (start=true) signal");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
