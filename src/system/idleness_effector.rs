use crate::armaf::{Actor, EffectorMessage};
use crate::external::display_server::DisplayServerController;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log;

pub struct IdlenessEffector<C: DisplayServerController> {
    timeout_sequence: Vec<i16>,
    current_position: usize,
    controller: C,
}

impl<C: DisplayServerController> IdlenessEffector<C> {
    pub fn new(controller: C, timeout_sequence: &Vec<i16>) -> IdlenessEffector<C> {
        IdlenessEffector {
            timeout_sequence: timeout_sequence.clone(),
            current_position: 0,
            controller,
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

    async fn move_in_sequence_by(&mut self, increment: isize) -> Result<()> {
        let new_position = self.current_position as isize + increment;
        if new_position > self.timeout_sequence.len() as isize - 1 || new_position < 0 {
            return Err(anyhow!(
                "IdlenessController message would cause timeout sequence under/overflow"
            ));
        }
        self.set_timeout(self.timeout_sequence[new_position as usize])
            .await?;
        self.current_position = new_position as usize;
        Ok(())
    }
}

#[async_trait]
impl<C: DisplayServerController> Actor<EffectorMessage, ()> for IdlenessEffector<C> {
    fn get_name(&self) -> String {
        "IdlenessEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<()> {
        match payload {
            EffectorMessage::Execute => self.move_in_sequence_by(1).await,
            EffectorMessage::Rollback => self.move_in_sequence_by(-1).await,
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        let initial_timeout = match self.get_current_timeout().await {
            Ok(initial_timeout) => initial_timeout,
            Err(err) => {
                log::error!("Failed getting initial timeout, setting it to -1: {}", err);
                -1
            }
        };
        let mut actual_timeout_sequence = vec![initial_timeout];
        actual_timeout_sequence.extend(self.timeout_sequence.drain(..));
        self.timeout_sequence = actual_timeout_sequence;
        Ok(())
    }

    async fn tear_down(&mut self) -> Result<()> {
        Ok(self.set_timeout(self.timeout_sequence[0]).await?)
    }
}
