use super::server::{spawn_server, Server};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

struct TestServer {
    current_number: usize,
    fail_at: usize,
    fail_initialization: bool,
    drop_notifier: mpsc::Sender<()>,
}

impl TestServer {
    fn new(fail_at: usize, fail_initialization: bool) -> (TestServer, mpsc::Receiver<()>) {
        let (drop_sender, drop_receiver) = mpsc::channel(1);
        (
            TestServer {
                current_number: 0,
                drop_notifier: drop_sender,
                fail_at,
                fail_initialization,
            },
            drop_receiver,
        )
    }
}

#[async_trait]
impl Server<(), usize> for TestServer {
    fn get_name(&self) -> String {
        "test_actor".to_owned()
    }

    async fn handle_message(&mut self, _: ()) -> Result<usize> {
        self.current_number += 1;
        if self.current_number == self.fail_at {
            Err(anyhow!("Saturated"))
        } else {
            Ok(self.current_number)
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.fail_initialization {
            Err(anyhow!("Forced initialization fail"))
        } else {
            Ok(())
        }
    }

    async fn tear_down(&mut self) -> Result<()> {
        Ok(self.drop_notifier.send(()).await?)
    }
}

#[tokio::test]
async fn test_happy_path() {
    let (server, mut notifier) = TestServer::new(10, false);
    let port = spawn_server(server).await.expect("No port returned");
    assert_eq!(port.request(()).await.unwrap(), 1);
    assert_eq!(port.request(()).await.unwrap(), 2);
    drop(port);
    notifier
        .recv()
        .await
        .expect("tear_down not called on server");
}

#[tokio::test]
async fn test_response_failure() {
    let (server, mut notifier) = TestServer::new(3, false);
    let port = spawn_server(server).await.expect("No port returned");
    assert_eq!(port.request(()).await.unwrap(), 1);
    assert_eq!(port.request(()).await.unwrap(), 2);
    assert!(port.request(()).await.is_err());
    drop(port);
    notifier
        .recv()
        .await
        .expect("tear_down not called on server");
}

#[tokio::test]
async fn test_initialization_failure() {
    let (server, _) = TestServer::new(3, true);
    assert!(spawn_server(server).await.is_err());
}
