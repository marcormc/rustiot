// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

pub mod http;
pub mod mqtt;
pub mod wifi;

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

use embedded_svc::storage::RawStorage;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::EspHttpServer,
    // errors::EspIOError,
    mqtt::client::EspMqttClient,
    nvs::{EspDefaultNvs, EspDefaultNvsPartition},
    wifi::EspWifi,
};
use log::{error, info, warn};
use std::str;
use std::sync::mpsc;
use std::thread;

use crate::mqtt::start_mqtt_client;
use crate::wifi::{wifi_ap_start, wifi_sta_start};

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
    RemoteCommand {
        command: String,
    },
}

impl State {
    /// Procesa los eventos según el estado actual.
    fn next(&mut self, event: Event) -> Option<State> {
        println!("next, state {:?}, event {:?}", self, event);
        match (self, event) {
            (
                State::Initial,
                Event::Credentials {
                    ssid,
                    user,
                    password,
                }
            ) => {
                // println!("ssid={}, user={}, password={}", ssid, user, password);
                info!("Recibido evento de provisionamiento");
                Some(State::Provisioned {
                    ssid,
                    user,
                    password,
                })
            }
            (State::Initial, Event::SensorData(data)) => {
                info!("Ignoring data (initial) {}", data);
                Some(State::Initial)
            }
            (
                State::Provisioned {
                    ssid: _,
                    user: _,
                    password: _,
                },
                Event::WifiConnected
            ) => Some(State::WifiConnected),
            (State::Provisioned { .. }, Event::SensorData(data)) => {
                info!("Ignoring data (initial) {}", data);
                // self.clone()
                None
            }
            (State::WifiConnected, Event::RemoteCommand { command }) => {
                info!("Remote command received {}", command);
                Some(State::WifiConnected)
            }
            (s, e) => {
                // panic!("Wrong transition {:#?}, {:#?}", s, e);
                error!("Wrong transition {:#?}, {:#?}", s, e);
                //s.clone()
                //State::Failure
                None
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
    mqttc: Option<EspMqttClient>,
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
            mqttc: None,
        };
        fsm.run();
        fsm
    }

    fn process_event(mut self, event: Event) -> Fsm<'a> {
        // Old state is being discarded here (consumed).
        // In the process, the fsm must be consumed too (self.state is mutable ref)
        if let Some(newstate) = self.state.next(event) {
            self.state = newstate;
            self.run();
        }
        self
    }

    /// Ejecuta las acciones necesarias al entrar en cada estado
    fn run(&mut self) {
        info!("******** Running state {:?}", self.state);
        match &self.state {
            State::Initial => {
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
                }
            }
            State::Provisioned {
                ssid,
                user: _,
                password,
            } => {
                info!("Trying to connect to wifi station.");
                info!("Using credentials {ssid}, {password}.");
                // stop http server if running
                if self.httpserver.is_some() {
                    info!("Deactivating HTTP server");
                    self.httpserver = None
                }
                // store credentials in NVS
                self.nvs.set_raw("ssid", ssid.as_bytes()).unwrap();
                self.nvs.set_raw("password", password.as_bytes()).unwrap();
                // connect to wifi using the credentials
                // TODO: handle possible errors, retry on error, backoff
                wifi_sta_start(&mut self.wifi, &self.sysloop).expect("Error activating STA");
                self.tx.send(Event::WifiConnected).unwrap();
            }
            State::WifiConnected => {
                info!("State WifiConnected. Now connect to server (not implemented)");
                self.mqttc =
                    Some(start_mqtt_client(self.tx.clone()).expect("Error connecting to mqtt"));
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
    if nvs.contains(key).unwrap() {
        let len = nvs.len(key).unwrap().unwrap();
        // println!("ssid len: {}", len);
        let mut buf: [u8; 100] = [0; 100];
        nvs.get_raw(key, &mut buf)?;
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
