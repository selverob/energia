use super::{
    effector_inventory::{self as ei, GetEffectorPort},
    idleness_controller::{Action, IdlenessController},
};
use crate::{
    armaf::{spawn_server, ActorPort, Effect, EffectorPort, Handle, HandleChild},
    control::{
        idleness_controller::ReconciliationBunches,
        sequencer::{GetRunningTime, Sequencer},
    },
    external::display_server::{DisplayServerController, SystemState},
    system::{inhibition_sensor::GetInhibitions, upower_sensor::PowerStatus},
};
use anyhow::{anyhow, Context, Result};
use logind_zbus::manager::Inhibitor;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::watch;

#[derive(Clone, Debug, Error)]
#[error("{0} is not a valid configuration name for a schedule")]
struct TryFromScheduleTypeError(String);

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum ScheduleType {
    ExternalPower,
    Battery,
    LowBattery,
}

impl TryFrom<&str> for ScheduleType {
    type Error = TryFromScheduleTypeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "external" => Ok(ScheduleType::ExternalPower),
            "battery" => Ok(ScheduleType::Battery),
            "low_battery" => Ok(ScheduleType::LowBattery),
            unknown => Err(TryFromScheduleTypeError(unknown.to_owned())),
        }
    }
}

type Schedule = HashMap<String, Duration>;

