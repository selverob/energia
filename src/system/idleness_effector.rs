use log;
use crate::armaf::ActorPort;

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq)]
pub struct SetTimeout(pub u64);

pub fn spawn() -> ActorPort<SetTimeout, (), ()> {
    let (port, mut rx) = ActorPort::<SetTimeout, (), ()>::make();
    tokio::spawn(async move {
        log::info!("Idleness effector started");
        while let option_req = rx.recv().await {
            match option_req {
                Some(req) => {
                    log::info!("Setting idleness timeout to {}", req.payload.0);
                    req.respond(Ok(()));
                }
                None => log::debug!("Spurious wakeup")
            }
        }
    });
    port
}
