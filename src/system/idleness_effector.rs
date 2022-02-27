use crate::armaf::{Actor, EffectorMessage};
use crate::external::display_server::DisplayServerController;
use anyhow::Result;
use async_trait::async_trait;
use log;

pub struct IdlenessEffector<C: DisplayServerController> {
    controller: C,
    initial_timeout: i16,
}

impl<C: DisplayServerController> IdlenessEffector<C> {
    pub fn new(controller: C) -> IdlenessEffector<C> {
        IdlenessEffector {
            controller,
            initial_timeout: -1,
        }
    }

    async fn get_current_timeout(&self) -> Result<i16> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.get_idleness_timeout()).await?
    }

    async fn set_timeout(&self, timeout: i16) -> Result<()> {
        let sent_controller = self.controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.set_idleness_timeout(timeout)).await?
    }
}

#[async_trait]
impl<C: DisplayServerController> Actor<EffectorMessage<i16>, ()> for IdlenessEffector<C> {
    fn get_name(&self) -> String {
        "IdlenessEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage<i16>) -> Result<()> {
        let timeout_to_set = match payload {
            EffectorMessage::Execute(timeout) => timeout,
            EffectorMessage::Rollback(_) => self.initial_timeout,
        };
        Ok(self.set_timeout(timeout_to_set).await?)
    }

    async fn initialize(&mut self) -> Result<()> {
        self.initial_timeout = match self.get_current_timeout().await {
            Ok(initial_timeout) => initial_timeout,
            Err(err) => {
                log::error!("Failed getting initial timeout, setting it to -1: {}", err);
                -1
            }
        };
        Ok(())
    }

    async fn tear_down(&mut self) -> Result<()> {
        Ok(self.set_timeout(self.initial_timeout).await?)
    }
}
