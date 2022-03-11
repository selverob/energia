use std::fmt::Debug;

use crate::armaf::ActorPort;
use log;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::oneshot;

/// Allow driving an actor using a [broadcast] channel.
///
/// Consumes an [ActorPort] and a [broadcast::Receiver] and retransmits each
/// message from the Receiver on the ActorPort as a [armaf::Request].
pub struct BroadcastAdapter(oneshot::Sender<()>);

impl BroadcastAdapter {
    pub fn new<P, E>(
        mut source_channel: broadcast::Receiver<P>,
        destination_port: ActorPort<P, (), E>,
    ) -> BroadcastAdapter
    where
        P: Send + 'static + Clone + Sync,
        E: Send + 'static + Debug,
    {
        let (drop_sender, mut drop_receiver) = oneshot::channel();

        tokio::spawn(async move {
            loop {
                select! {
                    Err(_) = &mut drop_receiver => return,
                    Ok(p) = source_channel.recv() => {
                        if let Err(e) = destination_port.request(p).await {
                            // TODO: Maybe return a channel on which errors can be consumed?
                            log::error!("Destination actor returned an error: {:?}", e);
                        }
                    }
                }
            }
        });

        BroadcastAdapter(drop_sender)
    }
}

#[cfg(test)]
mod test {
    use crate::armaf::ActorPort;

    use super::BroadcastAdapter;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_adapter() -> anyhow::Result<()> {
        let (broadcast_our, broadcast_for_adapter) = broadcast::channel(2);
        let (port, mut request_receiver) = ActorPort::<i32, (), std::io::Error>::make();
        let adapter = BroadcastAdapter::new(broadcast_for_adapter, port);
        broadcast_our.send(1).unwrap();
        let req_1 = request_receiver.recv().await.unwrap();
        assert_eq!(req_1.payload, 1);
        req_1.respond(Ok(())).unwrap();
        broadcast_our.send(2).unwrap();
        let req_2 = request_receiver.recv().await.unwrap();
        assert_eq!(req_2.payload, 2);
        req_2.respond(Ok(())).unwrap();
        drop(adapter);
        assert!(request_receiver.recv().await.is_none());
        Ok(())
    }
}
