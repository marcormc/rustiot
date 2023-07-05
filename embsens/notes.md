# embsens notes


## Descripción

Se usa mqttrust para manejar codificar y decodificar los paquetes del protocolo
MQTT. El envío y recepción se hace a través de un socket asíncrono de
embassy-net.  Embassy-net utiliza smoltcp por debajo como stack IP.  La gestión
del envío y recepción de paquetes debe hacerse manualmente en este mismo crate,
utilizando colas como buffer de paquetes mqtt.

[mqttrust](https://github.com/BlackbirdHQ/mqttrust)


El siguiente ejemplo se toma como punto de partida para la gestión de paquetes
MQTT ya que utiliza el crate mqttrust. Pero el envío y recepción se hace por un
socket síncrono con smoltcp, no es asíncrono y las llamadas bloquean por lo que
hace polling y utiliza bloques con esperas.

[GitHub - bjoernQ/esp32-rust-nostd-temperature-logger: MQTT temperature logger running on ESP32 in Rust (no-std / no RTOS)](https://github.com/bjoernQ/esp32-rust-nostd-temperature-logger)

Versión std del mismo ejemplo (no sirve en este caso):
[bjoernQ/esp32c3-rust-std-temperature-logger: MQTT temperature logger running on ESP32C3 in Rust (std)](https://github.com/bjoernQ/esp32c3-rust-std-temperature-logger)


## Actualización de librerías esp-wifi y embassy-net

Using embassy-net 0.1.0 to have the embassy_net::tcp::TcpSocket::can_recv()
function to check if there are any data waiting to be read before calling
read().await on the socket. If read() is called we have to wait to receive data
for the task to progress. This is to avoid locking this task. Other solution
is to have a task just for reciving.

Programar una máquina de estados con una cola de envío y recepción.

Other possible solution: create an mqtt driver layer over mqttrust using
embassy-net-driver-channel. (esto no parece viable, parece para programar
drivers de bajo nivel para hacer que el hardware funcione con embassy-net).

Ver manejo de async/await para varias tareas en paralelo (enviar y recibir)
y actuar cuando alguna de las 2 progresa, etc.
[embassy-net-driver-channel - crates.io: Rust Package Registry](https://crates.io/crates/embassy-net-driver-channel)


Publicado embassy-net 0.1.0. Esto implica tener que usar una versión mas moderna
de esp-wifi. Como han separado embassy-net y embassy-net-driver (el primero para
usarios de las librerías y el segundo para programar implementaciones de
hardware que funcionen con embassy-net), han eliminado la dependencia de
embassy-net de esp-wifi. Ahora esp-wifi solo depende de embassy-net-driver y ya
no de embassy-net.

Commit en el que eliminan la dependencia:
[Remove embassy-net dependency · esp-rs/esp-wifi@abd7d3f · GitHub](https://github.com/esp-rs/esp-wifi/commit/abd7d3fb5610bde6096878a95b698f36e9b183f8)


Historial de commits de esp-wifi. Se puede ver que es necesario el commit
f6c09ac que incluye ya los anteriores en los que hacen los cambios de
embassy-net y embassy-net-driver. Se escoge ese porque los mas modernos ya
implican cambiar esp-hal-common de 0.9.0 a 0.10.0 y eso implica muchos otros
cambios que aún no están implementados en otras dependencias que siguen en
0.9.0.

[Commits · esp-rs/esp-wifi · GitHub](https://github.com/esp-rs/esp-wifi/commits/main)



## Documentación relevante de sockets 

[TcpSocket in embassy\_net::tcp - Rust](https://docs.embassy.dev/embassy-net/git/default/tcp/struct.TcpSocket.html#method.set_timeout)
