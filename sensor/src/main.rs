// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

//use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
use std::str;
use std::sync::mpsc;
// https://doc.rust-lang.org/std/sync/mpsc/
use std::thread;
use std::time::Duration;

use esp_idf_svc::nvs::{EspDefaultNvs, EspNvsPartition, EspDefaultNvsPartition};
use esp_idf_sys;
use esp_idf_sys::{esp, EspError};

use embedded_svc::wifi::*;
use esp_idf_hal::peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::*;
use esp_idf_svc::wifi::*;

use anyhow::bail;
use log::*;

const SSID: &str = "harpoland";
const PASS: &str = "password";
use esp_idf_svc::netif::*;
// use esp_idf_svc::ping;
// use embedded_svc::ping::Ping;

// use std::sync::{Arc, Mutex};

use embedded_svc::storage::RawStorage;

#[derive(Debug, PartialEq)]
enum State {
    Initial,
    Provisioned,
    WifiConnected,
    ServerConnected,
}

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
}

impl State {
    fn next(self, event: Event) -> State {
        match (self, event) {
            (
                State::Initial,
                Event::Credentials {
                    ssid,
                    user,
                    password,
                },
            ) => {
                println!("ssid={}, user={}, password={}", ssid, user, password);
                State::Provisioned
            }
            (State::Provisioned, Event::WifiConnected) => State::WifiConnected,
            (s, e) => {
                panic!("Wrong transition {:#?}, {:#?}", s, e);
            }
        }
    }

    fn run(&self, tx: &mpsc::Sender<Event>, wifi: &mut Box<EspWifi>, sysloop: EspSystemEventLoop) {
        match *self {
            State::Initial => {
                println!("Initial state. Activating wifi access point.");
                wifi_ap_start(wifi, sysloop).expect("Imposible activar AP");
                println!("Wifi AP inicializado.");
            }
            State::Provisioned => {
                println!("State Provisioned. Trying to connect to wifi station.");
                thread::sleep(Duration::from_millis(5000));
                tx.send(Event::WifiConnected).unwrap();
                // let sysloop = EspSystemEventLoop::take().unwrap();
                // let peripherals = Peripherals::take().unwrap();
                // let mut _wifi = wifi(peripherals.modem, sysloop.clone()).unwrap();
                // let ap_infos = wifi.scan().unwrap();
            }
            State::WifiConnected => {
                println!("State WifiConnected. Trying to connect to server.");
            }
            State::ServerConnected => {
                println!("State ServerConnected. Start sending periodic data.");
            }
        }
    }
}

fn test_nvs() -> anyhow::Result<()> {
    info!("Leyendo credenciales desde NVS");
    let part = EspDefaultNvsPartition::take()?;
    let mut nvs = EspDefaultNvs::new(part, "storage", true).unwrap();

    let value = "harpoland";
    nvs.set_raw("ssid", value.as_bytes())?;
    
    let exists = nvs.contains("ssid")?;
    println!("Storage contains ssid: {}", exists);

    if exists {
        let len = nvs.len("ssid").unwrap().unwrap();
        println!("ssid len: {}", len);
        
        let mut buf: [u8; 100] = [0; 100];
        nvs.get_raw("ssid", &mut buf)?;
        println!("ssid buffer: {:?}", buf);
        let ssid = str::from_utf8(&buf[0..len])?;
        println!("ssid: {}", ssid);
    } else {
        println!("No existe clave ssid en el NVS");
    }
    nvs.remove("ssid")?;
    Ok(())
}


fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    test_nvs()?;
    
    info!("State transitions test.");

    let sysloop = EspSystemEventLoop::take()?;
    let peripherals = Peripherals::take().unwrap();
    // let mut wifi = wifi(peripherals.modem, sysloop.clone())?;
    let mut wifi = Box::new(EspWifi::new(peripherals.modem, sysloop.clone(), None)?);

    wifi_ap_start(&mut wifi, sysloop.clone()).expect("Imposible activar AP");

    println!("Inicialización del wifi terminada");

    let (tx, rx) = mpsc::channel();

    // crea el estado inicial
    let mut state = State::Initial;
    // state.run(&tx, &mut wifi, sysloop.clone());


    // Crea tarea para procesar eventos en la máquina de estados
    // La tarea se implenta en esp-idf-sys con Thread de FreeRTOS.
    let tx1 = tx.clone();
    // let mywifi = Arc::new(Mutex::new(wifi));
    let _ = thread::Builder::new()
        .name("threadfsm".to_string())
        .stack_size(8000)
        .spawn(move || {
            println!("Thread for FSM event processing started.");
            loop {
                let event = rx.recv().unwrap();
                println!("Event received: {:?}", event);
                state = state.next(event);
                println!("New state generated: {:?}", state);
                state.run(&tx1, &mut wifi, sysloop.clone());
            }
        });

    // envía enventos desde otro thread
    // let tx2 = tx.clone();
    // thread::spawn(move || {
    //     println!("Sending event from thread");
    //     let event = Event::Credentials {
    //         ssid: String::from("harpoland"),
    //         user: String::from("marco"),
    //         password: String::from("secret"),
    //     };
    //     tx2.send(event).unwrap();
    //     // thread::sleep(Duration::from_millis(100));
    // });
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



fn wifi_ap_start(wifi: &mut Box<EspWifi>, sysloop: EspSystemEventLoop)
                 -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(
        AccessPointConfiguration {
            ssid: "aptest".into(),
            channel: 1,
            ..Default::default()
        },
    )).expect("Error configurando wifi");

    wifi.start().expect("No se puede empezar el wifi");

    info!("Starting wifi...");
    
    // let sysloop = EspSystemEventLoop::take()?;
    if !WifiWait::new(&sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }
    return Ok(());
    info!("Connecting wifi...");
    println!("Connecting wifi... ***");

    wifi.connect()?;
    Ok(())
}
