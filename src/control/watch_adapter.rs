use std::{error::Error, fmt::Debug};

use crate::armaf::ActorPort;
use log;
use tokio::select;
use tokio::sync::oneshot;
use tokio::sync::watch;

/// Allow driving an actor using a [watch] channel.
///
/// Consumes an [ActorPort] and a [watch::Receiver] and retransmits each
/// message from the Receiver on the ActorPort as a [armaf::Request].
pub struct WatchAdapter(oneshot::Sender<()>);

impl WatchAdapter {
    pub fn new<P, E>(
        mut source_channel: watch::Receiver<P>,
        destination_port: ActorPort<P, (), E>,
    ) -> WatchAdapter
    where
        P: Send + 'static + Clone + Sync,
        E: Send + 'static + Debug,
    {
        let (drop_sender, mut drop_receiver) = oneshot::channel();

        tokio::spawn(async move {
            loop {
                select! {
                    Err(_) = &mut drop_receiver => return,
                    Ok(()) = source_channel.changed() => {
                        let to_forward = (*source_channel.borrow()).clone();
                        if let Err(e) = destination_port.request(to_forward).await {
                            // TODO: Maybe return a channel on which errors can be consumed?
                            log::error!("Destination actor returned an error: {:?}", e);
                        }
                    }
                }
            }
        });

        WatchAdapter(drop_sender)
    }
}

#[cfg(test)]
mod test {
    use crate::armaf::ActorPort;

    use super::WatchAdapter;
    use tokio::sync::watch;

    #[tokio::test]
    async fn test_adapter() -> anyhow::Result<()> {
        let (watch_our, watch_for_adapter) = watch::channel(0);
        let (port, mut request_receiver) = ActorPort::<i32, (), std::io::Error>::make();
        let adapter = WatchAdapter::new(watch_for_adapter, port);
        watch_our.send(1).unwrap();
        let req_1 = request_receiver.recv().await.unwrap();
        assert_eq!(req_1.payload, 1);
        req_1.respond(Ok(())).unwrap();
        watch_our.send(2).unwrap();
        let req_2 = request_receiver.recv().await.unwrap();
        assert_eq!(req_2.payload, 2);
        req_2.respond(Ok(())).unwrap();
        drop(adapter);
        assert!(request_receiver.recv().await.is_none());
        Ok(())
    }
}
