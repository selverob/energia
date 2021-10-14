use std::time::Duration;

use tokio::{time};
use crate::armaf::{self, ActorPort};

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
