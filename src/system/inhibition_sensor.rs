use crate::armaf::{self, ActorPort};
use logind_zbus::manager;
use log;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct GetInhibitions;

pub fn spawn(connection: zbus::Connection) -> ActorPort<GetInhibitions, Vec<manager::Inhibitor>, anyhow::Error> {
    let (port, mut rx) = ActorPort::<GetInhibitions, Vec<manager::Inhibitor>, anyhow::Error>::make();
    tokio::spawn(async move {
        log::debug!("Initializing manager proxy");
        match logind_zbus::manager::ManagerProxy::new(&connection).await {
            Ok(proxy) => {
                log::info!("Started");
                match rx.recv().await {
                    Some(req) => {
                        log::info!("Got message");
                        let logind_response = proxy.list_inhibitors().await;
                        req.respond(as_anyhow_result(logind_response)).expect("request response failed, is idleness controller dead?");
                    }
                    None => {
                        log::info!("Stopping");
                        return;
                    }
                }
            },
            Err(error) => {
                log::error!("Couldn't create a logind manager proxy: {}", error);
                armaf::error_loop(rx, "inhibition sensor failed to create login manager proxy".to_owned()).await;
            }
        }
    });
    port
}

fn as_anyhow_result<T, E: std::error::Error + Send + Sync + 'static>(result: Result<T, E>) -> anyhow::Result<T> {
    match result {
        Ok(t) => Ok(t),
        Err(e) => Err(anyhow::Error::new(e))
    }
}
