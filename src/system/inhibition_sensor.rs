use log;
use crate::armaf::ActorPort;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Inhibition;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct GetInhibitions;

pub fn spawn() -> ActorPort<GetInhibitions, Vec<Inhibition>, ()> {
    let (port, mut rx) = ActorPort::make();
    tokio::spawn(async move {
        log::info!("Started");
        loop {
            match rx.recv().await {
                Some(req) => {
                    log::info!("Got message");
                    req.respond(Ok(vec![Inhibition]));
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
