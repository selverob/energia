use std::cmp::Ordering;

use super::effect::Effect;
use crate::system::inhibition_sensor::GetInhibitions;
use crate::{
    armaf::{ActorPort, EffectorMessage, EffectorPort, Server},
    external::display_server::SystemState,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::{InhibitType, Inhibitor, Mode};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Stop;

pub struct IdlenessController {
    effect_bunches: Vec<Vec<Effect>>,
    current_bunch: usize,
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
        for effect in &self.effect_bunches[self.current_bunch] {
            log::debug!("Applying effect {}", effect.effect_name);
            if let Err(e) = effect.recipient.request(EffectorMessage::Execute).await {
                log::error!("Failed to apply effect {}: {:?}", effect.effect_name, e);
                continue;
            }
            match effect.rollback_strategy {
                crate::control::effect::RollbackStrategy::OnActivity => self.rollback_stack.push(effect.recipient.clone()),
                crate::control::effect::RollbackStrategy::Immediate => immediate_rollback_ports.push(effect.recipient.clone()),
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
            &self.effect_bunches[self.current_bunch]
                .iter()
                .flat_map(|e| e.causes_inhibitions.clone())
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
