mod config;
mod gpio;
mod http;
mod queue;
mod wifi;

use std::{future::Future, pin::Pin, sync::Arc};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{self, prelude::Peripherals, task::block_on},
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    timer::EspTaskTimerService,
};

#[derive(Debug)]
pub enum Never {}

fn run() -> anyhow::Result<Never> {
    // One-time initialization of the global config.
    config::init_config()?;

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let timer_service = EspTaskTimerService::new()?;
    let nvs_part = EspDefaultNvsPartition::take()?;

    let mut wifi = block_on(wifi::init(
        peripherals.modem,
        nvs_part.clone(),
        sysloop.clone(),
        timer_service.clone(),
    ))?;

    // Don't block waiting for the connection to be established just yet. We want to bring up the
    // HTTP server in the meantime so that users can potentially connect to the device in AP mode
    // while waiting for it to connect to the local network in STA mode (or in case it's unable
    // to).
    let connection: Pin<Box<dyn Future<Output = _>>> =
        if config::wifi_is_configured(nvs_part.clone())? {
            Box::pin(wifi::connect(&mut wifi, nvs_part.clone(), timer_service))
        } else {
            Box::pin(std::future::ready(Ok(())))
        };

    let signaler = Arc::new(gpio::Signaler::new());

    // Don't drop this.
    let _server = http::serve(nvs_part.clone(), Arc::clone(&signaler))?;

    block_on(connection)?;

    let mut mdns = EspMdns::take()?;
    wifi::configure_mdns(&mut mdns, &config::wifi_hostname()?)?;

    // Don't drop this.
    let _subscription = wifi::handle_events(&sysloop)?;

    gpio::listen(nvs_part, peripherals.pins, signaler)
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let Err(err) = run();
    log::error!("{:?}", err);
    hal::reset::restart();
}
