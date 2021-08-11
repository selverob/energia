use actix::prelude::*;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "anyhow::Result<()>")]
pub struct Execute;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "anyhow::Result<()>")]
pub struct Rollback;

#[derive(Message, PartialEq, Eq)]
#[rtype(result = "anyhow::Result<()>")]
pub struct Stop;
