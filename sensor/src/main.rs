// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

pub mod fsm;
pub mod http;
pub mod mqtt;
pub mod shtc3;
pub mod wifi;

// use embedded_svc::storage::RawStorage;
// use std::sync::{Arc, Mutex};
// use anyhow::Result;
// use std::{borrow::Cow, convert::TryFrom, thread::sleep, time::Duration};
// use log::{error, info, warn};
use self::fsm::{Event, Fsm};
use self::shtc3::start_sensor;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::{EspDefaultNvs, EspDefaultNvsPartition},
    wifi::EspWifi,
};
use log::info;
use std::sync::mpsc;
use std::thread::{self, sleep};
use std::time::*;

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
    // configure i2c bus
    // let pins = peripherals.pins;
    start_sensor(peripherals.pins, peripherals.i2c0, tx.clone())?;

    // let mywifi = Arc::new(Mutex::new(wifi));
    thread::Builder::new()
        .name("threadfsm".to_string())
        .stack_size(8000)
        .spawn(move || {
            info!("Thread for FSM event processing started.");
            // Option: start sensors timer here.
            let mut fsm = Fsm::new(tx, sysloop, wifi, nvs);
            loop {
                let event = rx.recv().unwrap();
                info!("Event received: {:?}", event);
                // fsm = fsm.process_event(event);
                fsm.process_event(event);
                // fsm = fsm.process_event(event);
                // info!("New state generated: {:?}", fsm.state);
            }
        })?;

    // just to keep this thread alive because it has timers running
    // Other option: start sensors timer in the fsm thread
    loop {
        sleep(Duration::from_secs(10));
        info!("Inactive");
    }
    // Ok(())
}
