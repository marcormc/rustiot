
use anyhow::Result;

use esp_idf_hal::{
    delay,
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};
use shtcx::{self, shtc3, PowerMode, ShtCx};
use shtcx::sensor_class::Sht2Gen;
use log::info;
// use embedded_svc::timer::TimerService;
// use embedded_svc::timer::*;
use esp_idf_svc::timer::*;
// use esp_idf_svc::systime::EspSystemTime;
use std::time::*;

pub struct ShtcSensor<'a> {
    pub sensor: ShtCx<Sht2Gen, I2cDriver<'a>>
}

impl<'a> ShtcSensor<'a> {
    //pub fn init_shtc3(peripherals: &'static mut Peripherals) -> Result<()> {
    pub fn new(i2c: I2cDriver<'static>) -> Self {
        ShtcSensor { sensor: shtc3(i2c) }
    }

    pub fn start_measurements<'b>(&'static mut self) -> Result<()> {
        let mut delay = delay::Ets;

        info!("About to schedule a periodic timer every five seconds");
        let periodic_timer = EspTimerService::new()?.timer(move || {
            info!("Tick from periodic timer");

            let temp = self.sensor
                .measure_temperature(PowerMode::NormalMode, &mut delay)
                .unwrap()
                .as_degrees_celsius();
            info!("Temperatura leída: {}", temp);
            // let now = EspSystemTime {}.now();

            // eventloop.post(&EventLoopMessage::new(now), None).unwrap();

            // client
            //     .publish(
            //         "rust-esp32-std-demo",
            //         QoS::AtMostOnce,
            //         false,
            //         format!("Now is {now:?}").as_bytes(),
            //     )
            //     .unwrap();
        })?;

        periodic_timer.every(Duration::from_secs(5))?;

        Ok(())
    }

}


//   //pub fn init_shtc3(peripherals: &'static mut Peripherals) -> Result<()> {
//   pub fn init_shtc3(i2c: I2cDriver) -> Result<()> {
//   
//       // let peripherals = Peripherals::take().unwrap();
//       let pins = &mut peripherals.pins;
//       let sda = &mut pins.gpio10;
//       let scl = &mut pins.gpio8;
//       let i2c = &mut peripherals.i2c0;
//       let config = I2cConfig::new().baudrate(100.kHz().into());
//       let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
//       let mut temp_sensor = shtc3(i2c);
//       let mut delay = delay::Ets;
//   
//       info!("About to schedule a periodic timer every five seconds");
//       let periodic_timer = EspTimerService::new()?.timer(move || {
//           info!("Tick from periodic timer");
//   
//           let temp = temp_sensor
//               .measure_temperature(PowerMode::NormalMode, &mut delay)
//               .unwrap()
//               .as_degrees_celsius();
//           info!("Temperatura leída: {}", temp);
//           // let now = EspSystemTime {}.now();
//   
//           // eventloop.post(&EventLoopMessage::new(now), None).unwrap();
//   
//           // client
//           //     .publish(
//           //         "rust-esp32-std-demo",
//           //         QoS::AtMostOnce,
//           //         false,
//           //         format!("Now is {now:?}").as_bytes(),
//           //     )
//           //     .unwrap();
//       })?;
//   
//       periodic_timer.every(Duration::from_secs(5))?;
//   
//       Ok(())
//   }
