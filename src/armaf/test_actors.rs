use super::actors::{spawn_actor, Actor};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

struct TestActor {
    current_number: usize,
    fail_at: usize,
    fail_initialization: bool,
    drop_notifier: mpsc::Sender<()>,
}

impl TestActor {
    fn new(fail_at: usize, fail_initialization: bool) -> (TestActor, mpsc::Receiver<()>) {
        let (drop_sender, drop_receiver) = mpsc::channel(1);
        (
            TestActor {
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
impl Actor<(), usize> for TestActor {
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
    let (actor, mut notifier) = TestActor::new(10, false);
    let port = spawn_actor(actor).await.expect("No port returned");
    assert_eq!(port.request(()).await.unwrap(), 1);
    assert_eq!(port.request(()).await.unwrap(), 2);
    drop(port);
    notifier
        .recv()
        .await
        .expect("tear_down not called on actor");
}

#[tokio::test]
async fn test_response_failure() {
    let (actor, mut notifier) = TestActor::new(3, false);
    let port = spawn_actor(actor).await.expect("No port returned");
    assert_eq!(port.request(()).await.unwrap(), 1);
    assert_eq!(port.request(()).await.unwrap(), 2);
    assert!(port.request(()).await.is_err());
    drop(port);
    notifier
        .recv()
        .await
        .expect("tear_down not called on actor");
}

#[tokio::test]
async fn test_initialization_failure() {
    let (actor, _) = TestActor::new(3, true);
    assert!(spawn_actor(actor).await.is_err());
}