use crate::armaf;
use logind_zbus::manager::InhibitType;

#[derive(Clone, Copy, Debug)]
pub enum RollbackStrategy {
    OnActivity,
    Immediate,
}

#[derive(Debug)]
pub struct Effect {
    pub name: String,
    pub inhibited_by: Vec<InhibitType>,
    pub recipient: armaf::EffectorPort,
    pub rollback_strategy: RollbackStrategy,
}

impl Effect {
    pub fn new(
        name: String,
        inhibited_by: Vec<InhibitType>,
        recipient: armaf::EffectorPort,
        rollback_strategy: RollbackStrategy,
    ) -> Effect {
        Effect {
            name,
            inhibited_by,
            recipient,
            rollback_strategy,
        }
    }
}
