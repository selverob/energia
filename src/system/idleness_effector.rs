use super::messages::*;
use actix::prelude::*;
use anyhow::Result;
use log::info;

#[derive(Message)]
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
        info!("IdlenessEffector started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("IdlenessEffector stopped");
    }
}

impl Handler<SetTimeout> for IdlenessEffector {
    type Result = Result<()>;

    fn handle(&mut self, _msg: SetTimeout, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}

impl Handler<Stop> for IdlenessEffector {
    type Result = Result<()>;

    fn handle(&mut self, _msg: Stop, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}
