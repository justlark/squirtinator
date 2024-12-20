use std::sync::Arc;

use esp_idf_svc::{
    http::{
        server::{Configuration, Connection, EspHttpServer, Request},
        Method,
    },
    io::Write,
    nvs::{EspNvsPartition, NvsPartitionId},
};
use serde::Deserialize;

use crate::{config, io};

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

#[derive(Debug, Deserialize)]
struct FreqSettingsFormBody {
    min_freq: u32,
    max_freq: u32,
}

impl FreqSettingsFormBody {
    fn save<P: NvsPartitionId>(&self, nvs_part: EspNvsPartition<P>) -> anyhow::Result<()> {
        config::set_freq_min(nvs_part.clone(), self.min_freq)?;
        config::set_freq_max(nvs_part.clone(), self.max_freq)?;

        log::info!("Frequency settings saved.");

        Ok(())
    }
}

pub fn serve<P>(
    nvs_part: EspNvsPartition<P>,
    signaler: Arc<io::Signaler>,
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

    let this_signaler = Arc::clone(&signaler);

    server.fn_handler(
        "/api/fire",
        Method::Post,
        move |req| -> anyhow::Result<()> {
            this_signaler.send(io::Signal::Fire);

            req.into_ok_response()?;

            Ok(())
        },
    )?;

    let this_signaler = Arc::clone(&signaler);

    server.fn_handler(
        "/api/start",
        Method::Post,
        move |req| -> anyhow::Result<()> {
            this_signaler.send(io::Signal::StartAuto);

            html_resp(
                req,
                200,
                r#"
                <button
                  id="auto-button"
                  role="switch"
                  aria-checked="true"
                  hx-post="/api/stop"
                  hx-swap="outerHTML"
                >
                  AUTO
                </button>
                "#,
            )
        },
    )?;

    let this_signaler = Arc::clone(&signaler);

    server.fn_handler(
        "/api/stop",
        Method::Post,
        move |req| -> anyhow::Result<()> {
            this_signaler.send(io::Signal::StopAuto);

            html_resp(
                req,
                200,
                r#"
                <button
                  id="auto-button"
                  role="switch"
                  aria-checked="false"
                  hx-post="/api/start"
                  hx-swap="outerHTML"
                >
                  AUTO
                </button>
                "#,
            )
        },
    )?;

    let this_signaler = Arc::clone(&signaler);

    server.fn_handler("/api/auto", Method::Get, move |req| -> anyhow::Result<()> {
        let is_auto = this_signaler.is_auto();

        let endpoint = if is_auto { "/api/stop" } else { "/api/start" };

        html_resp(
            req,
            200,
            format!(
                r#"
                    <button
                      id="auto-button"
                      role="switch"
                      aria-checked="{is_auto}"
                      hx-post="{endpoint}"
                      hx-swap="outerHTML"
                    >
                      AUTO
                    </button>
                    "#,
                is_auto = is_auto,
                endpoint = endpoint,
            ),
        )
    })?;

    let this_nvs_part = nvs_part.clone();

    server.fn_handler("/api/addr", Method::Get, move |req| -> anyhow::Result<()> {
        let addr = config::wifi_ip_addr(this_nvs_part.clone())?;

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

    let this_nvs_part = nvs_part.clone();

    server.fn_handler(
        "/api/settings/wifi",
        Method::Put,
        move |mut req| -> anyhow::Result<()> {
            let req_body = read_body(&mut req)?;
            let form_body = serde_urlencoded::from_bytes::<WifiSettingsFormBody>(&req_body)?;

            form_body.save(this_nvs_part.clone())?;

            html_resp(
                req,
                200,
                "<p>WiFi settings saved. Restart the device to connect to the new network.</p>",
            )?;

            Ok(())
        },
    )?;

    let this_nvs_part = nvs_part.clone();

    server.fn_handler(
        "/api/settings/wifi/ssid",
        Method::Get,
        move |req| -> anyhow::Result<()> {
            // There's no need to include the HTMX `hx-*` attributes when swapping this element in
            // for the one currently on the page, because this API endpoint will only be triggered
            // on first page load.
            if let Some(ssid) = config::wifi_ssid(this_nvs_part.clone())? {
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

    let this_nvs_part = nvs_part.clone();

    server.fn_handler(
        "/api/settings/freq",
        Method::Put,
        move |mut req| -> anyhow::Result<()> {
            let req_body = read_body(&mut req)?;
            let form_body = serde_urlencoded::from_bytes::<FreqSettingsFormBody>(&req_body)?;

            form_body.save(this_nvs_part.clone())?;

            req.into_status_response(204)?;

            Ok(())
        },
    )?;

    let this_nvs_part = nvs_part.clone();

    server.fn_handler("/api/settings/min-freq", Method::Get, move |req| -> anyhow::Result<()> {
        html_resp(
            req,
            200,
            &format!(
                r#"
                <input id="min-freq-input" type="range" name="min_freq" value="{default}" min="{min}" max="{max}"/>
                <span><span id="min-freq-value" class="slider-value">{default}</span>s</span>
                "#,
                default=config::freq_min(this_nvs_part.clone())?,
                min=config::freq_lower_bound(this_nvs_part.clone())?,
                max=config::freq_upper_bound(this_nvs_part.clone())?,
            ),
        )
    })?;

    let this_nvs_part = nvs_part.clone();

    server.fn_handler("/api/settings/max-freq", Method::Get, move |req| -> anyhow::Result<()> {
        html_resp(
            req,
            200,
            &format!(
                r#"
                <input id="max-freq-input" type="range" name="max_freq" value="{default}" min="{min}" max="{max}"/>
                <span><span id="max-freq-value" class="slider-value">{default}</span>s</span>
                "#,
                default=config::freq_max(this_nvs_part.clone())?,
                min=config::freq_lower_bound(this_nvs_part.clone())?,
                max=config::freq_upper_bound(this_nvs_part.clone())?,
            ),
        )
    })?;

    Ok(server)
}
