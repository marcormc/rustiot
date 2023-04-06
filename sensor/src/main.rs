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
            (State::Provisioned, Event::WifiConnected) => {
                State::WifiConnected
            }
            (s, e) => {
                panic!("Wrong transition {:#?}, {:#?}", s, e);
            }
        }
    }

    fn run(&self, tx: &mpsc::Sender<Event>) {
        match *self {
            State::Initial => {
                println!("Initial state. Activating wifi access point.");
            }
            State::Provisioned => {
                println!("State Provisioned. Trying to connect to wifi station.");
                thread::sleep(Duration::from_millis(5000));
                tx.send(Event::WifiConnected).unwrap();
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

    // crea el estado inicial
    let mut state = State::Initial;

    let (tx, rx) = mpsc::channel();

    // Crea tarea para procesar eventos en la máquina de estados
    // La tarea se implenta en esp-idf-sys con Thread de FreeRTOS.
    let tx1 = tx.clone();
    thread::spawn(move || {
        println!("Thread for FSM event processing started.");
        loop {
            let event = rx.recv().unwrap();
            println!("Event received: {:?}", event);
            state = state.next(event);
            println!("New state generated: {:?}", state);
            state.run(&tx1);
        }
    });


    // envía enventos desde otro thread
    let tx2 = tx.clone();
    thread::spawn(move || {
        println!("Sending event from thread");
        let event = Event::Credentials {
            ssid: String::from("harpoland"),
            user: String::from("marco"),
            password: String::from("secret"),
        };
        tx2.send(event).unwrap();

        // thread::sleep(Duration::from_millis(100));
        // tx.send(Message::WifiDisconnected(20)).unwrap();
    });

}

