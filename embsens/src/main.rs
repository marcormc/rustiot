#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::cell::RefCell;
use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_executor::Executor;
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, NoopMutex};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_println::println;
use hal::{
    clock::ClockControl,
    embassy,
    i2c::I2C,
    peripherals::{Interrupt, Peripherals, I2C0},
    prelude::*,
    timer::TimerGroup,
    Priority, Rtc, IO,
};
use icm42670::{prelude::*, Address, Icm42670};
use static_cell::StaticCell;

// I2C bus static reference for sharing bus between tasks
static I2C_BUS: StaticCell<NoopMutex<RefCell<I2C<I2C0>>>> = StaticCell::new();

// Embassy executor
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn run1() {
    loop {
        println!("Hello world from embassy using esp-hal-async!");
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

#[embassy_executor::task]
async fn run2() {
    loop {
        println!("Bing!");
        Timer::after(Duration::from_millis(5_000)).await;
    }
}

/// Embassy task to read accelerometer sensor on board (ICM42670)
#[embassy_executor::task]
async fn run_i2c(i2c: I2cDevice<'static, NoopRawMutex, I2C<'static, I2C0>>) {
    let mut icm = Icm42670::new(i2c, Address::Primary).unwrap();

    loop {
        let accel_norm = icm.accel_norm().unwrap();
        let gyro_norm = icm.gyro_norm().unwrap();
        println!(
            "ACCEL  =  X: {:+.04} Y: {:+.04} Z: {:+.04}\t\tGYRO  =  X: {:+.04} Y: {:+.04} Z: {:+.04}",
            accel_norm.x, accel_norm.y, accel_norm.z, gyro_norm.x, gyro_norm.y, gyro_norm.z);
        // delay.delay_ms(100u32);
        Timer::after(Duration::from_millis(100)).await;
    }
}

/// Embassy task to read external I2C sensor HTU21D similar to SI7021.
/// Not using device driver, writing and reading directly from i2c.
#[embassy_executor::task]
async fn run_htu(mut i2c: I2cDevice<'static, NoopRawMutex, I2C<'static, I2C0>>) {
    const SI7021_I2C_ADDRESS: u8 = 0x40;
    const MEASURE_RELATIVE_HUMIDITY: u8 = 0xE5;
    const MEASURE_TEMPERATURE: u8 = 0xE3;
    // const READ_TEMPERATURE: u8 = 0xE0;

    loop {
        // medición de temperatura
        let mut buf = [0u8; 2];
        i2c.write(SI7021_I2C_ADDRESS, &[MEASURE_TEMPERATURE])
            .unwrap();
        Timer::after(Duration::from_millis(50)).await;
        i2c.read(SI7021_I2C_ADDRESS, &mut buf).unwrap();
        // Escritura y lectura en una sola transacción: no funciona con
        // este sensor:
        // i2c.write_read(SI7021_I2C_ADDRESS, &[MEASURE_TEMPERATURE], &mut buf).unwrap();
        let word = u16::from_be_bytes(buf);
        let temp: f32 = 175.72 * word as f32 / 65536.0 - 46.85;
        println!("buf {:?}, word: {}, temperatura: {}", buf, word, temp);

        // medición de humedad
        i2c.write(SI7021_I2C_ADDRESS, &[MEASURE_RELATIVE_HUMIDITY])
            .unwrap();
        Timer::after(Duration::from_millis(50)).await;
        i2c.read(SI7021_I2C_ADDRESS, &mut buf).unwrap();
        // Escritura y lectura en una sola transacción: no funciona con
        // este sensor:
        // i2c.write_read(SI7021_I2C_ADDRESS, &[MEASURE_TEMPERATURE], &mut buf).unwrap();
        let word = u16::from_be_bytes(buf);
        let rel_hum = 125.0 * word as f32 / 65536.0 - 6.0;
        // rel_hum = rel_hum.max(0.0).min(100.0);
        println!("buf {:?}, word: {}, humedad: {}", buf, word, rel_hum);
    }
}

#[entry]
fn main() -> ! {
    println!("Embassy sharing I2C bus test.");
    let peripherals = Peripherals::take();
    let mut system = peripherals.SYSTEM.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // Disable the RTC and TIMG watchdog timers
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(
        peripherals.TIMG1,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt1 = timer_group1.wdt;

    // Disable watchdog timers
    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    // Only option in ESP32-C3 for clocks
    // #[cfg(feature = "embassy-time-systick")]
    embassy::init(
        &clocks,
        hal::systimer::SystemTimer::new(peripherals.SYSTIMER),
    );

    // Not available in ESP32-C3
    //    #[cfg(feature = "embassy-time-timg0")]
    //    embassy::init(&clocks, timer_group0.timer0);

    // i2c initialization. Pins GPIO10 SDA, GPIO8 CLK
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio10,
        io.pins.gpio8,
        400u32.kHz(),
        &mut system.peripheral_clock_control,
        &clocks,
    );
    // Create i2c_bus with static lifetime
    let i2c_bus = NoopMutex::new(RefCell::new(i2c));
    let i2c_bus = I2C_BUS.init(i2c_bus);

    // share the i2c bus between devices in embassy (sync)
    let i2c_dev1 = I2cDevice::new(i2c_bus);
    let i2c_dev2 = I2cDevice::new(i2c_bus);

    hal::interrupt::enable(Interrupt::I2C_EXT0, Priority::Priority1).unwrap();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(run1()).ok();
        spawner.spawn(run2()).ok();
        spawner.spawn(run_i2c(i2c_dev1)).ok();
        spawner.spawn(run_htu(i2c_dev2)).ok();
    });

    // println!("Hello world!");
    // loop {}
}
