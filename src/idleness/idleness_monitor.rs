use anyhow::Result;
use crossbeam_channel::Receiver;

pub trait IdlenessMonitor {
    fn get_idleness_channel(&self) -> Receiver<()>;
    fn set_idleness_timeout(&mut self, timeout_in_seconds: i16) -> Result<()>;
}
