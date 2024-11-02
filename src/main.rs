mod config;
mod http;
mod wifi;

use std::{future::Future, pin::Pin, sync::Arc};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        self, gpio,
        prelude::Peripherals,
        task::{block_on, queue::Queue},
    },
    mdns::EspMdns,
    nvs::EspDefaultNvsPartition,
    timer::EspTaskTimerService,
};

fn run() -> anyhow::Result<()> {
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

    // We use a queue of size 1 to pass messages from the HTTP server to trigger the GPIO pin. On
    // the sending side, we don't block if the queue is full. This has the effect that if the user
    // presses the button to trigger the toy while it's already doing something, it will be a no-op
    // rather then queue up multiple pulses over the GPIO pin. We want to wait until the toy is
    // done doing its thing before we allow it to be activated again.
    let pin_trigger_queue = Arc::new(Queue::new(1));

    // Don't drop this.
    let _server = http::serve(nvs_part.clone(), Arc::clone(&pin_trigger_queue))?;

    block_on(connection)?;

    let mut mdns = EspMdns::take()?;
    wifi::configure_mdns(&mut mdns, &config::wifi_hostname()?)?;

    // Don't drop this.
    let _subscription = wifi::handle_events(&sysloop)?;

    let mut pin_driver = gpio::PinDriver::output(config::io_pin(peripherals.pins)?)?;

    loop {
        let duration = config::io_duration()?;
        pin_trigger_queue.recv_front(hal::delay::BLOCK);

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

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    if let Err(err) = run() {
        log::error!("{:?}", err);
        hal::reset::restart();
    }
}
