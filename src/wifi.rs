use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    ipv4,
    netif::{EspNetif, NetifConfiguration},
    nvs::{EspNvsPartition, NvsDefault},
    wifi::{BlockingWifi, EspWifi, WifiDriver},
};

use crate::config::Config;

pub fn start(
    config: &Config,
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<Box<EspWifi<'static>>> {
    if config.access_point.ssid.is_empty() {
        return Err(anyhow::anyhow!("Access point WiFi SSID cannot be empty."));
    }

    let nvs_part = EspNvsPartition::<NvsDefault>::take()?;

    // Set a static, predictable gateway IP address.
    let mut ap_config = NetifConfiguration::wifi_default_router();
    if let ipv4::Configuration::Router(router_conf) = &mut ap_config.ip_configuration {
        router_conf.subnet.gateway = config.access_point.gateway()?;
    }

    // Set the client hostname.
    let mut sta_config = NetifConfiguration::wifi_default_client();
    if let ipv4::Configuration::Client(ipv4::ClientConfiguration::DHCP(client_conf)) =
        &mut sta_config.ip_configuration
    {
        client_conf.hostname = Some(
            config
                .wifi
                .hostname
                .as_str()
                .try_into()
                .map_err(|_| anyhow::anyhow!("WiFi hostname is too long."))?,
        );
    }

    let wifi_driver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part))?;
    let mut esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new_with_conf(&sta_config)?,
        EspNetif::new_with_conf(&ap_config)?,
    )?;

    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    wifi.set_configuration(&config.wifi_config()?)?;

    log::info!("Starting WiFi...");

    wifi.start()?;
    log::info!("WiFi started.");

    if config.wifi.is_configured() {
        wifi.connect()?;
        log::info!("WiFi connected.");
    }

    wifi.wait_netif_up()?;
    log::info!("WiFi netif up.");

    Ok(Box::new(esp_wifi))
}
