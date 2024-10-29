use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    nvs::{EspNvsPartition, NvsDefault},
    wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi},
};

use crate::config::WifiConfig;

const AUTH_METHOD: AuthMethod = AuthMethod::WPA2Personal;

pub fn start(
    config: &WifiConfig,
    modem: impl peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<Box<EspWifi<'static>>> {
    let nvs_part = EspNvsPartition::<NvsDefault>::take()?;

    if config.ssid.is_empty() {
        return Err(anyhow::anyhow!("WiFi SSID cannot be empty."));
    }

    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs_part))?;

    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    let default_config = AccessPointConfiguration::default();

    // TODO: Read the WiFi password from NVS, falling back to the configured default password. This
    // allows for the password to be changed at runtime.
    let config = &Configuration::AccessPoint(AccessPointConfiguration {
        ssid: config
            .ssid
            .as_str()
            .try_into()
            .map_err(|_| anyhow::anyhow!("WiFi SSID is too long."))?,
        ssid_hidden: config.hidden,
        auth_method: match &config.password {
            Some(password) if !password.is_empty() => AUTH_METHOD,
            _ => AuthMethod::None,
        },
        password: config
            .password
            .as_deref()
            .unwrap_or_default()
            .try_into()
            .map_err(|_| anyhow::anyhow!("WiFi password is too long."))?,
        channel: config.channel.unwrap_or(default_config.channel),
        ..default_config
    });

    wifi.set_configuration(config)?;

    log::info!("Starting WiFi...");

    wifi.start()?;
    wifi.wait_netif_up()?;

    let ip_addr = wifi.wifi().ap_netif().get_ip_info()?.ip;

    log::info!("WiFi started. IP address: {}", ip_addr);

    Ok(Box::new(esp_wifi))
}
