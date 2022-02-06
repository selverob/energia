use super::ActorPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage {
    Execute,
    Rollback,
}

pub type EffectorPort = ActorPort<EffectorMessage, (), ()>;
