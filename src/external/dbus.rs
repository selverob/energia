use log::info;
use zbus;

/// Handles initialization and cloning of [zbus::Connection]s. These are
/// clone-able and handle their own refcounts internally. This struct will
/// either create or provide clones of connections.
pub struct ConnectionFactory {
    system: Option<zbus::Connection>,
    session: Option<zbus::Connection>,
}

impl ConnectionFactory {
    /// Create a new ConnectionFactory.
    ///
    /// No connections are created upon calling this method.
    fn new() -> ConnectionFactory {
        ConnectionFactory {
            system: None,
            session: None,
        }
    }

    /// Get a connection to the system-wide D-Bus
    async fn get_system(&mut self) -> zbus::Result<zbus::Connection> {
        if let Some(c) = &self.system {
            Ok(c.clone())
        } else {
            info!("Creating a new connection to the system bus");
            self.system = Some(zbus::Connection::system().await?);
            Ok(self.system.as_ref().unwrap().clone())
        }
    }

    /// Get a connection to the session's / user's D-Bus
    async fn get_session(&mut self) -> zbus::Result<zbus::Connection> {
        if let Some(c) = &self.session {
            Ok(c.clone())
        } else {
            info!("Creating a new connection to the session bus");
            self.session = Some(zbus::Connection::session().await?);
            Ok(self.session.as_ref().unwrap().clone())
        }
    }
}

#[cfg(test)]
mod test {
    use super::ConnectionFactory;
    use anyhow::Result;
    use zbus::fdo::{self, DBusProxy};
    use zbus::{self, Connection};

    #[tokio::test]
    async fn test_session() -> Result<()> {
        let mut factory = ConnectionFactory::new();
        let session_1 = factory.get_session().await?;
        let session_2 = factory.get_session().await?;
        assert_eq!(get_bus_id(session_1).await?, get_bus_id(session_2).await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_system() -> Result<()> {
        let mut factory = ConnectionFactory::new();
        let system_1 = factory.get_system().await?;
        let system_2 = factory.get_system().await?;
        assert_eq!(get_bus_id(system_1).await?, get_bus_id(system_2).await?);
        Ok(())
    }

    async fn get_bus_id(c: Connection) -> fdo::Result<String> {
        let proxy = DBusProxy::builder(&c)
            .destination("org.freedesktop.DBus")?
            .path("/org/freedesktop/DBus")?
            .build()
            .await?;
        Ok(proxy.get_id().await?)
    }
}
