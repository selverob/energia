//! Centralized storage of effector's ActorPorts and lazy spawning of effectors
//!
//! This module is a hack for working around Effector trait not being object-safe
//! and it not being possible to make it object safe with the current
//! architecture.

use crate::{
    armaf::{Effect, Effector, EffectorPort, Server},
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server::DisplayServer,
    },
    system,
};
use anyhow::Result;
use std::collections::HashMap;

/// Get a vector of the names of all known effectors
pub fn get_known_effector_names() -> Vec<&'static str> {
    vec!["brightness", "dpms", "session", "sleep", "lock"]
}

/// Get effects provided by the named effector
pub fn get_effects_for_effector(effector_name: &str) -> Vec<Effect> {
    match effector_name {
        "brightness" => system::brightness_effector::BrightnessEffector.get_effects(),
        "dpms" => system::dpms_effector::DPMSEffector.get_effects(),
        "session" => system::session_effector::SessionEffector.get_effects(),
        "sleep" => system::sleep_effector::SleepEffector.get_effects(),
        "lock" => system::lock_effector::LockEffector.get_effects(),
        _ => unreachable!(),
    }
}

/// Resolve the correct effector according to the name passed in the message and
/// get its [EffectorPort].
///
/// If the effector has not yet been spawned by the receiving EffectorInventory,
/// it gets spawned.
pub struct GetEffectorPort(pub String);

/// An actor providing centralized storage of effector ports and name resolution
/// for them
pub struct EffectorInventory<B: BrightnessController, D: DisplayServer> {
    config: toml::Value,
    running_effectors: HashMap<String, EffectorPort>,
    dependency_provider: DependencyProvider<B, D>,
}

impl<B: BrightnessController, D: DisplayServer> EffectorInventory<B, D> {
    /// Create a new EffectorInventory
    pub fn new(
        config: toml::Value,
        dependency_provider: DependencyProvider<B, D>,
    ) -> EffectorInventory<B, D> {
        EffectorInventory {
            config,
            running_effectors: HashMap::new(),
            dependency_provider,
        }
    }
}

#[async_trait::async_trait]
impl<B: BrightnessController, D: DisplayServer> Server<GetEffectorPort, EffectorPort>
    for EffectorInventory<B, D>
{
    fn get_name(&self) -> String {
        "EffectorInventory".to_string()
    }

    async fn handle_message(&mut self, payload: GetEffectorPort) -> Result<EffectorPort> {
        let GetEffectorPort(ref effector_name) = payload;
        if self.running_effectors.contains_key(effector_name) {
            return Ok(self.running_effectors[effector_name].clone());
        }
        let config = self.config.get(effector_name);
        let port = spawn_effector(effector_name, &mut self.dependency_provider, config).await?;
        self.running_effectors.insert(payload.0, port.clone());
        Ok(port)
    }

    async fn tear_down(&mut self) -> Result<()> {
        for (effector, port) in self.running_effectors.drain() {
            log::info!("Terminating {}", effector);
            port.await_shutdown().await;
        }
        Ok(())
    }
}

pub async fn spawn_effector<B: BrightnessController, D: DisplayServer>(
    effector_name: &str,
    dependency_provider: &mut DependencyProvider<B, D>,
    config: Option<&toml::Value>,
) -> Result<EffectorPort> {
    let config_clone = config.cloned();
    match effector_name {
        "brightness" => {
            system::brightness_effector::BrightnessEffector
                .spawn(config_clone, dependency_provider)
                .await
        }
        "dpms" => {
            system::dpms_effector::DPMSEffector
                .spawn(config_clone, dependency_provider)
                .await
        }
        "session" => {
            system::session_effector::SessionEffector
                .spawn(config_clone, dependency_provider)
                .await
        }
        "sleep" => {
            system::sleep_effector::SleepEffector
                .spawn(config_clone, dependency_provider)
                .await
        }
        "lock" => {
            system::lock_effector::LockEffector
                .spawn(config_clone, dependency_provider)
                .await
        }
        _ => Err(anyhow::anyhow!("unknown effector")),
    }
}

pub fn resolve_effectors_for_effects() -> HashMap<String, (String, usize)> {
    let mut m = HashMap::new();
    for effector_name in get_known_effector_names().iter() {
        for (i, effect) in get_effects_for_effector(effector_name).iter().enumerate() {
            log::trace!(
                "Resolved effect {} to effector {}",
                effect.name,
                effector_name
            );
            m.insert(effect.name.to_string(), (effector_name.to_string(), i));
        }
    }
    m
}
