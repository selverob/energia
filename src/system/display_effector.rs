use std::marker::PhantomData;

use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::{
        brightness::BrightnessController,
        dependency_provider::DependencyProvider,
        display_server::{self as ds, DisplayServerController},
    },
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;

pub struct DisplayEffector;

#[async_trait]
impl Effector for DisplayEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![
            Effect::new(
                "screen_dim".to_owned(),
                vec![InhibitType::Idle],
                RollbackStrategy::OnActivity,
            ),
            Effect::new(
                "screen_off".to_owned(),
                vec![InhibitType::Idle],
                RollbackStrategy::OnActivity,
            ),
        ]
    }

    async fn spawn<B: BrightnessController, D: ds::DisplayServer>(
        self,
        _: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort> {
        let actor = DisplayEffectorActor::new(
            provider.get_brightness_controller(),
            provider.get_display_controller(),
        );
        spawn_server(actor).await
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DisplayState {
    On,
    Dimmed,
    Off,
}

pub struct DisplayEffectorActor<B: BrightnessController, D: ds::DisplayServerController> {
    current_state: DisplayState,
    brightness_controller: B,
    ds_controller: D,
    original_configuration: ServerConfiguration,
    original_brightness: Option<usize>,
}

impl<B: BrightnessController, D: ds::DisplayServerController> DisplayEffectorActor<B, D> {
    pub fn new(brightness_controller: B, ds_controller: D) -> DisplayEffectorActor<B, D> {
        DisplayEffectorActor {
            current_state: DisplayState::On,
            brightness_controller,
            ds_controller,
            original_configuration: ServerConfiguration {
                level: Some(ds::DPMSLevel::On),
                timeouts: ds::DPMSTimeouts::new(0, 0, 0),
            },
            original_brightness: None,
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
impl<B: BrightnessController, D: ds::DisplayServerController> Server<EffectorMessage, ()>
    for DisplayEffectorActor<B, D>
{
    fn get_name(&self) -> String {
        "DisplayEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<()> {
        match (self.current_state, payload) {
            (DisplayState::On, EffectorMessage::Execute) => {
                self.original_brightness = Some(self.dim_screen().await?);
                self.current_state = DisplayState::Dimmed;
            }
            (DisplayState::On, EffectorMessage::Rollback) => {
                return Err(anyhow!("Unmatched Rollback called on DisplayEffector"));
            }
            (DisplayState::Dimmed, EffectorMessage::Execute) => {
                self.set_dpms_level(ds::DPMSLevel::Off).await?;
                self.current_state = DisplayState::Off;
            }
            (DisplayState::Dimmed, EffectorMessage::Rollback) => {
                if let Some(b) = self.original_brightness {
                    self.brightness_controller.set_brightness(b).await?;
                } else {
                    return Err(anyhow!(
                        "Brightness rollback called without previous dimming."
                    ));
                }
                self.original_brightness = None;
                self.current_state = DisplayState::On;
            }
            (DisplayState::Off, EffectorMessage::Execute) => {
                return Err(anyhow!("Unmatched Execute called on DisplayEffector"));
            }
            (DisplayState::Off, EffectorMessage::Rollback) => {
                self.set_dpms_level(ds::DPMSLevel::On).await?;
                self.current_state = DisplayState::Dimmed;
            }
        }
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
        if let Some(b) = self.original_brightness {
            self.brightness_controller.set_brightness(b).await?;
        }
        Ok(())
    }
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
