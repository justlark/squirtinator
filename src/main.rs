// Necessary for conditional compilation of remote components.
// https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md#remote-components-idf-component-registry
#![allow(unexpected_cfgs)]

mod config;
mod gpio;
mod http;
mod wifi;

use std::{future::Future, pin::Pin, sync::Arc};

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

    // One-time initialization of the global config.
    config::init_config()?;

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let timer_service = EspTaskTimerService::new()?;
    let nvs_part = EspDefaultNvsPartition::take()?;

    let action: Arc<Mutex<RawMutex, dyn Action>> = Arc::new(Mutex::new(GpioAction::new(
        config::io_pin(peripherals.pins)?,
        config::io_duration()?,
    )?));

    let wifi = Arc::new(Mutex::new(block_on(wifi::init(
        peripherals.modem,
        nvs_part.clone(),
        sysloop.clone(),
        timer_service.clone(),
    ))?));

    // Don't block waiting for the connection to be established just yet. We want to bring up the
    // HTTP server in the meantime so that users can potentially connect to the device in AP mode
    // while waiting for it to connect to the local network in STA mode (or in case it's unable
    // to).
    let connection: Pin<Box<dyn Future<Output = _>>> =
        if config::wifi_is_configured(nvs_part.clone())? {
            Box::pin(wifi::connect(Arc::clone(&wifi), timer_service))
        } else {
            Box::pin(std::future::ready(Ok(())))
        };

    // Don't drop this.
    let _server = http::serve(Arc::clone(&wifi), nvs_part.clone(), Arc::clone(&action))?;

    block_on(connection)?;

    let mut mdns = EspMdns::take()?;
    wifi::configure_mdns(&mut mdns, &config::wifi_hostname()?)?;

    // Don't drop this.
    let _subscription = wifi::reset_on_disconnect(&sysloop)?;

    // Park the main thread indefinitely. Other threads will continue executing. We must use a loop
    // here because `std::thread::park()` does not guarantee that threads will stay parked forever.
    loop {
        std::thread::park();
    }
}
