use serde::Deserialize;

const TOML_CONFIG: &str = include_str!("../config.toml");

#[derive(Debug, Deserialize)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub wifi: WifiConfig,
}

impl Config {
    pub fn read() -> anyhow::Result<Self> {
        log::info!("Reading TOML config file.");
        Ok(toml::from_str(TOML_CONFIG)?)
    }
}
