use crate::system::messages;
use actix::Recipient;
use std::time::Duration;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Inhibition {
    Idle,
    Sleep,
    Shutdown,
}

#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub struct Effect {
    effect_name: String,
    effect_timeout: Duration, // The time which should pass from previous effect
    inhibited_by: Vec<Inhibition>,
    execute_recipient: Recipient<messages::Execute>,
    rollback_recipient: Recipient<messages::Rollback>,
}
