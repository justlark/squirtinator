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
const AUTH_METHOD: wifi::AuthMethod = wifi::AuthMethod::WPA2Personal;

const NVS_USER_NAMESPACE: &str = "user";

// We store persistent user preferences in their own NVS namespace.
//
// If you check the Justfile, you'll see that we erase the NVS partition before flashing the
// firmware. This is so that defaults in the config.toml file take precedence when the device is
// first flashed, but can be overwritten by the user via the UI.
pub fn user_nvs<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<EspNvs<P>> {
    Ok(EspNvs::new(nvs_part, NVS_USER_NAMESPACE, true)?)
}

#[derive(Debug, Deserialize)]
pub struct StaticWifiConfig {
    pub addr: String,
    pub gateway: String,
    pub mask: u8,
}

#[derive(Debug, Deserialize)]
pub struct WifiConfig {
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub hostname: String,
    #[serde(rename = "static")]
    pub static_ip: Option<StaticWifiConfig>,
    pub timeout: u32,
    pub max_attempts: u32,
}

impl WifiConfig {
    pub fn is_configured(&self) -> bool {
        self.ssid.is_some() && !self.ssid.as_ref().unwrap().is_empty()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout.into())
    }

    pub fn wifi_config(&self) -> anyhow::Result<Option<wifi::ClientConfiguration>> {
        Ok(match &self.ssid {
            Some(ssid) if !ssid.is_empty() => Some(wifi::ClientConfiguration {
                ssid: ssid
                    .as_str()
                    .try_into()
                    .map_err(|_| anyhow!("WiFi SSID is too long: {}", ssid))?,
                auth_method: match &self.password {
                    Some(password) if !password.is_empty() => AUTH_METHOD,
                    _ => wifi::AuthMethod::None,
                },
                password: self
                    .password
                    .as_deref()
                    .unwrap_or_default()
                    .try_into()
                    .map_err(|_| {
                        anyhow!(
                            "WiFi password is too long: {}",
                            &self.password.as_deref().unwrap_or_default()
                        )
                    })?,
                ..Default::default()
            }),
            _ => None,
        })
    }

    pub fn netif_config(&self) -> anyhow::Result<NetifConfiguration> {
        let mut sta_config = NetifConfiguration::wifi_default_client();

        sta_config.ip_configuration = match &self.static_ip {
            Some(static_config) => {
                log::info!("Setting WiFi client IP address to: {}", static_config.addr);

                let addr: ipv4::Ipv4Addr = static_config
                    .addr
                    .parse()
                    .map_err(|_| anyhow!("Invalid IP address: {}", static_config.addr))?;

                let gateway: ipv4::Ipv4Addr = static_config
                    .gateway
                    .parse()
                    .map_err(|_| anyhow!("Invalid IP address: {}", static_config.gateway))?;

                ipv4::Configuration::Client(ipv4::ClientConfiguration::Fixed(
                    ipv4::ClientSettings {
                        ip: addr,
                        subnet: ipv4::Subnet {
                            gateway,
                            mask: ipv4::Mask(static_config.mask),
                        },
                        ..Default::default()
                    },
                ))
            }
            None => {
                log::info!("Setting WiFi client hostname to: {}", self.hostname);

                ipv4::Configuration::Client(ipv4::ClientConfiguration::DHCP(
                    ipv4::DHCPClientSettings {
                        hostname: Some(self.hostname.as_str().try_into().map_err(|_| {
                            anyhow!("WiFi hostname is too long: {}", self.hostname)
                        })?),
                    },
                ))
            }
        };

        Ok(sta_config)
    }
}

#[derive(Debug, Deserialize)]
pub struct AccessPointConfig {
    pub ssid: String,
    pub password: Option<String>,
    pub hidden: bool,
    pub channel: Option<u8>,
    pub gateway: String,
}

impl AccessPointConfig {
    pub fn gateway(&self) -> anyhow::Result<Ipv4Addr> {
        self.gateway
            .parse()
            .map_err(|_| anyhow!("Invalid gateway IP address: {}", self.gateway))
    }

    pub fn wifi_config(&self) -> anyhow::Result<wifi::AccessPointConfiguration> {
        let default_config = wifi::AccessPointConfiguration::default();

        Ok(wifi::AccessPointConfiguration {
            ssid: self
                .ssid
                .as_str()
                .try_into()
                .map_err(|_| anyhow!("WiFi SSID is too long: {}", self.ssid))?,
            ssid_hidden: self.hidden,
            auth_method: match &self.password {
                Some(password) if !password.is_empty() => AUTH_METHOD,
                _ => wifi::AuthMethod::None,
            },
            password: self
                .password
                .as_deref()
                .unwrap_or_default()
                .try_into()
                .map_err(|_| {
                    anyhow!(
                        "WiFi password is too long: {}",
                        self.password.as_deref().unwrap_or_default()
                    )
                })?,
            channel: self.channel.unwrap_or(default_config.channel),
            ..default_config
        })
    }

    pub fn netif_config(&self) -> anyhow::Result<NetifConfiguration> {
        let mut router_config = NetifConfiguration::wifi_default_router();

        // Set a static, predictable gateway IP address.
        if let ipv4::Configuration::Router(config) = &mut router_config.ip_configuration {
            config.subnet.gateway = self.gateway()?;
        }

        Ok(router_config)
    }
}

#[derive(Debug, Deserialize)]
pub struct HttpConfig {
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct IoConfig {
    pub pin: u8,
    pub duration: u64,
}

impl IoConfig {
    pub fn pin(&self, pins: Pins) -> anyhow::Result<AnyOutputPin> {
        // Only match on the GPIO pins that are available on the ESP32-C3-DevKit-RUST-1 board.
        Ok(match self.pin {
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
            _ => bail!("Invalid GPIO pin number: {}", self.pin),
        })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration)
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub wifi: WifiConfig,
    pub access_point: AccessPointConfig,
    pub http: HttpConfig,
    pub io: IoConfig,
}

impl Config {
    pub fn wifi_config(&self) -> anyhow::Result<wifi::Configuration> {
        let ap_config = self.access_point.wifi_config()?;

        // The device always operates as an access point (AP mode), but operating as a client (STA
        // mode) is optional.
        match self.wifi.wifi_config()? {
            Some(client_config) => Ok(wifi::Configuration::Mixed(client_config, ap_config)),
            None => Ok(wifi::Configuration::AccessPoint(ap_config)),
        }
    }
}

trait ValueStore<T> {
    fn get_value(&mut self, key: &str) -> anyhow::Result<Option<T>>;
}

impl<P> ValueStore<String> for EspNvs<P>
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

impl Config {
    fn from_file() -> anyhow::Result<Self> {
        log::info!("Reading TOML config file.");
        Ok(toml::from_str(TOML_CONFIG)?)
    }

    pub fn read<P: NvsPartitionId>(nvs: &mut EspNvs<P>) -> anyhow::Result<Self> {
        let default = Self::from_file()?;
        Ok(Config {
            wifi: WifiConfig {
                ssid: nvs.get_value("wifi.ssid")?.or(default.wifi.ssid),
                password: nvs.get_value("wifi.password")?.or(default.wifi.password),
                hostname: default.wifi.hostname,
                static_ip: default.wifi.static_ip,
                timeout: default.wifi.timeout,
                max_attempts: default.wifi.max_attempts,
            },
            access_point: default.access_point,
            http: default.http,
            io: default.io,
        })
    }
}
