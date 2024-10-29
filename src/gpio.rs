use std::{sync::mpsc::SyncSender, time::Duration};

use esp_idf_svc::hal::gpio::{AnyOutputPin, Level, Pin, PinDriver};

// An action to take when the toy is activated.
pub trait Action: Send {
    fn exec(&mut self) -> anyhow::Result<()>;
}

// An `Action` which toggles a GPIO pin for a set duration in the background.
pub struct GpioAction {
    channel: SyncSender<()>,
}

impl GpioAction {
    pub fn new(pin: AnyOutputPin, duration: Duration) -> anyhow::Result<Self> {
        // We use a rendezvous channel so that messages to activate the pin don't get queued up. If
        // the user mashes the button to activate the toy, we want to discard all button presses
        // that occur while the toy is actively, uh, doing its thing.
        let (sender, receiver) = std::sync::mpsc::sync_channel(0);

        let pin_num = pin.pin();
        let mut driver = PinDriver::output(pin)?;

        // We can safely detach this thread because it will end when the sending half of the
        // channel is dropped, so there's no need to join it.
        std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                log::info!(
                    "Setting pin {} to high for {}ms.",
                    pin_num,
                    duration.as_millis()
                );
                driver.set_level(Level::High).unwrap();

                std::thread::sleep(duration);

                log::info!("Setting pin {} to low.", pin_num);
                driver.set_level(Level::Low).unwrap();
            }
        });

        Ok(Self { channel: sender })
    }
}

impl Action for GpioAction {
    fn exec(&mut self) -> anyhow::Result<()> {
        // This is a rendezvous channel. If the toy is already active, don't do anything.
        self.channel.try_send(())?;
        Ok(())
    }
}
