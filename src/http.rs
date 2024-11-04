use std::sync::Arc;

use esp_idf_svc::{
    hal::task::queue::Queue,
    http::{
        server::{Configuration, Connection, EspHttpServer, Request},
        Method,
    },
    io::Write,
    nvs::{EspNvsPartition, NvsPartitionId},
};
use serde::Deserialize;

use crate::config;

const HTML_INDEX: &[u8] = include_bytes!("../client/index.html");
const HTML_SETTINGS: &[u8] = include_bytes!("../client/settings.html");
const CSS: &[u8] = include_bytes!("../client/index.css");
const JS: &[u8] = include_bytes!("../client/index.js");
const HTMX: &[u8] = include_bytes!("../client/htmx.min.js.gz");

const BUF_SIZE: usize = 1024;
const HTTP_SERVER_STACK_SIZE: usize = 20480;

fn html_resp<C>(req: Request<C>, status: u16, body: impl AsRef<[u8]>) -> anyhow::Result<()>
where
    C: Connection,
    C::Error: std::error::Error + Send + Sync + 'static,
{
    req.into_response(status, None, &[("Content-Type", "text/html")])?
        .write_all(body.as_ref())?;

    Ok(())
}

fn read_body<C>(req: &mut Request<C>) -> anyhow::Result<Vec<u8>>
where
    C: Connection,
    C::Error: std::error::Error + Send + Sync + 'static,
{
    let mut body = Vec::new();
    let mut buf = vec![0; BUF_SIZE];

    while let Ok(len) = req.read(&mut buf) {
        if len == 0 {
            break;
        }

        body.extend_from_slice(&buf[..len]);
    }

    Ok(body)
}

#[derive(Debug, Deserialize)]
struct WifiSettingsFormBody {
    ssid: String,
    password: String,
}

impl WifiSettingsFormBody {
    fn save<P: NvsPartitionId>(&self, nvs_part: EspNvsPartition<P>) -> anyhow::Result<()> {
        config::set_wifi_ssid(
            nvs_part.clone(),
            if self.ssid.trim().is_empty() {
                None
            } else {
                Some(&self.ssid)
            },
        )?;

        config::set_wifi_password(
            nvs_part.clone(),
            if self.password.trim().is_empty() {
                None
            } else {
                Some(&self.password)
            },
        )?;

        log::info!("WiFi settings saved.");

        Ok(())
    }
}

pub fn serve<P>(
    nvs_part: EspNvsPartition<P>,
    pin_trigger_queue: Arc<Queue<()>>,
) -> anyhow::Result<EspHttpServer<'static>>
where
    P: NvsPartitionId + Send + Sync + 'static,
{
    let server_config = Configuration {
        http_port: config::http_port()?,
        stack_size: HTTP_SERVER_STACK_SIZE,
        ..Default::default()
    };

    let mut server = EspHttpServer::new(&server_config)?;

    //
    // Static assets
    //

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
        "/assets/index.js",
        Method::Get,
        |req| -> anyhow::Result<()> {
            let headers = [("Content-Type", "application/javascript")];

            let mut resp = req.into_response(200, None, &headers)?;
            resp.write_all(JS)?;

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

    //
    // HTML pages
    //

    server.fn_handler("/", Method::Get, |req| -> anyhow::Result<()> {
        html_resp(req, 200, HTML_INDEX)
    })?;

    server.fn_handler("/settings", Method::Get, |req| -> anyhow::Result<()> {
        html_resp(req, 200, HTML_SETTINGS)
    })?;

    //
    // API endpoints
    //

    server.fn_handler(
        "/api/activate",
        Method::Post,
        move |req| -> anyhow::Result<()> {
            // Don't block if the queue is full.
            if pin_trigger_queue.send_back((), 0).is_err() {
                log::info!("GPIO output pin is already active. Skipping this pulse.");
            }

            req.into_ok_response()?;

            Ok(())
        },
    )?;

    let user_nvs_part = nvs_part.clone();

    server.fn_handler("/api/addr", Method::Get, move |req| -> anyhow::Result<()> {
        let addr = config::wifi_ip_addr(user_nvs_part.clone())?;

        html_resp(
            req,
            200,
            &match addr {
                Some(addr) => format!(
                    "
                    <p>Your Squirtinator is connected to WiFi.</p>
                    <p>
                      http://{}.local<br />
                      http://{}
                    </p>
                    ",
                    &config::wifi_hostname()?,
                    addr,
                ),
                None => String::from(
                    "
                    <p>Your Squirtinator is not connected to WiFi.</p>
                    ",
                ),
            },
        )?;

        Ok(())
    })?;

    let user_nvs_part = nvs_part.clone();

    server.fn_handler("/api/min-freq", Method::Get, move |req| -> anyhow::Result<()> {
        html_resp(
            req,
            200,
            &format!(
                r#"
                <input id="min-freq-input" type="range" name="min_freq" value="{default}" min="{min}" max="{max}"/>
                <span id="min-freq-value" class="slider-value">{default}</span>
                "#,
                default=config::freq_default_min(user_nvs_part.clone())?,
                min=config::freq_lower_bound(user_nvs_part.clone())?,
                max=config::freq_upper_bound(user_nvs_part.clone())?,
            ),
        )
    })?;

    let user_nvs_part = nvs_part.clone();

    server.fn_handler("/api/max-freq", Method::Get, move |req| -> anyhow::Result<()> {
        html_resp(
            req,
            200,
            &format!(
                r#"
                <input id="max-freq-input" type="range" name="max_freq" value="{default}" min="{min}" max="{max}"/>
                <span id="max-freq-value" class="slider-value">{default}</span>
                "#,
                default=config::freq_default_max(user_nvs_part.clone())?,
                min=config::freq_lower_bound(user_nvs_part.clone())?,
                max=config::freq_upper_bound(user_nvs_part.clone())?,
            ),
        )
    })?;

    let user_nvs_part = nvs_part.clone();

    server.fn_handler(
        "/api/settings/wifi",
        Method::Put,
        move |mut req| -> anyhow::Result<()> {
            let req_body = read_body(&mut req)?;
            let form_body = serde_urlencoded::from_bytes::<WifiSettingsFormBody>(&req_body)?;

            form_body.save(user_nvs_part.clone())?;

            html_resp(
                req,
                200,
                "<p>WiFi settings saved. Restart the device to connect to the new network.</p>",
            )?;

            Ok(())
        },
    )?;

    let user_nvs_part = nvs_part.clone();

    server.fn_handler(
        "/api/settings/wifi/ssid",
        Method::Get,
        move |req| -> anyhow::Result<()> {
            // There's no need to include the HTMX `hx-*` attributes when swapping this element in
            // for the one currently on the page, because this API endpoint will only be triggered
            // on first page load.
            if let Some(ssid) = config::wifi_ssid(user_nvs_part.clone())? {
                html_resp(
                    req,
                    200,
                    format!(
                        r##"
                        <input
                          id="ssid-input"
                          name="ssid"
                          type="text"
                          value="{}"
                        />
                        "##,
                        ssid
                    ),
                )?;
            } else {
                req.into_response(204, None, &[])?;
            }

            Ok(())
        },
    )?;

    Ok(server)
}
