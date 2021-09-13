use std::time::Duration;

use tokio::{sync::{oneshot, mpsc}, time};
use crate::armaf::{self, ActorPort};

pub struct Increment;

pub fn spawn(timeout: Duration) -> armaf::ActorPort<Increment, usize, ()> {
    let (tx, mut rx) = mpsc::channel::<armaf::Request<Increment, usize, ()>>(8);
    tokio::spawn(async move {
        let mut count = 0;
        while let Some(req) = rx.recv().await {
            time::sleep(timeout).await;
            req.respond(Ok(count)).unwrap();
            count += 1;
        }
    });
    ActorPort::new(tx)
}
