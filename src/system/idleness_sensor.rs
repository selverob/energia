use log;
use tokio::sync::mpsc;
use crate::armaf::ActorPort;

// #[derive(Message)]
// #[rtype(result = "()")]
// pub struct SetSubscriber(Recipient<NewState>);

#[derive(PartialEq, Eq, Debug, Hash)]
pub enum IdlenessState {
    Idle,
    Active,
}

// TODO: Use spawn_blocking
pub fn spawn(subscriber: mpsc::Sender<IdlenessState>) -> ActorPort<(), (), ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Idleness sensor started");
        while let option_req = rx.recv().await {
            match option_req {
                Some(req) => {
                    log::info!("Idleness sensor got message");
                    req.respond(Ok(()));
                    subscriber.send(IdlenessState::Active);
                }
                None => log::debug!("Spurious wakeup")
            }
        }
    });
    port
}
