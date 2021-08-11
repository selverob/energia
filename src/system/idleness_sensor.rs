use actix::prelude::*;
use log::info;

// #[derive(Message)]
// #[rtype(result = "()")]
// pub struct SetSubscriber(Recipient<NewState>);

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "()")]
pub enum IdlenessState {
    Idle,
    Active,
}

pub struct IdlenessSensor {
    subscriber: Option<Recipient<IdlenessState>>,
}

impl IdlenessSensor {
    pub fn new() -> Self {
        IdlenessSensor { subscriber: None }
    }
}

impl Actor for IdlenessSensor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("IdlenessSensor started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("IdlenessSensor stopped");
    }
}
