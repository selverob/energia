use super::BrightnessController;
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::session::SessionProxy;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;
use zbus;
use zbus::zvariant::ObjectPath;

/// A [BrightnessController] which uses the kernel's /sys/class/backlight device
/// class to control the display brightness.
///
/// The brightness is read directly from the filesystem but writing is mediated
/// via logind Session's SetBrightness method, to allow root-less brightness
/// setting.
#[derive(Debug, Clone)]
pub struct LogindBrightnessController<'a> {
    device: String,
    device_path: String,
    max_brightness: usize,
    proxy: SessionProxy<'a>,
}

impl<'a> LogindBrightnessController<'a> {
    /// Create a new controller which will set the brightness on the device
    /// under /sys/class/backlight/{device}.
    pub async fn new(
        device: &str,
        connection: zbus::Connection,
        session_path: ObjectPath<'a>,
    ) -> Result<LogindBrightnessController<'a>> {
        let proxy = SessionProxy::builder(&connection)
            .path(session_path)?
            .build()
            .await?;

        let device_path = format!("/sys/class/backlight/{}", device);
        let max_brightness =
            read_number_from_file(format!("{}/{}", device_path, "max_brightness")).await?;
        Ok(LogindBrightnessController {
            device: device.to_string(),
            device_path,
            max_brightness,
            proxy,
        })
    }
}

#[async_trait]
impl BrightnessController for LogindBrightnessController<'_> {
    async fn get_brightness(&self) -> Result<usize> {
        let raw_brightness =
            read_number_from_file(&format!("{}/{}", self.device_path, "brightness")).await?;
        Ok(((raw_brightness as f64 / self.max_brightness as f64) * 100 as f64) as usize)
    }
    async fn set_brightness(&self, percentage: usize) -> Result<()> {
        if percentage > 100 {
            return Err(anyhow::anyhow!("Cannot set brightness higher than 100%"));
        }
        let resulting_brightness =
            (self.max_brightness as f64 * (percentage as f64 / 100.0)) as u32;
        Ok(self
            .proxy
            .set_brightness("backlight", &self.device, resulting_brightness)
            .await?)
    }
}

async fn read_number_from_file(path: impl AsRef<Path>) -> Result<usize> {
    let mut f = fs::File::open(path).await?;
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;
    Ok(contents.trim().parse()?)
}
