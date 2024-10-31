// Necessary for conditional compilation of remote components.
// https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md#remote-components-idf-component-registry
#![allow(unexpected_cfgs)]

mod config;
mod gpio;
mod http;
mod wifi;

use std::sync::{Arc, Mutex};

use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::prelude::Peripherals};
use gpio::{Action, GpioAction};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let sysloop = EspSystemEventLoop::take()?;

    let config = config::Config::read()?;

    let action: Arc<Mutex<dyn Action>> = Arc::new(Mutex::new(GpioAction::new(
        config.io.pin(peripherals.pins)?,
        config.io.duration(),
    )?));

    // Don't drop these.
    let wifi = wifi::start(&config, peripherals.modem, sysloop)?;
    let _server = http::serve(&config, wifi, Arc::clone(&action))?;

    // Park the main thread indefinitely. Other threads will continue executing. We must use a loop
    // here because `std::thread::park()` does not guarantee that threads will stay parked forever.
    loop {
        std::thread::park();
    }
}
