//! A framework for working with actor-based software systems loosely based on
//! the "Actor-based Runtime Model of Adaptable Feedback Control Loops" paper. 

mod actors;
mod effector;

pub mod test_controller;
pub mod test_sensor;

#[doc(inline)]
pub use actors::*;

//#[doc(inline)]
pub use effector::{EffectorMessage, EffectorPort};
