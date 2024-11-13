use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use esp_idf_svc::{
    hal::{
        self,
        gpio::{self, Pins},
        task::queue::Queue,
    },
    nvs::{EspNvsPartition, NvsPartitionId},
};
use rand::prelude::*;
use rand::rngs::SmallRng;

use crate::{config, Never};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Fire,
    StartAuto,
    StopAuto,
}

pub struct PinTriggerQueue(Queue<Signal>);

impl fmt::Debug for PinTriggerQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinTriggerQueue").finish_non_exhaustive()
    }
}

impl PinTriggerQueue {
    pub fn new() -> Self {
        // We use a rendezvous queue to pass messages from the HTTP server to trigger the GPIO pin.
        // On the sending side, we don't block if the queue is full. This has the effect that if
        // the user presses the button to trigger the toy while it's already doing something, it
        // will be a no-op rather then queue up multiple pulses over the GPIO pin. We want to wait
        // until the toy is done doing its thing before we allow it to be activated again.
        Self(Queue::new(0))
    }

    pub fn send(&self, signal: Signal) -> anyhow::Result<()> {
        self.0.send_back(signal, hal::delay::BLOCK)?;
        Ok(())
    }

    pub fn try_send(&self, signal: Signal) -> bool {
        if self.0.send_back(signal, 0).is_err() {
            // The queue is full.
            false
        } else {
            true
        }
    }

    pub fn recv(&self) -> Signal {
        if let Some((signal, _)) = self.0.recv_front(hal::delay::BLOCK) {
            signal
        } else {
            unreachable!();
        }
    }
}

pub fn listen<P>(
    nvs_part: EspNvsPartition<P>,
    pins: Pins,
    queue: Arc<PinTriggerQueue>,
) -> anyhow::Result<Never>
where
    P: NvsPartitionId + Send + Sync + 'static,
{
    let mut pin_driver = gpio::PinDriver::output(config::io_pin(pins)?)?;
    let mut rng = SmallRng::from_entropy();

    let is_auto_set = Arc::new(AtomicBool::new(false));
    let queue_recv = queue;

    let is_auto_get = Arc::clone(&is_auto_set);
    let queue_send = Arc::clone(&queue_recv);

    thread::spawn(move || loop {
        let mut wait_then_fire = || -> anyhow::Result<()> {
            // We read these on each loop iteration because they're configurable by the user and
            // may change at any time.
            let min_seconds = config::freq_min(nvs_part.clone())?;
            let max_seconds = config::freq_max(nvs_part.clone())?;

            let seconds_to_wait = rng.gen_range(min_seconds..max_seconds);

            if is_auto_get.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(seconds_to_wait.into()));
                queue_send.send(Signal::Fire)?;
            }

            Ok(())
        };

        if let Err(err) = wait_then_fire() {
            log::error!("{:?}", err);
        }
    });

    loop {
        let duration = config::io_duration()?;

        match queue_recv.recv() {
            Signal::StartAuto => {
                is_auto_set.store(true, Ordering::Relaxed);
                continue;
            }
            Signal::StopAuto => {
                is_auto_set.store(false, Ordering::Relaxed);
                continue;
            }
            Signal::Fire => {}
        }

        log::info!(
            "Setting GPIO pin {} to high for {}ms.",
            pin_driver.pin(),
            duration.as_millis(),
        );
        pin_driver.set_level(gpio::Level::High)?;

        thread::sleep(duration);

        log::info!("Setting GPIO pin {} to low.", pin_driver.pin(),);
        pin_driver.set_level(gpio::Level::Low)?;
    }
}
