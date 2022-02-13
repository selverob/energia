use crate::armaf::{ActorPort, EffectorMessage, EffectorPort, EffectorRequest};
use anyhow::Result;
use logind_zbus::{self, session::SessionProxy};
use std::process;
use tokio::sync::mpsc::Receiver;

pub enum LogindEffect {
    IdleHint,
    LockedHint,
}

pub fn spawn(connection: zbus::Connection) -> EffectorPort<LogindEffect> {
    let (port, rx) = ActorPort::make();
    tokio::spawn(async move {
        log::debug!("Obtaining current session");
        match get_session_proxy(&connection).await {
            Ok(proxy) => {
                log::info!("Started");
                processing_loop(rx, proxy).await;
            }
            Err(error) => {
                log::error!("Couldn't create a logind session proxy: {}", error);
                error_loop(rx).await;
            }
        }
    });
    port
}

async fn get_session_proxy<'c>(connection: &'c zbus::Connection) -> Result<SessionProxy<'c>> {
    let manager_proxy = logind_zbus::manager::ManagerProxy::new(&connection).await?;
    let path = manager_proxy.get_session_by_PID(process::id()).await?;
    Ok(SessionProxy::builder(connection)
        .path(path)?
        .build()
        .await?)
}

async fn processing_loop<'c>(
    mut rx: Receiver<EffectorRequest<LogindEffect>>,
    session_proxy: SessionProxy<'c>,
) {
    loop {
        let option_req = rx.recv().await;
        if option_req.is_none() {
            log::info!("Stopping");
            return;
        }
        let req = option_req.unwrap();
        let response = process_message(&session_proxy, &req.payload).await;
        req.respond(response).unwrap();
    }
}

async fn process_message<'c>(
    session_proxy: &SessionProxy<'c>,
    message: &EffectorMessage<LogindEffect>,
) -> Result<()> {
    let (effect, argument) = match message {
        EffectorMessage::Execute(a) => (a, true),
        EffectorMessage::Rollback(a) => (a, false),
    };
    match effect {
        LogindEffect::IdleHint => {
            log::info!("Setting idle hint in logind to {}", argument);
            // TODO: It seems like sometimes the changes are not immediately
            // visible to reading methods. Should we maybe try to wait until
            // they change?
            Ok(session_proxy.set_idle_hint(argument).await?)
        }
        LogindEffect::LockedHint => {
            log::info!("Setting locked hint in logind to {}", argument);
            // TODO: It seems like sometimes the changes are not immediately
            // visible to reading methods. Should we maybe try to wait until
            // they change?
            Ok(session_proxy.set_locked_hint(argument).await?)
        }
    }
}

async fn error_loop(mut rx: Receiver<EffectorRequest<LogindEffect>>) {
    loop {
        match rx.recv().await {
            None => {
                log::info!("Stopping");
                return;
            }
            Some(req) => {
                req.respond(Err(anyhow::format_err!(
                    "Logind effector couldn't find session, is dead."
                )))
                .unwrap();
            }
        }
    }
}
