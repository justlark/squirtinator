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

use crate::config::Config;

pub async fn connect(
    wifi: Arc<Mutex<RawMutex, AsyncWifi<EspWifi<'static>>>>,
    timeout: Duration,
    max_attempts: u32,
) -> anyhow::Result<bool> {
    for i in 0..max_attempts {
        let wait_result = async {
            let mut wifi = wifi.lock().await;

            wifi.wifi_mut().connect()?;

            wifi.wifi_wait(
                |this| this.wifi().driver().is_sta_connected().map(|s| !s),
                Some(timeout),
            )
            .await
        };

        match wait_result.await {
            Err(err) if err.code() == ESP_ERR_TIMEOUT => {
                log::warn!(
                    "WiFi connection timed out (attempt {} of {}). Retrying...",
                    i + 1,
                    max_attempts
                );
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(_) => {
                log::info!("WiFi connected.");

                wifi.lock().await.wait_netif_up().await?;
                log::info!("WiFi netif up.");

                return Ok(true);
            }
        }
    }

    log::error!(
        "Failed to connect to WiFi after {} attempts. Giving up.",
        max_attempts
    );

    Ok(false)
}

// Set up mDNS for local network discovery.
fn configure_mdns(mdns: &mut EspMdns, hostname: &str) -> anyhow::Result<()> {
    log::info!("Configuring mDNS with hostname: {}", hostname);
    mdns.set_hostname(hostname)?;
    mdns.set_instance_name(hostname)?;
    Ok(())
}

pub async fn init(
    config: &Config,
    modem: impl Peripheral<P = Modem> + 'static,
    mdns: &mut EspMdns,
    nvs_part: EspDefaultNvsPartition,
    sysloop: EspSystemEventLoop,
    timer_service: EspTaskTimerService,
) -> anyhow::Result<AsyncWifi<EspWifi<'static>>> {
    if config.access_point.ssid.is_empty() {
        return Err(anyhow!("Access point WiFi SSID cannot be empty."));
    }

    let wifi_driver: WifiDriver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part))?;
    let esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new_with_conf(&config.wifi.netif_config()?)?,
        EspNetif::new_with_conf(&config.access_point.netif_config()?)?,
    )?;

    let mut wifi = AsyncWifi::wrap(esp_wifi, sysloop, timer_service)?;

    wifi.set_configuration(&config.wifi_config()?)?;
    configure_mdns(mdns, &config.wifi.hostname)?;

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
