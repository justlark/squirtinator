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
