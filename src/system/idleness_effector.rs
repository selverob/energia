use crate::armaf::ActorPort;
use log;

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq)]
pub struct SetTimeout(pub u64);

pub fn spawn() -> ActorPort<SetTimeout, (), ()> {
    let (port, mut rx) = ActorPort::<SetTimeout, (), ()>::make();
    tokio::spawn(async move {
        log::info!("Started");
        loop {
            match rx.recv().await {
                Some(req) => {
                    log::info!("Setting idleness timeout to {}", req.payload.0);
                    req.respond(Ok(()));
                }
                None => {
                    log::debug!("Stopping");
                    return;
                }
            }
        }
    });
    port
}
