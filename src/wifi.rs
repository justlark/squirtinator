use std::{sync::Arc, time::Duration};

use anyhow::anyhow;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex as RawMutex, mutex::Mutex};
use esp_idf_svc::{
    eventloop::{self, EspSubscription, EspSystemEventLoop},
    hal::{self, modem::Modem, peripheral::Peripheral},
    mdns::EspMdns,
    netif::EspNetif,
    nvs::EspDefaultNvsPartition,
    sys::ESP_ERR_TIMEOUT,
    timer::EspTaskTimerService,
    wifi::{AsyncWifi, EspWifi, WifiDriver, WifiEvent},
};

use crate::config;

// Even when users have their toy configured in station (STA) mode, we still allow them to connect
// in access point (AP) mode so they have a way to reconfigure the SSID/password if the toy isn't
// able to connect to the local network for whatever reason.
//
// We can configure the networking stack in AP+STA mode, but I've found I'm unable to connect to
// the toy in AP mode when it's in the process of connecting to the local network in STA mode, as
// if the two can't happen concurrently.
//
// It appears some other folks have run into this issue:
// https://esp32.com/viewtopic.php?t=8063
//
// We could give up after a period of time and stop trying to connect, but that's an inappropriate
// solution for a device which is mobile and could therefore come back into WiFi range at any time.
// It's also frustrating for users, who will not have any log messages or helpful code comments to
// explain why they can't get their sex toy to work.
//
// As a compromise, we first attempt to connect to the local network "eagerly", *n* times. If that
// fails, we back off and only attempt to connect periodically, on a timer. With this solution:
//
// - We give the networking stack a few chances up front to set up a connection ASAP so users
// aren't waiting.
// - If it's not able to connect quickly enough, we open up some space for users to connect to the
// toy in AP mode.
// - We don't give up entirely, in case the toy is able to come back online later.
//
// Because this strategy is somewhat difficult to explain clearly in an annotated config file, I've
// elected to not make these defaults configurable.
#[derive(Debug, Clone, Copy)]
enum ConnectStrategy {
    Eager { attempt: u32 },
    Periodic,
}

impl Default for ConnectStrategy {
    fn default() -> Self {
        Self::Eager { attempt: 0 }
    }
}

impl ConnectStrategy {
    pub const MAX_ATTEMPTS: u32 = 3;
    pub const WAIT_DURATION: Duration = Duration::from_secs(10);

    pub fn attempt(&mut self) {
        match self {
            Self::Eager { attempt: attempts } if *attempts >= Self::MAX_ATTEMPTS => {
                *self = Self::Periodic;
            }
            Self::Eager { attempt: attempts } => {
                *attempts += 1;
            }
            Self::Periodic => {}
        }
    }
}

// This function doesn't return until/unless the STA-mode connection succeeds.
pub async fn connect(
    wifi: Arc<Mutex<RawMutex, AsyncWifi<EspWifi<'static>>>>,
    timer_service: EspTaskTimerService,
) -> anyhow::Result<()> {
    let mut strategy = ConnectStrategy::default();
    let mut timer = timer_service.timer_async()?;

    loop {
        match strategy {
            ConnectStrategy::Eager { attempt } => {
                log::info!(
                    "Connecting to WiFI in STA mode (attempt {} of {})...",
                    attempt + 1,
                    ConnectStrategy::MAX_ATTEMPTS
                );
            }
            ConnectStrategy::Periodic => {
                log::info!(
                    "Backing off. Waiting {}s before attempting to connect...",
                    ConnectStrategy::WAIT_DURATION.as_secs()
                );

                timer.after(ConnectStrategy::WAIT_DURATION).await?;

                log::info!("Connecting to WiFI in STA mode...");
            }
        }

        match wifi.lock().await.connect().await {
            Err(err) if err.code() == ESP_ERR_TIMEOUT => {
                log::warn!("WiFi connection attempt timed out. Retrying...",);

                strategy.attempt();

                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(_) => {
                log::info!("WiFi connected.");

                wifi.lock().await.wait_netif_up().await?;
                log::info!("WiFi netif up.");

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
