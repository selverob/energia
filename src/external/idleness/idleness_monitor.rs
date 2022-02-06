use anyhow::Result;
use tokio::sync::mpsc::Receiver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    Idle,
    Awakened,
}

pub trait IdlenessMonitor {
    fn get_idleness_channel(&mut self) -> &mut Receiver<SystemState>;
    fn set_idleness_timeout(&mut self, timeout_in_seconds: i16) -> Result<()>;
}
