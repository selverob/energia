use std::result::Result;

use tokio::sync::mpsc::error::SendError;
use tokio::sync::{mpsc, oneshot};

type ResponseReceiver<R, E> = oneshot::Receiver<Result<R, E>>;

pub struct Request<P, R, E> {
    pub payload: P,
    pub response_sender: oneshot::Sender<Result<R, E>>,
}

impl<P, R, E> Request<P, R, E> {
    pub fn new(payload: P) -> (Request<P, R, E>, ResponseReceiver<R, E>) {
        let (response_sender, response_receiver) = oneshot::channel();
        let request = Request {
            payload,
            response_sender,
        };
        (request, response_receiver)
    }

    pub fn respond(self, response: Result<R, E>) -> Result<(), Result<R, E>> {
        self.response_sender.send(response)
    }
}

#[derive(Debug)]
pub enum ActorRequestError<E> {
    SendError,
    RecvError,
    ActorError(E),
}

#[derive(Clone, Debug)]
pub struct ActorPort<P, R, E> {
    message_sender: mpsc::Sender<Request<P, R, E>>,
}

impl<P, R, E> ActorPort<P, R, E> {
    pub fn new(message_sender: mpsc::Sender<Request<P, R, E>>) -> ActorPort<P, R, E> {
        ActorPort { message_sender }
    }

    pub fn make() -> (ActorPort<P, R, E>, mpsc::Receiver<Request<P, R, E>>) {
        let (tx, rx) = mpsc::channel::<Request<P, R, E>>(8);
        (ActorPort::new(tx), rx)
    }

    pub async fn raw_request(
        &self,
        r: Request<P, R, E>,
    ) -> Result<(), SendError<Request<P, R, E>>> {
        self.message_sender.send(r).await
    }

    pub async fn request(&self, payload: P) -> Result<R, ActorRequestError<E>> {
        let (req, rx) = Request::new(payload);
        if self.raw_request(req).await.is_err() {
            return Err(ActorRequestError::SendError);
        }
        match rx.await {
            Err(_) => Err(ActorRequestError::RecvError),
            Ok(inner_result) => match inner_result {
                Ok(response) => Ok(response),
                Err(actor_error) => Err(ActorRequestError::ActorError(actor_error)),
            },
        }
    }
}
