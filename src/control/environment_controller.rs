use super::{
    effector_inventory as ei,
    idleness_controller::{Action, IdlenessController},
};
use crate::{
    armaf::{spawn_server, Effect, EffectorPort, Handle, HandleChild},
    control::{
        idleness_controller::ReconciliationBunches,
        sequencer::{GetRunningTime, Sequencer},
    },
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server::DisplayServer,
    },
    system::{inhibition_sensor::InhibitionSensor, upower_sensor::PowerSource},
};
use anyhow::{anyhow, Context, Result};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::sync::watch;

type Schedule = HashMap<String, Duration>;
type Sequence = Vec<(Duration, Vec<Action>)>;

pub struct EnvironmentController<B: BrightnessController, D: DisplayServer> {
    config: toml::Value,
    sequences: HashMap<PowerSource, Sequence>,
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
    ) -> EnvironmentController<B, D> {
        EnvironmentController {
            config: config.clone(),
            sequences: HashMap::new(),
            spawned_effectors: HashMap::new(),
            dependency_provider,
            handle_child: None,
            power_source_receiver,
        }
    }

    pub async fn spawn(mut self) -> Result<Handle> {
        let schedules = Self::parse_schedules(&self.config)?;
        if schedules.len() == 0 {
            return Err(anyhow!(
                "No schedule defined. Define either schedule.external or schedule.battery."
            ));
        }
        let effect_names_mapping = ei::resolve_effectors_for_effects();
        let mut sequences = HashMap::new();
        for (source, schedule) in schedules {
            sequences.insert(
                source,
                self.sequence_for_schedule(&schedule, &effect_names_mapping)
                    .await?,
            );
        }
        self.sequences = sequences;
        let (handle, receiver) = Handle::new();
        self.handle_child = Some(receiver);
        tokio::spawn(async move {
            if let Err(e) = self.main_loop().await {
                log::error!("Error in environment controller: {}", e);
            }
            self.tear_down().await;
        });
        Ok(handle)
    }

    async fn main_loop(&mut self) -> Result<()> {
        let inhibition_sensor = spawn_server(InhibitionSensor::new(
            self.dependency_provider
                .get_dbus_system_connection()
                .await?,
        ))
        .await?;
        let power_source = *self.power_source_receiver.borrow_and_update();
        log::info!("New power source is {:?}", power_source);
        let mut sequence = self.sequence_for_power_source(power_source);
        let mut reconciliation_context = ReconciliationContext::empty();
        loop {
            let (durations, actions) = sequence.clone().into_iter().unzip();

            let idleness_controller = IdlenessController::new(
                actions,
                reconciliation_context.starting_bunch,
                reconciliation_context.reconciliation_bunches,
                inhibition_sensor.clone(),
            );
            let sequencer = Sequencer::new(
                spawn_server(idleness_controller).await?,
                self.dependency_provider.get_display_controller(),
                self.dependency_provider.get_idleness_channel(),
                &durations_to_timeouts(&durations),
                reconciliation_context.starting_bunch,
                reconciliation_context.initial_sleep_shorten,
            );
            let sequencer_port = sequencer.spawn().await?;
            tokio::select! {
                _ = self.handle_child.as_mut().unwrap().should_terminate() => {
                    log::info!("Handle dropped, terminating");
                    sequencer_port.await_shutdown().await;
                    return Ok(());
                }
                _ = self.power_source_receiver.changed() => {
                    let running_time = match sequencer_port.request(GetRunningTime).await {
                        Ok(time) => time,
                        Err(e) => {
                            log::error!("Couldn't get running time from sequencer, assuming system is awakened: {:?}", e);
                            Duration::ZERO
                        }
                    };
                    sequencer_port.await_shutdown().await;
                    let power_source = *self.power_source_receiver.borrow_and_update();
                    log::info!("New power source is {:?}", power_source);
                    let new_sequence = self.sequence_for_power_source(power_source);
                    reconciliation_context = ReconciliationContext::calculate(&sequence, &new_sequence, running_time);
                    sequence = new_sequence;
                }
            }
        }
    }

    fn sequence_for_power_source(&self, source: PowerSource) -> Sequence {
        if self.sequences.contains_key(&source) {
            self.sequences[&source].clone()
        } else {
            log::warn!(
                "Schedule for power source {:?} is not defined, using a fallback schedule.",
                source
            );
            self.sequences.iter().next().unwrap().1.clone()
        }
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

    async fn sequence_for_schedule(
        &mut self,
        schedule: &Schedule,
        effect_names_mapping: &HashMap<String, (String, usize)>,
    ) -> Result<Sequence> {
        let mut m: HashMap<Duration, Vec<Effect>> = HashMap::new();
        for (effect_name, delay) in schedule.iter() {
            let effect = if effect_names_mapping.contains_key(effect_name) {
                let mapping_result = &effect_names_mapping[effect_name];
                ei::get_effects_for_effector(&mapping_result.0)[mapping_result.1].clone()
            } else {
                return Err(anyhow!("Unknown effect name {}", effect_name));
            };
            m.entry(*delay).or_insert(vec![]).push(effect);
        }

        let mut action_bunches: Sequence = Vec::new();
        for (timeout, effects) in m.into_iter() {
            action_bunches.push((
                timeout,
                self.bunch_to_actions(&effects, effect_names_mapping)
                    .await?,
            ))
        }
        action_bunches.sort_by_key(|bunch| bunch.0);
        Ok(action_bunches)
    }

    async fn bunch_to_actions(
        &mut self,
        bunch: &Vec<Effect>,
        effect_names_mapping: &HashMap<String, (String, usize)>,
    ) -> Result<Vec<Action>> {
        let mut actions = Vec::new();
        for effect in bunch.into_iter() {
            // Not checking for effect validity here, that's done on schedule parsing
            let effector_name = effect_names_mapping.get(&effect.name).unwrap().0.clone();
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
        let port = ei::spawn_effector(effector_name, &mut self.dependency_provider, config).await?;
        self.spawned_effectors
            .insert(effector_name.to_string(), port);
        Ok(())
    }

    async fn tear_down(self) {
        for (name, port) in self.spawned_effectors.into_iter() {
            log::debug!("Shutting {} down", name);
            if let Err(e) = Self::terminate_effector(port).await {
                log::error!(
                    "Couldn't terminate effector {}, its effects may persist: {}",
                    name,
                    e
                );
            }
        }
    }

    async fn terminate_effector(port: EffectorPort) -> Result<()> {
        let running_effects = port
            .request(crate::armaf::EffectorMessage::CurrentlyAppliedEffects)
            .await
            .context("Couldn't get running effect count")?;
        for _ in 0..running_effects {
            port.request(crate::armaf::EffectorMessage::Rollback)
                .await
                .context("Couldn't roll back effect")?;
        }
        port.await_shutdown().await;
        Ok(())
    }
}

struct ReconciliationContext {
    pub starting_bunch: usize,
    pub initial_sleep_shorten: Duration,
    pub reconciliation_bunches: ReconciliationBunches,
}

impl ReconciliationContext {
    pub fn empty() -> ReconciliationContext {
        Self::new(0, Duration::ZERO, ReconciliationBunches::new(None, None))
    }

    pub fn new(
        starting_bunch: usize,
        initial_sleep_shorten: Duration,
        reconciliation_bunches: ReconciliationBunches,
    ) -> ReconciliationContext {
        ReconciliationContext {
            starting_bunch,
            initial_sleep_shorten,
            reconciliation_bunches,
        }
    }

    pub fn calculate(
        old_sequence: &Sequence,
        new_sequence: &Sequence,
        running_time: Duration,
    ) -> ReconciliationContext {
        if running_time.is_zero() {
            return Self::empty();
        }
        let (executed_old_bunches, _) = Self::passed_bunch_count(old_sequence, running_time);
        let (new_starting_bunch, sleep_shorten) =
            Self::passed_bunch_count(new_sequence, running_time);
        let executed_actions: Vec<&Action> = old_sequence[0..executed_old_bunches]
            .iter()
            .flat_map(|bunch| &bunch.1)
            .collect();
        let unexecuted_actions: Vec<&Action> = new_sequence[0..new_starting_bunch]
            .iter()
            .flat_map(|bunch| &bunch.1)
            .collect();
        let reconciliation_bunches =
            Self::reconciliation_bunches(executed_actions, unexecuted_actions);
        Self::new(new_starting_bunch, sleep_shorten, reconciliation_bunches)
    }

    fn passed_bunch_count(sequence: &Sequence, running_time: Duration) -> (usize, Duration) {
        let mut executed = 0;
        let mut countdown = running_time;
        for bunch in sequence {
            if countdown >= bunch.0 {
                executed += 1;
                countdown = countdown.saturating_sub(bunch.0);
            }
        }

        (executed, countdown)
    }

    fn reconciliation_bunches(
        executed_actions: Vec<&Action>,
        unexecuted_actions: Vec<&Action>,
    ) -> ReconciliationBunches {
        let old_effect_names = Self::effect_names_from_actions(&executed_actions);
        let new_effect_names = Self::effect_names_from_actions(&unexecuted_actions);
        let old_set: HashSet<String> = HashSet::from_iter(old_effect_names);
        let new_set: HashSet<String> = HashSet::from_iter(new_effect_names);

        let effect_names_to_execute: HashSet<&String> = new_set.difference(&old_set).collect();
        let actions_to_execute: Vec<Action> = unexecuted_actions
            .iter()
            .filter_map(|action| {
                if effect_names_to_execute.contains(&action.effect.name) {
                    Some((*action).clone())
                } else {
                    None
                }
            })
            .collect();

        let effect_names_to_rollback: HashSet<&String> = old_set.difference(&new_set).collect();
        let ports_to_rollback: Vec<EffectorPort> = executed_actions
            .iter()
            .filter_map(|action| {
                if effect_names_to_rollback.contains(&action.effect.name) {
                    Some(action.recipient.clone())
                } else {
                    None
                }
            })
            .collect();

        let execute = if actions_to_execute.len() > 0 {
            Some(actions_to_execute)
        } else {
            None
        };

        let rollback = if ports_to_rollback.len() > 0 {
            Some(ports_to_rollback)
        } else {
            None
        };

        ReconciliationBunches::new(execute, rollback)
    }

    fn effect_names_from_actions(actions: &Vec<&Action>) -> Vec<String> {
        actions
            .iter()
            .map(|action| action.effect.name.clone())
            .collect()
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
