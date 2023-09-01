
use std::sync::mpsc;
use log::*;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use embedded_svc::{http::Method, io::Write};
use crate::fsm::Event;

pub fn start_http_server(tx: &mpsc::Sender<Event>) -> EspHttpServer {
    let mut server = EspHttpServer::new(&Configuration::default()).unwrap();
    let tx1 = tx.clone();
    server
        .fn_handler("/", Method::Get, move |request| {
            info!("http server: recibido request /");
            let html = index_html();
            let mut response = request.into_ok_response()?;
            response.write_all(html.as_bytes())?;

            // TODO: Leer formulario HTML con credenciales reales
            let event = Event::Credentials {
                wifi_ssid: "myssid".into(),
                wifi_psk: "mypassword".into(),
                mqtt_host: "test.mosquitto.org".into(),
                mqtt_user: None,
                mqtt_passwd: None,
            };
            tx1.send(event).unwrap();

            Ok(())
        })
        .unwrap();

    server
}

fn templated(content: impl AsRef<str>) -> String {
    format!(
        r#"
<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <title>esp-rs web server</title>
    </head>
    <body>
        {}
    </body>
</html>
"#,
        content.as_ref()
    )
}

fn index_html() -> String {
    templated("Hello from ESP32-C3!")
}
