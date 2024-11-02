use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, bail};
use esp_idf_svc::hal::gpio::{AnyOutputPin, Pins};
use esp_idf_svc::ipv4::{self, Ipv4Addr};
use esp_idf_svc::netif::NetifConfiguration;
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsPartitionId};
use esp_idf_svc::sys::ESP_ERR_NVS_INVALID_LENGTH;
use esp_idf_svc::wifi;
use serde::Deserialize;

const TOML_CONFIG: &str = include_str!("../config.toml");

const NVS_USER_NAMESPACE: &str = "user";

// We store persistent user preferences in their own NVS namespace.
//
// If you check the Justfile, you'll see that we erase the NVS partition before flashing the
// firmware. This is so that defaults in the config.toml file take precedence when the device is
// first flashed, but can be overwritten by the user via the UI.
fn user_nvs<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<EspNvs<P>> {
    Ok(EspNvs::new(nvs_part, NVS_USER_NAMESPACE, true)?)
}

static DEFAULT_CONFIG: OnceLock<Config> = OnceLock::new();

pub fn init_config() -> anyhow::Result<()> {
    if DEFAULT_CONFIG.set(Config::from_file()?).is_err() {
        log::warn!("Config already initialized.");
    }

    Ok(())
}

fn default_config() -> anyhow::Result<&'static Config> {
    DEFAULT_CONFIG
        .get()
        .ok_or_else(|| anyhow!("Config was never initialized."))
}

#[derive(Debug, Deserialize)]
struct StaticWifiConfig {
    addr: String,
    gateway: String,
    mask: u8,
}

#[derive(Debug, Deserialize)]
struct WifiConfig {
    ssid: Option<String>,
    password: Option<String>,
    hostname: String,
    #[serde(rename = "static")]
    static_ip: Option<StaticWifiConfig>,
}

#[derive(Debug, Deserialize)]
struct AccessPointConfig {
    ssid: String,
    password: Option<String>,
    hidden: bool,
    channel: Option<u8>,
    gateway: String,
}

#[derive(Debug, Deserialize)]
struct HttpConfig {
    port: u16,
}

#[derive(Debug, Deserialize)]
struct IoConfig {
    pin: u8,
    duration: u64,
}

#[derive(Debug, Deserialize)]
struct Config {
    wifi: WifiConfig,
    access_point: AccessPointConfig,
    http: HttpConfig,
    io: IoConfig,
}

impl Config {
    fn from_file() -> anyhow::Result<Self> {
        log::info!("Reading TOML config file.");
        Ok(toml::from_str(TOML_CONFIG)?)
    }
}

trait ValueSource<T> {
    fn get_value(&mut self, key: &str) -> anyhow::Result<Option<T>>;
}

impl<P> ValueSource<String> for EspNvs<P>
where
    P: NvsPartitionId,
{
    fn get_value(&mut self, key: &str) -> anyhow::Result<Option<String>> {
        // The NVS API will panic if the buffer isn't large enough to store the string. Let's set a
        // reasonable upper bound for the kinds of data we'll be storing.
        const BUF_SIZE: usize = 256;
        let mut buf = vec![0; BUF_SIZE];

        match self.get_str(key, &mut buf) {
            Ok(value) => Ok(value.map(ToOwned::to_owned)),
            Err(err) if err.code() == ESP_ERR_NVS_INVALID_LENGTH => {
                log::error!(
                    "Attempted to read a string value larger than {} bytes from NVS.",
                    BUF_SIZE
                );

                Err(err.into())
            }
            Err(err) => Err(err.into()),
        }
    }
}

pub fn wifi_is_configured<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<bool> {
    let ssid = wifi_ssid(nvs_part)?;
    Ok(ssid.is_some() && !ssid.as_ref().unwrap().is_empty())
}

// This isn't configuration per se; this is where we store the current IP address on the local
// network so that we can display it in the UI.
pub fn wifi_ip_addr<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
) -> anyhow::Result<Option<Ipv4Addr>> {
    let mut nvs = user_nvs(nvs_part)?;

    Ok(nvs
        .get_value("wifi.ip_addr")?
        .map(|addr| addr.parse())
        .transpose()?)
}

