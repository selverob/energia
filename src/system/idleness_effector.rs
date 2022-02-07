use crate::armaf::{ActorPort, EffectorMessage};
use crate::external::idleness::IdlenessController;
use anyhow::Result;
use log;

pub fn spawn<S: IdlenessController>(
    setter: S,
) -> ActorPort<EffectorMessage<i16>, (), anyhow::Error> {
    let (port, mut rx) = ActorPort::<EffectorMessage<i16>, (), anyhow::Error>::make();
    tokio::spawn(async move {
        log::info!("Starting");
        let initial_timeout = match get_current_timeout(&setter).await {
            Ok(initial_timeout) => initial_timeout,
            Err(err) => {
                log::error!("Failed getting initial timeout, setting it to -1: {}", err);
                -1
            }
        };

        loop {
            match rx.recv().await {
                Some(req) => {
                    let timeout_to_set = match req.payload {
                        EffectorMessage::Execute(timeout) => timeout,
                        EffectorMessage::Rollback => initial_timeout,
                    };
                    let response = set_timeout(timeout_to_set, &setter).await;
                    req.respond(response)
                        .expect("request response failed, is idleness controller dead?");
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

async fn get_current_timeout<S: IdlenessController>(setter: &S) -> Result<i16> {
    let sent_setter = setter.clone();
    tokio::task::spawn_blocking(move || sent_setter.get_idleness_timeout()).await?
}

async fn set_timeout<S: IdlenessController>(timeout: i16, setter: &S) -> Result<()> {
    let sent_setter = setter.clone();
    tokio::task::spawn_blocking(move || sent_setter.set_idleness_timeout(timeout)).await?
}
