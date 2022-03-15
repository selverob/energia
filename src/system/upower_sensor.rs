use anyhow::Result;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use upower_dbus::UPowerProxy;
use zbus::PropertyStream;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum PowerSource {
    Battery,
    External,
}

impl Into<PowerSource> for bool {
    fn into(self) -> PowerSource {
        if self {
            PowerSource::Battery
        } else {
            PowerSource::External
        }
    }
}

pub struct UPowerSensor {
    stream: PropertyStream<'static, bool>,
    updates_sender: watch::Sender<PowerSource>,
}

impl UPowerSensor {
    pub async fn new(system_connection: zbus::Connection) -> Result<watch::Receiver<PowerSource>> {
        let proxy = UPowerProxy::new(&system_connection).await?;
        let current_status = proxy.on_battery().await?.into();
        log::debug!(
            "Power source on spawn of UPowerSensor is {:?}",
            current_status
        );
        let stream = proxy.receive_on_battery_changed().await;
        let (updates_sender, updates_receiver) = watch::channel(current_status);
        let mut sensor = UPowerSensor {
            stream,
            updates_sender,
        };
        tokio::spawn(async move {
            sensor.run().await;
        });
        Ok(updates_receiver)
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                _ = self.updates_sender.closed() => {
                    log::info!("All receivers closed, terminating");
                    return;
                },
                Some(on_battery) = self.stream.next() => {
                    match on_battery.get().await {
                        Ok(value) => {
                            let power_source: PowerSource = value.into();
                            log::debug!("Power source change received. New power source: {:?}", power_source);
                            if let Err(e) = self.updates_sender.send(power_source) {
                                log::error!("Couldn't send power source change notification: {}", e);
                            }
                        },
                        Err(e) => {
                            log::error!("Fetching power source from change notification failed: {}", e);
                        }
                    }

                }
            }
        }
    }
}
