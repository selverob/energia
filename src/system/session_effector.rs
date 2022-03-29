use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server as ds,
    },
};
use anyhow::Result;
use async_trait::async_trait;
use log;
use logind_zbus::{self, manager::InhibitType, session::SessionProxy};
use std::process;

pub struct SessionEffector;

#[async_trait]
impl Effector for SessionEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![
            Effect::new(
                "idle_hint".to_owned(),
                vec![InhibitType::Idle],
                RollbackStrategy::OnActivity,
            ),
            Effect::new(
                "locked_hint".to_owned(),
                vec![InhibitType::Idle],
                RollbackStrategy::Immediate,
            ),
        ]
    }

    async fn spawn<B: BrightnessController, D: ds::DisplayServer>(
        &self,
        _: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort> {
        let actor = SessionEffectorActor::new(provider.get_dbus_system_connection().await?);
        spawn_server(actor).await
    }
}

pub struct SessionEffectorActor {
    connection: zbus::Connection,
    session_proxy: Option<SessionProxy<'static>>,
}

impl SessionEffectorActor {
    pub fn new(connection: zbus::Connection) -> SessionEffectorActor {
        SessionEffectorActor {
            connection,
            session_proxy: None,
        }
    }

    fn get_session_proxy(&self) -> &SessionProxy<'static> {
        self.session_proxy.as_ref().unwrap()
    }
}

#[async_trait]
impl Server<EffectorMessage, usize> for SessionEffectorActor {
    fn get_name(&self) -> String {
        "SessionEffector".to_owned()
    }

    async fn initialize(&mut self) -> Result<()> {
        let manager_proxy = logind_zbus::manager::ManagerProxy::new(&self.connection).await?;
        let path = manager_proxy.get_session_by_PID(process::id()).await?;
        self.session_proxy = Some(
            SessionProxy::builder(&self.connection)
                .path(path)?
                .build()
                .await?,
        );

        Ok(())
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<usize> {
        match payload {
            EffectorMessage::Execute => {
                log::debug!("Setting idle hint to true");
                self.get_session_proxy().set_idle_hint(true).await?;
                Ok(1)
            }
            EffectorMessage::Rollback => {
                log::debug!("Setting idle hint to false");
                self.get_session_proxy().set_idle_hint(false).await?;
                Ok(0)
            }
            EffectorMessage::CurrentlyAppliedEffects => {
                if self.get_session_proxy().idle_hint().await? {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
        }
    }
}