fn parse_schedules(config: &toml::Value) -> Result<HashMap<ScheduleType, Schedule>> {
    let mut schedules = HashMap::new();

    let empty_placeholder = toml::Value::Table(toml::value::Map::new());
    let schedule_tables = config
        .get("schedule")
        .unwrap_or(&empty_placeholder)
        .as_table()
        .unwrap_or(empty_placeholder.as_table().unwrap());

    for key in schedule_tables.keys() {
        let schedule_type: Result<ScheduleType, TryFromScheduleTypeError> = key.as_str().try_into();
        match schedule_type {
            Err(e) => log::error!("Problem when parsing a schedule: {}", e),
            Ok(typ) => {
                let schedule = parse_schedule(&schedule_tables[key])?;
                schedules.insert(typ, schedule);
            }
        }
    }

    Ok(schedules)
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

type Sequence = Vec<(Duration, Vec<Action>)>;

pub struct EnvironmentController<D: DisplayServerController> {
    config: toml::Value,
    sequences: HashMap<ScheduleType, Sequence>,
    effector_inventory: ActorPort<GetEffectorPort, EffectorPort, anyhow::Error>,
    inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
    ds_controller: D,
    idleness_channel: watch::Receiver<SystemState>,
    handle_child: Option<HandleChild>,
    power_status_receiver: watch::Receiver<PowerStatus>,
    low_power_treshold: Option<u64>,
}

impl<D: DisplayServerController> EnvironmentController<D> {
    pub fn new(
        config: &toml::Value,
        effector_inventory: ActorPort<GetEffectorPort, EffectorPort, anyhow::Error>,
        inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibitor>, anyhow::Error>,
        ds_controller: D,
        idleness_channel: watch::Receiver<SystemState>,
        power_status_receiver: watch::Receiver<PowerStatus>,
    ) -> EnvironmentController<D> {
        EnvironmentController {
            config: config.clone(),
            sequences: HashMap::new(),
            effector_inventory,
            inhibition_sensor,
            ds_controller,
            idleness_channel,
            handle_child: None,
            power_status_receiver,
            low_power_treshold: None,
        }
    }

    pub async fn spawn(mut self) -> Result<Handle> {
        let session_effector_port = self.get_effector("session").await?;
        let schedules = parse_schedules(&self.config)?;
        if schedules.is_empty() {
            return Err(anyhow!(
                "No schedule defined. Define either schedule.external or schedule.battery."
            ));
        }
        let effect_names_mapping = ei::resolve_effectors_for_effects();
        let mut sequences = HashMap::new();
        for (source, schedule) in schedules {
            sequences.insert(
                source,
                self.sequence_for_schedule(
                    &schedule,
                    &effect_names_mapping,
                    &session_effector_port,
                )
                .await?,
            );
        }
        self.sequences = sequences;
        self.get_low_power_treshold();
        let (handle, receiver) = Handle::new();
        self.handle_child = Some(receiver);
        tokio::spawn(async move {
            if let Err(e) = self.main_loop().await {
                log::error!("Error in environment controller: {}", e);
            }
        });
        Ok(handle)
    }

    fn get_low_power_treshold(&mut self) {
        let config_result = self
            .config
            .get("battery")
            .ok_or("no battery table defined")
            .and_then(|table| {
                table
                    .get("low_battery_percentage")
                    .ok_or("low_battery_percentage key is not defined")
            })
            .and_then(|value| {
                value
                    .as_integer()
                    .ok_or("battery.low_battery_percentage is not an integer")
            });
        let low_power_schedule_defined = self.sequences.contains_key(&ScheduleType::LowBattery);
        match config_result {
            Ok(treshold) => self.low_power_treshold = Some(treshold as u64),
            Err(e) if low_power_schedule_defined => {
                log::error!("Low power schedule is defined but {} in configuration. Schedule will never be used.", e);
            }
            _ => {}
        }
    }

    async fn main_loop(&mut self) -> Result<()> {
        let power_status = *self.power_status_receiver.borrow_and_update();
        let mut schedule_type = self.power_status_to_schedule_type(power_status);
        log::info!("Will use schedule for {:?}", schedule_type);
        let mut sequence = self.sequence_for_schedule_type(schedule_type);
        let mut reconciliation_context = ReconciliationContext::empty();
        loop {
            // New actors' initialization
            let (durations, actions) = sequence.clone().into_iter().unzip();

            let idleness_controller = IdlenessController::new(
                actions,
                reconciliation_context.starting_bunch,
                reconciliation_context.reconciliation_bunches,
                self.inhibition_sensor.clone(),
            );
            let sequencer = Sequencer::new(
                spawn_server(idleness_controller).await?,
                self.ds_controller.clone(),
                self.idleness_channel.clone(),
                &durations_to_timeouts(&durations),
                reconciliation_context.starting_bunch,
                reconciliation_context.initial_sleep_shorten,
            );
            let sequencer_port = sequencer.spawn().await?;

            // Waiting for termination or schedule change
            loop {
                tokio::select! {
                    _ = self.handle_child.as_mut().unwrap().should_terminate() => {
                        log::info!("Handle dropped, terminating");
                        sequencer_port.await_shutdown().await;
                        return Ok(());
                    }
                    _ = self.power_status_receiver.changed() => {
                        let power_status = *self.power_status_receiver.borrow_and_update();
                        let new_schedule_type = self.power_status_to_schedule_type(power_status);
                        if new_schedule_type != schedule_type {
                            schedule_type = new_schedule_type;
                            break;
                        }
                    }
                }
            }

            // Generating the reconciliation context and shutting down old actors
            log::info!("Will use schedule for {:?}", schedule_type);
            let running_time = match sequencer_port.request(GetRunningTime).await {
                Ok(time) => time,
                Err(e) => {
                    log::error!("Couldn't get running time from sequencer, assuming system is awakened: {:?}", e);
                    Duration::ZERO
                }
            };
            sequencer_port.await_shutdown().await;
            let new_sequence = self.sequence_for_schedule_type(schedule_type);
            reconciliation_context =
                ReconciliationContext::calculate(&sequence, &new_sequence, running_time);
            log::debug!("Reconciliation context is {:?}", reconciliation_context);
            sequence = new_sequence;
        }
    }

    fn power_status_to_schedule_type(&self, status: PowerStatus) -> ScheduleType {
        match (status, self.low_power_treshold) {
            (PowerStatus::External, _) => ScheduleType::ExternalPower,
            (PowerStatus::Battery(_), None) => ScheduleType::Battery,
            (PowerStatus::Battery(percentage), Some(treshold)) => {
                if percentage > treshold {
                    ScheduleType::Battery
                } else {
                    ScheduleType::LowBattery
                }
            }
        }
    }

    fn sequence_for_schedule_type(&self, typ: ScheduleType) -> Sequence {
        if self.sequences.contains_key(&typ) {
            return self.sequences[&typ].clone();
        }
        log::warn!(
            "Schedule of type {:?} is not defined, using a fallback schedule.",
            typ
        );
        let schedule_substitutions = vec![
            (ScheduleType::LowBattery, ScheduleType::Battery),
            (ScheduleType::Battery, ScheduleType::ExternalPower),
        ];
        for (original_type, substitution_type) in schedule_substitutions.iter() {
            if typ == *original_type && self.sequences.contains_key(substitution_type) {
                return self.sequences[substitution_type].clone();
            }
        }

        self.sequences.iter().next().unwrap().1.clone()
    }

    async fn sequence_for_schedule(
        &mut self,
        schedule: &Schedule,
        effect_names_mapping: &HashMap<String, (String, usize)>,
        session_effector: &EffectorPort,
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
        action_bunches[0]
            .1
            .push(self.idle_hint_action(session_effector.clone()));
        Ok(action_bunches)
    }

    async fn bunch_to_actions(
        &mut self,
        bunch: &Vec<Effect>,
        effect_names_mapping: &HashMap<String, (String, usize)>,
    ) -> Result<Vec<Action>> {
        let mut actions = Vec::new();
        for effect in bunch.iter() {
            // Not checking for effect validity here, that's done on schedule parsing
            let effector_name = effect_names_mapping.get(&effect.name).unwrap().0.as_ref();
            actions.push(Action::new(
                effect.clone(),
                self.get_effector(effector_name).await?,
            ));
        }
        Ok(actions)
    }

    fn idle_hint_action(&self, session_effector: EffectorPort) -> Action {
        Action::new(
            ei::get_effects_for_effector("session")[0].clone(),
            session_effector,
        )
    }

    async fn get_effector(&self, name: &str) -> Result<EffectorPort> {
        Ok(self
            .effector_inventory
            .request(GetEffectorPort(name.to_string()))
            .await?)
    }
}

#[derive(Debug)]
struct ReconciliationContext {
    pub starting_bunch: usize,
    pub initial_sleep_shorten: Duration,
    pub reconciliation_bunches: ReconciliationBunches,
}

impl ReconciliationContext {
    pub fn empty() -> ReconciliationContext {
        Self::new(
            0,
            Duration::ZERO,
            ReconciliationBunches::new(None, None, HashSet::new()),
        )
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
        let (provisional_starting_bunch, provisional_sleep_shorten) =
            Self::passed_bunch_count(new_sequence, running_time);
        // If the system is already idle, we don't want it to wake up on power source change
        let (new_starting_bunch, sleep_shorten) =
            if executed_old_bunches == 1 && provisional_starting_bunch == 0 {
                (1, Duration::ZERO)
            } else {
                (provisional_starting_bunch, provisional_sleep_shorten)
            };
        let executed_actions: Vec<&Action> = old_sequence[0..executed_old_bunches]
            .iter()
            .flat_map(|bunch| &bunch.1)
            .collect();
        let missed_actions: Vec<&Action> = new_sequence[0..new_starting_bunch]
            .iter()
            .flat_map(|bunch| &bunch.1)
            .collect();
        let future_actions: Vec<&Action> = new_sequence[new_starting_bunch..new_sequence.len()]
            .iter()
            .flat_map(|bunch| &bunch.1)
            .collect();
        let reconciliation_bunches =
            Self::reconciliation_bunches(executed_actions, missed_actions, future_actions);
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
        missed_actions: Vec<&Action>,
        future_actions: Vec<&Action>,
    ) -> ReconciliationBunches {
        let old_effect_names = Self::effect_names_from_actions(&executed_actions);
        let new_effect_names = Self::effect_names_from_actions(&missed_actions);
        let old_set: HashSet<String> = HashSet::from_iter(old_effect_names);
        let new_set: HashSet<String> = HashSet::from_iter(new_effect_names);

        let effect_names_to_execute: HashSet<&String> = new_set.difference(&old_set).collect();
        let actions_to_execute: Vec<Action> = missed_actions
            .iter()
            .filter_map(|action| {
                if effect_names_to_execute.contains(&action.effect.name) {
                    Some((*action).clone())
                } else {
                    None
                }
            })
            .collect();

        // We need to rollback everything that the old controller executed,
        // since the idleness controller doesn't initialize its rollback stack
        // by itself.
        let ports_to_rollback: Vec<EffectorPort> = executed_actions
            .iter()
            .map(|action| action.recipient.clone())
            .collect();

        let execute = if !actions_to_execute.is_empty() {
            Some(actions_to_execute)
        } else {
            None
        };

        let rollback = if !ports_to_rollback.is_empty() {
            Some(ports_to_rollback)
        } else {
            None
        };

        ReconciliationBunches::new(
            execute,
            rollback,
            Self::skip_effects(executed_actions, future_actions),
        )
    }

    fn skip_effects(
        executed_actions: Vec<&Action>,
        future_actions: Vec<&Action>,
    ) -> HashSet<String> {
        let executed_set: HashSet<String> =
            HashSet::from_iter(Self::effect_names_from_actions(&executed_actions));
        let future_set: HashSet<String> =
            HashSet::from_iter(Self::effect_names_from_actions(&future_actions));

        executed_set
            .intersection(&future_set)
            .map(|s| s.to_string())
            .collect()
    }

    fn effect_names_from_actions(actions: &[&Action]) -> Vec<String> {
        actions
            .iter()
            .map(|action| action.effect.name.clone())
            .collect()
    }
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
    use crate::armaf::RollbackStrategy;

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

    fn empty_action(bunch: usize, effect: usize) -> Action {
        let (message_sender, _) = tokio::sync::mpsc::channel(1);
        let (_, shutdown_notifier) = tokio::sync::watch::channel(());
        Action::new(
            Effect::new(
                format!("{}-{}", bunch, effect),
                vec![],
                RollbackStrategy::OnActivity,
            ),
            crate::armaf::ActorPort::new(message_sender, shutdown_notifier),
        )
    }

    fn make_sequence(description: &Vec<(Duration, usize)>) -> Sequence {
        let mut sequence = Vec::new();
        for (bunch_index, (timeout, action_count)) in description.iter().enumerate() {
            let bunch = (0..*action_count)
                .map(|i| empty_action(bunch_index, i))
                .collect();
            sequence.push((*timeout, bunch));
        }
        sequence
    }

    fn action_names(actions: Vec<Action>) -> Vec<String> {
        actions
            .into_iter()
            .map(|action| action.effect.name)
            .collect()
    }

    #[test]
    fn test_reconciliation_at_start() {
        let seq1 = make_sequence(&vec![
            (Duration::from_secs(30), 3),
            (Duration::from_secs(30), 2),
        ]);
        let seq2 = make_sequence(&vec![
            (Duration::from_secs(40), 2),
            (Duration::from_secs(10), 5),
        ]);
        let context = ReconciliationContext::calculate(&seq1, &seq2, Duration::ZERO);
        assert_eq!(context.initial_sleep_shorten, Duration::ZERO);
        assert_eq!(context.starting_bunch, 0);
        assert!(context.reconciliation_bunches.execute.is_none());
        assert!(context.reconciliation_bunches.rollback.is_none());
        assert_eq!(context.reconciliation_bunches.skip_effects.len(), 0);
    }

    #[test]
    fn test_reconciliation_rollback() {
        let seq1 = make_sequence(&vec![
            (Duration::from_secs(30), 3),
            (Duration::from_secs(30), 2),
        ]);
        let seq2 = make_sequence(&vec![
            (Duration::from_secs(40), 2),
            (Duration::from_secs(10), 5),
        ]);
        let context = ReconciliationContext::calculate(&seq1, &seq2, Duration::from_secs(45));
        assert_eq!(context.initial_sleep_shorten, Duration::from_secs(5));
        assert_eq!(context.starting_bunch, 1);
        assert!(context.reconciliation_bunches.execute.is_none());
        assert_eq!(context.reconciliation_bunches.rollback.unwrap().len(), 3);
        assert_eq!(context.reconciliation_bunches.skip_effects.len(), 0);
    }

    #[test]
    fn test_reconciliation_basic() {
        let seq1 = make_sequence(&vec![
            (Duration::from_secs(30), 3),
            (Duration::from_secs(30), 3),
            (Duration::from_secs(30), 2),
        ]);
        let seq2 = make_sequence(&vec![
            (Duration::from_secs(40), 5),
            (Duration::from_secs(60), 5),
        ]);
        let context = ReconciliationContext::calculate(&seq1, &seq2, Duration::from_secs(65));
        assert_eq!(context.initial_sleep_shorten, Duration::from_secs(25));
        assert_eq!(context.starting_bunch, 1);
        assert_eq!(
            action_names(context.reconciliation_bunches.execute.unwrap()),
            vec!["0-3", "0-4"]
        );
        assert_eq!(context.reconciliation_bunches.rollback.unwrap().len(), 6);
        assert_eq!(
            context.reconciliation_bunches.skip_effects,
            HashSet::from(["1-0".to_owned(), "1-1".to_owned(), "1-2".to_owned()])
        );
    }

    #[test]
    fn test_reconciliation_stays_in_idle() {
        let seq1 = make_sequence(&vec![
            (Duration::from_secs(10), 3),
            (Duration::from_secs(20), 3),
        ]);
        let seq2 = make_sequence(&vec![
            (Duration::from_secs(20), 5),
            (Duration::from_secs(40), 5),
        ]);
        let context = ReconciliationContext::calculate(&seq1, &seq2, Duration::from_secs(15));
        assert_eq!(context.initial_sleep_shorten, Duration::ZERO);
        assert_eq!(context.starting_bunch, 1);
        assert_eq!(
            action_names(context.reconciliation_bunches.execute.unwrap()),
            vec!["0-3", "0-4"]
        );
        assert_eq!(context.reconciliation_bunches.rollback.unwrap().len(), 3);
        assert_eq!(context.reconciliation_bunches.skip_effects.len(), 0);
    }
}
