use crate::armaf::{EffectorMessage, EffectorPort, Handle};

pub struct DBusController {
    path: Option<String>,
    lock_effector: EffectorPort,
}

impl DBusController {
    pub fn new(path: Option<&str>, lock_effector: EffectorPort) -> DBusController {
        DBusController {
            path: path.map(|s| s.to_owned()),
            lock_effector,
        }
    }

    pub async fn spawn(self) -> anyhow::Result<Handle> {
        let (handle, mut handle_child) = Handle::new();
        let path = self
            .path
            .clone()
            .unwrap_or("/org/energia/Manager".to_string());
        let connection = zbus::ConnectionBuilder::session()?
            .name("org.energia.Manager")?
            .serve_at(path.as_str(), self)?
            .build()
            .await?;

        log::debug!("Bound to D-Bus");
        tokio::spawn(async move {
            let moved_connection = connection;
            handle_child.should_terminate().await;
            if let Err(e) = moved_connection
                .object_server()
                .remove::<Self, String>(path)
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
