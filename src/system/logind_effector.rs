use crate::armaf::{ActorPort, EffectorMessage};
use log::info;

pub fn spawn() -> ActorPort<EffectorMessage, (), ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Logind effector started");
        loop {
            let option_req = rx.recv().await;
            if option_req.is_none() {
                log::debug!("Spurious wakeup");
                continue;
            }
            let req = option_req.unwrap();
            match req.payload {
                EffectorMessage::Execute => {
                    log::info!("Setting idleness in logind");
                    req.respond(Ok(()));
                },
                EffectorMessage::Rollback => {
                    log::info!("Setting activity in logind");
                    req.respond(Ok(()));
                },
                EffectorMessage::Stop => {
                    log::info!("Stopping");
                    req.respond(Ok(()));
                    return;
                }
            }
        }
    });
    port
}
