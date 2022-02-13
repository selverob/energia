use crate::armaf::{error_loop, ActorPort, EffectorMessage, EffectorPort, EffectorRequest};
use anyhow::Result;
use logind_zbus::{self, session::SessionProxy};
use std::process;
use tokio::sync::mpsc::Receiver;

pub enum DisplayEffect {
    Dim,
    TurnOff,
}
