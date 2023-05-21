// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

pub mod http;

//use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
// use std::mem::swap;
// use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
// use esp_idf_sys;
// use esp_idf_sys::{esp, EspError};
// use esp_idf_hal::peripheral;
// use esp_idf_sys::CONFIG_NEWLIB_STDOUT_LINE_ENDING_CRLF;
// use esp_idf_svc::ping;
// use embedded_svc::ping::Ping;
// use embedded_svc::wifi;
// use std::sync::{Arc, Mutex};
// Ver https://doc.rust-lang.org/std/sync/mpsc/

// const SSID: Option<&'static str> = option_env!("WIFI_SSID");
// const PASS: Option<&'static str> = option_env!("WIFI_PASS");

use anyhow::bail;
use embedded_svc::storage::RawStorage;
use embedded_svc::wifi::*;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::EspHttpServer,
    netif::{EspNetif, EspNetifWait},
    nvs::{EspDefaultNvs, EspDefaultNvsPartition},
    wifi::{EspWifi, WifiWait},
    // errors::EspIOError,
};
use log::{error, info, warn};
use std::net::Ipv4Addr;
use std::str;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Estados de la máquina de estados finitos
#[derive(Debug, PartialEq)]
enum State {
    Initial,
    Provisioned {
        ssid: String,
        user: String,
        password: String,
    },
    WifiConnected,
    // ServerConnected,
    Failure,
}

/// Eventos que se pueden pasar entre threads a la máquina de estados.
#[derive(Debug, Clone)]
pub enum Event {
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
        self.run();
        self
    }

    /// Ejecuta las acciones necesarias al entrar en cada estado
    fn run(&mut self) {
        info!("******** Running state {:?}", self.state);
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
                    info!("Credentials not found in NVS.");
                    info!("Activating wifi AP.");
                    wifi_ap_start(&mut self.wifi, &self.sysloop).expect("Error activating AP");
                    info!("Activating HTTP server");
                    self.httpserver = Some(crate::http::start_http_server(&self.tx));

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
                if self.httpserver.is_some() {
                    info!("Deactivating HTTP server");
                    self.httpserver = None
                }
                // almacenamiento de credenciales en NVS
                self.nvs.set_raw("ssid", ssid.as_bytes()).unwrap();
                self.nvs.set_raw("password", password.as_bytes()).unwrap();
                // thread::sleep(Duration::from_millis(10000));
                wifi_sta_start(&mut self.wifi, &self.sysloop).expect("Error activating STA");
                info!("Sending Event::WifiConnected from State::Provisioned.");
                self.tx.send(Event::WifiConnected).unwrap();
            }
            State::WifiConnected => {
                info!("State WifiConnected. Trying to connect to server.");
            }
            // State::ServerConnected => {
            //     info!("State ServerConnected. Start sending periodic data.");
            // }
            State::Failure => {
                error!("Current state is Failure");
            }
        }
    }
}

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

fn wifi_sta_start(wifi: &mut Box<EspWifi>, sysloop: &EspSystemEventLoop) -> anyhow::Result<()> {
    // wifi.stop()?;
    wifi.set_configuration(&embedded_svc::wifi::Configuration::Client(
        embedded_svc::wifi::ClientConfiguration {
            ssid: "harpoland".into(),
            password: "alcachofatoxica".into(),
            // channel: Some(1), //channel,
            ..Default::default()
        },
    ))
    .expect("Error configurando wifi sta");

    wifi.start()?;

    info!("Starting wifi...");

    if !WifiWait::new(sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }

    info!("Connecting wifi...");

    wifi.connect()?;

    if !EspNetifWait::new::<EspNetif>(wifi.sta_netif(), sysloop)?.wait_with_timeout(
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

    println!("Wifi sta activado {}", wifi.is_connected().unwrap());
    Ok(())
}

fn wifi_ap_start(wifi: &mut Box<EspWifi>, sysloop: &EspSystemEventLoop) -> anyhow::Result<()> {
    wifi.set_configuration(&embedded_svc::wifi::Configuration::AccessPoint(
        embedded_svc::wifi::AccessPointConfiguration {
            ssid: "aptest".into(),
            channel: 1,
            ..Default::default()
        },
    ))
    .expect("Error configurando wifi ap");

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
