use std::sync::Arc;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex as RawMutex, mutex::Mutex};
use esp_idf_svc::{
    hal::task::block_on,
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write,
    nvs::{EspNvs, EspNvsPartition, NvsPartitionId},
    wifi::{AsyncWifi, EspWifi},
};
use serde::Deserialize;

use crate::{
    config::{user_nvs, Config},
    gpio::Action,
};

const HTML_INDEX: &[u8] = include_bytes!("../client/index.html");
const HTML_SETTINGS: &[u8] = include_bytes!("../client/settings.html");
const CSS: &[u8] = include_bytes!("../client/index.css");
const HTMX: &[u8] = include_bytes!("../client/htmx.min.js.gz");

const BUF_SIZE: usize = 1024;

pub fn serve<P>(
    config: &Config,
    wifi: Arc<Mutex<RawMutex, AsyncWifi<EspWifi<'static>>>>,
    nvs_part: EspNvsPartition<P>,
    action: Arc<Mutex<RawMutex, dyn Action>>,
) -> anyhow::Result<EspHttpServer<'static>>
where
    P: NvsPartitionId + Send + Sync + 'static,
{
    let server_config = Configuration {
        http_port: config.http.port,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_config)?;

    server.fn_handler("/", Method::Get, |req| -> anyhow::Result<()> {
        let headers = [("Content-Type", "text/html")];

        let mut resp = req.into_response(200, None, &headers)?;
        resp.write_all(HTML_INDEX)?;

        Ok(())
    })?;

    server.fn_handler("/settings", Method::Get, |req| -> anyhow::Result<()> {
        let headers = [("Content-Type", "text/html")];

        let mut resp = req.into_response(200, None, &headers)?;
        resp.write_all(HTML_SETTINGS)?;

        Ok(())
    })?;

    server.fn_handler(
        "/assets/index.css",
        Method::Get,
        |req| -> anyhow::Result<()> {
            let headers = [("Content-Type", "text/css")];

            let mut resp = req.into_response(200, None, &headers)?;
            resp.write_all(CSS)?;

            Ok(())
        },
    )?;

    server.fn_handler(
        "/assets/htmx.min.js",
        Method::Get,
        |req| -> anyhow::Result<()> {
            let headers = [
                ("Content-Type", "text/javascript"),
                ("Content-Encoding", "gzip"),
            ];

            let mut resp = req.into_response(200, None, &headers)?;
            resp.write_all(HTMX)?;

            Ok(())
        },
    )?;

    server.fn_handler(
        "/api/activate",
        Method::Post,
        move |req| -> anyhow::Result<()> {
            block_on(action.lock()).exec()?;
            req.into_ok_response()?;
            Ok(())
        },
    )?;

    let hostname = config.wifi.hostname.clone();

    server.fn_handler("/api/addr", Method::Get, move |req| -> anyhow::Result<()> {
        let mut resp = req.into_response(200, None, &[("Content-Type", "text/html")])?;

        let wifi = block_on(wifi.lock());

        let addr = if wifi.wifi().driver().is_sta_connected()? {
            Some(wifi.wifi().sta_netif().get_ip_info()?.ip)
        } else {
            None
        };

        let body = match addr {
            Some(addr) => format!(
                "
                <p>Your Squirtinator is connected to WiFi:</p>
                <p>
                  http://{}.local<br />
                  http://{}
                </p>
                ",
                &hostname, addr,
            ),
            None => String::from(
                "
                <p>Your Squirtinator is not connected to WiFi.</p>
                ",
            ),
        };

        resp.write_all(body.as_bytes())?;

        Ok(())
    })?;

    #[derive(Debug, Deserialize)]
    struct WifiSettingsFormBody {
        ssid: String,
        password: String,
    }

    impl WifiSettingsFormBody {
        fn save<P: NvsPartitionId>(&self, nvs: &mut EspNvs<P>) -> anyhow::Result<()> {
            nvs.set_str("wifi.ssid", &self.ssid)?;

            if !self.password.is_empty() {
                nvs.set_str("wifi.password", &self.password)?;
            }

            log::info!("WiFi settings saved.");

            Ok(())
        }
    }

    server.fn_handler(
        "/api/settings/wifi",
        Method::Put,
        move |mut req| -> anyhow::Result<()> {
            let mut body = Vec::new();
            let mut buf = vec![0; BUF_SIZE];

            while let Ok(len) = req.read(&mut buf) {
                if len == 0 {
                    break;
                }

                body.extend_from_slice(&buf[..len]);
            }

            let form_body = serde_urlencoded::from_bytes::<WifiSettingsFormBody>(&body)?;

            let mut user_nvs = user_nvs(nvs_part.clone())?;
            form_body.save(&mut user_nvs)?;

            req.into_ok_response()?;

            Ok(())
        },
    )?;

    Ok(server)
}
