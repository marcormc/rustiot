use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

//use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
use std::sync::mpsc;
// https://doc.rust-lang.org/std/sync/mpsc/
use std::thread;
use std::time::Duration;


#[derive(Debug, PartialEq)]
enum State {
    Initial,
    Provisioned,
    WifiConnected,
    ServerConnected,
}

#[derive(Debug, Clone)]
enum Event {
    Credentials { ssid: String, user: String, password: String },
    WifiConnected,
    WifiDisconnected,
    MqttConnected,
    MqttDisconnected,
}

impl State {
    fn next(self, event: Event) -> State {
        match (self, event) {
            (State::Initial, Event::Credentials { ssid, user, password }) => {
                println!("ssid={}, user={}, password={}",
                    ssid, user, password);
                State::Provisioned
            }
            (s, e) => {
                panic!("Wrong transition {:#?}, {:#?}", s, e);
            }
        }
    }

    fn run(&self) {
        match *self {
            State::Initial => {
                println!("Initial state. Activating wifi access point.");
            }
            State::Provisioned => {
                println!("State Provisioned. Trying to connect to wifi station.");
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


fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();


    println!("State transitions test.");
    let mut state = State::Initial;


    // envía enventos desde otro thread
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        println!("Sending event from thread");
        let event = Event::Credentials {
            ssid: String::from("harpoland"),
            user: String::from("marco"),
            password: String::from("secret"),
        };
        tx.send(event).unwrap();
        // thread::sleep(Duration::from_millis(100));
        // tx.send(Message::WifiDisconnected(20)).unwrap();
    });

    // recibe los eventos en este thread
    let event = rx.recv().unwrap();
    println!("Event received: {:?}", event);
    state = state.next(event);
    println!("New state generated: {:?}", state);
    state.run();
}
