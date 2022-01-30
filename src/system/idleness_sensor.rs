use crate::armaf::ActorPort;
use log;
use tokio::sync::mpsc;

#[derive(PartialEq, Eq, Debug, Hash)]
pub enum IdlenessState {
    Idle,
    Active,
}

// TODO: Use spawn_blocking
pub fn spawn(subscriber: mpsc::Sender<IdlenessState>) -> ActorPort<(), (), ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Started");
        loop {
            match rx.recv().await {
                Some(req) => {
                    log::info!("Idleness sensor got message");
                    req.respond(Ok(()));
                    let _ = subscriber.send(IdlenessState::Active).await;
                }
                None => {
                    log::info!("Stopping");
                    return;
                }
            }
        }
    });
    port
}
