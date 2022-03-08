use crate::armaf;
use logind_zbus::manager::InhibitType;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum RollbackStrategy {
    ControllerInitiated(armaf::ActorPort<armaf::EffectorMessage, (), ()>),
    UserInitiated,
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub effect_name: String,
    pub effect_timeout: Duration, // The time which should pass from previous effect
    pub inhibited_by: Vec<InhibitType>,
    pub execute_recipient: armaf::ActorPort<armaf::EffectorMessage, (), ()>,
    pub rollback_recipient: RollbackStrategy,
}
