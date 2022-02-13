use super::actors;
use tokio;

#[tokio::test]
async fn test_request_response() {
    let new_return = actors::Request::new(());
    let request: actors::Request<(), bool, ()> = new_return.0;
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
    let port = spawn_two_increments_one_error();
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
    if let actors::ActorRequestError::ActorError(e) = error {
        assert_eq!(e.to_string(), "Saturated");
        assert_eq!(e.kind(), std::io::ErrorKind::Other);
    } else {
        panic!("An error from Actor is not translated correctly");
    }
}

#[tokio::test]
async fn test_request_errors() {
    let port = spawn_two_increments_one_error();
    let recv_error = port
        .request(TestActorMessage::Terminate)
        .await
        .expect_err("Actor should close the oneshot channel when terminating");
    if let actors::ActorRequestError::RecvError = recv_error {
    } else {
        panic!("A RecvError is not translated correctly");
    }
    let send_error = port
        .request(TestActorMessage::Increment)
        .await
        .expect_err("Actor request channel is still sendable after actor termination");
    if let actors::ActorRequestError::SendError = send_error {
    } else {
        panic!("A SendError is not translated correctly");
    }
}

enum TestActorMessage {
    Increment,
    // Don't use this in your code! Actors should terminate on their own, used just for testing.
    Terminate,
}

fn spawn_two_increments_one_error() -> actors::ActorPort<TestActorMessage, usize, std::io::Error> {
    let (port, mut rx) = actors::ActorPort::make();
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
    });
    port
}
