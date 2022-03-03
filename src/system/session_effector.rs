use crate::armaf::{Actor, EffectorMessage};
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::{self, session::SessionProxy};
use std::process;

pub enum SessionEffect {
    IdleHint,
    LockedHint,
}

pub struct SessionEffector {
    connection: zbus::Connection,
    session_proxy: Option<SessionProxy<'static>>,
}

impl SessionEffector {
    pub fn new(connection: zbus::Connection) -> SessionEffector {
        SessionEffector {
            connection,
            session_proxy: None,
        }
    }
}

#[async_trait]
impl Actor<EffectorMessage<SessionEffect>, ()> for SessionEffector {
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

    async fn handle_message(&mut self, payload: EffectorMessage<SessionEffect>) -> Result<()> {
        let (effect, argument) = match payload {
            EffectorMessage::Execute(a) => (a, true),
            EffectorMessage::Rollback(a) => (a, false),
        };
        match effect {
            SessionEffect::IdleHint => {
                log::info!("Setting idle hint in logind to {}", argument);
                // TODO: It seems like sometimes the changes are not immediately
                // visible to reading methods. Should we maybe try to wait until
                // they change?
                Ok(self
                    .session_proxy
                    .as_ref()
                    .unwrap()
                    .set_idle_hint(argument)
                    .await?)
            }
            SessionEffect::LockedHint => {
                log::info!("Setting locked hint in logind to {}", argument);
                // TODO: It seems like sometimes the changes are not immediately
                // visible to reading methods. Should we maybe try to wait until
                // they change?
                Ok(self
                    .session_proxy
                    .as_ref()
                    .unwrap()
                    .set_locked_hint(argument)
                    .await?)
            }
        }
    }
}
