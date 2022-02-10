use super::{DisplayServerInterface, IdlenessController, SystemState};
use anyhow::Result;
use std::io::{Error, ErrorKind};
use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

/// A mock [DisplayServerInterface], usable for testing
pub struct Interface {
    timeout: Arc<Mutex<Cell<i16>>>,
    should_fail: Arc<Mutex<Cell<bool>>>,
    sender: watch::Sender<SystemState>,
    receiver: watch::Receiver<SystemState>,
}

impl Interface {
    pub fn new(timeout: i16) -> Interface {
        let (sender, receiver) = watch::channel(SystemState::Awakened);
        Interface {
            timeout: Arc::new(Mutex::new(Cell::new(timeout))),
            should_fail: Arc::new(Mutex::new(Cell::new(false))),
            sender,
            receiver,
        }
    }

    pub fn set_failure_mode(&self, fail: bool) {
        self.should_fail.lock().unwrap().set(fail);
    }

    pub fn notify_state_transition(&self, new_state: SystemState) -> Result<()> {
        Ok(self.sender.send(new_state)?)
    }
}

impl DisplayServerInterface for Interface {
    type Controller = Controller;

    fn get_idleness_channel(&self) -> watch::Receiver<SystemState> {
        self.receiver.clone()
    }

    fn get_idleness_controller(&self) -> Self::Controller {
        Controller {
            timeout: self.timeout.clone(),
            should_fail: self.should_fail.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Controller {
    timeout: Arc<Mutex<Cell<i16>>>,
    should_fail: Arc<Mutex<Cell<bool>>>,
}

impl IdlenessController for Controller {
    fn set_idleness_timeout(&self, timeout_in_seconds: i16) -> Result<()> {
        if self.should_fail.lock().unwrap().get() {
            Err(anyhow::Error::new(Error::new(
                ErrorKind::Other,
                "Mock failure",
            )))
        } else {
            Ok(self.timeout.lock().unwrap().set(timeout_in_seconds))
        }
    }

    fn get_idleness_timeout(&self) -> Result<i16> {
        if self.should_fail.lock().unwrap().get() {
            Err(anyhow::Error::new(Error::new(
                ErrorKind::Other,
                "Mock failure",
            )))
        } else {
            Ok(self.timeout.lock().unwrap().get())
        }
    }
}