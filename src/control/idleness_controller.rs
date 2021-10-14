use super::effect::Effect;
use crate::armaf::{ActorPort, EffectorPort, EffectorMessage, Request};
use crate::system::{
    idleness_effector::{self, SetTimeout},
    idleness_sensor::{self, IdlenessState},
    inhibition_sensor::{self, GetInhibitions, Inhibition},
};
use log;
use std::collections::VecDeque;
use tokio::sync::mpsc;

enum State {
    Waiting,
    ProcessingEffect,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Stop;

pub struct IdlenessController {
    effects: Vec<Effect>,
    idleness_rx: mpsc::Receiver<IdlenessState>,
    
    idleness_effector: ActorPort<SetTimeout, (), ()>,
    inhibition_sensor: ActorPort<GetInhibitions, Vec<Inhibition>, ()>,
    effect_queue: VecDeque<Effect>,
    rollback_stack: Vec<EffectorPort>,
    stop_receiver: mpsc::Receiver<Request<Stop, (), ()>>,
}

pub fn spawn(effects: Vec<Effect>) -> ActorPort<Stop, (), ()> {
    log::debug!("Configuring new IdlenessController");
    let (idleness_tx, idleness_rx) = mpsc::channel(8);
    let idleness_effector = idleness_effector::spawn();
    let inhibition_sensor = inhibition_sensor::spawn();
    idleness_sensor::spawn(idleness_tx);
    
    let (port, rx) = ActorPort::make();
    let mut controller = IdlenessController {
        effects,
        idleness_rx,
        idleness_effector,
        inhibition_sensor,
        effect_queue: VecDeque::new(),
        rollback_stack: vec![],
        stop_receiver: rx
    };

    tokio::spawn(async move {
        controller.spawn().await;
    });

    port
}

impl IdlenessController {
    fn reinitialize_effect_queue(&mut self) {
        self.effect_queue = self.effects.iter().cloned().collect();
    }

    async fn spawn(&mut self) {
        log::info!("IdlenessController started");

        loop {
            tokio::select! {
                idleness_state = self.idleness_rx.recv() => {
                    log::debug!("Got new idleness state: {:?}", idleness_state);
                }
                _ = self.stop_receiver.recv() => {
                    log::info!("IdlenessController stopping");
                }
            }
        }
    }

    async fn reset(&mut self) {
        log::info!("Resetting IdlenessController");
        while let Some(effector) = self.rollback_stack.pop() {
            effector.request(EffectorMessage::Rollback).await;
        }
        self.reinitialize_effect_queue();
        let timeout_in_seconds = self.effects[0].effect_timeout.as_secs();
        self.idleness_effector.request(idleness_effector::SetTimeout(timeout_in_seconds)).await;
    }
}
