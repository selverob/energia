//! A framework for working with actor-based software systems loosely based on
//! the "Actor-based Runtime Model of Adaptable Feedback Control Loops" paper.

mod actors;
mod effector;

#[doc(inline)]
pub use actors::*;

//#[doc(inline)]
pub use effector::{EffectorMessage, EffectorPort};

#[cfg(test)]
mod test;
