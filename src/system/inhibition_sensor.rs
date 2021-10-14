use crate::armaf::ActorPort;
use log;

pub struct Inhibition;

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct GetInhibitions;

pub fn spawn() -> ActorPort<GetInhibitions, Vec<Inhibition>, ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Inhibition sensor started");
        loop {
            match rx.recv().await {
                Some(req) => {
                    log::info!("Inhibition sensor got message");
                    req.respond(Ok(vec![Inhibition]));
                }
                None => log::debug!("Spurious wakeup"),
            }
        }
    });
    port
}
