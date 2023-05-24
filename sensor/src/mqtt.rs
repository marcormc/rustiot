use core::time::Duration;
use embedded_svc::mqtt::client::{Details::Complete, Event::Received, QoS};
use std::thread;
// use embedded_svc::mqtt::client::QoS;

use crate::Event;
use esp_idf_svc::mqtt::client::{EspMqttClient, EspMqttMessage, MqttClientConfiguration};
// use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_sys::EspError;
use log::{error, info, warn};
use std::sync::mpsc;

pub fn start_mqtt_client(mut tx: mpsc::Sender<Event>) -> Result<EspMqttClient, EspError> {
    // let broker_url = if app_config.mqtt_user != "" {
    //     format!(
    //         "mqtt://{}:{}@{}",
    //         app_config.mqtt_user, app_config.mqtt_pass, app_config.mqtt_host
    //     )
    // } else {
    //     format!("mqtt://{}", app_config.mqtt_host)
    // };

    let broker_url = "mqtt://test.mosquitto.org";

    let mqtt_config = MqttClientConfiguration::default();

    // let tx1 = tx.clone();
    let mut client = EspMqttClient::new(
        broker_url,
        &mqtt_config,
        // move |_| { },
        // move |message_event| match message_event {
        //     Ok(Received(msg)) => info!("mensaje recibido {:?}", msg),
        //     _ => warn!("Received from MQTT: {:?}", message_event),
        // },
        move |message_event| match message_event {
            Ok(Received(msg)) => process_message(msg, &mut tx),
            _ => warn!("Received from MQTT: {:?}", message_event),
        },
    )?;

    // client.publish(&hello_topic(UUID), QoS::AtLeastOnce, true, payload)?;
    // let payload: &[u8] = &[];
    client.publish(
        "/rust/test",
        QoS::AtLeastOnce,
        true,
        b"greetings from rust node.",
    )?;
    info!("Greeting mqtt message sent.");

    // client.subscribe("/rust/command", QoS::AtLeastOnce)?;
    thread::sleep(Duration::from_millis(100));

    client.subscribe("/rust/command", QoS::AtLeastOnce)?;
    info!("Subscribed to mqtt topic.");

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

fn process_message(message: &EspMqttMessage, tx: &mut mpsc::Sender<Event>) {
    match message.details() {
        Complete => {
            let message_data: &[u8] = message.data();
            info!("Mensaje mqtt: {:?}, data: {:?}", message, message_data);
            // if let Ok(ColorData::BoardLed(color)) = ColorData::try_from(message_data) {
            //     info!("{}", color);
            //     if let Err(e) = led.set_pixel(color) {
            //         error!("Could not set board LED: {:?}", e)
            //     };
            // }
            // let command = String::try_from(message_data);
            // let command = str::from_utf8(message_data);
            let command = String::from(std::str::from_utf8(&message_data).unwrap());
            let event = Event::RemoteCommand { command };
            tx.send(event).unwrap();
        }
        _ => error!("Could not proccess command"),
    }
}
