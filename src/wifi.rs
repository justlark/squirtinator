use std::{cmp, time::Duration};

use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::{self, EspSubscription, EspSystemEventLoop},
    hal::{self, modem::Modem, peripheral::Peripheral},
    mdns::EspMdns,
    netif::EspNetif,
    nvs::{EspDefaultNvsPartition, EspNvsPartition, NvsPartitionId},
    sys::ESP_ERR_TIMEOUT,
    timer::EspTaskTimerService,
    wifi::{AsyncWifi, EspWifi, WifiDriver, WifiEvent},
};

use crate::config;

// In my testing, it can sometimes take the device a few attempts to connect to the local network,
// even with a strong signal.
//
// While it's prudent to use exponential backoff, we also want to get the device connected ASAP,
// because sex toys are a particular class of device that users have very little patience for
// debugging (and why should they). So we make *n* "eager" attempts first, before we start applying
// backoff.
//
// We also cap the maximum backoff; a mobile device such as this may be on an unstable network or
// move in and out of range of networks, so we don't want to give up entirely.
//
// We need to be somewhat aggressive about connection attempts because users won't have the benefit
// of log messages or helpful code comments to explain why they can't get their sex toy to work.
#[derive(Debug, Clone, Copy)]
enum ConnectStrategy {
    Eager { attempt: u32 },
    Backoff { time: Duration },
}

impl Default for ConnectStrategy {
    fn default() -> Self {
        Self::Eager { attempt: 0 }
    }
}

impl ConnectStrategy {
    pub const EAGER_ATTEMPTS: u32 = 3;
    pub const MAX_BACKOFF: Duration = Duration::from_secs(2u64.pow(4));
    pub const BACKOFF_MULTIPLIER: u32 = 2;

    pub fn next_attempt(&mut self) {
        match self {
            Self::Eager { attempt: attempts } if *attempts >= Self::EAGER_ATTEMPTS => {
                *self = Self::Backoff {
                    time: Duration::from_secs(1),
                };
            }
            Self::Eager { attempt: attempts } => {
                *attempts += 1;
            }
            Self::Backoff { time } => {
                *time = cmp::min(*time * Self::BACKOFF_MULTIPLIER, Self::MAX_BACKOFF);
            }
        }
    }
}

pub async fn connect<P: NvsPartitionId>(
    wifi: &mut AsyncWifi<EspWifi<'static>>,
    nvs_part: EspNvsPartition<P>,
    timer_service: EspTaskTimerService,
) -> anyhow::Result<()> {
    let mut strategy = ConnectStrategy::default();
    let mut timer = timer_service.timer_async()?;

    loop {
        strategy.next_attempt();

        match strategy {
            ConnectStrategy::Eager { attempt } => {
                log::info!(
                    "Connecting to WiFI in STA mode (attempt {} of {})...",
                    attempt,
                    ConnectStrategy::EAGER_ATTEMPTS
                );
            }
            ConnectStrategy::Backoff { time } => {
                log::info!(
                    "Backing off. Waiting {}s before attempting to connect...",
                    time.as_secs()
                );

                timer.after(time).await?;

                log::info!("Connecting to WiFI in STA mode...");
            }
        }

        match wifi.connect().await {
            Err(err) if err.code() == ESP_ERR_TIMEOUT => {
                log::warn!("WiFi connection attempt timed out. Retrying...",);
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(_) => {
                log::info!("WiFi connected.");

                wifi.wait_netif_up().await?;
                log::info!("WiFi netif up.");

                let addr = wifi.wifi().sta_netif().get_ip_info()?.ip;
                config::set_wifi_ip_addr(nvs_part, Some(addr))?;

                return Ok(());
            }
        }
    }
}

// Set up mDNS for local network discovery. This allows you to access the toy by its `.local`
// domain name.
pub fn configure_mdns(mdns: &mut EspMdns, hostname: &str) -> anyhow::Result<()> {
    log::info!("Configuring mDNS with hostname: {}", hostname);
    mdns.set_hostname(hostname)?;
    mdns.set_instance_name(hostname)?;
    Ok(())
}

pub async fn init(
    modem: impl Peripheral<P = Modem> + 'static,
    nvs_part: EspDefaultNvsPartition,
    sysloop: EspSystemEventLoop,
    timer_service: EspTaskTimerService,
) -> anyhow::Result<AsyncWifi<EspWifi<'static>>> {
    if config::access_point_ssid()?.is_empty() {
        return Err(anyhow!("Access point WiFi SSID cannot be empty."));
    }

    config::set_wifi_ip_addr(nvs_part.clone(), None)?;

    let wifi_driver: WifiDriver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part.clone()))?;
    let esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new_with_conf(&config::wifi_netif_config()?)?,
        EspNetif::new_with_conf(&config::access_point_netif_config()?)?,
    )?;

    let mut wifi = AsyncWifi::wrap(esp_wifi, sysloop, timer_service)?;

    wifi.set_configuration(&config::wifi_config(nvs_part.clone())?)?;

    log::info!("Starting WiFi...");

    wifi.start().await?;
    log::info!("WiFi started.");

    Ok(wifi)
}

pub fn handle_events(
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
