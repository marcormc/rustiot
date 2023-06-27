# Ejemplo con Embassy (no-std), i2c y wifi

Compilar con :

    export SSID=myssid
     export PASSWORD=mypassword
    cargo build

Basado en:

- [esp-rs/esp-wifi](https://github.com/esp-rs/esp-wifi)
- [esp-wifi/examples-esp32c3](https://github.com/esp-rs/esp-wifi/tree/main/examples-esp32c3).  Ejemplo `examples/embassy_dhcp.rs`.

Se modifican las features eliminando las opcionales y haciendo fijas
las imprescindibles para que corra en el esp32-c3.

Se eliminan las macros genericas del ejemplo de wifi (examples-util).
[examples-util](https://github.com/esp-rs/esp-wifi/blob/main/examples-util/src/lib.rs)
sustituyéndolas por código específico para esp32-c3.

El problema era que auque todo compile bien, es necesario añadir una opción
al compilador para que enlace los binarios generados en la compilación de
librerías C del crate esp-wifi-sys. En concreto es necesario añadir el fichero
rom_functions.x

Modificar el fichero `.cargo/config.toml` añadiendo la línea:

    "-C", "link-arg=-Trom_functions.x",


Se utiliza el timer SystemTimer (SYSTIMER) para inicizaliza el driver esp-wifi.
Como ya se ha utilizado (movido) no se puede reutilizar como timer para Embassy.
Para Embassy se use otro timer: TimerGroup con peripherals.TIMG0. Para esto
tiene que estar activada la feature *embassy-time-timg0* en el crate
*esp32c3-hal*.

Parece que no es necesario desactivar los watchdog timers, cosa que aparece por
defecto en todos los ejemplos anteriores, es extraño, comprobar.

