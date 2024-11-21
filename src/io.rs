use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use esp_idf_svc::{
    hal::gpio,
    hal::i2c,
    nvs::{EspNvsPartition, NvsPartitionId},
};
use rand::prelude::*;
use rand::rngs::SmallRng;

use crate::{config, queue::RendezvousQueue, Never};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Fire,
    StartAuto,
    StopAuto,
}

#[derive(Debug)]
pub struct Signaler {
    fire_queue: RendezvousQueue<()>,
    auto_queue: RendezvousQueue<bool>,
    is_auto: AtomicBool,
}

impl Signaler {
    pub fn new() -> Self {
        Self {
            fire_queue: RendezvousQueue::new(),
            auto_queue: RendezvousQueue::new(),
            is_auto: AtomicBool::new(false),
        }
    }

    pub fn send(&self, signal: Signal) {
        match signal {
            Signal::Fire => {
                // We don't block if the queue is full. This has the effect that if the user
                // presses the button to trigger the toy while it's already doing something, it
                // will be a no-op rather then queue up multiple pulses over the GPIO pin. We want
                // to wait until the toy is done doing its thing before we allow it to be activated
                // again.
                if !self.fire_queue.try_send(()) {
                    log::info!("GPIO output pin is already active. Skipping this pulse.");
                }
            }
            // Staring or stopping auto mode should immediately override the previous setting
            // without blocking.
            Signal::StartAuto => {
                self.auto_queue.try_recv();
                self.auto_queue.send(true);
                self.is_auto.store(true, Ordering::Relaxed);
                log::info!("Starting auto mode.");
            }
            Signal::StopAuto => {
                self.auto_queue.try_recv();
                self.auto_queue.send(false);
                self.is_auto.store(false, Ordering::Relaxed);
                log::info!("Stopping auto mode.");
            }
        }
    }

    pub fn is_auto(&self) -> bool {
        self.is_auto.load(Ordering::Relaxed)
    }
}

pub fn listen<P>(
    nvs_part: EspNvsPartition<P>,
    i2c: i2c::I2C0,
    pins: gpio::Pins,
    signaler: Arc<Signaler>,
) -> anyhow::Result<Never>
where
    P: NvsPartitionId + Send + Sync + 'static,
{
    let mut rng = SmallRng::from_entropy();
    let this_signaler = Arc::clone(&signaler);

    thread::spawn(move || {
        let mut fire = || -> anyhow::Result<()> {
            // We read these each time because they're configurable by the user and may change at
            // any time.
            let min_seconds = config::freq_min(nvs_part.clone())?;
            let max_seconds = config::freq_max(nvs_part.clone())?;

            let seconds_to_wait = rng.gen_range(min_seconds..max_seconds);
            thread::sleep(Duration::from_secs(seconds_to_wait.into()));

            // Check in case auto mode was disabled while we were sleeping.
            if this_signaler.auto_queue.try_peek() != Some(false) {
                this_signaler.fire_queue.try_send(());
            }

            Ok(())
        };

        let mut is_auto = false;

        let mut wait_then_fire = || -> anyhow::Result<()> {
            if is_auto {
                // Poll for whether the user has disabled auto mode.
                if this_signaler.auto_queue.try_recv() == Some(false) {
                    is_auto = false;
                } else {
                    fire()?;
                }
            // Block until the user enables auto mode so we don't get caught in a busy loop.
            } else if this_signaler.auto_queue.recv() {
                is_auto = true;
                fire()?;
            }

            Ok(())
        };

        loop {
            if let Err(err) = wait_then_fire() {
                log::error!("{:?}", err);
            }
        }
    });

    let mut pins = config::io_pins(pins)?;
    let address = config::io_address()?;
    let message = config::io_message()?;
    let baudrate = config::io_baudrate()?;
    let timeout = config::io_timeout()?;

    let i2c_config = i2c::I2cConfig {
        baudrate: baudrate.into(),
        ..Default::default()
    };

    let mut driver = i2c::I2cDriver::new(i2c, pins.sda_pin()?, pins.scl_pin()?, &i2c_config)?;

    loop {
        // Wait until we get a message to trigger the pump over I2C.
        signaler.fire_queue.recv();

        log::info!(
            "Activating the pump over I2C at address {:#04x} with message {:?}.",
            address,
            message,
        );

        driver.write(address, &message, timeout)?;
    }
}
