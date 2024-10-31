use std::{
    any::Any,
    net::Ipv4Addr,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::{self, EspSubscription, EspSystemEventLoop},
    hal,
    mdns::EspMdns,
    netif::EspNetif,
    nvs::{EspNvsPartition, NvsDefault},
    sys::ESP_ERR_TIMEOUT,
    wifi::{BlockingWifi, EspWifi, WifiDriver, WifiEvent},
};

use crate::config::Config;

// We're pretty greedy with the reconnection backoff, because it's quite frustrating when your sex
// toy disconnects mid-session and takes a while to reconnect. Hence linear rather than exponential
// backoff.
const BACKOFF_DURATION_START: Duration = Duration::from_secs(1);
const BACKOFF_DURATION_MAX: Duration = Duration::from_secs(5);
const BACKOFF_DURATION_STEP: Duration = Duration::from_secs(1);

// This is a mechanism for sending requests to the WiFi driver and receiving responses. It allows
// us to avoid having to pass around the WiFi driver. Instead, there's a dedicated thread that owns
// the driver and processes requests over channels.
pub trait Request {
    type Response;

    fn respond(&self, wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<Self::Response>;
}

// A request which gets the current IP address of the WiFi STA interface.
#[derive(Debug)]
pub struct IpAddrRequest;

impl Request for IpAddrRequest {
    type Response = Box<Option<Ipv4Addr>>;

    fn respond(
        &self,
        wifi: &mut BlockingWifi<EspWifi<'static>>,
    ) -> anyhow::Result<Box<Option<Ipv4Addr>>> {
        Ok(Box::new(if wifi.wifi().driver().is_sta_connected()? {
            Some(wifi.wifi().sta_netif().get_ip_info()?.ip)
        } else {
            None
        }))
    }
}

type RequestHandlerFn = Box<
    dyn FnOnce(&mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<Box<dyn Any + Send>> + Send,
>;

pub struct RequestHandler {
    requests: mpsc::SyncSender<RequestHandlerFn>,
    responses: mpsc::Receiver<anyhow::Result<Box<dyn Any + Send>>>,
}

impl RequestHandler {
    pub fn new(mut wifi: BlockingWifi<EspWifi<'static>>) -> Self {
        let (request_sender, request_receiver) = mpsc::sync_channel::<RequestHandlerFn>(0);
        let (response_sender, response_receiver) = mpsc::sync_channel(1);

        // We can safely detach this thread because it will end when the sending half of the
        // channel is dropped, so there's no need to join it.
        std::thread::spawn(move || {
            while let Ok(request_fn) = request_receiver.recv() {
                let response = request_fn(&mut wifi);
                response_sender
                    .send(response)
                    .expect("WiFi request response channel closed.");
            }
        });

        Self {
            requests: request_sender,
            responses: response_receiver,
        }
    }
    pub fn request<T>(&mut self, request: T) -> anyhow::Result<T::Response>
    where
        T: Request + Send + 'static,
        T::Response: Send,
    {
        self.requests
            .send(Box::new(|wifi: &mut BlockingWifi<EspWifi<'static>>| {
                let request = request;
                let response = request.respond(wifi)?;
                Ok(Box::new(response))
            }))
            .map_err(|_| anyhow!("WiFi request thread has exited."))?;

        self.responses
            .recv()
            .map_err(|_| anyhow!("WiFi request thread has exited."))?
            .and_then(|response| {
                response
                    .downcast::<T::Response>()
                    .map_err(|_| anyhow!("WiFi request response type mismatch."))
            })
            .map(|response| *response)
    }
}

fn backoff(backoff_duration: &mut Duration) {
    if *backoff_duration > Duration::ZERO {
        log::info!(
            "Waiting {}s before making a reconnection attempt.",
            backoff_duration.as_secs()
        );

        std::thread::sleep(*backoff_duration);
    }

    if *backoff_duration < BACKOFF_DURATION_MAX {
        *backoff_duration += BACKOFF_DURATION_STEP;
    }
}

fn connect_with_retry(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let mut backoff_duration = BACKOFF_DURATION_START;

    loop {
        match wifi.connect() {
            Err(err) if err.code() == ESP_ERR_TIMEOUT => {
                log::warn!("WiFi connection timed out. Retrying...");

                backoff(&mut backoff_duration);

                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(_) => {
                log::info!("WiFi connected.");

                wifi.wait_netif_up()?;
                log::info!("WiFi netif up.");

                break;
            }
        }
    }

    Ok(())
}

// Set up mDNS for local network discovery.
#[allow(unused_variables)]
fn configure_mdns(mdns: &mut EspMdns, hostname: &str) -> anyhow::Result<()> {
    log::info!("Configuring mDNS with hostname: {}", hostname);
    mdns.set_hostname(hostname)?;
    mdns.set_instance_name(hostname)?;
    Ok(())
}

pub fn start(
    config: &Config,
    modem: impl hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    mdns: Arc<Mutex<EspMdns>>,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<RequestHandler> {
    if config.access_point.ssid.is_empty() {
        return Err(anyhow!("Access point WiFi SSID cannot be empty."));
    }

    let nvs_part = EspNvsPartition::<NvsDefault>::take()?;

    let wifi_driver: WifiDriver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part))?;
    let esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new_with_conf(&config.wifi.netif_config()?)?,
        EspNetif::new_with_conf(&config.access_point.netif_config()?)?,
    )?;

    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

    wifi.set_configuration(&config.wifi_config()?)?;
    configure_mdns(&mut mdns.lock().unwrap(), &config.wifi.hostname)?;

    log::info!("Starting WiFi...");

    wifi.start()?;
    log::info!("WiFi started.");

    if config.wifi.is_configured() {
        connect_with_retry(&mut wifi)?;
    }

    Ok(RequestHandler::new(wifi))
}

pub fn reset_on_disconnect(
    eventloop: &EspSystemEventLoop,
) -> anyhow::Result<EspSubscription<'static, eventloop::System>> {
    Ok(eventloop.subscribe::<WifiEvent, _>(move |event| {
        if let WifiEvent::StaDisconnected = event {
            log::warn!("WiFi disconnected. Resetting...");

            // There is probably a more elegant solution to reconnecting to WiFi, but I wasn't able
            // to figure it out. This approach has the benefit of ensuring the toy stops whatever
            // it's doing once it disconnects (and the user isn't able to control it anymore). This
            // is an important safety feature for a sex toy.
            hal::reset::restart();
        }
    })?)
}
