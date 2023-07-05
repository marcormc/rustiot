# mqtt con no-std en Rust

- Soporte MQTTv5
- TLS
- no_std
- Que funcione con el stack de red de Embassy.


## Opciones que pueden funcionar

### rust-mqtt

Parece viable, al menos con rp2040.

[Problem when trying to connect with tcp socket on rarspberry pico w · Issue #1543 · embassy-rs/embassy · GitHub](https://github.com/embassy-rs/embassy/issues/1543)
Parece que aquí estan usando embasssy (con rp2040) y el stack de red smoltcp
con embassy-net y funciona con rust-mqtt. 
Sirve como ejemplo para el rp2040, utiliza la conexión wifi con el chip
cyw43 de la placa pico.
Si se puede usar smoltcp sobre embassy-net es posible que también funcione en
esp32-c3 porque estoy usando embassy-net ya.

[GitHub - obabec/rust-mqtt: Rust native mqtt client for both std and no\_std environmnents.](https://github.com/obabec/rust-mqtt)
Para std y no-std.
Client library provides async API which can be used with various executors.
Currently, supporting only MQTTv5 but everything is prepared to extend support
also for MQTTv3 which is planned during year 2022.
Ligado a Drogue-iot y parece con muy pocas descargas.


### mqttrust

Parece viable aunque hay que programar bastante, pero hay un ejemplo.
El ejemplo (temperature logger) no incluye embassy y hay algunos bucles con esperas).

[GitHub - bjoernQ/esp32-rust-nostd-temperature-logger: MQTT temperature logger running on ESP32 in Rust (no-std / no RTOS)](https://github.com/bjoernQ/esp32-rust-nostd-temperature-logger)
Aquí usan mqttrust sobre smoltcp. Sin embasssy, con esp32-hal, no-std.
Es un ejemplo de proyecto completo con lectura de sensor y envío por MQTT
basado en esp-hal.
Se hace una conexión socket con embassy-net y sobre ese socket crea una
capa intermedia para usar mqttrust para enviar sobre el socket.
Parece que embassy-net utiliza internamente smoltcp por lo que cualquier
librería de mqtt que use smoltcp puede ser compatible.


### mqttrust

[mqttrust — embedded dev in Rust // Lib.rs](https://lib.rs/crates/mqttrust)
no_alloc (heapless), no-std, secure

Parece tener 2 partes: mqttrust y mqttrs-core. Esta última tiene algunos ejemplos
pero todos utilizan std. No está bien documentado.

### mqttrs

[GitHub - 00imvj00/mqttrs: Async Mqtt encoder and decoder for rust.](https://github.com/00imvj00/mqttrs)
Soporte no_std opcional. Parece de mas bajo nivel pero puede que funcione.


### embedded-mqtt

[GitHub - keithduncan/embedded-mqtt: A no\_std mqtt encoder/decoder in pure Rust for use in embedded systems.](https://github.com/keithduncan/embedded-mqtt)
no-std por defecto.


## Otras opciones a valorar

Valorar la opción de usar paho-mqtt (wrapper en C, con FFI).
[How to Use MQTT in Rust - DEV Community](https://dev.to/emqx/how-to-use-mqtt-in-rust-5fne)


Proyecto fin de grado implementando un cliente MQTT asíncrono embebido con Rust
y Embassy.
**Información interesante para la memoria.**
Basado en la firmware Drogue-iot.
[24465.pdf](https://theses.cz/id/c73sn4/24465.pdf)


## Opciones que parece que no funcionan

### mqrstt

No vale para no-std.

Discusión sobre MQTT con no-std en esp32-c3. Uso de 2 cores con mas de 1 executor
de embasssy.  HTTP server con websockets (discusión).
Se discute sobre implementaciones no-std de MQTT y en particular sobre mqrstt.
[ESP32-Wi-Fi-Lamp in Rust Pt 2: Connecting to Wifi! : r/rust](https://www.reddit.com/r/rust/comments/12zwqpv/esp32wifilamp_in_rust_pt_2_connecting_to_wifi/)
Quizá pueda usarse con no-std porque solo necesita que se le proporcione un
stream que implemente smol o tokio. Es posible que smoltcp sea una implementación
de smol que no requiere std y es precisamente la que usa embassy-net.
Parece que no soporta no-std debido a que usa el heap.
[GitHub - GunnarMorrigan/mqrstt: Pure rust sync and async MQTTv5 client](https://github.com/GunnarMorrigan/mqrstt)


### Rumqttd

Deprecated (hace 3 años).

[Rumqttd — Rust utility // Lib.rs](https://lib.rs/crates/rumqttd)
Depende de Tokio: parece no embedded. Pero en github dice que el servidor sí
es embebible (supongo que con std).


