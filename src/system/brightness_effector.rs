use crate::{
    armaf::{
        spawn_server, Effect, Effector, EffectorMessage, EffectorPort, RollbackStrategy, Server,
    },
    external::{
        brightness::BrightnessController, dependency_provider::DependencyProvider,
        display_server as ds,
    },
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use logind_zbus::manager::InhibitType;

pub struct BrightnessEffector;

#[async_trait]
impl Effector for BrightnessEffector {
    fn get_effects(&self) -> Vec<Effect> {
        vec![Effect::new(
            "screen_dim".to_owned(),
            vec![InhibitType::Idle],
            RollbackStrategy::OnActivity,
        )]
    }

    async fn spawn<B: BrightnessController, D: ds::DisplayServer>(
        &self,
        _: Option<toml::Value>,
        provider: &mut DependencyProvider<B, D>,
    ) -> Result<EffectorPort> {
        let actor = BrightnessEffectorActor::new(provider.get_brightness_controller());
        spawn_server(actor).await
    }
}

pub struct BrightnessEffectorActor<B: BrightnessController> {
    brightness_controller: B,
    original_brightness: Option<usize>,
}

impl<B: BrightnessController> BrightnessEffectorActor<B> {
    pub fn new(brightness_controller: B) -> BrightnessEffectorActor<B> {
        BrightnessEffectorActor {
            brightness_controller,
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
}

#[async_trait]
impl<B: BrightnessController> Server<EffectorMessage, usize> for BrightnessEffectorActor<B> {
    fn get_name(&self) -> String {
        "BrightnessEffector".to_owned()
    }

    async fn handle_message(&mut self, payload: EffectorMessage) -> Result<usize> {
        match payload {
            EffectorMessage::Execute => {
                if self.original_brightness.is_some() {
                    return Err(anyhow!("Trying to dim an already dimmed display."));
                }
                self.original_brightness = Some(self.dim_screen().await?);
                Ok(1)
            }
            EffectorMessage::Rollback => {
                if let Some(b) = self.original_brightness {
                    self.brightness_controller.set_brightness(b).await?;
                } else {
                    return Err(anyhow!("Rollback called without previous dimming."));
                }
                self.original_brightness = None;
                Ok(0)
            }
            EffectorMessage::CurrentlyAppliedEffects => {
                if self.original_brightness.is_some() {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
        }
    }

    async fn tear_down(&mut self) -> Result<()> {
        if let Some(b) = self.original_brightness {
            self.brightness_controller.set_brightness(b).await?;
        }
        Ok(())
    }
}
