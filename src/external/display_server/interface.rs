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

/// Represents the power saving level of the system's screens
///
/// Enum variant documentation copied from https://www.x.org/releases/X11R7.7/doc/xextproto/dpms.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DPMSLevel {
    /// In use
    On,
    /// Blanked, low power
    Standby,
    /// Blanked, lower power
    Suspend,
    /// Shut off, awaiting activity
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DPMSTimeouts {
    pub standby: u16,
    pub suspend: u16,
    pub off: u16,
}

impl DPMSTimeouts {
    pub fn new(standby: u16, suspend: u16, off: u16) -> DPMSTimeouts {
        DPMSTimeouts {
            standby,
            suspend,
            off,
        }
    }
}

/// The interface between Energia and the user's display server for the purposes
/// of detecting and controlling system's idleness behavior and display settings.
pub trait DisplayServer: Send {
    type Controller: DisplayServerController;

    /// Get a [Receiver] on which notification about system idleness state changes can be received.
    fn get_idleness_channel(&self) -> Receiver<SystemState>;

    /// Get a structure which will allow controlling the system's display server.
    fn get_controller(&self) -> Self::Controller;
}

/// Control for the system's display server
pub trait DisplayServerController: 'static + Send + Sync + Clone {
    /// Set the time of user's inactivity after which the display server should
    /// notify about user's idleness
    fn set_idleness_timeout(&self, timeout_in_seconds: i16) -> Result<()>;

    /// Get the time of inactivity after which the system is considered idle
    fn get_idleness_timeout(&self) -> Result<i16>;

    /// Force the system into active state, as if the user has just performed activity
    fn force_activity(&self) -> Result<()>;

    /// Get the system's support for DPMS
    fn is_dpms_capable(&self) -> Result<bool>;

    /// Get the power saving level of the system's screens.
    /// If DPMS is disabled, None is returned.
    fn get_dpms_level(&self) -> Result<Option<DPMSLevel>>;

    /// Set the power saving level of the system's screens
    fn set_dpms_level(&self, level: DPMSLevel) -> Result<()>;

    /// Enable or disable DPMS on the system's displays.
    /// To get the state, check the Option variant returned
    /// by [DisplayServerController::get_dpms_level]
    fn set_dpms_state(&self, enabled: bool) -> Result<()>;

    /// Get the timeouts after which the screen transitions into different DPMS levels
    fn get_dpms_timeouts(&self) -> Result<DPMSTimeouts>;

    /// Set the timeouts after which the screen transitions into different DPMS levels
    fn set_dpms_timeouts(&self, timeouts: DPMSTimeouts) -> Result<()>;
}
