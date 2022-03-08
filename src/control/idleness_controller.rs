use super::effect::Effect;
use crate::system::{inhibition_sensor::GetInhibitions, sequencer::Sequencer};
use crate::{
    armaf::{ActorPort, EffectorMessage, EffectorPort, Request, Server},
    external::display_server::SystemState,
};
use async_trait::async_trait;
use logind_zbus::manager::Inhibitor;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Stop;

pub struct IdlenessController {
    effect_bunches: Vec<Vec<Effect>>,
    rollback_stack: Vec<EffectorPort>,

    inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
}

impl IdlenessController {
    fn new(
        effect_bunches: Vec<Vec<Effect>>,
        inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
    ) -> IdlenessController {
        IdlenessController {
            effect_bunches,
            inhibition_sensor,
            rollback_stack: Vec::new(),
        }
    }

    async fn handle_idleness(&mut self) {
        unimplemented!();
    }

    async fn handle_wakeup(&mut self) {
        unimplemented!();
    }
}

#[async_trait]
impl Server<SystemState, ()> for IdlenessController {
    fn get_name(&self) -> String {
        "IdlenessController".to_owned()
    }

    async fn handle_message(&mut self, system_state: SystemState) -> anyhow::Result<()> {
        Ok(())
    }
}
