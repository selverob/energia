use anyhow::Result;
use crossbeam_channel::Receiver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    Idle,
    Awakened,
}

pub trait IdlenessMonitor {
    fn get_idleness_channel(&self) -> Receiver<SystemState>;
    fn set_idleness_timeout(&mut self, timeout_in_seconds: i16) -> Result<()>;
}
