use crate::armaf::{Actor, EffectorMessage};
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::{self, session::SessionProxy};
use std::process;

pub enum LogindEffect {
    IdleHint,
    LockedHint,
}

pub struct LogindEffector {
    connection: zbus::Connection,
    session_proxy: Option<SessionProxy<'static>>,
}

impl LogindEffector {
    pub fn new(connection: zbus::Connection) -> LogindEffector {
        LogindEffector {
            connection,
            session_proxy: None,
        }
    }
}

#[async_trait]
impl Actor<EffectorMessage<LogindEffect>, ()> for LogindEffector {
    fn get_name(&self) -> String {
        "LogindEffector".to_owned()
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

    async fn handle_message(&mut self, payload: EffectorMessage<LogindEffect>) -> Result<()> {
        let (effect, argument) = match payload {
            EffectorMessage::Execute(a) => (a, true),
            EffectorMessage::Rollback(a) => (a, false),
        };
        match effect {
            LogindEffect::IdleHint => {
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
            LogindEffect::LockedHint => {
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
