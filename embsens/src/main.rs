#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

pub use esp32c3_hal as hal;
use mqttrust::SubscribeTopic;

// use embassy_executor::_export::StaticCell;

use core::cell::RefCell;
use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_executor::{Executor, Spawner};
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, Stack, StackResources};
// use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
// si hay 1 solo core (1 solo thread) (feature cortex-m)
// use embassy_sync::blocking_mutex::raw::thread_mode::ThreadModeRawMutex;

use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, raw::NoopRawMutex, NoopMutex};
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_backtrace as _;
use esp_println::logger::init_logger;
use esp_println::println;
use esp_wifi::wifi::{WifiController, WifiDevice, WifiEvent, WifiMode, WifiState};
use esp_wifi::{initialize, EspWifiInitFor};
use hal::system::SystemExt;
use hal::{
    clock::{ClockControl, CpuClock},
    embassy,
    i2c::I2C,
    peripherals::{Interrupt, Peripherals, I2C0},
    prelude::*,
    timer::TimerGroup,
    Priority, Rng, Rtc, IO,
};
use icm42670::{prelude::*, Address, Icm42670};
use static_cell::StaticCell;

use crate::tiny_mqtt::TinyMqtt;
use core::fmt::Write;
use mqttrust::encoding::v4::Pid;
mod tiny_mqtt;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

macro_rules! singleton {
    ($val:expr) => {{
        type T = impl Sized;
        static STATIC_CELL: StaticCell<T> = StaticCell::new();
        let (x,) = STATIC_CELL.init(($val,));
        x
    }};
}

// #[derive(Debug, Clone)]
#[derive(Debug)]
pub enum Signal {
    WifiStaConnected,
    WifiConnected(Ipv4Cidr),
    WifiDisconnected,
    TempHumData { temp: f32, hum: f32 },
    AccelDataData([f32; 6]),
}

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

// I2C bus static reference for sharing bus between tasks
// https://docs.embassy.dev/embassy-embedded-hal/git/default/shared_bus/blocking/i2c/index.html)
static I2C_BUS: StaticCell<NoopMutex<RefCell<I2C<I2C0>>>> = StaticCell::new();

// si hay 1 solo core (1 solo thread)
// static CHANNEL: Channel<ThreadModeRawMutex, Signal, 10> = Channel::new();
// static CHANNEL: Channel<NoopRawMutex, Signal, 10> = Channel::new();
// static CHANNEL: Channel<NoopRawMutex, u32, 10> = Channel::new();
static CHANNEL: Channel<CriticalSectionRawMutex, Signal, 10> = Channel::new();

static MQTT: StaticCell<Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>>> = StaticCell::new();


#[entry]
fn main() -> ! {
    init_logger(log::LevelFilter::Info);
    println!("embwifis: Embassy wifi + sharing I2C bus test.");

    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let mut peripheral_clock_control = system.peripheral_clock_control;
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock160MHz).freeze();

    // Disable the RTC
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    rtc.swd.disable();
    rtc.rwdt.disable();

    let timer = hal::systimer::SystemTimer::new(peripherals.SYSTIMER).alarm0;

    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    // let wifi = examples_util::get_wifi!(peripherals);
    let (wifi, _) = peripherals.RADIO.split();
    let (wifi_interface, controller) = esp_wifi::wifi::new_with_mode(&init, wifi, WifiMode::Sta);

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks, &mut peripheral_clock_control);
    embassy::init(&clocks, timer_group0.timer0);

    // let config = Config::Dhcp(Default::default());
    let config = Config::dhcpv4(Default::default());

    let seed = 1234; // very random, very secure seed

    // Init network stack
    let stack = &*singleton!(Stack::new(
        wifi_interface,
        config,
        singleton!(StackResources::<3>::new()),
        seed
    ));

    // i2c initialization. Pins GPIO10 SDA, GPIO8 CLK
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio10,
        io.pins.gpio8,
        400u32.kHz(),
        // &mut system.peripheral_clock_control,
        &mut peripheral_clock_control,
        &clocks,
    );
    // Create i2c_bus with static lifetime
    let i2c_bus = NoopMutex::new(RefCell::new(i2c));
    let i2c_bus = I2C_BUS.init(i2c_bus);

    // share the i2c bus between devices in embassy (sync)
    let i2c_dev1 = I2cDevice::new(i2c_bus);
    let i2c_dev2 = I2cDevice::new(i2c_bus);

    hal::interrupt::enable(Interrupt::I2C_EXT0, Priority::Priority1).unwrap();


    // socket for MQTT
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);
    // let mqtt = TinyMqtt::new("esp32", socket, esp_wifi::current_millis, None);
    // let mut mqtt = Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>>;
    // Mutex::new(RefCell::new(mqtt))
    // let mqtt = MQTT.init(Mutex::new(RefCell::new(mqtt)));

    //let mqtt = TinyMqtt::new("esp32", esp_wifi::current_millis, None, &stack);
    let mqtt = TinyMqtt::new("esp32", socket, esp_wifi::current_millis, None, &stack);
    let mqtt = MQTT.init(Mutex::new(RefCell::new(mqtt)));
    
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(fsm()).ok();
        spawner.spawn(connection(controller)).ok();
        spawner.spawn(net_task(&stack)).ok();
        // spawner.spawn(http_task(&stack)).ok();
        // spawner.spawn(mqtt_task(&stack, spawner, socket, &mqtt)).ok();
        spawner.spawn(mqtt_task(&stack, spawner, mqtt)).ok();
        // spawner.spawn(run_i2c(i2c_dev1)).ok();
        // spawner.spawn(run_htu(i2c_dev2)).ok();
        spawner.spawn(run1(spawner)).ok();
        // spawner.spawn(run2()).ok();
    })
}

