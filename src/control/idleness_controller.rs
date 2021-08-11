use super::effect::Effect;
use crate::system::{
    idleness_effector,
    idleness_sensor::{self, IdlenessState},
    inhibition_sensor,
    messages::{Rollback, Stop},
};
use actix::prelude::*;
use log;
use std::collections::VecDeque;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "()")]
struct Reset {}

enum State {
    Waiting,
    ProcessingEffect,
}

pub struct IdlenessController {
    effects: Vec<Effect>,
    idleness_sensor: Option<Addr<idleness_sensor::IdlenessSensor>>,
    idleness_effector: Option<Addr<idleness_effector::IdlenessEffector>>,
    inhibition_sensor: Option<Addr<inhibition_sensor::InhibitionSensor>>,
    effect_queue: VecDeque<Effect>,
    rollback_stack: Vec<Recipient<Rollback>>,
}

impl IdlenessController {
    pub fn new(effects: Vec<Effect>) -> IdlenessController {
        IdlenessController {
            effects,
            idleness_sensor: None,
            idleness_effector: None,
            inhibition_sensor: None,
            effect_queue: VecDeque::new(),
            rollback_stack: vec![],
        }
    }

    fn reinitialize_effect_queue(&mut self) {
        self.effect_queue = self.effects.iter().cloned().collect();
    }
}

impl Actor for IdlenessController {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("IdlenessController starting");
        self.idleness_sensor = Some(idleness_sensor::IdlenessSensor::new().start());
        self.idleness_effector = Some(idleness_effector::IdlenessEffector.start());
        self.inhibition_sensor = Some(inhibition_sensor::InhibitionSensor.start());
        log::info!("IdlenessController started");
        ctx.notify(Reset {});
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("IdlenessController stopped");
    }
}

impl Handler<Reset> for IdlenessController {
    type Result = ();
    fn handle(&mut self, _msg: Reset, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Resetting IdlenessController");
        while let Some(effector) = self.rollback_stack.pop() {
            match effector.do_send(Rollback) {
                Err(send_error) => {
                    log::error!("Error when sending rollback message: {:?}", send_error)
                }
                Ok(()) => (),
            }
        }
        self.reinitialize_effect_queue()
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

impl Handler<IdlenessState> for IdlenessController {
    type Result = ();
    fn handle(&mut self, msg: IdlenessState, _ctx: &mut Context<Self>) -> Self::Result {
        ()
    }
}
