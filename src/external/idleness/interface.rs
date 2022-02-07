use anyhow::Result;
use tokio::sync::watch::Receiver;

/// Represents a change in the idleness state of the system.
///
/// When a user is actively using the system, it's awake. After a certain time
/// of the user not using the system, it transfers into an idle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    /// Notifies about the system transitioning from being awake to being idle
    Idle,
    /// Notifies about the system transitioning from being idle to being awake
    Awakened,
}

/// The interface between Energia and the user's display server for the purposes
/// of detecting and controlling system's idleness behavior.
pub trait DisplayServerInterface<'a> {
    type Setter: IdlenessSetter<'a>;

    /// Get a [Receiver] on which notification about system idleness state changes can be received.
    fn get_idleness_channel(&self) -> Receiver<SystemState>;

    /// Get a structure which will allow controlling the system's idleness behavior.
    fn get_idleness_setter(&'a self) -> Self::Setter;
}

/// Control for the system's idleness behavior
pub trait IdlenessSetter<'a> {
    /// Set the time of user's inactivity after which the display server should
    /// notify about user's idleness
    fn set_idleness_timeout(&self, timeout_in_seconds: i16) -> Result<()>;

    /// Get the time of inactivity after which the system is considered idle
    fn get_idleness_timeout(&self) -> Result<i16>;
}
