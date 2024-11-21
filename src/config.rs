use std::fmt;
use std::sync::OnceLock;

use anyhow::{anyhow, bail};
use esp_idf_svc::hal::gpio;
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
    sda_pin: u8,
    scl_pin: u8,
    address: u8,
    message: Vec<u8>,
    baudrate: u32,
    timeout: u32,
}

#[derive(Debug, Deserialize)]
struct FreqConfig {
    lower_bound: u32,
    upper_bound: u32,
    default_min: u32,
    default_max: u32,
}

#[derive(Debug, Deserialize)]
struct Config {
    wifi: WifiConfig,
    access_point: AccessPointConfig,
    http: HttpConfig,
    io: IoConfig,
    frequency: FreqConfig,
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

impl<P> ValueSource<u32> for EspNvs<P>
where
    P: NvsPartitionId,
{
    fn get_value(&mut self, key: &str) -> anyhow::Result<Option<u32>> {
        Ok(self.get_u32(key)?)
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
        .map(|addr: String| addr.parse())
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

struct GpioPins {
    gpio0: Option<gpio::Gpio0>,
    gpio1: Option<gpio::Gpio1>,
    gpio2: Option<gpio::Gpio2>,
    gpio3: Option<gpio::Gpio3>,
    gpio4: Option<gpio::Gpio4>,
    gpio5: Option<gpio::Gpio5>,
    gpio6: Option<gpio::Gpio6>,
    gpio7: Option<gpio::Gpio7>,
    gpio8: Option<gpio::Gpio8>,
    gpio9: Option<gpio::Gpio9>,
    gpio10: Option<gpio::Gpio10>,
    gpio18: Option<gpio::Gpio18>,
    gpio19: Option<gpio::Gpio19>,
    gpio20: Option<gpio::Gpio20>,
    gpio21: Option<gpio::Gpio21>,
}

impl fmt::Debug for GpioPins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpioPins").finish_non_exhaustive()
    }
}

impl From<gpio::Pins> for GpioPins {
    fn from(pins: gpio::Pins) -> Self {
        Self {
            gpio0: Some(pins.gpio0),
            gpio1: Some(pins.gpio1),
            gpio2: Some(pins.gpio2),
            gpio3: Some(pins.gpio3),
            gpio4: Some(pins.gpio4),
            gpio5: Some(pins.gpio5),
            gpio6: Some(pins.gpio6),
            gpio7: Some(pins.gpio7),
            gpio8: Some(pins.gpio8),
            gpio9: Some(pins.gpio9),
            gpio10: Some(pins.gpio10),
            gpio18: Some(pins.gpio18),
            gpio19: Some(pins.gpio19),
            gpio20: Some(pins.gpio20),
            gpio21: Some(pins.gpio21),
        }
    }
}

impl GpioPins {
    pub fn io_pin(&mut self, pin: u8) -> anyhow::Result<gpio::AnyIOPin> {
        let maybe_any_pin = match pin {
            0 => self.gpio0.take().map(Into::into),
            1 => self.gpio1.take().map(Into::into),
            2 => self.gpio2.take().map(Into::into),
            3 => self.gpio3.take().map(Into::into),
            4 => self.gpio4.take().map(Into::into),
            5 => self.gpio5.take().map(Into::into),
            6 => self.gpio6.take().map(Into::into),
            7 => self.gpio7.take().map(Into::into),
            8 => self.gpio8.take().map(Into::into),
            9 => self.gpio9.take().map(Into::into),
            10 => self.gpio10.take().map(Into::into),
            18 => self.gpio18.take().map(Into::into),
            19 => self.gpio19.take().map(Into::into),
            20 => self.gpio20.take().map(Into::into),
            21 => self.gpio21.take().map(Into::into),
            _ => bail!("Invalid GPIO pin number: {}", pin),
        };

        maybe_any_pin.ok_or_else(|| anyhow!("GPIO pin {} is already in use.", pin))
    }
}

pub struct IoPins {
    pins: GpioPins,
    sda_pin: u8,
    scl_pin: u8,
}

impl fmt::Debug for IoPins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IoPins")
            .field("sda_pin", &self.sda_pin)
            .field("scl_pin", &self.scl_pin)
            .finish_non_exhaustive()
    }
}

impl IoPins {
    pub fn sda_pin(&mut self) -> anyhow::Result<gpio::AnyIOPin> {
        self.pins.io_pin(self.sda_pin)
    }

    pub fn scl_pin(&mut self) -> anyhow::Result<gpio::AnyIOPin> {
        self.pins.io_pin(self.scl_pin)
    }
}

pub fn io_pins(pins: gpio::Pins) -> anyhow::Result<IoPins> {
    Ok(IoPins {
        pins: pins.into(),
        sda_pin: default_config()?.io.sda_pin,
        scl_pin: default_config()?.io.scl_pin,
    })
}

pub fn io_address() -> anyhow::Result<u8> {
    default_config().map(|config| config.io.address)
}

pub fn io_message() -> anyhow::Result<Vec<u8>> {
    default_config().map(|config| config.io.message.clone())
}

pub fn io_baudrate() -> anyhow::Result<u32> {
    default_config().map(|config| config.io.baudrate)
}

pub fn io_timeout() -> anyhow::Result<u32> {
    default_config().map(|config| config.io.timeout)
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

pub fn freq_lower_bound<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<u32> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs
        .get_value("freq.lower_bound")?
        .unwrap_or(default.frequency.lower_bound))
}

pub fn freq_upper_bound<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<u32> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs
        .get_value("freq.upper_bound")?
        .unwrap_or(default.frequency.upper_bound))
}

pub fn freq_min<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<u32> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs
        .get_value("freq.min")?
        .unwrap_or(default.frequency.default_min))
}

pub fn set_freq_min<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
    default_min: u32,
) -> anyhow::Result<()> {
    let nvs = user_nvs(nvs_part)?;
    nvs.set_u32("freq.min", default_min)?;

    Ok(())
}

pub fn freq_max<P: NvsPartitionId>(nvs_part: EspNvsPartition<P>) -> anyhow::Result<u32> {
    let mut nvs = user_nvs(nvs_part)?;
    let default = default_config()?;
    Ok(nvs
        .get_value("freq.max")?
        .unwrap_or(default.frequency.default_max))
}

pub fn set_freq_max<P: NvsPartitionId>(
    nvs_part: EspNvsPartition<P>,
    default_max: u32,
) -> anyhow::Result<()> {
    let nvs = user_nvs(nvs_part)?;
    nvs.set_u32("freq.max", default_max)?;

    Ok(())
}
