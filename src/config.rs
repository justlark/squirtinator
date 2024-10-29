use std::time::Duration;

use anyhow::bail;
use esp_idf_svc::hal::gpio::{AnyOutputPin, Pins};
use serde::Deserialize;

const TOML_CONFIG: &str = include_str!("../config.toml");

#[derive(Debug, Deserialize)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: Option<String>,
    pub hidden: bool,
    pub channel: Option<u8>,
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
    pub http: HttpConfig,
    pub io: IoConfig,
}

impl Config {
    pub fn read() -> anyhow::Result<Self> {
        log::info!("Reading TOML config file.");
        Ok(toml::from_str(TOML_CONFIG)?)
    }
}
