use std::{sync::mpsc::Sender, time::Duration};

use esp_idf_svc::hal::gpio::{AnyOutputPin, Level, PinDriver};

// An action to take when the toy is activated.
pub trait Action {
    fn exec(&mut self) -> anyhow::Result<()>;
}

// An `Action` which toggles a GPIO pin for a set duration in the background.
pub struct GpioAction {
    channel: Sender<()>,
}

impl GpioAction {
    pub fn new(pin: AnyOutputPin, duration: Duration) -> anyhow::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();

        let mut driver = PinDriver::output(pin)?;

        // We can safely detach this thread because it will end when the sending half of the
        // channel is dropped, so there's no need to join it.
        std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                driver.set_level(Level::High).unwrap();
                std::thread::sleep(duration);
                driver.set_level(Level::Low).unwrap();
            }
        });

        Ok(Self { channel: sender })
    }
}

impl Action for GpioAction {
    fn exec(&mut self) -> anyhow::Result<()> {
        self.channel.send(())?;
        Ok(())
    }
}
