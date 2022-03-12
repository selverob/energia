//! Basic primitives for constructing a simple actor system on top of Tokio tasks.

use std::{fmt::Debug, result::Result};
use thiserror::Error;
use tokio::sync::{mpsc, mpsc::error::SendError, oneshot};

/// A shorthand type defining a [oneshot::Receiver] which is used to receive the
/// results of an operation invoked by a [Request].
type ResponseReceiver<R, E> = oneshot::Receiver<Result<R, E>>;

/// A request sent to an actor.
///
/// A Request contains a generic payload which has to match the payload accepted
/// by the [ActorPort] and a [oneshot] channel on which the result of the
/// operation or an error will be returned.
pub struct Request<P, R, E> {
    pub payload: P,
    pub response_sender: oneshot::Sender<Result<R, E>>,
}

impl<P, R, E> Request<P, R, E> {
    /// Creates a new [Request], populating the struct with the given payload and
    /// constructing a correctly typed [oneshot] channel. The created
    /// [oneshot::Sender] is stored inside the request, while the
    /// [ResponseReceiver] is returned.
    pub fn new(payload: P) -> (Request<P, R, E>, ResponseReceiver<R, E>) {
        let (response_sender, response_receiver) = oneshot::channel();
        let request = Request {
            payload,
            response_sender,
        };
        (request, response_receiver)
    }

    /// A convenience method for sending a response on the [Request]'s [oneshot]
    /// channel.
    pub fn respond(self, response: Result<R, E>) -> Result<(), Result<R, E>> {
        self.response_sender.send(response)
    }
}

/// An error occuring during the exchange of messages with an actor.
#[derive(Debug, Error, Clone)]
pub enum ActorRequestError<E: Debug> {
    #[error("error when sending message to actor")]
    SendError,

    #[error("error while awating request response channel")]
    RecvError,

    #[error("internal actor error: {0:?}")]
    ActorError(E),
}

/// A communication channel with an actor.
///
/// This is the main primitive of the actor system. There is no structure which
/// can be used to reference an actor, which is only a [tokio::task].
/// An ActorPort allows you to send [Request]s to the actor. The
/// [oneshot] channel contained in the [Request] is then used to communicate the
/// results back to its sender.
///
/// ActorPorts are clone-able. When an actor is spawned, it will generally
/// return a single ActorPort which can then be cloned and stored in multiple
/// places. This has two important implications:
///
/// 1. An actor must not assume that it's only communicating with a single
///    different actor. If it does make this assumption (for example by
///    expecting a specified series of messages in a defined order), that should
///    be documented.
///
/// 2. An actor should not expect a specific message instructing it to stop
///    itself. Any cleanup actions should be performed once a None is returned
///    on from the [mpsc::Receiver::recv], indicating that there all
///    [mpsc::Sender]s have been dropped.
#[derive(Debug)]
pub struct ActorPort<P, R, E: Debug> {
    message_sender: mpsc::Sender<Request<P, R, E>>,
}

// #[derive(Debug)] creates an implementation of Clone
// which only applies if all the type parameters are Clone.
// E tends to be anyhow::Error, which is not Clone, so
// most the ActorPorts would not be Clone with the derived
// implementation.
impl<P, R, E: Debug> Clone for ActorPort<P, R, E> {
    fn clone(&self) -> Self {
        Self {
            message_sender: self.message_sender.clone(),
        }
    }
}

impl<P, R, E: Debug> ActorPort<P, R, E> {
    /// Creates a new ActorPort which will send requests through the given Sender
    pub fn new(message_sender: mpsc::Sender<Request<P, R, E>>) -> ActorPort<P, R, E> {
        ActorPort { message_sender }
    }

    /// A convenience function for creating an ActorPort initialized with a
    /// Sender side of an [mpsc] channe.
    ///
    /// The Receiver side is returned too. This function can be used to simplify
    /// actor initialization. The Receiver is moved into the [tokio::task] for
    /// the actor while the ActorPort is returned to the caller.
    pub fn make() -> (ActorPort<P, R, E>, mpsc::Receiver<Request<P, R, E>>) {
        let (tx, rx) = mpsc::channel::<Request<P, R, E>>(8);
        (ActorPort::new(tx), rx)
    }

    /// Sends a [Request] to the actor. Does not do anything else. Prefer using
    /// the [Self::request] method.
    pub async fn raw_request(
        &self,
        r: Request<P, R, E>,
    ) -> Result<(), SendError<Request<P, R, E>>> {
        self.message_sender.send(r).await
    }

    /// Constructs a [Request] with the given payload sends it on this port and
    /// waits for the actor's response.
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
