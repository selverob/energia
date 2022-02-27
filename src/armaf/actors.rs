use super::ActorPort;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log;
use tokio::sync::oneshot;

#[async_trait]
pub trait Actor<P, R>: Send + 'static {
    fn get_name(&self) -> String;

    async fn handle_message(&mut self, payload: P) -> Result<R>;

    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    async fn tear_down(&mut self) -> Result<()> {
        Ok(())
    }
}

pub async fn spawn_actor<P, R>(
    mut actor: impl Actor<P, R>,
) -> Result<ActorPort<P, R, anyhow::Error>>
where
    P: Send + 'static,
    R: Send + 'static,
{
    let name = actor.get_name();
    log::debug!("{} spawning", name);
    let (port, mut rx) = ActorPort::make();
    let (initialization_sender, initialization_receiver) = oneshot::channel::<Result<()>>();
    tokio::spawn(async move {
        let name = actor.get_name();
        let init_result = actor.initialize().await;
        let had_init_error = init_result.is_err();
        initialization_sender
            .send(init_result)
            .expect("Initialization sender failure");
        if had_init_error {
            return;
        }
        log::info!("{} initialized successfully", name);
        loop {
            match rx.recv().await {
                Some(req) => {
                    let res = actor.handle_message(req.payload).await;
                    if let Err(e) = &res {
                        log::error!("{} message handler returned error: {}", name, e);
                    }
                    if let Err(_) = req.response_sender.send(res) {
                        log::error!(
                            "{} failed to respond to request (requester went away?)",
                            name
                        );
                    }
                }
                None => {
                    log::debug!("{} stopping", name);
                    if let Err(e) = actor.tear_down().await {
                        log::error!("{} failed to tear down: {}", name, e);
                    }
                    return;
                }
            }
        }
    });

    match initialization_receiver.await {
        Ok(Ok(_)) => Ok(port),
        Ok(Err(e)) => {
            log::error!("Error initializing {}: {}", name, e);
            Err(e)
        }
        Err(e) => Err(anyhow!(e)),
    }
}

// struct ActorManager<A> {
//     actor: A,
//     recv_chan: mpsc::Receiver<Request<P, R>>
// }
