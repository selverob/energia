use actix::prelude::*;
use log::info;

pub struct Inhibition;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "Vec<Inhibition>")]
pub struct GetInhibitions;

pub struct InhibitionSensor;

impl Actor for InhibitionSensor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("InhibitionSensor started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        info!("InhibitionSensor stopped");
    }
}
