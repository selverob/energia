use crate::armaf::{
    error_loop, ActorPort, EffectorMessage, EffectorPort, EffectorRequest, Request,
};
use crate::external::brightness::BrightnessController;
use crate::external::display_server::{self as ds, DisplayServerController};
use anyhow::Result;
use logind_zbus::{self, session::SessionProxy};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReadDirStream;
use tokio_stream::StreamExt;

pub enum DisplayEffect {
    Dim,
    TurnOff,
}

/// Stores display server configuration, so that it can be restored once the
/// actor terminates
#[derive(Clone, Copy)]
struct ServerConfiguration {
    level: Option<ds::DPMSLevel>,
    timeouts: ds::DPMSTimeouts,
}

impl ServerConfiguration {
    async fn fetch<C: DisplayServerController>(controller: &C) -> Result<ServerConfiguration> {
        let level_controller = controller.clone();
        let level_handle = tokio::task::spawn_blocking(move || level_controller.get_dpms_level());

        let timeouts_controller = controller.clone();
        let timeouts_handle =
            tokio::task::spawn_blocking(move || timeouts_controller.get_dpms_timeouts());

        Ok(ServerConfiguration {
            level: level_handle.await??,
            timeouts: timeouts_handle.await??,
        })
    }

    async fn apply<C: ds::DisplayServerController>(self, controller: &C) -> Result<()> {
        let level_controller = controller.clone();
        let level_handle = if let Some(level) = self.level {
            tokio::task::spawn_blocking(move || level_controller.set_dpms_level(level))
        } else {
            tokio::task::spawn_blocking(move || level_controller.set_dpms_state(false))
        };

        let timeouts_controller = controller.clone();
        let timeouts_handle = tokio::task::spawn_blocking(move || {
            timeouts_controller.set_dpms_timeouts(self.timeouts)
        });

        level_handle.await??; // Not exactly the most elegant error handling, but eh. If this fails, it's not a catastrophe, more like a bit annoying.
        Ok(timeouts_handle.await??)
    }
}

fn spawn<B: BrightnessController, D: ds::DisplayServerController>(
    brightness_controller: B,
    ds_controller: D,
) -> EffectorPort<DisplayEffect> {
    let (port, rx) = ActorPort::make();
    tokio::spawn(async move {
        log::debug!("Obtaining original display server settings");
        let original_configuration = match ServerConfiguration::fetch(&ds_controller).await {
            Ok(configuration) => Some(configuration),
            Err(e) => {
                log::error!("Failed to obtain original display server settings: {}", e);
                None
            }
        };
        processing_loop(rx, &brightness_controller, &ds_controller).await;

        if let Some(config) = original_configuration {
            log::debug!("Rolling back display configuration");
            if let Err(e) = config.apply(&ds_controller).await {
                log::error!("Error when rolling back display configuration: {}", e);
            }
        }
    });
    port
}

async fn processing_loop<B: BrightnessController, D: ds::DisplayServerController>(
    mut rx: Receiver<EffectorRequest<DisplayEffect>>,
    brightness_controller: &B,
    ds_controller: &D,
) {
    log::info!("Started");
    let mut old_brightness: Option<usize> = None;
    loop {
        match rx.recv().await {
            Some(req) => match req.payload {
                EffectorMessage::Execute(DisplayEffect::Dim) => {
                    match dim_screen(brightness_controller).await {
                        Ok(b) => {
                            old_brightness = Some(b);
                            req.respond(Ok(())).unwrap();
                        }
                        Err(err) => req.respond(Err(err)).unwrap(),
                    }
                }
                EffectorMessage::Execute(DisplayEffect::TurnOff) => {
                    req.respond(set_dpms_level(ds::DPMSLevel::Off, ds_controller).await)
                        .unwrap();
                }
                EffectorMessage::Rollback(DisplayEffect::Dim) => {
                    if let Some(b) = old_brightness {
                        req.respond(brightness_controller.set_brightness(b).await.and(Ok(())))
                            .unwrap();
                    } else {
                        log::error!("Brightness rollback called without previous dimming.");
                        req.respond(Ok(())).unwrap();
                    }
                }
                EffectorMessage::Rollback(DisplayEffect::TurnOff) => {
                    req.respond(set_dpms_level(ds::DPMSLevel::On, ds_controller).await)
                        .unwrap();
                }
            },
            None => {
                log::info!("Terminating");
                return;
            }
        }
    }
}

async fn dim_screen<B: BrightnessController>(brightness_controller: &B) -> Result<usize> {
    let current_brightness = brightness_controller.get_brightness().await?;
    brightness_controller
        .set_brightness(current_brightness / 2)
        .await?;
    Ok(current_brightness)
}

async fn set_dpms_level<D: DisplayServerController>(
    level: ds::DPMSLevel,
    ds_controller: &D,
) -> Result<()> {
    let sent_controller = ds_controller.clone();
    tokio::task::spawn_blocking(move || sent_controller.set_dpms_level(level)).await?
}
