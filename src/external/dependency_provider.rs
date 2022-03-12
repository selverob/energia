use super::{
    brightness::{
        logind::LogindBrightnessController, mock::MockBrightnessController, BrightnessController,
    },
    dbus,
    display_server::{self, x11::X11Interface, DisplayServer, SystemState},
};
use anyhow::{anyhow, Result};
use tokio::sync::watch;

pub struct DependencyProvider<D: DisplayServer, B: BrightnessController> {
    dbus_factory: Option<dbus::ConnectionFactory>,
    display_server: D,
    brightness_controller: B,
}

impl<D: DisplayServer, B: BrightnessController> DependencyProvider<D, B> {
    pub fn new(
        dbus_factory: Option<dbus::ConnectionFactory>,
        display_server: D,
        brightness_controller: B,
    ) -> DependencyProvider<D, B> {
        DependencyProvider {
            dbus_factory,
            display_server,
            brightness_controller,
        }
    }

    pub async fn get_dbus_system_connection(&mut self) -> Result<zbus::Connection> {
        if let Some(factory) = self.dbus_factory.as_mut() {
            Ok(factory.get_system().await?)
        } else {
            Err(anyhow!(
                "No DBus connection factory in dependency DependencyProvider"
            ))
        }
    }

    pub async fn get_dbus_session_connection(&mut self) -> Result<zbus::Connection> {
        if let Some(factory) = self.dbus_factory.as_mut() {
            Ok(factory.get_session().await?)
        } else {
            Err(anyhow!(
                "No DBus connection factory in dependency DependencyProvider"
            ))
        }
    }

    pub fn get_idleness_channel(&self) -> watch::Receiver<SystemState> {
        self.display_server.get_idleness_channel()
    }

    pub fn get_display_controller(&self) -> D::Controller {
        self.display_server.get_controller()
    }

    pub fn get_brightness_controller(&self) -> B {
        self.brightness_controller.clone()
    }
}

impl DependencyProvider<X11Interface, LogindBrightnessController> {
    pub async fn make_system() -> Result<Self> {
        let mut dbus_factory = dbus::ConnectionFactory::new();
        let connection = dbus_factory.get_system().await?;
        let manager_proxy = logind_zbus::manager::ManagerProxy::new(&connection).await?;
        let path = manager_proxy.get_session_by_PID(std::process::id()).await?;
        let brightness_controller =
            LogindBrightnessController::new("intel_backlight", connection, path).await?;
        Ok(DependencyProvider::new(
            Some(dbus_factory),
            X11Interface::new(None)?,
            brightness_controller,
        ))
    }
}

impl DependencyProvider<display_server::mock::Interface, MockBrightnessController> {
    pub fn make_mock() -> Self {
        DependencyProvider::new(
            None,
            display_server::mock::Interface::new(60),
            MockBrightnessController::new(50),
        )
    }
}

#[cfg(test)]
mod test {
    use crate::external::display_server::DisplayServerController;

    use super::*;

    #[tokio::test]
    async fn test_mock() {
        let mut provider = DependencyProvider::make_mock();
        provider
            .get_dbus_session_connection()
            .await
            .expect_err("Dbus should not be present in mock provider");
        provider
            .get_dbus_system_connection()
            .await
            .expect_err("Dbus should not be present in mock provider");
        assert_eq!(
            provider
                .get_brightness_controller()
                .get_brightness()
                .await
                .unwrap(),
            50
        );
        assert_eq!(
            provider
                .get_display_controller()
                .get_idleness_timeout()
                .unwrap(),
            60
        );
        assert_eq!(
            *provider.get_idleness_channel().borrow(),
            SystemState::Awakened
        );
    }
}
