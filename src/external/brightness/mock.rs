use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use async_trait::async_trait;

use super::BrightnessController;

/// A mock [BrightnessController], usable when testing the actors using the trait.
#[derive(Clone)]
pub struct MockBrightnessController {
    percentage: Arc<Mutex<Cell<usize>>>,
    should_fail: bool,
}

impl MockBrightnessController {
    /// Create a new controller, with the specified initial brightness
    pub fn new(initial_brightness: usize) -> MockBrightnessController {
        MockBrightnessController {
            percentage: Arc::new(Mutex::new(Cell::new(initial_brightness))),
            should_fail: false,
        }
    }

    /// Set whether operations on this controller should return an error or not
    pub fn set_failure_mode(&mut self, should_fail: bool) {
        self.should_fail = should_fail;
    }
}

#[async_trait]
impl BrightnessController for MockBrightnessController {
    async fn get_brightness(&self) -> Result<usize> {
        if self.should_fail {
            Err(anyhow::anyhow!("Mock BrightnessController is failing"))
        } else {
            Ok(self.percentage.lock().unwrap().get())
        }
    }
    async fn set_brightness(&self, percentage: usize) -> Result<()> {
        if percentage > 100 {
            return Err(anyhow::anyhow!("Cannot set brightness higher than 100%"));
        }
        if self.should_fail {
            return Err(anyhow::anyhow!("Mock BrightnessController is failing"));
        }
        self.percentage.lock().unwrap().set(percentage);
        Ok(())
    }
}