#[embassy_executor::task]
async fn fsm() {
    loop {
        let signal = CHANNEL.recv().await;
        println!("Señal recibida: {:?}", signal);
    }
}

/// Establish connection with the wifi access point
/// It keep trying every 5 s in case of error.
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.get_capabilities());
    loop {
        match esp_wifi::wifi::get_wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.into(),
                password: PASSWORD.into(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start().await.unwrap();
            println!("Wifi started!");
        }
        println!("About to connect...");

        match controller.connect().await {
            Ok(_) => {
                println!("Wifi connected!");
                CHANNEL.send(Signal::WifiStaConnected).await;
            }
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

/// Handles network events
#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static>>) {
    stack.run().await
}

/// Connects to HTTP server to retrieve a web page
#[embassy_executor::task]
async fn http_task(stack: &'static Stack<WifiDevice<'static>>) {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            CHANNEL.send(Signal::WifiConnected(config.address)).await;
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        Timer::after(Duration::from_millis(1_000)).await;

        let mut socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);

        socket.set_timeout(Some(Duration::from_secs(10)));

        let remote_endpoint = (Ipv4Address::new(142, 250, 185, 115), 80);
        println!("connecting...");
        let r = socket.connect(remote_endpoint).await;
        if let Err(e) = r {
            println!("connect error: {:?}", e);
            continue;
        }
        println!("connected!");
        let mut buf = [0; 1024];
        loop {
            use embedded_io::asynch::Write;
            let r = socket
                .write_all(b"GET / HTTP/1.0\r\nHost: www.mobile-j.de\r\n\r\n")
                .await;
            if let Err(e) = r {
                println!("write error: {:?}", e);
                break;
            }
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    println!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            };
            println!("{}", core::str::from_utf8(&buf[..n]).unwrap());
        }
        Timer::after(Duration::from_millis(3000)).await;
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
        CHANNEL
            .send(Signal::AccelDataData([
                accel_norm.x,
                accel_norm.y,
                accel_norm.z,
                gyro_norm.x,
                gyro_norm.y,
                gyro_norm.z,
            ]))
            .await;
        Timer::after(Duration::from_millis(5000)).await;
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
        Timer::after(Duration::from_millis(4000)).await;
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
        CHANNEL
            .send(Signal::TempHumData { temp, hum: rel_hum })
            .await;
    }
}

