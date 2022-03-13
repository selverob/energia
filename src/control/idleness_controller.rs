use crate::{
    armaf::{ActorPort, Effect, EffectorMessage, EffectorPort, RollbackStrategy, Server},
    external::display_server::SystemState,
    system::inhibition_sensor::GetInhibitions,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::{InhibitType, Inhibitor, Mode};

pub struct Action {
    effect: Effect,
    recipient: EffectorPort,
}

impl Action {
    pub fn new(effect: Effect, recipient: EffectorPort) -> Action {
        Action { effect, recipient }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Stop;

pub struct IdlenessController {
    action_bunches: Vec<Vec<Action>>,
    current_bunch: usize,
    rollback_stack: Vec<EffectorPort>,

    inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
}

impl IdlenessController {
    pub fn new(
        action_bunches: Vec<Vec<Action>>,
        inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
    ) -> IdlenessController {
        IdlenessController {
            action_bunches,
            current_bunch: 0,
            inhibition_sensor,
            rollback_stack: Vec::new(),
        }
    }

    async fn handle_idleness(&mut self) -> Result<()> {
        if self.is_current_bunch_inhibited().await {
            return Err(anyhow!("Upcoming bunch is inhibited"));
        }

        let mut immediate_rollback_ports: Vec<EffectorPort> = Vec::new();
        for action in &self.action_bunches[self.current_bunch] {
            log::debug!("Applying effect {}", action.effect.name);
            if let Err(e) = action.recipient.request(EffectorMessage::Execute).await {
                log::error!("Failed to apply effect {}: {:?}", action.effect.name, e);
                continue;
            }
            match action.effect.rollback_strategy {
                RollbackStrategy::OnActivity => self.rollback_stack.push(action.recipient.clone()),
                RollbackStrategy::Immediate => {
                    immediate_rollback_ports.push(action.recipient.clone())
                }
            }
        }

        rollback_all(&mut immediate_rollback_ports).await;

        self.current_bunch += 1;
        Ok(())
    }

    async fn get_inhibitors(&mut self) -> Vec<Inhibitor> {
        let inhibitors = match self.inhibition_sensor.request(GetInhibitions).await {
            Ok(i) => i,
            Err(e) => {
                log::error!(
                    "Couldn't get inhibitions, will continue as if none exist: {:?}",
                    e
                );
                Vec::new()
            }
        };

        // Delay inhibitors are handled automatically by systemd
        inhibitors
            .into_iter()
            .filter(|i| i.mode() == Mode::Block)
            .collect()
    }

    async fn is_current_bunch_inhibited(&mut self) -> bool {
        let inhibitors = self.get_inhibitors().await;
        let upcoming_inhibition_types: Vec<InhibitType> = dedup_inhibit_types(
            &self.action_bunches[self.current_bunch]
                .iter()
                .flat_map(|e| e.effect.inhibited_by.clone())
                .collect(),
        );

        let mut is_inhibited = false;

        for t in upcoming_inhibition_types {
            for i in find_inhibitors_with_type(&inhibitors, t) {
                is_inhibited = true;
                log::info!(
                    "Not moving to next idleness level, {:?} inhibited by {} with reason {}",
                    t,
                    i.who(),
                    i.why(),
                );
            }
        }
        is_inhibited
    }

    async fn handle_wakeup(&mut self) -> Result<()> {
        log::info!("System awakened, rolling back all effects");
        rollback_all(&mut self.rollback_stack).await;
        self.current_bunch = 0;
        Ok(())
    }
}

#[async_trait]
impl Server<SystemState, ()> for IdlenessController {
    fn get_name(&self) -> String {
        "IdlenessController".to_owned()
    }

    async fn handle_message(&mut self, system_state: SystemState) -> Result<()> {
        match system_state {
            SystemState::Awakened => self.handle_wakeup().await?,
            SystemState::Idle => self.handle_idleness().await?,
        }
        Ok(())
    }
}

fn find_inhibitors_with_type(
    inhibitors: &Vec<Inhibitor>,
    inhibit_type: InhibitType,
) -> Vec<&Inhibitor> {
    let mut found = Vec::new();
    for inhibitor in inhibitors {
        if inhibitor.what().types().contains(&inhibit_type) {
            found.push(inhibitor);
        }
    }
    found
}

fn dedup_inhibit_types(duplicated: &Vec<InhibitType>) -> Vec<InhibitType> {
    let mut deduped = Vec::new();
    for t in duplicated {
        if !deduped.contains(t) {
            deduped.push(*t);
        }
    }
    deduped
}

async fn rollback_all(rollback_vec: &mut Vec<EffectorPort>) {
    while let Some(port) = rollback_vec.pop() {
        if let Err(e) = port.request(EffectorMessage::Rollback).await {
            log::error!("Error on rollback: {:?}", e);
        }
    }
}
