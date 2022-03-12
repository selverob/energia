//! Type definitions for implementation of Effectors.

use super::{ActorPort, Request};
use crate::external::{
    brightness::BrightnessController, dependency_provider::DependencyProvider,
    display_server::DisplayServer,
};
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage {
    Execute,
    Rollback,
}

pub type EffectorPort = ActorPort<EffectorMessage, (), anyhow::Error>;
pub type EffectorRequest = Request<EffectorMessage, (), anyhow::Error>;

#[derive(Clone, Copy, Debug)]
pub enum RollbackStrategy {
    OnActivity,
    Immediate,
}

#[derive(Debug)]
pub struct Effect {
    pub name: String,
    pub inhibited_by: Vec<InhibitType>,
    pub rollback_strategy: RollbackStrategy,
}

impl Effect {
    pub fn new(
        name: String,
        inhibited_by: Vec<InhibitType>,
        rollback_strategy: RollbackStrategy,
    ) -> Effect {
        Effect {
            name,
            inhibited_by,
            rollback_strategy,
        }
    }
}

#[async_trait]
pub trait Effector: Send {
    fn get_effects(&self) -> Vec<Effect>;
    async fn spawn<B: BrightnessController, D: DisplayServer>(
        self,
        config: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort>;
}
