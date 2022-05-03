//! Type definitions for implementation of Effectors.

use super::ActorPort;
use crate::external::{
    brightness::BrightnessController, dependency_provider::DependencyProvider,
    display_server::DisplayServer,
};
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;

/// A common message type for controlling effectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage {
    /// Execute the next effect of the effector
    Execute,
    /// Roll back the last applied effect
    Rollback,
    /// Get the number of currently applied effects
    CurrentlyAppliedEffects,
}

/// The ActorPort used to control a runnning effector
///
/// When an effect is successfully executed or rolled back, the effector should
/// return the number of currently applied effects. Any error that occurs should
/// be wrapped in an [anyhow::Error].
pub type EffectorPort = ActorPort<EffectorMessage, usize, anyhow::Error>;

/// The way in which an effect should be rolled back
#[derive(Clone, Copy, Debug)]
pub enum RollbackStrategy {
    /// Roll the effect back after the user becomes active
    OnActivity,
    /// Roll the effect back immediately after all effects from its bunch have
    /// been executed
    Immediate,
    /// Do not roll back the effect
    None,
}

/// An action that an effector can perform
#[derive(Debug, Clone)]
pub struct Effect {
    /// Name of the effect, used for logging and in the configuration file
    pub name: String,
    /// Logind inhibitions which inhibit this effect. Almost all effects are inhibited at least by [InhibitType::Idle]
    pub inhibited_by: Vec<InhibitType>,
    /// The rollback strategy which a controler should apply to the effect
    pub rollback_strategy: RollbackStrategy,
}

impl Effect {
    /// Create a new Effect
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

/// A descriptor of an effector, allows getting the available effects and spawning the effector
#[async_trait]
pub trait Effector: Send + Sync + 'static {
    // The Self: Sized constraints on each method are to make this trait object-safe,
    // since storing effectors as trait objects is its basic rationale

    /// Get a list of effects the effector can provide, in the order they will be applied
    fn get_effects(&self) -> Vec<Effect>
    where
        Self: Sized;

    /// Parse the configuration of the effector, fetch its dependencies and
    /// spawn the Tokio task representing its actor
    async fn spawn<B: BrightnessController, D: DisplayServer>(
        &self,
        config: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort>
    where
        Self: Sized;
}
