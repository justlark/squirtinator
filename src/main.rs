// Necessary for conditional compilation of remote components.
// https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md#remote-components-idf-component-registry
#![allow(unexpected_cfgs)]

mod config;
mod gpio;
mod http;
mod wifi;

use std::{future::Future, pin::Pin, sync::Arc};

use config::user_nvs;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex as RawMutex, mutex::Mutex};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{prelude::Peripherals, task::block_on},
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    timer::EspTaskTimerService,
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
    let timer_servie = EspTaskTimerService::new()?;

    let nvs_part = EspDefaultNvsPartition::take()?;
    let mut nvs = user_nvs(nvs_part.clone())?;

    let config = config::Config::read(&mut nvs)?;

    // So we can access the default NVS partition later.
    drop(nvs);

    let action: Arc<Mutex<RawMutex, dyn Action>> = Arc::new(Mutex::new(GpioAction::new(
        config.io.pin(peripherals.pins)?,
        config.io.duration(),
    )?));

    let wifi = Arc::new(Mutex::new(block_on(wifi::init(
        &config,
        peripherals.modem,
        nvs_part.clone(),
        sysloop.clone(),
        timer_servie,
    ))?));

    let connected: Pin<Box<dyn Future<Output = _>>> = if config.wifi.is_configured() {
        Box::pin(wifi::connect(
            Arc::clone(&wifi),
            config.wifi.timeout(),
            config.wifi.max_attempts,
        ))
    } else {
        Box::pin(std::future::ready(Ok(false)))
    };

    // Don't drop this.
    let _server = http::serve(
        &config,
        Arc::clone(&wifi),
        nvs_part.clone(),
        Arc::clone(&action),
    )?;

    let mut mdns = EspMdns::take()?;

    // Don't drop this.
    let _subscription = if block_on(connected)? {
        wifi::configure_mdns(&mut mdns, &config.wifi.hostname)?;
        Some(wifi::reset_on_disconnect(&sysloop)?)
    } else {
        None
    };

    // Park the main thread indefinitely. Other threads will continue executing. We must use a loop
    // here because `std::thread::park()` does not guarantee that threads will stay parked forever.
    loop {
        std::thread::park();
    }
}
