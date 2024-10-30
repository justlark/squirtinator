use std::{
    ffi::CStr,
    sync::{Arc, Mutex},
};

use esp_idf_svc::{
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write,
    tls::X509,
};

use crate::{config::HttpConfig, gpio::Action};

const HTML_INDEX: &[u8] = include_bytes!("../client/index.html");
const CSS: &[u8] = include_bytes!("../client/index.css");
const HTMX: &[u8] = include_bytes!("../client/htmx.min.js.gz");

// We cannot use `include_str!()` because we need this to be a null-terminated cstr.
const CA_CERT: &CStr = c"-----BEGIN CERTIFICATE-----
MIIBQDCB86ADAgECAhR+IwDID+HpMG/DxaT1f2fziXBoWTAFBgMrZXAwFjEUMBIG
A1UEAwwLc3F1aXJ0aW4ubWUwHhcNMjQxMDMwMTIxMDQ1WhcNMzQxMDI4MTIxMDQ1
WjAWMRQwEgYDVQQDDAtzcXVpcnRpbi5tZTAqMAUGAytlcAMhADNG4RuOunP4I8kk
1JEVUsTQ9MwIjDLExC+IRPvusyH3o1MwUTAdBgNVHQ4EFgQUyi7sXddTjN1BzXvD
j0GyLIqig0QwHwYDVR0jBBgwFoAUyi7sXddTjN1BzXvDj0GyLIqig0QwDwYDVR0T
AQH/BAUwAwEB/zAFBgMrZXADQQDi7e6QkqajVR8NljP9KsL9LaVq9lL0+A5OnTHl
4OxhNqlqUqUF6W3zm9JMtOWtliUx6VDU5/3Hv/G/cG3wxokM
-----END CERTIFICATE-----";

const PRIV_KEY: &CStr = c"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIGQt6sZuM2xAAlIn7UIb4WwYFFBs7Ft0E13Awpjll5jz
-----END PRIVATE KEY-----";

pub fn serve(
    config: &HttpConfig,
    action: Arc<Mutex<dyn Action>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let ca_cert = X509::pem(CA_CERT);
    let priv_key = X509::pem(PRIV_KEY);

    let server_config = Configuration {
        http_port: config.http_port,
        https_port: config.https_port,
        #[cfg(esp_idf_esp_https_server_enable)]
        server_certificate: Some(ca_cert),
        #[cfg(esp_idf_esp_https_server_enable)]
        private_key: Some(priv_key),
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
