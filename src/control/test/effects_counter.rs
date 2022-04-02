use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};

use crate::armaf::{ActorPort, EffectorPort};

pub struct EffectsCounter {
    running_effects: Arc<Mutex<Cell<isize>>>,
    port: EffectorPort,
}

impl EffectsCounter {
    pub fn new() -> EffectsCounter {
        let our_running_effects = Arc::new(Mutex::new(Cell::new(0)));
        let running_effects = our_running_effects.clone();

        let (port, mut rx) = ActorPort::make();

        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let delta = match req.payload {
                    crate::armaf::EffectorMessage::Execute => 1,
                    crate::armaf::EffectorMessage::Rollback => -1,
                    crate::armaf::EffectorMessage::CurrentlyAppliedEffects => 0,
                };
                *running_effects.lock().unwrap().get_mut() += delta;
                req.respond(Ok(running_effects.lock().unwrap().get() as usize))
                    .unwrap();
            }
        });

        EffectsCounter {
            running_effects: our_running_effects,
            port,
        }
    }

    pub fn ongoing_effect_count(&self) -> isize {
        self.running_effects.lock().unwrap().get()
    }

    pub fn get_port(&self) -> EffectorPort {
        self.port.clone()
    }
}
