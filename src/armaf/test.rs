use std::error::Error;

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
        port.request(Increment)
            .await
            .expect("Expected a successful response"),
        0
    );
    assert_eq!(
        port.request(Increment)
            .await
            .expect("Expected a successful response"),
        1
    );
    let error = port
        .request(Increment)
        .await
        .expect_err("Expected an error from actor");
    if let actors::ActorRequestError::ActorError(e) = error {
        assert_eq!(e.to_string(), "Saturated");
        assert_eq!(e.kind(), std::io::ErrorKind::Other);
    }
}

pub struct Increment;

pub fn spawn_two_increments_one_error() -> actors::ActorPort<Increment, usize, std::io::Error> {
    let (port, mut rx) = actors::ActorPort::make();
    tokio::spawn(async move {
        let mut count = 0;
        while let Some(req) = rx.recv().await {
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
    });
    port
}
