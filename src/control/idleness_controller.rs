use actix::prelude::*;
use super::effect::Effect;
use crate::system::{idleness_effector, idleness_sensor, inhibition_sensor, messages::Stop};
use log::{debug, info};

pub struct IdlenessController {
    effects: Vec<Effect>,
    idleness_sensor: Option<Addr<idleness_sensor::IdlenessSensor>>,
    idleness_effector: Option<Addr<idleness_effector::IdlenessEffector>>,
    inhibition_sensor: Option<Addr<inhibition_sensor::InhibitionSensor>>
}

impl IdlenessController {
    pub fn new() -> IdlenessController {
        IdlenessController {
            effects: vec![],
            idleness_sensor: None,
            idleness_effector: None,
            inhibition_sensor: None
        }
    }
}

impl Actor for IdlenessController {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("IdlenessController starting");
        self.idleness_sensor = Some(idleness_sensor::IdlenessSensor::new().start());
        self.idleness_effector = Some(idleness_effector::IdlenessEffector.start());
        self.inhibition_sensor = Some(inhibition_sensor::InhibitionSensor.start());
        info!("IdlenessController started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) { 
        info!("IdlenessController stopped");
    }
}

impl Handler<Stop> for IdlenessController {
    type Result = anyhow::Result<()>;
    fn handle(&mut self, _msg: Stop, _ctx: &mut Context<Self>) -> Self::Result {
        self.idleness_sensor = None;
        self.idleness_effector = None;
        self.inhibition_sensor = None;
        Ok(())
    }
}
