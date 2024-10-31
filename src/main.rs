// Necessary for conditional compilation of remote components.
// https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md#remote-components-idf-component-registry
#![allow(unexpected_cfgs)]

mod config;
mod gpio;
mod http;
mod wifi;

const NVS_USER_NAMESPACE: &str = "user";

use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::prelude::Peripherals,
    mdns::EspMdns,
    nvs::{EspDefaultNvsPartition, EspNvs},
};
use gpio::{Action, GpioAction};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let sysloop = EspSystemEventLoop::take()?;

    // We store persistent user preferences their own NVS namespace in the default partition.
    let nvs_part = EspDefaultNvsPartition::take()?;
    let mut user_nvs = EspNvs::new(nvs_part, NVS_USER_NAMESPACE, true)?;

    let config = config::Config::read(&mut user_nvs)?;

    // Otherwise we won't be able to access the default partition later.
    drop(user_nvs);

    let action: Arc<Mutex<dyn Action>> = Arc::new(Mutex::new(GpioAction::new(
        config.io.pin(peripherals.pins)?,
        config.io.duration(),
    )?));

    let mdns = Arc::new(Mutex::new(EspMdns::take()?));

    let wifi_request_handler = Arc::new(Mutex::new(wifi::start(
        &config,
        peripherals.modem,
        Arc::clone(&mdns),
        EspDefaultNvsPartition::take()?,
        sysloop.clone(),
    )?));

    // Don't drop this.
    let _server = http::serve(
        &config,
        Arc::clone(&wifi_request_handler),
        Arc::clone(&action),
    )?;

    // Don't drop this.
    let _subscription = wifi::reset_on_disconnect(&sysloop)?;

    // Park the main thread indefinitely. Other threads will continue executing. We must use a loop
    // here because `std::thread::park()` does not guarantee that threads will stay parked forever.
    loop {
        std::thread::park();
    }
}
