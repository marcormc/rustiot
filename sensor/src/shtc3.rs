use crate::Event;
use anyhow::Result;
use esp_idf_hal::gpio::Pins;
use esp_idf_hal::i2c::I2C0;
use std::sync::mpsc;

use esp_idf_hal::{
    delay,
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};
use log::info;
use shtcx::sensor_class::Sht2Gen;
use shtcx::{self, shtc3, PowerMode, ShtCx};
// use embedded_svc::timer::TimerService;
// use embedded_svc::timer::*;
use esp_idf_svc::timer::*;
// use esp_idf_svc::systime::EspSystemTime;
use std::time::*;

pub struct ShtcSensor<'a> {
    pub sensor: ShtCx<Sht2Gen, I2cDriver<'a>>,
}

pub fn start_sensor(pins: Pins, i2c: I2C0, tx: mpsc::Sender<Event>) -> Result<()> {
    info!("Starting sensor shtc3");

    // let temp_sens = ShtcSensor::new(i2c);
    // let mut temp_sensor = shtc3(i2c);
    // let temp_sensor = shtc3(*i2c);
    // let sensor = Arc::new(Mutex::new(temp_sensor));

    let sda = pins.gpio10;
    let scl = pins.gpio8;
    // let i2c = peripherals.i2c0;
    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
    let mut temp_sensor = shtc3(i2c);

    let mut delay = delay::Ets;

    let periodic_timer = EspTimerService::new()?.timer(move || {
        info!("Tick from periodic timer");
        // let sen = sensor.clone();
        // let temp = self.temp_sens.sensor
        // let temp = sen.lock().unwrap()
        let temp = temp_sensor
            .measure_temperature(PowerMode::NormalMode, &mut delay)
            .unwrap()
            .as_degrees_celsius();
        info!("Temperature shtc3 sensor: {}", temp);
        let event = Event::SensorData(temp);
        tx.send(event).unwrap();
    })?;

    info!("Starting measurements every 5 seconds ");
    periodic_timer.every(Duration::from_secs(5))?;

    Ok(())
}
