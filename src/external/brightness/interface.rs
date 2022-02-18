use anyhow::Result;
use async_trait::async_trait;

/// A trait allowing to set display brightness
#[async_trait]
pub trait BrightnessController: Send + Sync + Clone + 'static {
    /// Get the current display brightness
    async fn get_brightness(&self) -> Result<usize>;

    /// Set the current display brightness
    async fn set_brightness(&self, percentage: usize) -> Result<()>;
}
