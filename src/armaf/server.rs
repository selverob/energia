//! Server abstraction on top of [super::ports]

use super::ActorPort;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::oneshot;

/// A trait which allows you to write server code for Server-like Actors (which
/// just receive requests on their ActorPorts and then respond to them) in a
/// structured way. Servers run in Tokio tasks and have three lifecycle phases.
///
/// Initialization occurs first and the [spawn_server] function doesn't return
/// until it has finished, either successfully or with an error.
///
/// Then, in handling phase, handle_message is invoked to process each request
/// sent to the [ActorPort] returned by [spawn_server].
///
/// After all [ActorPort]s for the server get dropped, the teardown phase is
/// entered. Server can perform any asynchronous clean up tasks it needs to do.
/// For example, it can return a system component to a state in which it was
/// before the server took control of it (for example, reset the display
/// brightness it was controlling). Keep in mind that this method is only
/// supposed to be used for asynchronous clean up tasks. All other kinds of
/// cleanup should be performed using an impl of [Drop].
///
/// # Examples
///
/// An example server implemented using this trait:
///
/// ```rust
/// struct TestServer{
///     current_number: usize,
///     fail_at: usize,
///     fail_initialization: bool,
///     drop_notifier: mpsc::Sender<()>,
/// }
///
/// impl TestServer{
///     fn new(fail_at: usize, fail_initialization: bool) -> (TestActor, mpsc::Receiver<()>) {
///         let (drop_sender, drop_receiver) = mpsc::channel(1);
///         (
///             TestServer{
///                 current_number: 0,
///                 drop_notifier: drop_sender,
///                 fail_at,
///                 fail_initialization,
///             },
///             drop_receiver,
///         )
///     }
/// }
///
/// #[async_trait]
/// impl Server<(), usize> for TestServer{
///     fn get_name(&self) -> String {
///         "test_server".to_owned()
///     }
///
///     async fn handle_message(&mut self, _: ()) -> Result<usize> {
///         self.current_number += 1;
///         if self.current_number == self.fail_at {
///             Err(anyhow!("Saturated"))
///         } else {
///             Ok(self.current_number)
///         }
///     }
///
///     async fn initialize(&mut self) -> Result<()> {
///         if self.fail_initialization {
///             Err(anyhow!("Forced initialization fail"))
///         } else {
///             Ok(())
///         }
///     }
///
///     async fn tear_down(&mut self) -> Result<()> {
///         Ok(self.drop_notifier.send(()).await?)
///     }
/// }
/// ```
#[async_trait]
pub trait Server<P, R>: Send + 'static {
    /// Returns the name of the Server, which is used in logging messages
    fn get_name(&self) -> String;

    /// Handle a request sent to the [ActorPort] of the Actor.
    ///
    /// The returned success or failure are sent to the requester using
    /// [super::Request<P, R, E>::respond] method.
    async fn handle_message(&mut self, payload: P) -> Result<R>;

    /// Performs server initialization tasks.
    ///
    /// An error in this method will cause
    /// [spawn_server] to fail with the error. Default implementation just returns `Ok(())`
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Perform server teardown / cleanup tasks.
    ///
    /// Since this method is invoked at a non-deterministic time (after the
    /// Actor's [ActorPort]s are dropped), the errors are only logged,
    /// nothing else is done with them.
    async fn tear_down(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Starts a task for the given [Server] and handles low-level details of request
/// receiving and response sending.
///
/// See [Server] for more information about when its methods are called.
///
/// This method waits for the initialization of the server to be done before
/// returning the [ActorPort]. If initialization fails, an error is returned
/// instead.
pub async fn spawn_server<P, R>(
    mut server: impl Server<P, R>,
) -> Result<ActorPort<P, R, anyhow::Error>>
where
    P: Send + 'static,
    R: Send + 'static,
{
    let name = server.get_name();
    log::debug!("{} spawning", name);
    let (port, mut rx) = ActorPort::make();
    let (initialization_sender, initialization_receiver) = oneshot::channel::<Result<()>>();
    tokio::spawn(async move {
        let name = server.get_name();
        let init_result = server.initialize().await;
        let had_init_error = init_result.is_err();
        initialization_sender
            .send(init_result)
            .expect("Initialization sender failure");
        if had_init_error {
            return;
        }
        log::info!("{} initialized successfully", name);
        loop {
            match rx.recv().await {
                Some(req) => {
                    let res = server.handle_message(req.payload).await;
                    if let Err(e) = &res {
                        log::error!("{} message handler returned error: {}", name, e);
                    }
                    if req.response_sender.send(res).is_err() {
                        log::error!(
                            "{} failed to respond to request (requester went away?)",
                            name
                        );
                    }
                }
                None => {
                    log::debug!("{} stopping", name);
                    if let Err(e) = server.tear_down().await {
                        log::error!("{} failed to tear down: {}", name, e);
                    }
                    log::debug!("{} stopped", name);
                    return;
                }
            }
        }
    });

    match initialization_receiver.await {
        Ok(Ok(_)) => Ok(port),
        Ok(Err(e)) => {
            log::error!("Error initializing {}: {}", name, e);
            Err(e)
        }
        Err(e) => Err(anyhow!(e)),
    }
}
