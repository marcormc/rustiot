#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use esp_backtrace as _;
use esp_println::println;
use hal::{
    clock::ClockControl,
    peripherals::{Interrupt, Peripherals, I2C0},
    embassy,
    i2c::I2C,
    prelude::*,
    timer::TimerGroup,
    Priority,
    IO,
    Rtc};
use embassy_executor::Executor;
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;
use icm42670::{prelude::*, Address, Icm42670};
// use lis3dh_async::{Lis3dh, Range, SlaveAddr};
use shared_bus;
use shared_bus::I2cProxy;
use shared_bus::NullMutex;
use shared_bus::new_atomic_check;


use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_sync::blocking_mutex::{NoopMutex, raw::NoopRawMutex};
use core::cell::RefCell;

static I2C_BUS: StaticCell<NoopMutex<RefCell<I2C<I2C0>>>> = StaticCell::new();

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

// async fn run_i2c(i2c: I2C<'static, I2C0>) {
#[embassy_executor::task]
async fn run_i2c(i2c: I2cDevice<'static, NoopRawMutex, I2C<'static, I2C0>>) {
// async fn run_i2c(i2c:  I2cProxy<'static, NullMutex<I2C<'static, I2C0>>>) {
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

    //let mut lis3dh = Lis3dh::new_i2c(i2c, SlaveAddr::Alternate).await.unwrap();
    //lis3dh.set_range(Range::G8).await.unwrap();

    //loop {
    //    let norm = lis3dh.accel_norm().await.unwrap();
    //    esp_println::println!("X: {:+.5}  Y: {:+.5}  Z: {:+.5}", norm.x, norm.y, norm.z);

    //    Timer::after(Duration::from_millis(100)).await;
    //}
}

#[embassy_executor::task]
async fn run_htu(mut i2c: I2cDevice<'static, NoopRawMutex, I2C<'static, I2C0>>) {
// async fn run_htu(mut i2c: I2cProxy<'static, NullMutex<I2C<'static, I2C0>>>) {
// async fn run_htu(bus: I2C<'static, I2C0>) {
    const SI7021_I2C_ADDRESS: u8 = 0x40;
    const MEASURE_RELATIVE_HUMIDITY: u8 = 0xE5;
    const MEASURE_TEMPERATURE: u8 = 0xE3;
    const READ_TEMPERATURE: u8 = 0xE0;

    loop {
        // medición de temperatura
        let mut buf = [0u8; 2];
        i2c.write(SI7021_I2C_ADDRESS, &[MEASURE_TEMPERATURE]).unwrap();
        Timer::after(Duration::from_millis(50)).await;
        i2c.read(SI7021_I2C_ADDRESS, &mut buf).unwrap();
        // Escritura y lectura en una sola transacción: no funciona con
        // este sensor:
        // i2c.write_read(SI7021_I2C_ADDRESS, &[MEASURE_TEMPERATURE], &mut buf).unwrap();
        let word = u16::from_be_bytes(buf);
        let temp: f32 = 175.72 * word as f32 / 65536.0 - 46.85;
        println!("buf {:?}, word: {}, temperatura: {}", buf, word, temp);

        // medición de humedad
        i2c.write(SI7021_I2C_ADDRESS, &[MEASURE_RELATIVE_HUMIDITY]).unwrap();
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

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    println!("Init!");
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


    // #[cfg(feature = "embassy-time-systick")]
    embassy::init(
        &clocks,
        hal::systimer::SystemTimer::new(peripherals.SYSTIMER),
    );

    // #[cfg(feature = "embassy-time-timg0")]
    // embassy::init(&clocks, timer_group0.timer0);

    // inicializa i2c
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio10,
        io.pins.gpio8,
        400u32.kHz(),
        &mut system.peripheral_clock_control,
        &clocks,
    );
    let i2c_bus = NoopMutex::new(RefCell::new(i2c));
    let i2c_bus = I2C_BUS.init(i2c_bus);

    let i2c_dev1 = I2cDevice::new(i2c_bus);
    let i2c_dev2 = I2cDevice::new(i2c_bus);

    //let mut icm = Icm42670::new(i2c_dev1, Address::Primary).unwrap();
    //let accel_norm = icm.accel_norm().unwrap();
    //let gyro_norm = icm.gyro_norm().unwrap();
    //println!(
    //    "ACCEL  =  X: {:+.04} Y: {:+.04} Z: {:+.04}\t\tGYRO  =  X: {:+.04} Y: {:+.04} Z: {:+.04}",
    //    accel_norm.x, accel_norm.y, accel_norm.z, gyro_norm.x, gyro_norm.y, gyro_norm.z);



    //let mut icm2 = Icm42670::new(i2c_dev2, Address::Primary).unwrap();
    //let accel_norm = icm2.accel_norm().unwrap();
    //let gyro_norm = icm2.gyro_norm().unwrap();
    //println!(
    //    "ACCEL  =  X: {:+.04} Y: {:+.04} Z: {:+.04}\t\tGYRO  =  X: {:+.04} Y: {:+.04} Z: {:+.04}",
    //    accel_norm.x, accel_norm.y, accel_norm.z, gyro_norm.x, gyro_norm.y, gyro_norm.z);



    hal::interrupt::enable(Interrupt::I2C_EXT0, Priority::Priority1).unwrap();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(run1()).ok();
        spawner.spawn(run2()).ok();
        // spawner.spawn(run_i2c(bus).ok();
        // spawner.spawn(run_htu(bus.acquire_i2c())).ok();
        spawner.spawn(run_i2c(i2c_dev1)).ok();
        spawner.spawn(run_htu(i2c_dev2)).ok();
    });

    // println!("Hello world!");
    // loop {}
}
