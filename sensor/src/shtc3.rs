use crate::Event;
use anyhow::Result;
use esp_idf_hal::gpio::Pins;
use esp_idf_hal::i2c::I2C0;
use esp_idf_hal::{
    delay,
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};
use esp_idf_svc::timer::*;
use log::info;
use shtcx::sensor_class::Sht2Gen;
use shtcx::{self, shtc3, PowerMode, ShtCx};
use std::sync::mpsc;
use std::time::*;

pub struct ShtcSensor<'a> {
    pub sensor: ShtCx<Sht2Gen, I2cDriver<'a>>,
}

pub fn start_sensor(pins: Pins, i2c: I2C0, tx: mpsc::Sender<Event>) -> Result<EspTimer> {
    info!("Starting sensor shtc3");

    let sda = pins.gpio10;
    let scl = pins.gpio8;
    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
    let mut temp_sensor = shtc3(i2c);

    let mut delay = delay::Ets;

    let periodic_timer = EspTimerService::new()?.timer(move || {
        let temp = temp_sensor
            .measure_temperature(PowerMode::NormalMode, &mut delay)
            .unwrap()
            .as_degrees_celsius();
        info!("Temperature reading: {} °C", temp);
        let event = Event::SensorData(temp);
        tx.send(event).unwrap();
    })?;

    info!("Starting measurements every 5 seconds ");
    periodic_timer.every(Duration::from_secs(5))?;
    Ok(periodic_timer)
}
