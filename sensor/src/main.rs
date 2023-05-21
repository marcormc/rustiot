// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

//use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
use std::str;
use std::sync::mpsc;
// https://doc.rust-lang.org/std/sync/mpsc/
use std::thread;
use std::time::Duration;
// use std::mem::swap;

// use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
// use esp_idf_sys;
// use esp_idf_sys::{esp, EspError};

use esp_idf_svc::{
    // eventloop::EspSystemEventLoop,
    http::server::{Configuration, EspHttpServer},
    nvs::{EspDefaultNvs, EspDefaultNvsPartition}, errors::EspIOError,
};
use embedded_svc::{http::Method, io::Write};

use embedded_svc::wifi::*;
use esp_idf_hal::peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::*;
use esp_idf_svc::wifi::*;

use anyhow::bail;
use esp_idf_sys::CONFIG_NEWLIB_STDOUT_LINE_ENDING_CRLF;
use log::*;

// const SSID: Option<&'static str> = option_env!("WIFI_SSID");
// const PASS: Option<&'static str> = option_env!("WIFI_PASS");
const SSID: &str = "harpoland";
const PASS: &str = "password";

use esp_idf_svc::netif::*;
// use esp_idf_svc::ping;
// use embedded_svc::ping::Ping;

use std::sync::{Arc, Mutex};

use embedded_svc::storage::RawStorage;

/// Estados de la máquina de estados finitos
#[derive(Debug, PartialEq)]
enum State {
    // Initial { httpserver: Option<EspHttpServer> },
    Initial, // { httpserver: Box<EspHttpServer> },
    Provisioned {
        ssid: String,
        user: String,
        password: String,
    },
    WifiConnected,
    ServerConnected,
    Failure,
}

/// Eventos que se pueden pasar entre threads a la máquina de estados.
#[derive(Debug, Clone)]
enum Event {
    Credentials {
        ssid: String,
        user: String,
        password: String,
    },
    WifiConnected,
    WifiDisconnected,
    MqttConnected,
    MqttDisconnected,
    SensorData(u32),
}

impl State {
    /// Procesa los eventos según el estado actual.
    fn next(self, event: Event) -> State {
        println!("next, state {:?}, event {:?}", self, event);
        match (self, event) {
            (
                State::Initial,
                Event::Credentials {
                    ssid,
                    user,
                    password,
                },
            ) => {
                // println!("ssid={}, user={}, password={}", ssid, user, password);
                info!("Recibido evento de provisionamiento");
                State::Provisioned {
                    ssid,
                    user,
                    password,
                }
            }
            (State::Initial, Event::SensorData(data)) => {
                info!("Ignoring data (initial) {}", data);
                State::Initial
            }
            (
                State::Provisioned {
                    ssid: _,
                    user: _,
                    password: _,
                },
                Event::WifiConnected,
            ) => State::WifiConnected,
            (State::Provisioned { .. }, Event::SensorData(data)) => {
                info!("Ignoring data (initial) {}", data);
                // self.clone()
                State::Failure
            }

            (s, e) => {
                // panic!("Wrong transition {:#?}, {:#?}", s, e);
                error!("Wrong transition {:#?}, {:#?}", s, e);
                //s.clone()
                //State::Failure
                s
            }
        }
    }
}

// struct Fsm<'a> {
struct Fsm<'a> {
    state: State,
    tx: mpsc::Sender<Event>,
    sysloop: EspSystemEventLoop,
    wifi: Box<EspWifi<'a>>,
    nvs: EspDefaultNvs,
    httpserver: Option<EspHttpServer>,
}

impl<'a> Fsm<'a> {
    fn new(
        tx: mpsc::Sender<Event>,
        sysloop: EspSystemEventLoop,
        wifi: Box<EspWifi<'a>>,
        nvs: EspDefaultNvs,
    ) -> Self {
        let mut fsm = Self {
            state: State::Initial,
            tx,
            sysloop,
            wifi,
            nvs,
            httpserver: None,
        };
        fsm.run();
        fsm
    }

    fn process_event(mut self, event: Event) -> Fsm<'a> {
        // Old state is being discarded here (consumed).
        // In the process, the fsm must be consumed too (self.state is mutable ref)
        self.state = self.state.next(event);

