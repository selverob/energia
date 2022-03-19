use super::idleness_controller::{Action, IdlenessController};
use crate::{
    armaf::{spawn_server, Effect, Effector, EffectorPort, Handle, HandleChild},
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server::DisplayServer,
    },
    system::{
        self, inhibition_sensor::InhibitionSensor, sequencer::Sequencer, upower_sensor::PowerSource,
    },
};
use anyhow::{anyhow, Context, Result};
use std::{collections::HashMap, time::Duration};
use tokio::sync::watch;

type Schedule = HashMap<String, Duration>;

pub struct EnvironmentController<B: BrightnessController, D: DisplayServer> {
    config: toml::Value,
    schedules: HashMap<PowerSource, Schedule>,
    effect_names_mapping: HashMap<String, (String, usize)>,
    spawned_effectors: HashMap<String, EffectorPort>,
    dependency_provider: DependencyProvider<B, D>,
    handle_child: Option<HandleChild>,
    power_source_receiver: watch::Receiver<PowerSource>,
}

impl<B: BrightnessController, D: DisplayServer> EnvironmentController<B, D> {
    pub fn new(
        config: &toml::Value,
        dependency_provider: DependencyProvider<B, D>,
        power_source_receiver: watch::Receiver<PowerSource>,
    ) -> Result<EnvironmentController<B, D>> {
        let schedules = Self::parse_schedules(&config)?;
        if schedules.len() == 0 {
            return Err(anyhow!(
                "No schedule defined. Define either schedule.external or schedule.battery."
            ));
        }
        Ok(EnvironmentController {
            config: config.clone(),
            schedules,
            effect_names_mapping: Self::resolve_effectors_for_effects(),
            spawned_effectors: HashMap::new(),
            dependency_provider,
            handle_child: None,
            power_source_receiver,
        })
    }

    pub async fn spawn(mut self) -> Handle {
        let (handle, receiver) = Handle::new();
        log::trace!("{:?}", self.effect_names_mapping);
        self.handle_child = Some(receiver);
        tokio::spawn(async move {
            if let Err(e) = self.main_loop().await {
                log::error!("Error in environment controller: {}", e);
            }
        });
        handle
    }

    async fn main_loop(&mut self) -> Result<()> {
        let inhibition_sensor = spawn_server(InhibitionSensor::new(
            self.dependency_provider
                .get_dbus_system_connection()
                .await?,
        ))
        .await?;
        loop {
            let power_source = *self.power_source_receiver.borrow_and_update();
            log::info!("New power source is {:?}", power_source);
            let schedule = if self.schedules.contains_key(&power_source) {
                &self.schedules[&power_source]
            } else {
                log::warn!(
                    "Schedule for power source {:?} is not defined, using a fallback schedule.",
                    power_source
                );
                self.fallback_schedule()
            };

            let bunches_and_timeouts = self
                .bunches_and_timeouts_for_schedule(schedule)
                .expect("Couldn't launch all effectors");
            let (durations, effects) = bunches_and_timeouts.into_iter().unzip();
            let actions = self.effects_to_actions(&effects).await?;

            let idleness_controller = IdlenessController::new(actions, inhibition_sensor.clone());
            let sequencer = Sequencer::new(
                spawn_server(idleness_controller).await?,
                self.dependency_provider.get_display_controller(),
                self.dependency_provider.get_idleness_channel(),
                &durations_to_timeouts(&durations),
            );
            let sequencer_handle = sequencer.spawn().await?;
            tokio::select! {
                _ = self.handle_child.as_mut().unwrap().should_terminate() => {
                    sequencer_handle.await_shutdown().await;
                    log::info!("Handle dropped, terminating");
                    return Ok(());
                }
                _ = self.power_source_receiver.changed() => {
                    sequencer_handle.await_shutdown().await;
                }
            }
        }
    }

    fn fallback_schedule(&self) -> &Schedule {
        self.schedules.iter().next().unwrap().1
    }

