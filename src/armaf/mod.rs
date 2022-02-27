//! A framework for working with actor-based software systems loosely based on
//! the "Actor-based Runtime Model of Adaptable Feedback Control Loops" paper.
//!
//! You will certainly use the [Request] and [ActorPort] types when interacting
//! with Actors. They are enough to write a simple actor system, but the
//! resulting code will be messy and will probably have bugs around some tricky
//! parts of actor lifecycle (handling initialization and teardown errors). Thus
//! we recommend that you use the [Actor] trait and [spawn_actor] function,
//! which allow you to write actors in a structured way.
//!
//! However, [Actor] is an async trait, which may lead to a small performance
//! penalty. It will probably be negligible for your use-case, but there is
//! still the option of working with [ActorPort]s directly.

mod actors;
mod effector;
mod ports;

#[doc(inline)]
pub use ports::*;

#[doc(inline)]
pub use actors::*;

//#[doc(inline)]
pub use effector::*;

#[cfg(test)]
mod test_ports;

#[cfg(test)]
mod test_actors;
