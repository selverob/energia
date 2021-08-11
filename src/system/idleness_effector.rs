use super::messages::*;
use actix::prelude::*;
use anyhow::Result;
use log;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "anyhow::Result<()>")]
pub struct SetTimeout(usize);

pub struct IdlenessEffector;

impl IdlenessEffector {
    pub fn new() -> Self {
        IdlenessEffector {}
    }
}

impl Actor for IdlenessEffector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("IdlenessEffector started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        log::info!("IdlenessEffector stopped");
    }
}

impl Handler<SetTimeout> for IdlenessEffector {
    type Result = Result<()>;

    fn handle(&mut self, msg: SetTimeout, _ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("Setting timeout to {:?}", msg.0);
        Ok(())
    }
}

impl Handler<Stop> for IdlenessEffector {
    type Result = Result<()>;

    fn handle(&mut self, _msg: Stop, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}
