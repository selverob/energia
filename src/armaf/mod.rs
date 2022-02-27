//! A framework for working with actor-based software systems loosely based on
//! the "Actor-based Runtime Model of Adaptable Feedback Control Loops" paper.

mod actors;
mod effector;
mod ports;

#[doc(inline)]
pub use ports::*;

//#[doc(inline)]
pub use effector::*;

#[cfg(test)]
mod test_ports;

#[cfg(test)]
mod test_actors;
