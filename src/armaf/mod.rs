pub mod test_sensor;
pub mod test_controller;
pub mod actors;
pub mod effector;

pub use actors::*;

// pub enum ActorRequest {
//     Apply,
//     Rollback
// }

// pub trait ActorPort<P, R, E>
// where Self: Clone {
//     fn ask(r: Request<P, R, E>);
// }

// pub trait Effector<E: Error>
// where Self: ActorPort<ActorRequest, (), E> {}
