use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server as ds,
    },
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::{InhibitType, ManagerProxy, PrepareForSleepStream};
use std::time::Duration;
use tokio_stream::StreamExt;

pub struct SleepEffector;

#[async_trait]
impl Effector for SleepEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![Effect::new(
            "sleep".to_owned(),
            vec![InhibitType::Sleep],
            RollbackStrategy::Immediate,
        )]
    }

    async fn spawn<B: BrightnessController, D: ds::DisplayServer>(
        self,
        _: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort> {
        let actor = SleepEffectorActor::new(provider.get_dbus_system_connection().await?);
        spawn_server(actor).await
    }
}

pub struct SleepEffectorActor {
    connection: zbus::Connection,
    manager_proxy: Option<ManagerProxy<'static>>,
    sleep_signal_stream: Option<PrepareForSleepStream<'static>>,
}

impl SleepEffectorActor {
    pub fn new(connection: zbus::Connection) -> SleepEffectorActor {
        SleepEffectorActor {
            connection,
            manager_proxy: None,
            sleep_signal_stream: None,
        }
    }
}

#[async_trait]
impl Server<EffectorMessage, ()> for SleepEffectorActor {
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
