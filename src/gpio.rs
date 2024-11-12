use std::sync::Arc;

use esp_idf_svc::hal::{
    self,
    gpio::{self, Pins},
    task::queue::Queue,
};

use crate::{config, Never};

pub fn start_loop(pins: Pins, queue: Arc<Queue<()>>) -> anyhow::Result<Never> {
    let mut pin_driver = gpio::PinDriver::output(config::io_pin(pins)?)?;

    loop {
        let duration = config::io_duration()?;
        queue.recv_front(hal::delay::BLOCK);

        log::info!(
            "Setting GPIO pin {} to high for {}ms.",
            pin_driver.pin(),
            duration.as_millis(),
        );
        pin_driver.set_level(gpio::Level::High)?;

        std::thread::sleep(duration);

        log::info!("Setting GPIO pin {} to low.", pin_driver.pin(),);
        pin_driver.set_level(gpio::Level::Low)?;
    }
}
