use crate::mqtt::{send_temperature, start_mqtt_client};
use crate::wifi::{wifi_ap_start, wifi_sta_start};
use anyhow::Result;
use embedded_svc::storage::RawStorage;
use esp_idf_svc::{eventloop::EspSystemEventLoop, wifi::EspWifi};
use esp_idf_svc::{http::server::EspHttpServer, mqtt::client::EspMqttClient, nvs::EspDefaultNvs};
use log::{error, info, warn};
use std::str;
use std::sync::mpsc;

///
/// Estados de la máquina de estados finitos
#[derive(Debug, PartialEq)]
pub enum State {
    Initial,
    Provisioned {
        wifi_ssid: String,
        wifi_psk: String,
        mqtt_host: String,
        mqtt_user: Option<String>,
        mqtt_passwd: Option<String>,
    },
    WifiConnected,
    ServerConnected,
    Failure,
}

/// Eventos que se pueden pasar entre threads a la máquina de estados.
#[derive(Debug, Clone)]
pub enum Event {
    Credentials {
        wifi_ssid: String,
        wifi_psk: String,
        mqtt_host: String,
        mqtt_user: Option<String>,
        mqtt_passwd: Option<String>,
    },
    WifiConnected,
    WifiDisconnected,
    MqttConnected,
    MqttDisconnected,
    SensorData(f32),
    RemoteCommand {
        command: String,
    },
}

impl State {
    /// Manage the changes from one state to another
    fn next(&mut self, event: Event) -> Option<State> {
        // println!("next, state {:?}, event {:?}", self, event);
        match (self, event) {
            (
                State::Initial,
                Event::Credentials {
                    wifi_ssid,
                    wifi_psk,
                    mqtt_host,
                    mqtt_user,
                    mqtt_passwd,
                },
            ) => {
                // println!("ssid={}, user={}, password={}", ssid, user, password);
                info!("Recibido evento de provisionamiento");
                Some(State::Provisioned {
                    wifi_ssid,
                    wifi_psk,
                    mqtt_host,
                    mqtt_user,
                    mqtt_passwd,
                })
            }
            (State::Provisioned { .. }, Event::WifiConnected) => Some(State::WifiConnected),
            (State::WifiConnected, Event::MqttConnected) => Some(State::ServerConnected),
            (State::WifiConnected, Event::RemoteCommand { command }) => {
                info!("Remote command received {}", command);
                Some(State::WifiConnected)
            }
            (_s, _e) => {
                // error!("State {:#?}, event {:#?} not expected.", s, e);
                None
            }
        }
    }
}

pub struct Fsm<'a> {
    pub state: State,
    pub tx: mpsc::Sender<Event>,
    pub sysloop: EspSystemEventLoop,
    pub wifi: Box<EspWifi<'a>>,
    pub nvs: EspDefaultNvs,
    pub httpserver: Option<EspHttpServer>,
    pub mqttc: Option<EspMqttClient>,
    // temp_sens: ShtcSensor<'a>,
    // i2c: I2cDriver<'a>,
    // temp_sensor: ShtCx<Sht2Gen, I2cDriver<'a>>,
    mqtt_host: Option<String>,
    mqtt_user: Option<String>,
    mqtt_passwd: Option<String>,
}

impl<'a> Fsm<'a> {
    pub fn new(
        tx: mpsc::Sender<Event>,
        sysloop: EspSystemEventLoop,
        wifi: Box<EspWifi<'a>>,
        nvs: EspDefaultNvs,
        //temp_sens: ShtcSensor,
        // i2c: I2cDriver<'a>,
        // temp_sensor: ShtCx<Sht2Gen, I2cDriver>,
    ) -> Self {
        let mut fsm = Self {
            state: State::Initial,
            tx,
            sysloop,
            wifi,
            nvs,
            //  temp_sensor,
            // i2c,
            // temp_sens,
            httpserver: None,
            mqttc: None,
            mqtt_host: None,
            mqtt_user: None,
            mqtt_passwd: None,
        };
        fsm.enter_state();
        fsm
    }

    // pub fn process_event(&mut self, event: Event) -> Fsm<'a> {
    pub fn process_event(&mut self, event: Event) {
        // handle events that keep the machine in current state
        self.handle_event(&event);
        // handle events that make the machine's state to change
        if let Some(newstate) = self.state.next(event) {
            self.state = newstate;
            self.enter_state();
        }
    }

