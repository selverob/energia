use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::dependency_provider::DependencyProvider,
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;
use serde::Deserialize;
use tokio::process::{Child, Command};

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
        _: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort>
    where
        B: crate::external::brightness::BrightnessController,
        D: crate::external::display_server::DisplayServer,
    {
        if config.is_none() {
            bail!("When lock is in schedule, [lock] section must be provided in config");
        }
        let command_strings = config.unwrap().try_into()?;
        let actor = LockEffectorActor::new(command_strings);
        spawn_server(actor).await
    }
}

pub struct LockEffectorActor {
    command: CommandStrings,
    child: Option<Child>,
}

impl LockEffectorActor {
    pub fn new(command: CommandStrings) -> LockEffectorActor {
        LockEffectorActor {
            command,
            child: None,
        }
    }

    fn update_child_status(&mut self) {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Err(e) => {
                    log::error!(
                        "Couldn't check the locker status, will assume it's dead: {}",
                        e
                    );
                    self.child = None
                }
                Ok(Some(status)) => {
                    log::debug!("Locker exited with status {}", status);
                    self.child = None
                }
                Ok(None) => {}
            }
        }
    }
}

#[async_trait]
impl Server<EffectorMessage, usize> for LockEffectorActor {
    fn get_name(&self) -> String {
        "LockEffector".to_string()
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<usize> {
        self.update_child_status();
        let is_locked = self.child.is_some();
        match payload {
            EffectorMessage::Execute => {
                if is_locked {
                    bail!("System is already locked");
                }
                self.child = Some(
                    Command::new(&self.command.command)
                        .args(&self.command.args)
                        .spawn()?,
                );
                Ok(1)
            }
            EffectorMessage::Rollback => {
                if is_locked {
                    self.child.take().unwrap().wait().await?;
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
