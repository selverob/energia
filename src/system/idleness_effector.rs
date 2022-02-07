use crate::armaf::ActorPort;
use crate::external::idleness::IdlenessSetter;
use log;

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq)]
pub struct SetTimeout(pub u64);

pub fn spawn<'a, S: IdlenessSetter<'a>>(setter: S) -> ActorPort<SetTimeout, (), ()> {
    let (port, mut rx) = ActorPort::<SetTimeout, (), ()>::make();
    tokio::spawn(async move {
        log::info!("Started");
        loop {
            match rx.recv().await {
                Some(req) => {
                    log::info!("Setting idleness timeout to {}", req.payload.0);
                    // let res = tokio::task::spawn_blocking(move || {
                    //     setter.set_idleness_timeout(timeout_in_seconds)
                    // })
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
