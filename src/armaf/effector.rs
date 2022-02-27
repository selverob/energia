//! Type definitions for implementation of Effectors.

use super::{ActorPort, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage<T> {
    Execute(T),
    Rollback(T),
}

pub type EffectorPort<T> = ActorPort<EffectorMessage<T>, (), anyhow::Error>;
pub type EffectorRequest<T> = Request<EffectorMessage<T>, (), anyhow::Error>;
