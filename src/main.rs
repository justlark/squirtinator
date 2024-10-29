mod config;
mod wifi;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::prelude::Peripherals,
    nvs::{EspNvsPartition, NvsDefault},
};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let sysloop = EspSystemEventLoop::take()?;

    let config = config::Config::read()?;

    let _wifi = wifi::start(&config.wifi, peripherals.modem, sysloop)?;

    std::thread::sleep(std::time::Duration::from_secs(300));

    Ok(())
}
