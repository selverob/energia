//! Type definitions for implementation of Effectors.

use super::{ActorPort, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage {
    Execute,
    Rollback,
}

pub type EffectorPort = ActorPort<EffectorMessage, (), anyhow::Error>;
pub type EffectorRequest = Request<EffectorMessage, (), anyhow::Error>;