pub fn set_wifi_ip_addr<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
    ip_addr: Option<Ipv4Addr>,
) -> anyhow::Result<()> {
    let mut nvs = user_nvs(nvs_part)?;

    if let Some(ip_addr) = ip_addr {
        nvs.set_str("wifi.ip_addr", &ip_addr.to_string())?;
    } else {
        nvs.remove("wifi.ip_addr")?;
    };

    Ok(())
}

pub fn wifi_ssid<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
) -> anyhow::Result<Option<String>> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs.get_value("wifi.ssid")?.or(default.wifi.ssid.clone()))
}

pub fn set_wifi_ssid<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
    ssid: Option<&str>,
) -> anyhow::Result<()> {
    let mut nvs = user_nvs(nvs_part)?;

    if let Some(ssid) = ssid {
        nvs.set_str("wifi.ssid", ssid)?;
    } else {
        nvs.remove("wifi.ssid")?;
    }

    Ok(())
}

pub fn wifi_password<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
) -> anyhow::Result<Option<String>> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs
        .get_value("wifi.password")?
        .or(default.wifi.password.clone()))
}

pub fn set_wifi_password<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let mut nvs = user_nvs(nvs_part)?;

    if let Some(password) = password {
        nvs.set_str("wifi.password", password)?;
    } else {
        nvs.remove("wifi.password")?;
    }

    Ok(())
}

pub fn wifi_hostname() -> anyhow::Result<String> {
    default_config().map(|config| config.wifi.hostname.clone())
}

pub fn wifi_static_ip_addr() -> anyhow::Result<Option<Ipv4Addr>> {
    match &default_config()?.wifi.static_ip {
        Some(config) => config
            .addr
            .parse()
            .map_err(|_| anyhow!("Invalid IP address: {}", config.addr))
            .map(Some),
        None => Ok(None),
    }
}

pub fn wifi_static_ip_gateway() -> anyhow::Result<Option<Ipv4Addr>> {
    match &default_config()?.wifi.static_ip {
        Some(config) => config
            .gateway
            .parse()
            .map_err(|_| anyhow!("Invalid gateway IP address: {}", config.gateway))
            .map(Some),
        None => Ok(None),
    }
}

pub fn wifi_static_ip_mask() -> anyhow::Result<Option<ipv4::Mask>> {
    default_config().map(|config| {
        config
            .wifi
            .static_ip
            .as_ref()
            .map(|config| ipv4::Mask(config.mask))
    })
}

pub fn access_point_ssid() -> anyhow::Result<String> {
    default_config().map(|config| config.access_point.ssid.clone())
}

pub fn access_point_password() -> anyhow::Result<Option<String>> {
    default_config().map(|config| config.access_point.password.clone())
}

pub fn access_point_hidden() -> anyhow::Result<bool> {
    default_config().map(|config| config.access_point.hidden)
}

pub fn access_point_channel() -> anyhow::Result<Option<u8>> {
    default_config().map(|config| config.access_point.channel)
}

pub fn access_point_gateway() -> anyhow::Result<Ipv4Addr> {
    let gateway = &default_config()?.access_point.gateway;

    gateway
        .parse()
        .map_err(|_| anyhow!("Invalid gateway IP address: {}", gateway))
}

pub fn http_port() -> anyhow::Result<u16> {
    default_config().map(|config| config.http.port)
}

pub fn io_pin(pins: Pins) -> anyhow::Result<AnyOutputPin> {
    let pin_num = default_config()?.io.pin;

    // Only match on the GPIO pins that are available on the ESP32-C3-DevKit-RUST-1 board.
    Ok(match pin_num {
        0 => pins.gpio0.into(),
        1 => pins.gpio1.into(),
        2 => pins.gpio2.into(),
        3 => pins.gpio3.into(),
        4 => pins.gpio4.into(),
        5 => pins.gpio5.into(),
        6 => pins.gpio6.into(),
        7 => pins.gpio7.into(),
        8 => pins.gpio8.into(),
        9 => pins.gpio9.into(),
        10 => pins.gpio10.into(),
        18 => pins.gpio18.into(),
        19 => pins.gpio19.into(),
        20 => pins.gpio20.into(),
        21 => pins.gpio21.into(),
        _ => bail!("Invalid GPIO pin number: {}", pin_num),
    })
}