    /// It handles the events that keep the machine in the same state
    fn handle_event(&mut self, event: &Event) {
        match (&self.state, event) {
            (State::ServerConnected { .. }, Event::SensorData(data)) => {
                info!("Sending temperature sensor data to MQTT: {} °C", data);
                let mqttc = self.mqttc.as_mut().unwrap();
                send_temperature(mqttc, *data);
            }
            (_s, _e) => {}
        }
    }

    /// It runs the acctions needed when the machine enters a new state
    fn enter_state(&mut self) {
        info!("******** Entering state {:?}", self.state);
        match &self.state {
            State::Initial => {
                let wifi_ssid = read_nvs_string(&mut self.nvs, "wifi_ssid").unwrap();
                let wifi_psk = read_nvs_string(&mut self.nvs, "wifi_psk").unwrap();
                let mqtt_host = read_nvs_string(&mut self.nvs, "mqtt_host").unwrap();
                let mqtt_user = read_nvs_string(&mut self.nvs, "mqtt_user").unwrap();
                let mqtt_passwd = read_nvs_string(&mut self.nvs, "mqtt_passwd").unwrap();
                // self.temp_sens.start_measurements().expect("Error initializing sensor shtc3");
                // self.start_sensor().expect("Error initializing sensor shtc3");
                if let (Some(wifi_ssid), Some(wifi_psk), Some(mqtt_host)) =
                    (wifi_ssid, wifi_psk, mqtt_host)
                {
                    info!(
                        "Credentials from NVS: ssid = {}, psk = {}, mqtt = {},{:?},{:?}",
                        wifi_ssid, wifi_psk, mqtt_host, mqtt_user, mqtt_passwd
                    );
                    // provisioned: generate event to change state
                    let event = Event::Credentials {
                        wifi_ssid,
                        wifi_psk,
                        mqtt_host,
                        mqtt_user,
                        mqtt_passwd,
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
                wifi_ssid,
                wifi_psk,
                mqtt_host,
                mqtt_user,
                mqtt_passwd,
            } => {
                info!("Trying to connect to wifi station.");
                info!("Using credentials {wifi_ssid}, {wifi_psk}.");
                // stop http server if running
                if self.httpserver.is_some() {
                    info!("Deactivating HTTP server");
                    self.httpserver = None
                }
                // store mqtt credentials in Fsm (wifi credentials not stored)
                // self.mqtt_host = Some(mqtt_passwd.as_deref().clone());
                // self.mqtt_user = Some(mqtt_passwd.as_deref().unwrap().to_string());
                self.mqtt_host = Some(mqtt_host.clone());
                self.mqtt_user = mqtt_user.clone();
                self.mqtt_passwd = mqtt_passwd.clone();
                // store credentials permanently in NVS
                self.nvs.set_raw("wifi_ssid", wifi_ssid.as_bytes()).unwrap();
                self.nvs.set_raw("wifi_psk", wifi_psk.as_bytes()).unwrap();
                self.nvs.set_raw("mqtt_host", mqtt_host.as_bytes()).unwrap();
                if let (Some(mqtt_user), Some(mqtt_passwd)) = (mqtt_user, mqtt_passwd) {
                    self.nvs.set_raw("mqtt_user", mqtt_user.as_bytes()).unwrap();
                    self.nvs
                        .set_raw("mqtt_passwd", mqtt_passwd.as_bytes())
                        .unwrap();
                }
                // connect to wifi using the credentials
                // TODO: handle possible errors, retry on error, backoff
                wifi_sta_start(&mut self.wifi, &self.sysloop, wifi_ssid, wifi_psk)
                    .expect("Error activating STA");
                self.tx.send(Event::WifiConnected).unwrap();
            }
            State::WifiConnected => {
                info!("State WifiConnected.");
                // let host = self.mqtt_host.as_deref();
                let res = start_mqtt_client(
                    self.tx.clone(),
                    self.mqtt_host.as_deref().unwrap(),
                    self.mqtt_user.as_deref(),
                    self.mqtt_passwd.as_deref(),
                );
                match res {
                    Ok(mqttc) => {
                        self.mqttc = Some(mqttc);
                        info!("Connected to MQTT server.");
                    }
                    Err(err) => {
                        error!("Error connecting to MQTT server: {}", err);
                    }
                }
                self.tx.send(Event::MqttConnected).unwrap();
            }
            State::ServerConnected => {
                info!("State ServerConnected. Start sending periodic data.");
            }
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
