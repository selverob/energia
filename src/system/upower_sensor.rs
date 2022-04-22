//! Detects the computer's power source and battery percentage and notifies
//! other actors about changes to them

use anyhow::Result;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use upower_dbus::{DeviceProxy, UPowerProxy};
use zbus::PropertyStream;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum PowerStatus {
    Battery(u64),
    External,
}

impl PowerStatus {
    fn new(on_battery: bool, percentage: u64) -> PowerStatus {
        if on_battery {
            PowerStatus::Battery(percentage)
        } else {
            PowerStatus::External
        }
    }
}

pub struct UPowerSensor {
    battery_percentage: u64,
    on_battery: bool,

    source_stream: PropertyStream<'static, bool>,
    percentage_stream: PropertyStream<'static, f64>,
    updates_sender: watch::Sender<PowerStatus>,
}

impl UPowerSensor {
    pub async fn new(system_connection: zbus::Connection) -> Result<watch::Receiver<PowerStatus>> {
        let proxy = UPowerProxy::new(&system_connection).await?;
        let on_battery = proxy.on_battery().await?;
        let source_stream = proxy.receive_on_battery_changed().await;
        let display_device_proxy =
            Self::get_display_device_proxy(&system_connection, &proxy).await?;
        let percentage_stream = display_device_proxy.receive_percentage_changed().await;
        let battery_percentage = display_device_proxy.percentage().await? as u64;
        let init_value = PowerStatus::new(on_battery, battery_percentage);
        log::debug!("Power source on spawn of UPowerSensor is {:?}", init_value);
        let (updates_sender, updates_receiver) = watch::channel(init_value);
        let mut sensor = UPowerSensor {
            source_stream,
            battery_percentage,
            updates_sender,
            percentage_stream,
            on_battery,
        };
        tokio::spawn(async move {
            sensor.run().await;
        });
        Ok(updates_receiver)
    }

    async fn get_display_device_proxy(
        connection: &zbus::Connection,
        proxy: &UPowerProxy<'_>,
    ) -> Result<DeviceProxy<'static>> {
        let path = proxy.get_display_device().await?;
        Ok(DeviceProxy::builder(connection).path(path)?.build().await?)
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                _ = self.updates_sender.closed() => {
                    log::info!("All receivers closed, terminating");
                    return;
                },
                Some(received_on_battery) = self.source_stream.next() => {
                    match received_on_battery.get().await {
                        Ok(value) => {
                            self.on_battery = value;
                            self.update_sender();
                        },
                        Err(e) => {
                            log::error!("Fetching power source from change notification failed: {}", e);
                        }
                    };
                },
                Some(received) = self.percentage_stream.next() => {
                    match received.get().await {
                        Ok(percentage) => {
                            self.battery_percentage = percentage as u64;
                            if self.on_battery {
                                self.update_sender();
                            }
                        },
                        Err(e) => {
                            log::error!("Fetching percentage from change notification failed: {}", e);
                        }
                    }
                }
            }
        }
    }

    fn update_sender(&self) {
        let status = PowerStatus::new(self.on_battery, self.battery_percentage);
        log::debug!("Updating power status: {:?}", status);
        if let Err(e) = self.updates_sender.send(status) {
            log::error!("Couldn't send power source change notification: {}", e);
        }
    }
}
