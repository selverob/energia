use anyhow::Result;
use async_trait::async_trait;

/// A trait allowing to set display brightness
#[async_trait]
pub trait BrightnessController {
    async fn get_brightness(&self) -> Result<usize>;
    async fn set_brightness(&self, percentage: usize) -> Result<()>;
}
