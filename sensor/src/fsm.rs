
use embedded_svc::storage::RawStorage;
use esp_idf_svc::{
    http::server::EspHttpServer,
    mqtt::client::EspMqttClient,
    nvs::EspDefaultNvs,
};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::EspWifi,
};

use log::{error, info, warn};
use std::str;
use std::sync::mpsc;

use crate::mqtt::start_mqtt_client;
use crate::wifi::{wifi_ap_start, wifi_sta_start};

///
/// Estados de la máquina de estados finitos
#[derive(Debug, PartialEq)]
pub enum State {
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
pub struct Fsm<'a> {
    pub state: State,
    pub tx: mpsc::Sender<Event>,
    pub sysloop: EspSystemEventLoop,
    pub wifi: Box<EspWifi<'a>>,
    pub nvs: EspDefaultNvs,
    pub httpserver: Option<EspHttpServer>,
    pub mqttc: Option<EspMqttClient>,
}

impl<'a> Fsm<'a> {
    pub fn new(
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

    pub fn process_event(mut self, event: Event) -> Fsm<'a> {
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
