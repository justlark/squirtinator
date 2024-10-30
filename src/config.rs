use std::time::Duration;

use anyhow::bail;
use esp_idf_svc::hal::gpio::{AnyOutputPin, Pins};
use esp_idf_svc::ipv4::Ipv4Addr;
use esp_idf_svc::wifi;
use serde::Deserialize;

const TOML_CONFIG: &str = include_str!("../config.toml");
const AUTH_METHOD: wifi::AuthMethod = wifi::AuthMethod::WPA2Personal;

#[derive(Debug, Deserialize)]
pub struct WifiConfig {
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub hostname: String,
}

impl WifiConfig {
    pub fn is_configured(&self) -> bool {
        self.ssid.is_some() && !self.ssid.as_ref().unwrap().is_empty()
    }

    pub fn config(&self) -> anyhow::Result<Option<wifi::ClientConfiguration>> {
        Ok(match &self.ssid {
            Some(ssid) if !ssid.is_empty() => Some(wifi::ClientConfiguration {
                ssid: ssid
                    .as_str()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("WiFi SSID is too long."))?,
                auth_method: match &self.password {
                    Some(password) if !password.is_empty() => AUTH_METHOD,
                    _ => wifi::AuthMethod::None,
                },
                password: self
                    .password
                    .as_deref()
                    .unwrap_or_default()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("WiFi password is too long."))?,
                ..Default::default()
            }),
            _ => None,
        })
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
        Ok(self
            .gateway
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|_| anyhow::anyhow!("Invalid gateway IP address."))
            .map(TryInto::<[u8; 4]>::try_into)?
            .map_err(|_| anyhow::anyhow!("Invalid gateway IP address."))?
            .into())
    }

    pub fn config(&self) -> anyhow::Result<wifi::AccessPointConfiguration> {
        let default_config = wifi::AccessPointConfiguration::default();

        Ok(wifi::AccessPointConfiguration {
            ssid: self
                .ssid
                .as_str()
                .try_into()
                .map_err(|_| anyhow::anyhow!("WiFi SSID is too long."))?,
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
                .map_err(|_| anyhow::anyhow!("WiFi password is too long."))?,
            channel: self.channel.unwrap_or(default_config.channel),
            ..default_config
        })
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
        let ap_config = self.access_point.config()?;

        // The device always operates as an access point (AP mode), but operating as a client (STA
        // mode) is optional.
        match self.wifi.config()? {
            Some(client_config) => Ok(wifi::Configuration::Mixed(client_config, ap_config)),
            None => Ok(wifi::Configuration::AccessPoint(ap_config)),
        }
    }
}

impl Config {
    pub fn read() -> anyhow::Result<Self> {
        log::info!("Reading TOML config file.");
        Ok(toml::from_str(TOML_CONFIG)?)
    }
}
