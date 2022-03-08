use super::{DisplayServer, DisplayServerController, SystemState};
use anyhow::Result;
use std::io::{Error, ErrorKind};
use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

struct SharedState {
    timeout: i16,
    should_fail: bool,
    dpms_enabled: bool,
    dpms_level: super::DPMSLevel,
    dpms_timeouts: super::DPMSTimeouts,
    sender: watch::Sender<SystemState>,
}

/// A mock [DisplayServer], usable for testing
pub struct Interface {
    receiver: watch::Receiver<SystemState>,
    shared_state: Arc<Mutex<RefCell<SharedState>>>,
}

impl Interface {
    pub fn new(timeout: i16) -> Interface {
        let (sender, receiver) = watch::channel(SystemState::Awakened);
        Interface {
            shared_state: Arc::new(Mutex::new(RefCell::new(SharedState {
                timeout: timeout,
                should_fail: false,
                dpms_enabled: true,
                dpms_level: super::DPMSLevel::On,
                dpms_timeouts: super::DPMSTimeouts::new(10, 20, 30),
                sender,
            }))),
            receiver,
        }
    }

    pub fn set_failure_mode(&self, fail: bool) {
        self.shared_state.lock().unwrap().borrow_mut().should_fail = fail;
    }

    pub fn notify_state_transition(&self, new_state: SystemState) -> Result<()> {
        Ok(self
            .shared_state
            .lock()
            .unwrap()
            .borrow_mut()
            .sender
            .send(new_state)?)
    }
}

impl DisplayServer for Interface {
    type Controller = Controller;

    fn get_idleness_channel(&self) -> watch::Receiver<SystemState> {
        self.receiver.clone()
    }

    fn get_controller(&self) -> Self::Controller {
        Controller {
            state: self.shared_state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Controller {
    state: Arc<Mutex<RefCell<SharedState>>>,
}

impl DisplayServerController for Controller {
    fn set_idleness_timeout(&self, timeout_in_seconds: i16) -> Result<()> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            Ok(self.state.lock().unwrap().borrow_mut().timeout = timeout_in_seconds)
        }
    }

    fn get_idleness_timeout(&self) -> Result<i16> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            Ok(self.state.lock().unwrap().borrow_mut().timeout)
        }
    }

    fn force_activity(&self) -> Result<()> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            Ok(self
                .state
                .lock()
                .unwrap()
                .borrow_mut()
                .sender
                .send(SystemState::Awakened)?)
        }
    }

    fn is_dpms_capable(&self) -> Result<bool> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            Ok(true)
        }
    }

    fn get_dpms_level(&self) -> Result<Option<super::DPMSLevel>> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else if self.state.lock().unwrap().borrow_mut().dpms_enabled {
            Ok(Some(self.state.lock().unwrap().borrow_mut().dpms_level))
        } else {
            Ok(None)
        }
    }

    fn set_dpms_level(&self, level: super::DPMSLevel) -> Result<()> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            self.state.lock().unwrap().borrow_mut().dpms_level = level;
            Ok(())
        }
    }

    fn set_dpms_state(&self, enabled: bool) -> Result<()> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            self.state.lock().unwrap().borrow_mut().dpms_enabled = enabled;
            Ok(())
        }
    }

    fn get_dpms_timeouts(&self) -> Result<super::DPMSTimeouts> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            Ok(self.state.lock().unwrap().borrow_mut().dpms_timeouts)
        }
    }

    fn set_dpms_timeouts(&self, timeouts: super::DPMSTimeouts) -> Result<()> {
        if self.state.lock().unwrap().borrow_mut().should_fail {
            Err(make_error())
        } else {
            self.state.lock().unwrap().borrow_mut().dpms_timeouts = timeouts;
            Ok(())
        }
    }
}

fn make_error() -> anyhow::Error {
    anyhow::Error::new(Error::new(ErrorKind::Other, "Mock failure"))
}
