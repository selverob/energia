use tokio::sync::{broadcast, mpsc};

use crate::{
    armaf,
    external::display_server::DisplayServerController,
    system::sleep_sensor::{ReadyToSleep, SleepUpdate},
};

pub struct SleepController<C: DisplayServerController> {
    sleep_channel: broadcast::Receiver<SleepUpdate>,
    lock_effector: Option<armaf::EffectorPort>,
    ds_controller: C,
    handle_child: Option<armaf::HandleChild>,
}

impl<C: DisplayServerController> SleepController<C> {
    pub fn new(
        sleep_channel: broadcast::Receiver<SleepUpdate>,
        lock_effector: Option<armaf::EffectorPort>,
        ds_controller: C,
    ) -> SleepController<C> {
        SleepController {
            sleep_channel,
            lock_effector,
            ds_controller,
            handle_child: None,
        }
    }

    pub async fn spawn(mut self) -> armaf::Handle {
        let (handle, handle_child) = armaf::Handle::new();
        self.handle_child = Some(handle_child);

        tokio::spawn(async move {
            self.main_loop().await;
        });

        handle
    }

    async fn main_loop(&mut self) {
        loop {
            tokio::select! {
                _ = self.handle_child.as_mut().unwrap().should_terminate() => {
                    return;
                }
                update = self.sleep_channel.recv() => {
                    match update {
                        Err(e) => {
                            log::error!("Sleep sensor receive error: {}", e);
                            return;
                        }
                        Ok(SleepUpdate::WokenUp) => {
                            self.force_activity().await;
                        }
                        Ok(SleepUpdate::GoingToSleep(ack_channel)) => {
                            self.handle_sleep(ack_channel).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle_sleep(&mut self, ack_channel: mpsc::Sender<ReadyToSleep>) {
        if let Some(ref effector) = self.lock_effector {
            if let Err(e) = effector.request(armaf::EffectorMessage::Execute).await {
                log::error!("Failed to lock system before going to sleep: {}", e);
            }
        }
        if let Err(e) = ack_channel.send(ReadyToSleep).await {
            log::error!("Acknowledging sleep readiness failed: {}", e);
        }
    }

    async fn force_activity(&mut self) {
        let sent_controller = self.ds_controller.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || sent_controller.force_activity()).await
        {
            log::error!("Couldn't force activate display server: {}", e);
        }
    }
}
