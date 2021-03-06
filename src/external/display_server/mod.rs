//! Implements APIs for interacting with display servers

mod interface;

pub use interface::*;

pub mod mock;
pub mod x11;

#[cfg(test)]
mod test;
