use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write,
};

use crate::{config::HttpConfig, gpio::Action};

const HTML_INDEX: &[u8] = include_bytes!("../client/index.html");
const CSS: &[u8] = include_bytes!("../client/index.css");
const HTMX: &[u8] = include_bytes!("../client/htmx.min.js.gz");

pub fn serve(
    config: &HttpConfig,
    action: Arc<Mutex<dyn Action>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let server_config = Configuration {
        http_port: config.port,
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

    Ok(server)
}