    fn resolve_effectors_for_effects() -> HashMap<String, (String, usize)> {
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

    fn parse_schedules(config: &toml::Value) -> Result<HashMap<PowerSource, Schedule>> {
        let mut schedules = HashMap::new();

        let empty_placeholder = toml::Value::String(String::new());
        let schedule_dict = config.get("schedule").unwrap_or(&empty_placeholder);

        let possible_schedules = vec![
            ("battery", PowerSource::Battery),
            ("external", PowerSource::External),
        ];
        for (key, power_source) in possible_schedules {
            if let Some(schedule_config) = schedule_dict.get(key) {
                let schedule = Self::parse_schedule(schedule_config)?;
                schedules.insert(power_source, schedule);
            } else {
                log::debug!("No {} schedule found", key);
            };
        }
        Ok(schedules)
    }

    fn parse_schedule(schedule_config: &toml::Value) -> Result<Schedule> {
        let table = schedule_config
            .as_table()
            .ok_or(anyhow!("Schedule should be a table, not a scalar or array"))?;
        let mut m = HashMap::new();
        for (key, value) in table {
            if let Some(value_str) = value.as_str() {
                m.insert(key.to_string(), parse_duration(value_str)?);
            } else {
                return Err(anyhow!(
                    "timeout for {} is not a string in duration format",
                    key
                ));
            }
        }
        Ok(m)
    }

    fn bunches_and_timeouts_for_schedule(
        &self,
        schedule: &Schedule,
    ) -> Result<Vec<(Duration, Vec<Effect>)>> {
        let mut m: HashMap<Duration, Vec<Effect>> = HashMap::new();
        for (effect_name, delay) in schedule.iter() {
            let effect = if self.effect_names_mapping.contains_key(effect_name) {
                let mapping_result = &self.effect_names_mapping[effect_name];
                get_effects_for_effector(&mapping_result.0)[mapping_result.1].clone()
            } else {
                return Err(anyhow!("Unknown effect name {}", effect_name));
            };
            m.entry(*delay).or_insert(vec![]).push(effect);
        }

        let mut bunches: Vec<(Duration, Vec<Effect>)> = m.into_iter().collect();
        bunches.sort_by_key(|bunch| bunch.0);
        Ok(bunches)
    }

    async fn effects_to_actions(&mut self, bunches: &Vec<Vec<Effect>>) -> Result<Vec<Vec<Action>>> {
        let mut action_bunches = Vec::new();
        for bunch in bunches.iter() {
            action_bunches.push(self.bunch_to_action(bunch).await?);
        }
        Ok(action_bunches)
    }

    async fn bunch_to_action(&mut self, bunch: &Vec<Effect>) -> Result<Vec<Action>> {
        let mut actions = Vec::new();
        for effect in bunch.into_iter() {
            // Not checking for effect validity here, that's done on schedule parsing
            let effector_name = self
                .effect_names_mapping
                .get(&effect.name)
                .unwrap()
                .0
                .clone();
            if !self.spawned_effectors.contains_key(&effector_name) {
                self.spawn_effector_by_name(&effector_name).await?;
            }
            actions.push(Action::new(
                effect.clone(),
                self.spawned_effectors[&effector_name].clone(),
            ));
        }
        Ok(actions)
    }

    async fn spawn_effector_by_name(&mut self, effector_name: &str) -> Result<()> {
        let config = self.config.get(effector_name);
        let port = spawn_effector(effector_name, &mut self.dependency_provider, config).await?;
        self.spawned_effectors
            .insert(effector_name.to_string(), port);
        Ok(())
    }
}

// This whole section is a hack working around Effector trait not being
// object-safe and it not being possible to make it object safe with the current
// architecture.

fn get_known_effector_names() -> Vec<&'static str> {
    vec!["display", "session", "sleep"]
}

fn get_effects_for_effector(effector_name: &str) -> Vec<Effect> {
    match effector_name {
        "display" => system::display_effector::DisplayEffector.get_effects(),
        "session" => system::session_effector::SessionEffector.get_effects(),
        "sleep" => system::sleep_effector::SleepEffector.get_effects(),
        _ => unreachable!(),
    }
}

async fn spawn_effector<B: BrightnessController, D: DisplayServer>(
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

fn parse_duration(string: &str) -> Result<Duration> {
    let mut seconds = 0;
    for substr in string.split_ascii_whitespace() {
        seconds += match substr.chars().nth(substr.len() - 1) {
            Some('s') => parse_duration_numeric(substr)?,
            Some('m') => parse_duration_numeric(substr)? * 60,
            Some('h') => parse_duration_numeric(substr)? * 3600,
            Some(_) => {
                return Err(anyhow!(
                    "syntax error in duration: Duration compoment {} doesn't have a unit",
                    substr
                ))
            }
            None => {
                return Err(anyhow!(
                    "syntax error in duration: Duration compoment {} too short",
                    substr
                ))
            }
        }
    }

    Ok(Duration::from_secs(seconds))
}

fn parse_duration_numeric(component: &str) -> Result<u64> {
    component[0..component.len() - 1]
        .parse()
        .context("syntax error in duration: numeric component couldn't be parsed")
}

/// Convert a [Vec] of durations into a [Vec] of second timeouts, each one
/// representing the offset from the previous one.
///
/// ```
/// let durations = vec![Duration::from_secs(5), Duration::from_secs(30), Duration::from_secs(60), Duration::from_secs(3600)];
/// let timeouts = durations_to_timeouts(&durations);
/// assert_eq!(timeouts, vec![5, 25, 30, 3540]);
/// ```
fn durations_to_timeouts(durations: &Vec<Duration>) -> Vec<u64> {
    let mut timeouts = vec![durations[0].as_secs()];
    for (i, duration) in durations[1..].iter().enumerate() {
        timeouts.push(duration.saturating_sub(durations[i]).as_secs());
    }
    timeouts
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_duration_parsing() {
        assert_eq!(parse_duration("54s").unwrap(), Duration::from_secs(54));
        assert_eq!(parse_duration("32m").unwrap(), Duration::from_secs(32 * 60));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(3600 * 2));
        assert_eq!(parse_duration("2m 30s").unwrap(), Duration::from_secs(150));
        assert_eq!(parse_duration("1h 30s").unwrap(), Duration::from_secs(3630));
        assert_eq!(
            parse_duration("5m 1h").unwrap(),
            Duration::from_secs(65 * 60)
        );
        assert!(parse_duration("5m6h").is_err());
        assert!(parse_duration("5mh").is_err());
        assert!(parse_duration("5m 6d").is_err());
    }

    #[test]
    fn test_duration_to_timeout_conversion() {
        let durations = vec![
            Duration::from_secs(5),
            Duration::from_secs(30),
            Duration::from_millis(30050),
            Duration::from_secs(60),
            Duration::from_secs(3600),
        ];
        let timeouts = durations_to_timeouts(&durations);
        assert_eq!(timeouts, vec![5, 25, 0, 29, 3540]);
    }
}
