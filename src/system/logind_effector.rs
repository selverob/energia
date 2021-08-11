use super::messages::*;
use actix::prelude::*;
use anyhow::Result;
use log::info;

pub struct LogindEffector;

impl Actor for LogindEffector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("LogindEffector started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("LogindEffector stopped");
    }
}

impl Handler<Execute> for LogindEffector {
    type Result = Result<()>;

    fn handle(&mut self, _msg: Execute, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}

impl Handler<Rollback> for LogindEffector {
    type Result = Result<()>;

    fn handle(&mut self, _msg: Rollback, _ctx: &mut Context<Self>) -> Self::Result {
        Ok(())
    }
}
