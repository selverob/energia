//! Turns the computer's screen on and off using DPMS

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
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;

pub struct DPMSEffector;

#[async_trait]
impl Effector for DPMSEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![Effect::new(
            "screen_off".to_owned(),
            vec![InhibitType::Idle],
            RollbackStrategy::OnActivity,
        )]
    }

    async fn spawn<B: BrightnessController, D: ds::DisplayServer>(
        &self,
        _: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort> {
        let actor = DPMSEffectorActor::new(provider.get_display_controller());
        spawn_server(actor).await
    }
}

pub struct DPMSEffectorActor<D: ds::DisplayServerController> {
    display_off: bool,
    ds_controller: D,
    original_configuration: ServerConfiguration,
}

impl<D: ds::DisplayServerController> DPMSEffectorActor<D> {
    pub fn new(ds_controller: D) -> DPMSEffectorActor<D> {
        DPMSEffectorActor {
            display_off: false,
            ds_controller,
            original_configuration: ServerConfiguration {
                level: Some(ds::DPMSLevel::On),
                timeouts: ds::DPMSTimeouts::new(0, 0, 0),
            },
        }
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
impl<D: ds::DisplayServerController> Server<EffectorMessage, usize> for DPMSEffectorActor<D> {
    fn get_name(&self) -> String {
        "DPMSEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<usize> {
        match payload {
            EffectorMessage::Execute => {
                self.set_dpms_level(ds::DPMSLevel::Off).await?;
                self.display_off = true;
                Ok(1)
            }
            EffectorMessage::Rollback => {
                self.set_dpms_level(ds::DPMSLevel::On).await?;
                self.display_off = false;
                Ok(0)
            }
            EffectorMessage::CurrentlyAppliedEffects => {
                if self.display_off {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
        }
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
