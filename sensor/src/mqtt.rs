use core::time::Duration;
use embedded_svc::mqtt::client::{Details::Complete, Event::Received, QoS};
use std::thread;

use crate::fsm::Event;
use esp_idf_svc::mqtt::client::{EspMqttClient, EspMqttMessage, MqttClientConfiguration};
use esp_idf_sys::EspError;
use log::{error, info, warn};
use std::sync::mpsc;


/// Starts the connection to MQTT server.
/// It uses host, user and passwd as credentials for the server.
/// tx: queue to send commands to the FSM (when a message is received)
///   - It publish a welcome message at /rust/test
///   - It subscribe to /rust/command to receive commands
pub fn start_mqtt_client(
    mut tx: mpsc::Sender<Event>,
    host: &str,
    user: Option<&str>,
    passwd: Option<&str>,
) -> Result<EspMqttClient, EspError> {
    let broker_url = if let (Some(user), Some(passwd)) = (user, passwd) {
        format!("mqtt://{}:{}@{}", user, passwd, host)
    } else {
        format!("mqtt://{}", host)
    };

    let mqtt_config = MqttClientConfiguration::default();

    // connect to MQTT server
    let mut client = EspMqttClient::new(
        broker_url,
        &mqtt_config,
        // process messages received from server
        move |message_event| match message_event {
            Ok(Received(msg)) => process_message(msg, &mut tx),
            _ => warn!("mqtt debug: received from mqtt client: {:?}", message_event),
        },
    )?;

    info!("Sending mqtt welcome message.");
    client.publish(
        "/rust/test",
        QoS::AtLeastOnce,
        true,
        b"Rust sensor node connected to MQTT.",
    )?;

    // Subscribe to receive commands from MQTT server
    info!("Subscribing to mqtt topic /rust/command");
    // it is necessary ta wait a little before subscribing
    thread::sleep(Duration::from_millis(100));
    client.subscribe("/rust/command", QoS::AtLeastOnce)?;
    // With error handling
    // let res = client.subscribe("/rust/command", QoS::AtLeastOnce)?;
    // match res {
    //     Ok(id) => {
    //         println!("Suscrito con id {}", id);
    //     }
    //     Err(error) => {
    //         println!("Error en subscripcion {}", error);
    //     }
    // };

    Ok(client)
}

/// Handles messages received from MQTT server
fn process_message(message: &EspMqttMessage, tx: &mut mpsc::Sender<Event>) {
    match message.details() {
        Complete => {
            let message_data: &[u8] = message.data();
            info!(
                "Received message from MQTT server: {:?}, data: {:?}",
                message, message_data
            );
            let command = String::from(std::str::from_utf8(&message_data).unwrap());
            let event = Event::RemoteCommand { command };
            // send event to the Fsm
            tx.send(event).unwrap();
        }
        _ => error!("Could not proccess command"),
    }
}


/// Send a temperature to MQTT server
pub fn send_temperature(mqttc: &mut EspMqttClient, temp: f32) {
    info!("Sending mqtt data.");
    mqttc.publish(
        "/rust/temperature",
        QoS::AtLeastOnce,
        true,
        format!("{}", temp).as_bytes(),
    )
    .expect("Error sending data to MQTT server.");
}