        // swap(self.state.next(event), self.state);
        self.run();
        self
    }

    // fn process_event(&mut self, event: Event) {
    //     // Old state is being discarded here (consumed).
    //     self.state = self.state.next(event);
    //     // std::mem::replace(&mut self.state, self.state.next(event));
    //     // std::mem::swap(self.state.next(event), self.state);
    //     self.state.run(
    //         &self.tx,
    //         &mut self.wifi,
    //         self.sysloop.clone(),
    //         &mut self.nvs,
    //     );
    // }

    /// Ejecuta las acciones necesarias al entrar en cada estado
    fn run(
        &mut self, // tx: &mpsc::Sender<Event>,
                   // wifi: &mut Box<EspWifi>,
                   // sysloop: EspSystemEventLoop,
                   // nvs: &mut EspDefaultNvs,
                   // fsm: &mut Fsm,
    ) {
        match &self.state {
            State::Initial => {
                info!("Entering State::Initial.");
                let ssid = read_nvs_string(&mut self.nvs, "ssid").unwrap();
                let password = read_nvs_string(&mut self.nvs, "password").unwrap();
                if let (Some(ssid), Some(password)) = (ssid, password) {
                    info!(
                        "Credentials from NVS: ssid = {}, password = {}",
                        ssid, password
                    );
                    // provisioned: generate event to change state
                    let event = Event::Credentials {
                        ssid,
                        user: String::from("marco"),
                        password,
                    };
                    self.tx.send(event).unwrap();
                } else {
                    info!("Credentials not found in NVS. Activating wifi AP.");
                    wifi_ap_start(&mut self.wifi, &self.sysloop).expect("Error activating AP");
                    info!("Wifi AP started.");
                    // TODO: iniciar servidor HTTP.
                    info!("Se inicia servidor http");
                    self.httpserver = Some(self.start_http_server());

                    // TODO: Provisionamiento temporal para depurar.
                    // let ssid = "harpoland";
                    // let password = "alcachofatoxica";
                    // self.nvs.set_raw("ssid", ssid.as_bytes()).unwrap();
                    // self.nvs.set_raw("password", password.as_bytes()).unwrap();
                }
            }
            State::Provisioned {
                ssid,
                user,
                password,
            } => {
                info!("Entering State::Provisioned.");
                info!("Trying to connect to wifi station.");
                info!("Using credentials {ssid}, {user}, {password}.");
                // TODO: conectar al wifi.
                thread::sleep(Duration::from_millis(10000));
                info!("Sending Event::WifiConnected from State::Provisioned.");
                self.tx.send(Event::WifiConnected).unwrap();
            }
            State::WifiConnected => {
                info!("State WifiConnected. Trying to connect to server.");
            }
            State::ServerConnected => {
                info!("State ServerConnected. Start sending periodic data.");
            }
            State::Failure => {
                error!("Current state is Failure");
            }
        }
    }

    fn start_http_server(&self) -> EspHttpServer {
        let mut server = EspHttpServer::new(&Configuration::default()).unwrap();
        
        let tx1 = self.tx.clone();
        
        server.fn_handler("/", Method::Get, move |request| {
            info!("http server: recibido request /");
            let html = index_html();
            let mut response = request.into_ok_response()?;
            response.write_all(html.as_bytes())?;

            let event = Event::Credentials {
                ssid: String::from("harpoland"),
                user: String::from("marco"),
                password: String::from("alcachofatoxica"),
            };
            tx1.send(event).unwrap();
            
            Ok(())
        }).unwrap();
        
        server
    }
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









// fn test_nvs() -> anyhow::Result<()> {
//     info!("Leyendo credenciales desde NVS");
//     let part = EspDefaultNvsPartition::take()?;
//     let mut nvs = EspDefaultNvs::new(part, "storage", true).unwrap();
//
//     let value = "harpoland";
//     nvs.set_raw("ssid", value.as_bytes())?;
//
//     let exists = nvs.contains("ssid")?;
//     println!("Storage contains ssid: {}", exists);
//
//     if exists {
//         let len = nvs.len("ssid").unwrap().unwrap();
//         println!("ssid len: {}", len);
//
//         let mut buf: [u8; 100] = [0; 100];
//         nvs.get_raw("ssid", &mut buf)?;
//         println!("ssid buffer: {:?}", buf);
//         let ssid = str::from_utf8(&buf[0..len])?;
//         println!("ssid: {}", ssid);
//     } else {
//         println!("No existe clave ssid en el NVS");
//     }
//     nvs.remove("ssid")?;
//     Ok(())
// }

