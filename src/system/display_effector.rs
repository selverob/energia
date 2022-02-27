use crate::armaf::{Actor, EffectorMessage};
use crate::external::brightness::BrightnessController;
use crate::external::display_server::{self as ds, DisplayServerController};
use anyhow::{anyhow, Result};
use async_trait::async_trait;

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
            tokio::task::spawn_blocking(move || -> Result<()> {
                level_controller.set_dpms_state(true)?;
                level_controller.set_dpms_level(level)?;
                Ok(())
            })
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

pub struct DisplayEffector<B: BrightnessController, D: ds::DisplayServerController> {
    brightness_controller: B,
    ds_controller: D,
    original_configuration: ServerConfiguration,
    previous_brightness: Option<usize>,
}

impl<B: BrightnessController, D: ds::DisplayServerController> DisplayEffector<B, D> {
    pub fn new(brightness_controller: B, ds_controller: D) -> DisplayEffector<B, D> {
        DisplayEffector {
            brightness_controller,
            ds_controller,
            original_configuration: ServerConfiguration {
                level: Some(ds::DPMSLevel::On),
                timeouts: ds::DPMSTimeouts::new(0, 0, 0),
            },
            previous_brightness: None,
        }
    }

    async fn dim_screen(&self) -> Result<usize> {
        let current_brightness = self.brightness_controller.get_brightness().await?;
        self.brightness_controller
            .set_brightness(current_brightness / 2)
            .await?;
        Ok(current_brightness)
    }

    async fn set_dpms_level(&self, level: ds::DPMSLevel) -> Result<()> {
        let sent_controller = self.ds_controller.clone();
        tokio::task::spawn_blocking(move || sent_controller.set_dpms_level(level)).await?
    }

    async fn prepare_dpms(&self) {
        let config = ServerConfiguration {
            level: Some(ds::DPMSLevel::On),
            timeouts: ds::DPMSTimeouts::new(0, 0, 0),
        };
        if let Err(e) = config.apply(&self.ds_controller).await {
            log::error!("Couldn't prepare DPMS for display effector: {}", e);
        }
    }
}

#[async_trait]
impl<B: BrightnessController, D: ds::DisplayServerController>
    Actor<EffectorMessage<DisplayEffect>, ()> for DisplayEffector<B, D>
{
    fn get_name(&self) -> String {
        "DisplayEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage<DisplayEffect>) -> Result<()> {
        match payload {
            EffectorMessage::Execute(DisplayEffect::Dim) => {
                self.previous_brightness = Some(self.dim_screen().await?);
            }
            EffectorMessage::Execute(DisplayEffect::TurnOff) => {
                self.set_dpms_level(ds::DPMSLevel::Off).await?;
            }
            EffectorMessage::Rollback(DisplayEffect::Dim) => {
                if let Some(b) = self.previous_brightness {
                    self.brightness_controller.set_brightness(b).await?;
                } else {
                    return Err(anyhow!(
                        "Brightness rollback called without previous dimming."
                    ));
                }
                self.previous_brightness = None;
            }
            EffectorMessage::Rollback(DisplayEffect::TurnOff) => {
                self.set_dpms_level(ds::DPMSLevel::On).await?
            }
        };
        Ok(())
    }

    async fn initialize(&mut self) -> Result<()> {
        self.original_configuration = ServerConfiguration::fetch(&self.ds_controller).await?;
        self.prepare_dpms().await;
        Ok(())
    }

    async fn tear_down(&mut self) -> Result<()> {
        self.original_configuration
            .apply(&self.ds_controller)
            .await?;
        if let Some(b) = self.previous_brightness {
            self.brightness_controller.set_brightness(b).await?;
        }
        Ok(())
    }
}
