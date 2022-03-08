use crate::armaf::{Server, EffectorMessage};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log;
use logind_zbus::{self, session::SessionProxy};
use std::process;

#[derive(Debug, Clone, Copy)]
pub enum SessionState {
    Active,
    IdleHinted,
    LockedHinted,
}

pub struct SessionEffector {
    session_state: SessionState,
    connection: zbus::Connection,
    session_proxy: Option<SessionProxy<'static>>,
}

impl SessionEffector {
    pub fn new(connection: zbus::Connection) -> SessionEffector {
        SessionEffector {
            session_state: SessionState::Active,
            connection,
            session_proxy: None,
        }
    }

    fn get_session_proxy(&self) -> &SessionProxy<'static> {
        self.session_proxy.as_ref().unwrap()
    }
}

#[async_trait]
impl Server<EffectorMessage, ()> for SessionEffector {
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

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<()> {
        match (self.session_state, payload) {
            (SessionState::Active, EffectorMessage::Execute) => {
                log::debug!("Setting idle hint to true");
                self.get_session_proxy().set_idle_hint(true).await?;
                self.session_state = SessionState::IdleHinted;
            }
            (SessionState::Active, EffectorMessage::Rollback) => {
                return Err(anyhow!("Unmatched Rollback called on SessionEffector"));
            }
            (SessionState::IdleHinted, EffectorMessage::Execute) => {
                log::debug!("Setting locked hint to true");
                self.get_session_proxy().set_locked_hint(true).await?;
                self.session_state = SessionState::LockedHinted;
            }
            (SessionState::IdleHinted, EffectorMessage::Rollback) => {
                log::debug!("Setting idle hint to false");
                self.get_session_proxy().set_idle_hint(false).await?;
                self.session_state = SessionState::Active;
            }
            (SessionState::LockedHinted, EffectorMessage::Execute) => {
                return Err(anyhow!("Too many Execute messages sent to SessionEffector"));
            }
            (SessionState::LockedHinted, EffectorMessage::Rollback) => {
                log::debug!("Setting locked hint to false");
                self.get_session_proxy().set_locked_hint(false).await?;
                self.session_state = SessionState::IdleHinted;
            }
        }
        Ok(())
    }
}
