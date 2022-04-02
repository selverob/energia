use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::dependency_provider::DependencyProvider,
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use logind_zbus::{manager::InhibitType, session::SessionProxy};
use serde::Deserialize;
use tokio::{
    process::Command,
    sync::oneshot::{self, error::TryRecvError},
};

#[derive(Debug, Clone, Deserialize)]
pub struct CommandStrings {
    command: String,
    args: Vec<String>,
}

pub struct LockEffector;

#[async_trait]
impl Effector for LockEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![Effect::new(
            "lock".to_owned(),
            vec![InhibitType::Idle],
            RollbackStrategy::None,
        )]
    }

    async fn spawn<B, D>(
        &self,
        config: Option<toml::Value>,
        dp: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort>
    where
        B: crate::external::brightness::BrightnessController,
        D: crate::external::display_server::DisplayServer,
    {
        if config.is_none() {
            bail!("When lock is in schedule, [lock] section must be provided in config");
        }
        let command_strings = config.unwrap().try_into()?;
        let actor = LockEffectorActor::new(command_strings, dp.get_dbus_system_connection().await?);
        spawn_server(actor).await
    }
}

pub struct LockEffectorActor {
    command: CommandStrings,
    status_receiver: Option<oneshot::Receiver<Result<()>>>,
    connection: zbus::Connection,
    session_proxy: Option<SessionProxy<'static>>,
}

impl LockEffectorActor {
    pub fn new(command: CommandStrings, system_connection: zbus::Connection) -> LockEffectorActor {
        LockEffectorActor {
            command,
            status_receiver: None,
            connection: system_connection,
            session_proxy: None,
        }
    }

    fn update_child_status(&mut self) {
        if let Some(receiver) = self.status_receiver.as_mut() {
            match receiver.try_recv() {
                Ok(value) => {
                    if let Err(e) = value {
                        log::error!("Error occurred in locker watch task: {}", e);
                    }
                    self.status_receiver = None
                }
                Err(TryRecvError::Closed) => {
                    log::error!("Locker watch task died.");
                    self.status_receiver = None
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn spawn_locker(&mut self) {
        let (sender, receiver) = oneshot::channel();
        self.status_receiver = Some(receiver);
        let sent_command = self.command.clone();
        let sent_proxy = self.session_proxy.as_ref().unwrap().clone();
        tokio::spawn(async move {
            let spawn_res = Command::new(sent_command.command)
                .args(sent_command.args)
                .spawn();
            match spawn_res {
                Err(e) => {
                    let _ = sender.send(Err(anyhow::Error::new(e)));
                    return;
                }
                Ok(mut process) => {
                    if let Err(e) = sent_proxy.set_locked_hint(true).await {
                        log::error!("Failed to set locked hint on the session: {}", e);
                    }
                    let res = process.wait().await;
                    log::debug!("Locker has quit");
                    if let Err(e) = sent_proxy.set_locked_hint(false).await {
                        log::error!("Failed to unset locked hint on the session: {}", e);
                    }
                    let _ = sender.send(res.map(|_| ()).map_err(|e| anyhow::Error::new(e)));
                }
            }
        });
    }
}

#[async_trait]
impl Server<EffectorMessage, usize> for LockEffectorActor {
    fn get_name(&self) -> String {
        "LockEffector".to_string()
    }

    async fn initialize(&mut self) -> Result<()> {
        let manager_proxy = logind_zbus::manager::ManagerProxy::new(&self.connection).await?;
        let path = manager_proxy.get_session_by_PID(std::process::id()).await?;
        self.session_proxy = Some(
            SessionProxy::builder(&self.connection)
                .path(path)?
                .build()
                .await?,
        );
        Ok(())
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<usize> {
        self.update_child_status();
        let is_locked = self.status_receiver.is_some();
        match payload {
            EffectorMessage::Execute => {
                if is_locked {
                    bail!("System is already locked");
                }
                self.spawn_locker();
                Ok(1)
            }
            EffectorMessage::Rollback => {
                if is_locked {
                    self.status_receiver.take().unwrap().await??;
                }
                Ok(0)
            }
            EffectorMessage::CurrentlyAppliedEffects => {
                if is_locked {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
        }
    }
}