pub fn io_duration() -> anyhow::Result<Duration> {
    Ok(Duration::from_millis(
        default_config().map(|config| config.io.duration)?,
    ))
}

pub fn wifi_client_config<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
) -> anyhow::Result<Option<wifi::ClientConfiguration>> {
    let ssid = wifi_ssid(nvs_part.clone())?;
    let password = wifi_password(nvs_part.clone())?;

    Ok(match ssid {
        Some(ssid) if !ssid.is_empty() => Some(wifi::ClientConfiguration {
            ssid: ssid
                .as_str()
                .try_into()
                .map_err(|_| anyhow!("WiFi SSID is too long: {}", ssid))?,
            auth_method: match &password {
                Some(password) if !password.is_empty() => wifi::AuthMethod::default(),
                _ => wifi::AuthMethod::None,
            },
            password: password
                .as_deref()
                .unwrap_or_default()
                .try_into()
                .map_err(|_| {
                    anyhow!(
                        "WiFi password is too long: {}",
                        password.as_deref().unwrap_or_default()
                    )
                })?,
            ..Default::default()
        }),
        _ => None,
    })
}

pub fn wifi_netif_config() -> anyhow::Result<NetifConfiguration> {
    let mut sta_config = NetifConfiguration::wifi_default_client();

    let hostname = wifi_hostname()?;

    sta_config.ip_configuration = match (
        wifi_static_ip_addr()?,
        wifi_static_ip_mask()?,
        wifi_static_ip_gateway()?,
    ) {
        (Some(addr), Some(mask), Some(gateway)) => {
            log::info!("Setting WiFi client IP address to: {}", addr);

            ipv4::Configuration::Client(ipv4::ClientConfiguration::Fixed(ipv4::ClientSettings {
                ip: addr,
                subnet: ipv4::Subnet { gateway, mask },
                ..Default::default()
            }))
        }
        _ => {
            log::info!("Setting WiFi client hostname to: {}", hostname);

            ipv4::Configuration::Client(ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                hostname: Some(
                    hostname
                        .as_str()
                        .try_into()
                        .map_err(|_| anyhow!("WiFi hostname is too long: {}", hostname))?,
                ),
            }))
        }
    };

    Ok(sta_config)
}

pub fn access_point_config() -> anyhow::Result<wifi::AccessPointConfiguration> {
    let default_config = wifi::AccessPointConfiguration::default();

    let ssid = access_point_ssid()?;
    let password = access_point_password()?;
    let channel = access_point_channel()?;

    Ok(wifi::AccessPointConfiguration {
        ssid: ssid
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("WiFi SSID is too long: {}", ssid))?,
        ssid_hidden: access_point_hidden()?,
        auth_method: match &password {
            Some(password) if !password.is_empty() => wifi::AuthMethod::default(),
            _ => wifi::AuthMethod::None,
        },
        password: password
            .as_deref()
            .unwrap_or_default()
            .try_into()
            .map_err(|_| {
                anyhow!(
                    "WiFi password is too long: {}",
                    password.as_deref().unwrap_or_default()
                )
            })?,
        channel: channel.unwrap_or(default_config.channel),
        ..default_config
    })
}

pub fn access_point_netif_config() -> anyhow::Result<NetifConfiguration> {
    let mut router_config = NetifConfiguration::wifi_default_router();

    // Set a static, predictable gateway IP address.
    if let ipv4::Configuration::Router(config) = &mut router_config.ip_configuration {
        config.subnet.gateway = access_point_gateway()?;
    }

    Ok(router_config)
}

pub fn wifi_config<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
) -> anyhow::Result<wifi::Configuration> {
    let ap_config = access_point_config()?;

    // The device always operates as an access point (AP mode), but operating as a client (STA
    // mode) is optional.
    match wifi_client_config(nvs_part)? {
        Some(client_config) => Ok(wifi::Configuration::Mixed(client_config, ap_config)),
        None => Ok(wifi::Configuration::AccessPoint(ap_config)),
    }
}
