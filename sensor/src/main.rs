// use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

pub mod fsm;
pub mod shtc3;
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

// use embedded_svc::storage::RawStorage;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::{EspDefaultNvs, EspDefaultNvsPartition},
    wifi::EspWifi,
};
// use log::{error, info, warn};
use log::info;
use std::sync::mpsc;
use std::thread;

use crate::fsm::Fsm;
use crate::shtc3::ShtcSensor;

// use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly.
    // See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let part = EspDefaultNvsPartition::take()?;
    let nvs = EspDefaultNvs::new(part, "storage", true).unwrap();

    // configure i2c bus
    let peripherals = Peripherals::take().unwrap();
    let pins = peripherals.pins;
    let sda = pins.gpio10;
    let scl = pins.gpio8;
    let i2c = peripherals.i2c0;
    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
    let temp_sens = ShtcSensor::new(i2c);

    info!("Inicializando wifi");
    let sysloop = EspSystemEventLoop::take()?;
    // let mut wifi = wifi(peripherals.modem, sysloop.clone())?;
    let wifi = Box::new(EspWifi::new(peripherals.modem, sysloop.clone(), None)?);
    info!("Inicialización del wifi terminada");

    let (tx, rx) = mpsc::channel();

    // let mywifi = Arc::new(Mutex::new(wifi));
    thread::Builder::new()
        .name("threadfsm".to_string())
        .stack_size(8000)
        .spawn(move || {
            info!("Thread for FSM event processing started.");
            let mut fsm = Fsm::new(tx, sysloop, wifi, nvs, temp_sens);
            loop {
                let event = rx.recv().unwrap();
                info!("Event received: {:?}", event);
                fsm = fsm.process_event(event);
                // fsm = fsm.process_event(event);
                // info!("New state generated: {:?}", fsm.state);
            }
        })?;

    Ok(())
}
