use std::time::Duration;

use crate::armaf::{self, ActorPort};
use tokio::time;

pub struct Increment;

pub fn spawn(timeout: Duration) -> armaf::ActorPort<Increment, usize, ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        let mut count = 0;
        while let Some(req) = rx.recv().await {
            time::sleep(timeout).await;
            req.respond(Ok(count)).unwrap();
            count += 1;
        }
    });
    port
}
