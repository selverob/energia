use std::time::Duration;
use actix::Recipient;
use crate::system::messages;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Inhibition {
    Idle,
    Sleep,
    Shutdown
}

#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub struct Effect {
    effect_name: String,
    effect_timeout: Duration,
    inhibited_by: Vec<Inhibition>,
    execute_recipient: Recipient<messages::Execute>,
    rollback_recipient: Recipient<messages::Rollback>,
}
