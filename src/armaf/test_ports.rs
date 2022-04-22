use super::ports;
use std::sync::{atomic::AtomicBool, Arc};
use tokio;

#[tokio::test]
async fn test_request_response() {
    let new_return = ports::Request::new(());
    let request: ports::Request<(), bool, ()> = new_return.0;
    let receiver = new_return.1;
    assert_eq!(request.payload, ());
    request
        .respond(Ok(true))
        .expect("Channel failure when sending response");
    let response = receiver
        .await
        .expect("Channel failure when receiving response");
    assert_eq!(response, Ok(true));
}

#[tokio::test]
async fn test_actor_port() {
    let termination_flag = make_termination_flag();
    let port = spawn_two_increments_one_error(termination_flag.clone());
    assert_eq!(
        port.request(TestActorMessage::Increment)
            .await
            .expect("Expected a successful response"),
        0
    );
    assert_eq!(
        port.request(TestActorMessage::Increment)
            .await
            .expect("Expected a successful response"),
        1
    );
    let error = port
        .request(TestActorMessage::Increment)
        .await
        .expect_err("Expected an error from actor");
    if let ports::ActorRequestError::Actor(e) = error {
        assert_eq!(e.to_string(), "Saturated");
        assert_eq!(e.kind(), std::io::ErrorKind::Other);
    } else {
        panic!("An error from Actor is not translated correctly");
    }
    assert!(!termination_flag
        .as_ref()
        .load(std::sync::atomic::Ordering::Acquire));
    port.await_shutdown().await;
    assert!(termination_flag
        .as_ref()
        .load(std::sync::atomic::Ordering::Acquire));
}

#[tokio::test]
async fn test_request_errors() {
    let termination_flag = make_termination_flag();
    let port = spawn_two_increments_one_error(termination_flag.clone());
    assert!(!termination_flag
        .as_ref()
        .load(std::sync::atomic::Ordering::Acquire));
    let recv_error = port
        .request(TestActorMessage::Terminate)
        .await
        .expect_err("Actor should close the oneshot channel when terminating");
    if let ports::ActorRequestError::Recv = recv_error {
    } else {
        panic!("A RecvError is not translated correctly");
    }
    let send_error = port
        .request(TestActorMessage::Increment)
        .await
        .expect_err("Actor request channel is still sendable after actor termination");
    if let ports::ActorRequestError::Send = send_error {
    } else {
        panic!("A SendError is not translated correctly");
    }

    // This will hang forever in case the shutdown notifier's Sender side is not
    // closed correctly on Drop
    port.await_shutdown().await;
}

enum TestActorMessage {
    Increment,
    // Don't use this in your code! Actors should terminate on their own, used just for testing.
    Terminate,
}

fn spawn_two_increments_one_error(
    termination_flag: Arc<AtomicBool>,
) -> ports::ActorPort<TestActorMessage, usize, std::io::Error> {
    let (port, mut rx) = ports::ActorPort::make();
    tokio::spawn(async move {
        let mut count = 0;
        while let Some(req) = rx.recv().await {
            match req.payload {
                TestActorMessage::Increment => {
                    if count < 2 {
                        req.respond(Ok(count)).expect("Couldn't respond to request");
                        count += 1;
                    } else {
                        req.respond(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Saturated",
                        )))
                        .expect("Couldn't respond to request");
                    }
                }
                // This code is intentionally incorrect, an actor should always
                // respond to a message it has been send. We want to test error
                // handling in ActorPort though. Also, actors shouldn't need
                // termination messages.
                TestActorMessage::Terminate => return,
            }
        }
        termination_flag
            .as_ref()
            .store(true, std::sync::atomic::Ordering::Release);
    });
    port
}

#[tokio::test]
async fn test_handle_drop() {
    let flag = make_termination_flag();
    let handle = spawn_handle_tester(flag.clone());
    assert!(!flag.as_ref().load(std::sync::atomic::Ordering::Acquire));
    drop(handle);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(flag.as_ref().load(std::sync::atomic::Ordering::Acquire));
}

#[tokio::test]
async fn test_handle_await() {
    let flag = make_termination_flag();
    let handle = spawn_handle_tester(flag.clone());
    assert!(!flag.as_ref().load(std::sync::atomic::Ordering::Acquire));
    handle.await_shutdown().await;
    assert!(flag.as_ref().load(std::sync::atomic::Ordering::Acquire));
}

fn spawn_handle_tester(termination_flag: Arc<AtomicBool>) -> ports::Handle {
    let (handle, mut handle_child) = ports::Handle::new();
    tokio::spawn(async move {
        handle_child.should_terminate().await;
        termination_flag
            .as_ref()
            .store(true, std::sync::atomic::Ordering::Release);
    });
    handle
}

fn make_termination_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
