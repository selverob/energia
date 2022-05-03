//! Basic primitives for constructing a simple actor system on top of Tokio tasks.

use std::{fmt::Debug, result::Result};
use thiserror::Error;
use tokio::sync::{mpsc, mpsc::error::SendError, oneshot, watch};

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
    Send,

    #[error("error while awating request response channel")]
    Recv,

    #[error("internal actor error: {0:?}")]
    Actor(E),
}

/// A communication channel with an actor.
///
/// This is the main primitive of the actor system. There is no general
/// structure which can be used to reference an actor, which is only a
/// [tokio::task]. An ActorPort allows you to send [Request]s to the actor. The
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
///    on from the [mpsc::Receiver::recv], indicating that all [mpsc::Sender]s
///    have been dropped.
#[derive(Debug)]
pub struct ActorPort<P, R, E: Debug> {
    message_sender: mpsc::Sender<Request<P, R, E>>,
    shutdown_receiver: watch::Receiver<()>,
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
            shutdown_receiver: self.shutdown_receiver.clone(),
        }
    }
}

impl<P, R, E: Debug> ActorPort<P, R, E> {
    /// Creates a new ActorPort which will send requests through the given Sender
    pub fn new(
        message_sender: mpsc::Sender<Request<P, R, E>>,
        shutdown_receiver: watch::Receiver<()>,
    ) -> ActorPort<P, R, E> {
        ActorPort {
            message_sender,
            shutdown_receiver,
        }
    }

    /// A convenience function for creating an ActorPort initialized with a
    /// Sender side of an [mpsc] channe.
    ///
    /// An [ActorReceiver] is returned too. This function can be used to simplify
    /// actor initialization. The Receiver is moved into the [tokio::task] for
    /// the actor while the ActorPort is returned to the caller.
    pub fn make() -> (ActorPort<P, R, E>, ActorReceiver<P, R, E>) {
        let (req_tx, req_rx) = mpsc::channel::<Request<P, R, E>>(8);
        let (shutdown_tx, shutdown_rx) = watch::channel(());
        (
            ActorPort::new(req_tx, shutdown_rx),
            ActorReceiver::new(req_rx, shutdown_tx),
        )
    }

    /// Sends a [Request] to the actor. Does not do anything else. Prefer using
    /// the [Self::request] method.
    pub async fn raw_request(
        &self,
        r: Request<P, R, E>,
    ) -> Result<(), SendError<Request<P, R, E>>> {
        self.message_sender.send(r).await
    }

    pub async fn request_with_timeout(
        &self,
        timeout: std::time::Duration,
        payload: P,
    ) -> Result<R, ActorRequestError<E>> {
        let sleep = tokio::time::sleep(timeout);
        tokio::pin!(sleep);

        tokio::select! {
            res = self.request(payload) => {
                res
            }
            _ = &mut sleep => {
                Err(ActorRequestError::Recv)
            }
        }
    }

    /// Constructs a [Request] with the given payload sends it on this port and
    /// waits for the actor's response.
    /// Constructs a [Request] with the given payload sends it on this port and
    /// waits for the actor's response.
    pub async fn request(&self, payload: P) -> Result<R, ActorRequestError<E>> {
        let (req, rx) = Request::new(payload);
        if self.raw_request(req).await.is_err() {
            return Err(ActorRequestError::Send);
        }
        match rx.await {
            Err(_) => Err(ActorRequestError::Recv),
            Ok(inner_result) => match inner_result {
                Ok(response) => Ok(response),
                Err(actor_error) => Err(ActorRequestError::Actor(actor_error)),
            },
        }
    }

    /// Await actor termination
    ///
    /// Drops this port's message sender and waits until all the other clones of
    /// this ActorPort are dropped or have this method called and the actor
    /// terminates. An actor is considered to terminate once it drops its
    /// [ActorReceiver].
    pub async fn await_shutdown(self) {
        // We first need to drop our message sender because the actors are
        // supposed treat closing of their message receivers as a shutdown
        // signal.
        drop(self.message_sender);
        let mut shutdown_receiver = self.shutdown_receiver;

        // Now we just wait until all other message senders are closed, actor
        // registers that and finishes its cleanup, dropping the ActorPort.
        let result = shutdown_receiver.changed().await;
        assert!(result.is_err());
    }
}

/// The receiving side of an [ActorPort].
///
/// Contains a [mpsc::Receiver] which can either be used directly or which can
/// be called through the convenience `recv` method on this struct.
///
/// This struct also handles termination notification for [ActorPorts](ActorPort), thus the
/// dropping this struct must be the last thing an actor does. Performing any
/// operations after that will break [`ActorPort::await_shutdown`].
#[derive(Debug)]
pub struct ActorReceiver<P, R, E: Debug> {
    pub request_receiver: mpsc::Receiver<Request<P, R, E>>,
    _shutdown_notifier: watch::Sender<()>,
}

impl<P, R, E: Debug> ActorReceiver<P, R, E> {
    /// Create a new [ActorReceiver]
    pub fn new(
        request_receiver: mpsc::Receiver<Request<P, R, E>>,
        shutdown_notifier: watch::Sender<()>,
    ) -> Self {
        ActorReceiver {
            request_receiver,
            _shutdown_notifier: shutdown_notifier,
        }
    }

    /// Call the recv method on this struct's request_receiver.
    ///
    /// The semantics of this method are exactly the same as the semantics of
    /// [mpsc::Receiver]'s recv method.
    pub async fn recv(&mut self) -> Option<Request<P, R, E>> {
        self.request_receiver.recv().await
    }
}

/// A handle which allows signalizing termination / drop to actors and waiting
/// for their termination.
///
/// You can consider a Handle to be a specific kind of [ActorPort] which
/// enforces single-parent semantics (i.e. an actor has a specific parent actor
/// which handles its lifecycle) and doesn't support sending any messages apart
/// from the termination signal to the actor.
///
/// This handle contains a [oneshot::Sender] which is closed once the handle is
/// dropped. Thus, an actor can await an error on the [oneshot::Receiver]
/// returned from the [Handle::new()] method and interpret it as a signal to
/// terminate itself.
pub struct Handle(ActorPort<(), (), ()>);

impl Handle {
    /// Create a new Handle and return it and its associated receiver.
    ///
    /// The handle should be returned to the spawning actor while the actor which
    /// wants to be notified about its drop should keep the returned [oneshot::Receiver]
    pub fn new() -> (Handle, HandleChild) {
        let (port, receiver) = ActorPort::make();
        (Handle(port), HandleChild(receiver))
    }

    pub async fn await_shutdown(self) {
        self.0.await_shutdown().await
    }
}

/// The side of the handle belonging to the child actor.
///
/// This struct can be used to check whether the parent actor requested the
/// actor to terminate. Since it's based on [ActorPort], same caveats apply - it
/// should never be dropped during actor operation or while clean up is still
/// pending.
pub struct HandleChild(ActorReceiver<(), (), ()>);

impl HandleChild {
    /// Wait until the parent [Handle] is dropped or its
    /// [await_shutdown](`Handle::await_shutdown`) method is called.
    ///
    /// Since this function will not return until these conditions are
    /// fulfilled, you should call it within a [tokio::select!] block.
    pub async fn should_terminate(&mut self) {
        let res = self.0.recv().await;
        assert!(res.is_none());
    }
}