fn read_nvs_string(nvs: &mut EspDefaultNvs, key: &str) -> Result<Option<String>, anyhow::Error> {
    if nvs.contains(&key).unwrap() {
        let len = nvs.len(&key).unwrap().unwrap();
        // println!("ssid len: {}", len);
        let mut buf: [u8; 100] = [0; 100];
        nvs.get_raw(&key, &mut buf)?;
        // println!("ssid buffer: {:?}", buf);
        let value = String::from(str::from_utf8(&buf[0..len])?);
        // println!("value: {}", value);
        Ok(Some(value))
    } else {
        warn!("Key {key} not found in NVS");
        Ok(None)
    }
    // nvs.remove("ssid")?;
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly.
    // See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let part = EspDefaultNvsPartition::take()?;
    let nvs = EspDefaultNvs::new(part, "storage", true).unwrap();

    let peripherals = Peripherals::take().unwrap();

    info!("Inicializando wifi");
    let sysloop = EspSystemEventLoop::take()?;
    // let mut wifi = wifi(peripherals.modem, sysloop.clone())?;
    let wifi = Box::new(EspWifi::new(peripherals.modem, sysloop.clone(), None)?);
    info!("Inicialización del wifi terminada");

    let (tx, rx) = mpsc::channel();

    // crea el estado inicial
    // let mut state = State::Initial;
    // state.run(&tx, &mut wifi, sysloop.clone());

    // Crea tarea para procesar eventos en la máquina de estados
    // La tarea se implenta en esp-idf-sys con Thread de FreeRTOS.
    // let tx1 = tx.clone();
    // let mut fsm = Fsm::new(tx.clone(), sysloop, wifi, nvs);
    // let mywifi = Arc::new(Mutex::new(wifi));
    thread::Builder::new()
        .name("threadfsm".to_string())
        .stack_size(8000)
        .spawn(move || {
            info!("Thread for FSM event processing started.");
            let mut fsm = Fsm::new(tx, sysloop, wifi, nvs);
            loop {
                let event = rx.recv().unwrap();
                info!("Event received: {:?}", event);
                fsm = fsm.process_event(event);
                // fsm = fsm.process_event(event);
                // info!("New state generated: {:?}", fsm.state);
            }
        })?;

    // envía enventos desde otro thread
    // thread::spawn(move || {
    //     println!("Sending event from thread");
    //     let event = Event::SensorData(54);
    //     tx.send(event).unwrap();
    //     thread::sleep(Duration::from_secs(10));
    // });
    // tr.join().unwrap();
    Ok(())
}

fn wifi(
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> anyhow::Result<Box<EspWifi<'static>>> {
    use std::net::Ipv4Addr;

    // use esp_idf_svc::handle::RawHandle;

    let mut wifi = Box::new(EspWifi::new(modem, sysloop.clone(), None)?);

    info!("Wifi created, about to scan");

    let ap_infos = wifi.scan()?;

    let ours = ap_infos.into_iter().find(|a| a.ssid == SSID);

    let channel = if let Some(ours) = ours {
        info!(
            "Found configured access point {} on channel {}",
            SSID, ours.channel
        );
        Some(ours.channel)
    } else {
        info!(
            "Configured access point {} not found during scanning, will go with unknown channel",
            SSID
        );
        None
    };

    wifi.set_configuration(&embedded_svc::wifi::Configuration::Mixed(
        //wifi.set_configuration(&Configuration::Mixed(
        embedded_svc::wifi::ClientConfiguration {
            ssid: SSID.into(),
            password: PASS.into(),
            channel,
            ..Default::default()
        },
        embedded_svc::wifi::AccessPointConfiguration {
            ssid: "aptest".into(),
            channel: channel.unwrap_or(1),
            ..Default::default()
        },
    ))?;

    wifi.start()?;

    info!("Starting wifi...");

    if !WifiWait::new(&sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }

    info!("Connecting wifi...");
    println!("Connecting wifi... ***");

    wifi.connect()?;

    if !EspNetifWait::new::<EspNetif>(wifi.sta_netif(), &sysloop)?.wait_with_timeout(
        Duration::from_secs(20),
        || {
            wifi.is_connected().unwrap()
                && wifi.sta_netif().get_ip_info().unwrap().ip != Ipv4Addr::new(0, 0, 0, 0)
        },
    ) {
        bail!("Wifi did not connect or did not receive a DHCP lease");
    }

    let ip_info = wifi.sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    // ping(ip_info.subnet.gateway)?;

    Ok(wifi)
}

fn wifi_ap_start(wifi: &mut Box<EspWifi>, sysloop: &EspSystemEventLoop) -> anyhow::Result<()> {
    wifi.set_configuration(&embedded_svc::wifi::Configuration::AccessPoint(
        embedded_svc::wifi::AccessPointConfiguration {
            ssid: "aptest".into(),
            channel: 1,
            ..Default::default()
        },
    ))
    .expect("Error configurando wifi");

    wifi.start().expect("No se puede empezar el wifi");

    info!("Starting wifi...");

    // let sysloop = EspSystemEventLoop::take()?;
    if !WifiWait::new(&sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }
    Ok(())
    // info!("Connecting wifi...");
    // println!("Connecting wifi... ***");

    // wifi.connect()?;
    // Ok(())
}
