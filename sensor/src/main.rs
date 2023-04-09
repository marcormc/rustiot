// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

//use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
use std::str;
use std::sync::mpsc;
// https://doc.rust-lang.org/std/sync/mpsc/
use std::thread;
use std::time::Duration;
// use std::mem::swap;

use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition};
// use esp_idf_sys;
// use esp_idf_sys::{esp, EspError};

use embedded_svc::wifi::*;
use esp_idf_hal::peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::*;
use esp_idf_svc::wifi::*;

use anyhow::bail;
use log::*;

// const SSID: Option<&'static str> = option_env!("WIFI_SSID");
// const PASS: Option<&'static str> = option_env!("WIFI_PASS");
const SSID: &str = "harpoland";
const PASS: &str = "password";

use esp_idf_svc::netif::*;
// use esp_idf_svc::ping;
// use embedded_svc::ping::Ping;

// use std::sync::{Arc, Mutex};

use embedded_svc::storage::RawStorage;

/// Estados de la máquina de estados finitos
#[derive(Debug, PartialEq, Clone)]
enum State {
    Initial,
    Provisioned {
        ssid: String,
        user: String,
        password: String,
    },
    WifiConnected,
    ServerConnected,
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
    fn next(&self, event: Event) -> State {
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
                self.clone()
            }

            (s, e) => {
                // panic!("Wrong transition {:#?}, {:#?}", s, e);
                error!("Wrong transition {:#?}, {:#?}", s, e);
                s.clone()
            }
        }
    }

    /// Ejecuta las acciones necesarias al entrar en cada estado
    fn run(
        &self,
        tx: &mpsc::Sender<Event>,
        wifi: &mut Box<EspWifi>,
        sysloop: EspSystemEventLoop,
        nvs: &mut EspDefaultNvs,
        // fsm: &Fsm,
    ) {
        match self {
            State::Initial => {
                info!("Entering State::Initial.");
                let ssid = read_nvs_string(nvs, "ssid").unwrap();
                let password = read_nvs_string(nvs, "password").unwrap();
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
                    tx.send(event).unwrap();
                } else {
                    info!("Credentials not found in NVS. Activating wifi AP.");
                    wifi_ap_start(wifi, sysloop).expect("Error activating AP");
                    info!("Wifi AP started.");
                    // TODO: iniciar servidor HTTP.
                    // TODO: Provisionamiento temporal para depurar.
                    let ssid = "harpoland";
                    let password = "alcachofatoxica";
                    nvs.set_raw("ssid", ssid.as_bytes()).unwrap();
                    nvs.set_raw("password", password.as_bytes()).unwrap();
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
                tx.send(Event::WifiConnected).unwrap();
            }
            State::WifiConnected => {
                info!("State WifiConnected. Trying to connect to server.");
            }
            State::ServerConnected => {
                info!("State ServerConnected. Start sending periodic data.");
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
        };
        fsm.state
            .run(&fsm.tx, &mut fsm.wifi, fsm.sysloop.clone(), &mut fsm.nvs);
        fsm
    }

    fn process_event(&mut self, event: Event) {
        self.state = self.state.next(event);
        // Old state is being discarded here.
        // TODO: intentar consumir self en llamada a State.next
        // swap(self.state.next(event), self.state);
        self.state.run(
            &self.tx,
            &mut self.wifi,
            self.sysloop.clone(),
            &mut self.nvs,
        );
    }
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
    let mut fsm = Fsm::new(tx.clone(), sysloop, wifi, nvs);
    // let mywifi = Arc::new(Mutex::new(wifi));
    thread::Builder::new()
        .name("threadfsm".to_string())
        .stack_size(8000)
        .spawn(move || {
            info!("Thread for FSM event processing started.");
            loop {
                let event = rx.recv().unwrap();
                info!("Event received: {:?}", event);
                fsm.process_event(event);
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

    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            ssid: SSID.into(),
            password: PASS.into(),
            channel,
            ..Default::default()
        },
        AccessPointConfiguration {
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

fn wifi_ap_start(wifi: &mut Box<EspWifi>, sysloop: EspSystemEventLoop) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: "aptest".into(),
        channel: 1,
        ..Default::default()
    }))
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
