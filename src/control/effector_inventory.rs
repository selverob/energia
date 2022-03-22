use crate::{
    armaf::{Effect, Effector, EffectorPort},
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server::DisplayServer,
    },
    system,
};
use anyhow::Result;
/// This module is a hack working around Effector trait not being object-safe
/// and it not being possible to make it object safe with the current
/// architecture.
use std::collections::HashMap;

pub fn get_known_effector_names() -> Vec<&'static str> {
    vec!["display", "session", "sleep"]
}

pub fn get_effects_for_effector(effector_name: &str) -> Vec<Effect> {
    match effector_name {
        "display" => system::display_effector::DisplayEffector.get_effects(),
        "session" => system::session_effector::SessionEffector.get_effects(),
        "sleep" => system::sleep_effector::SleepEffector.get_effects(),
        _ => unreachable!(),
    }
}

pub async fn spawn_effector<B: BrightnessController, D: DisplayServer>(
    effector_name: &str,
    dependency_provider: &mut DependencyProvider<B, D>,
    config: Option<&toml::Value>,
) -> Result<EffectorPort> {
    let config_clone = config.map(|c| c.clone());
    match effector_name {
        "display" => {
            system::display_effector::DisplayEffector
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
        _ => unreachable!(),
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
