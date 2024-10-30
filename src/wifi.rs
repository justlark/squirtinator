use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripheral,
    ipv4,
    netif::{EspNetif, NetifConfiguration, NetifStack},
    nvs::{EspNvsPartition, NvsDefault},
    wifi::{
        AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration, EspWifi, WifiDriver,
    },
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

    // Set a static, predictable gateway IP address.
    let mut netif_conf = NetifConfiguration::wifi_default_router();
    if let ipv4::Configuration::Router(router_conf) = &mut netif_conf.ip_configuration {
        router_conf.subnet.gateway = config.gateway()?;
    }

    let wifi_driver = WifiDriver::new(modem, sysloop.clone(), Some(nvs_part))?;
    let mut esp_wifi = EspWifi::wrap_all(
        wifi_driver,
        EspNetif::new(NetifStack::Sta)?,
        EspNetif::new_with_conf(&netif_conf)?,
    )?;

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
