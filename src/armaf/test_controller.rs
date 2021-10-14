use std::time::Duration;
use super::test_sensor::Increment;

use tokio::{select, time};
use crate::armaf::{self, ActorPort};
use log::info;


pub fn spawn(period: Duration, sensor: armaf::ActorPort<Increment, usize, ()>) -> armaf::ActorPort<(), (), ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        let interval = time::interval(period);
        tokio::pin!(interval);
        loop {
            select! {
                _ = interval.tick() => {
                    info!("Polling sensor");
                    let result = sensor.request(Increment).await;
                    info!("Controller got {:?}", result);
                }
                req = rx.recv() => {
                    info!("Controller quitting");
                    if req.is_some() {
                        req.unwrap().respond(Ok(()));
                    }
                    break;
                }
            }
        }
    });
    port
}
