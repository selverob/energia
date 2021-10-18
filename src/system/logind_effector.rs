use crate::armaf::{ActorPort, EffectorMessage, EffectorPort};
use log::info;

pub fn spawn() -> EffectorPort {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Started");
        loop {
            let option_req = rx.recv().await;
            if option_req.is_none() {
                log::info!("Stopping");
                return;
            }
            let req = option_req.unwrap();
            match req.payload {
                EffectorMessage::Execute => {
                    log::info!("Setting idleness in logind");
                    req.respond(Ok(()));
                }
                EffectorMessage::Rollback => {
                    log::info!("Setting activity in logind");
                    req.respond(Ok(()));
                }
            }
        }
    });
    port
}
