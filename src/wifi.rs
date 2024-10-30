use anyhow::anyhow;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    netif::EspNetif,
    nvs::{EspNvsPartition, NvsDefault},
    sys::ESP_ERR_TIMEOUT,
    wifi::{BlockingWifi, EspWifi, WifiDriver},
};

use crate::config::Config;

#[allow(unused_variables)]
fn configure_mdns(hostname: &str) -> anyhow::Result<bool> {
    #[cfg(esp_idf_comp_espressif__mdns_enabled)]
    {
        use esp_idf_svc::mdns::EspMdns;

        log::info!("Configuring mDNS hostname: {}", hostname);
        let mut mdns = EspMdns::take()?;
        mdns.set_hostname(hostname)?;

        // Don't drop this.
        std::mem::forget(mdns);

        Ok(true)
    }

    #[cfg(not(esp_idf_comp_espressif__mdns_enabled))]
    {
        log::info!("Skipping mDNS setup.");
        Ok(false)
    }
}

pub fn start(
    config: &Config,
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<EspWifi<'static>> {
    if config.access_point.ssid.is_empty() {
        return Err(anyhow!("Access point WiFi SSID cannot be empty."));
    }

    let nvs_part = EspNvsPartition::<NvsDefault>::take()?;

    let wifi_driver: WifiDriver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part))?;
    let mut esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new_with_conf(&config.wifi.netif_config()?)?,
        EspNetif::new_with_conf(&config.access_point.netif_config()?)?,
    )?;

    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    wifi.set_configuration(&config.wifi_config()?)?;

    log::info!("Starting WiFi...");

    wifi.start()?;
    log::info!("WiFi started.");

    if config.wifi.is_configured() {
        loop {
            match wifi.connect() {
                Err(err) if err.code() == ESP_ERR_TIMEOUT => {
                    log::warn!("WiFi connection timed out. Retrying...");
                    continue;
                }
                Err(err) => return Err(err.into()),
                Ok(_) => break,
            }
        }

        log::info!("WiFi connected.");
    }

    wifi.wait_netif_up()?;
    log::info!("WiFi netif up.");

    // Set up mDNS for local network discovery.
    configure_mdns(&config.wifi.hostname)?;

    Ok(esp_wifi)
}
