use super::ActorPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectorMessage<T> {
    Execute(T),
    Rollback,
}

pub type EffectorPort<T> = ActorPort<EffectorMessage<T>, (), ()>;
