use crate::armaf::{ActorPort, EffectorMessage, EffectorPort};
use crate::external::display_server::DisplayServerController;
use anyhow::Result;
use log;

pub fn spawn<S: DisplayServerController>(setter: S) -> EffectorPort<i16> {
    let (port, mut rx) = ActorPort::make();
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
                        EffectorMessage::Rollback(_) => initial_timeout,
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

async fn get_current_timeout<S: DisplayServerController>(setter: &S) -> Result<i16> {
    let sent_setter = setter.clone();
    tokio::task::spawn_blocking(move || sent_setter.get_idleness_timeout()).await?
}

async fn set_timeout<S: DisplayServerController>(timeout: i16, setter: &S) -> Result<()> {
    let sent_setter = setter.clone();
    tokio::task::spawn_blocking(move || sent_setter.set_idleness_timeout(timeout)).await?
}
