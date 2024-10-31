use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write,
};

use crate::{config::Config, gpio::Action, wifi};

const HTML_INDEX: &[u8] = include_bytes!("../client/index.html");
const CSS: &[u8] = include_bytes!("../client/index.css");
const HTMX: &[u8] = include_bytes!("../client/htmx.min.js.gz");

pub fn serve(
    config: &Config,
    wifi: Arc<Mutex<wifi::RequestHandler>>,
    action: Arc<Mutex<dyn Action>>,
) -> anyhow::Result<EspHttpServer<'static>> {
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
            action.lock().unwrap().exec()?;
            req.into_ok_response()?;
            Ok(())
        },
    )?;

    let hostname = config.wifi.hostname.clone();

    server.fn_handler("/api/addr", Method::Get, move |req| -> anyhow::Result<()> {
        let mut resp = req.into_response(200, None, &[("Content-Type", "text/html")])?;
        let addr = wifi.lock().unwrap().request(wifi::IpAddrRequest)?;
        // let addr = if wifi.driver().is_sta_connected()? {
        //     Some(wifi.sta_netif().get_ip_info()?.ip)
        // } else {
        //     None
        // };

        let body = match *addr {
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

    Ok(server)
}
