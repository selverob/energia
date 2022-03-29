//! Type definitions for implementation of Effectors.

use super::ActorPort;
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
    CurrentlyAppliedEffects,
}

pub type EffectorPort = ActorPort<EffectorMessage, usize, anyhow::Error>;

#[derive(Clone, Copy, Debug)]
pub enum RollbackStrategy {
    OnActivity,
    Immediate,
    None
}

#[derive(Debug, Clone)]
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
pub trait Effector: Send + Sync + 'static {
    // The Self: Sized constraints on each method are to make this trait object-safe,
    // since storing effectors as trait objects is its basic rationale

    fn get_effects(&self) -> Vec<Effect>
    where
        Self: Sized;

    async fn spawn<B: BrightnessController, D: DisplayServer>(
        &self,
        config: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort>
    where
        Self: Sized;
}