/// Example additional Embassy task. Periodically print message.
#[embassy_executor::task]
async fn run1(spawner: Spawner) {
    spawner.spawn(run2()).ok();
    loop {
        println!("Hello fron embassy task run1.");
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

/// Example additional Embassy task. Periodically print message.
#[embassy_executor::task]
async fn run2() {
    loop {
        println!("Hello fron embassy task run2.");
        Timer::after(Duration::from_millis(5_000)).await;
    }
}

/// Embassy task to send data to MQTT server
#[embassy_executor::task]
async fn mqtt_task(stack: &'static Stack<WifiDevice<'static>>,
                   spawner: Spawner,
                   // socket: TcpSocket<'static>) {
                   mqtt: &'static Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>>) {
    // let mut rx_buffer = [0; 4096];
    // klet mut tx_buffer = [0; 4096];

    // Wait until network is connected
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // Wait until network has IPv4 configuration (interface has IP address)
    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        // let mut tmqtt = TinyMqtt::new("esp32", esp_wifi::current_millis, None, &stack);

        // Open TCP socket to MQTT server test.mosquitto.org:  91.121.93.94:1883
        // let mut socket = TcpSocket::new(&stack, &mut rx_buffer, &mut tx_buffer);
        let remote_endpoint = (Ipv4Address::new(91, 121, 93, 94), 1883);
        println!("connecting socket...");
        {
            let shared = mqtt.lock().await;
            shared.borrow_mut().socket.set_timeout(Some(Duration::from_secs(30)));
            let r = shared.borrow_mut().socket.connect(remote_endpoint).await;
            if let Err(e) = r {
                println!("connect error: {:?}", e);
                // keep trying to open socket
                Timer::after(Duration::from_millis(5_000)).await;
                continue;
            }
            println!("TCP socket connected to MQTT server!");
        }


        // Send connect MQTT package to server
        // let mut tmqtt = TinyMqtt::new("esp32", socket, esp_wifi::current_millis, None);

        {
            let shared = mqtt.lock().await;
            if let Err(e) = shared.borrow_mut().connect(
                // Ipv4Address::new(52, 54, 163, 195), // io.adafruit.com
                remote_endpoint.0,
                // 1883,
                remote_endpoint.1,
                60,
                None, // Some(ADAFRUIT_IO_USERNAME),
                None, // Some(ADAFRUIT_IO_KEY.as_bytes()),
            ) {
                println!(
                    "Error connecting to MQTT server. Retrying in 10 seconds. Error is {:?}",
                    e
                );
                Timer::after(Duration::from_millis(10_000)).await;
                continue;
            }
            println!("Connected to MQTT broker");
        }

        // start task to receive MQTT packets asynchronously
        // let mqtt_m = Arc::from(mqtt);
        // let mqtt_m = NoopMutex::new(RefCell::new(mqtt));
        // let mqtt_m: Mutex<NoopRawMutex, RefCell<TinyMqtt>> = Mutex::new(RefCell::new(mqtt));
        // let mqtt: &'static mut Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>> = MQTT.init(Mutex::new(RefCell::new(tmqtt)));
        // let mqtt = MQTT.init(Mutex::new(RefCell::new(tmqtt)));
        
        // let mqttc = RefCell::new(mqtt);

        // let m2 = mqtt_m.clone();
        // let m1 = mqtt_m.;
        // let m2 = mqtt_m.borrow();
        // let mut mqtt_m2 = NoopMutex::new(RefCell::new(mqtt));
        // NoopMutex<RefCell<I2C<I2C0>>>
        // spawner.spawn(mqtt_receiver(mqtt_m.get_mut()));
        // spawner.spawn(mqtt_receiver(&mqttc.borrow()));
        spawner.spawn(mqtt_receiver(&mqtt));

        // Subscribe to topic /command
        let topics = [SubscribeTopic {
            topic_path: "/embsens/command",
            qos: mqttrust::QoS::AtLeastOnce,
        }];
        println!("Initial poll before sending subscribe");

        {
            let shared = mqtt.lock().await;
            if shared.borrow_mut().poll().await.is_err() {
                println!("Error in poll");
            }
            if shared.borrow_mut().subscribe(None, &topics).is_err() {
                println!("error sending subscribe packet");
            }
        }
        // el poll se hace desde la tarea mqtt_receiver
        // loop {
        //     // interval between polls
        //     Timer::after(Duration::from_millis(2_000)).await;
        //     if mqtt.get_mut().borrow_mut().poll().await.is_err() {
        //         println!("Error in poll (subscribe)");
        //         break;
        //     }
        // }

        // Publish MQTT message every 5 s
        let mut topic_name: heapless::String<32> = heapless::String::new();
        write!(topic_name, "/embsens/test{}", 1).ok();
        let mut pkt_num = 10;
        loop {
            // interval between packets
            Timer::after(Duration::from_millis(2_000)).await;

            // process received packages and send pending packages
            // println!("iniciando poll");
            // if mqtt.get_mut().borrow_mut().poll().await.is_err() {
            //     println!("Error in poll");
            //     break;
            // }

            // prepare message payload
            let temperature = 42.0;
            let mut msg: heapless::String<32> = heapless::String::new();
            write!(msg, "{}", temperature).ok();
            {
                let shared = mqtt.lock().await;
                // send publish mqtt packet, with package identifier pkt_num
                if shared.borrow_mut()
                    .publish_with_pid(
                        Some(Pid::try_from(pkt_num).unwrap()),
                        &topic_name,
                        msg.as_bytes(),
                        mqttrust::QoS::AtLeastOnce,
                    )
                    .is_err()
                {
                    println!("error enviando paquete");
                    // force reconnection to server
                    Timer::after(Duration::from_millis(5_000)).await;
                    break;
                }
            }
            pkt_num += 1;
        }
        Timer::after(Duration::from_millis(5_000)).await;
    }
}

/// Embassy task to receive data from MQTT server
#[embassy_executor::task]
// async fn mqtt_receiver(mqtt: &mut NoopMutex<RefCell<TinyMqtt<'static>>>) {
// async fn mqtt_receiver(mqtt: &Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>>) {
async fn mqtt_receiver(mqtt: &'static Mutex<NoopRawMutex, RefCell<TinyMqtt<'static>>>) {
    loop {
        {
            let shared = mqtt.lock().await;
            if shared.borrow_mut().receive().await.is_err() {
                println!("Error receiving data from mqtt server");
            }
        }
        // mqtt.lock(|m| { m.borrow_mut() });
        // if mqtt.get_mut().borrow_mut().receive().await.is_err() {
        // if mqtt..borrow_mut().receive().await.is_err() {
    }
}
