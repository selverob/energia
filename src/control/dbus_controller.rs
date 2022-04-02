use crate::armaf::{EffectorMessage, EffectorPort, Handle};

#[derive(Debug, Clone, Copy)]
pub enum BusType {
    Session,
    System,
}

pub struct DBusController {
    path: String,
    name: String,
    bus_type: BusType,
    lock_effector: EffectorPort,
}

impl DBusController {
    pub fn new(
        path: &str,
        name: &str,
        bus_type: BusType,
        lock_effector: EffectorPort,
    ) -> DBusController {
        DBusController {
            path: path.to_string(),
            name: name.to_string(),
            bus_type,
            lock_effector,
        }
    }

    pub async fn spawn(self) -> anyhow::Result<Handle> {
        let (handle, mut handle_child) = Handle::new();
        let builder = match self.bus_type {
            BusType::System => zbus::ConnectionBuilder::system()?,
            BusType::Session => zbus::ConnectionBuilder::session()?,
        };
        let moved_path = self.path.clone();
        let connection = builder
            .name(self.name.clone().as_str())?
            .serve_at(moved_path.as_str(), self)?
            .build()
            .await?;

        log::debug!("Bound to D-Bus");
        tokio::spawn(async move {
            let moved_connection = connection;
            handle_child.should_terminate().await;
            if let Err(e) = moved_connection
                .object_server()
                .remove::<Self, String>(moved_path)
                .await
            {
                log::error!("Failed to unregister server: {}", e);
            }
            log::debug!("Terminated");
        });
        Ok(handle)
    }
}

#[zbus::dbus_interface(name = "org.energia.Manager")]
impl DBusController {
    async fn lock(&self) -> zbus::fdo::Result<()> {
        log::info!("Locking system");
        if let Err(e) = self.lock_effector.request(EffectorMessage::Execute).await {
            Err(zbus::fdo::Error::Failed(format!("{}", e)))
        } else {
            Ok(())
        }
    }
}
