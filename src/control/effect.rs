use crate::armaf;
use logind_zbus::manager::InhibitType;

#[derive(Clone, Copy, Debug)]
pub enum RollbackStrategy {
    OnActivity,
    Immediate,
}

#[derive(Debug)]
pub struct Effect {
    pub effect_name: String,
    pub inhibited_by: Vec<InhibitType>,
    pub causes_inhibitions: Vec<InhibitType>,
    pub recipient: armaf::EffectorPort,
    pub rollback_strategy: RollbackStrategy,
}
