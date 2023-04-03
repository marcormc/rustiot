use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported

use esp_idf_sys::{xQueueGenericCreate, xQueueGenericSend, xQueueReceive, QueueHandle_t};
// use std::ptr;
use core::ffi::c_void;
use std::thread;
use std::time::Duration;

// Create a `static mut` that holds the queue handle.
static mut EVENT_QUEUE: Option<QueueHandle_t> = None;

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    // Create an event queue
    const QUEUE_TYPE_BASE: u8 = 0;
    const ITEM_SIZE: u32 = 4;
    const QUEUE_SIZE: u32 = 10;
    unsafe {
        EVENT_QUEUE = Some(xQueueGenericCreate(QUEUE_SIZE, ITEM_SIZE, QUEUE_TYPE_BASE));
    }

    const QUEUE_WAIT_TICKS: u32 = 10;
    const COPY_POSITION: i32 = 0;
    // ::core::ffi::c_void,
    // std::ptr::null_mut(),
    for i in 1..4 {
        let mut val: i32 = 40 + i;
        let msg: *mut c_void = &mut val as *mut _ as *mut c_void;
        unsafe {
            xQueueGenericSend(EVENT_QUEUE.unwrap(), msg, QUEUE_WAIT_TICKS, COPY_POSITION);
        }
        thread::sleep(Duration::from_millis(100));
    }

    println!("Comienza lectura de mensajes en cola.");

    loop {
        // Maximum delay
        const QUEUE_WAIT_TICKS: u32 = 1000;

        // 8. Receive the event from the queue.
        let mut val: i32 = 0;
        let buffer: *mut c_void = &mut val as *mut _ as *mut c_void;
        //let buffer: *const i32 = std::ptr::null_mut();
        // let mut v = std::mem::MaybeUninit::uninit();
        // let res = xQueueReceive(EVENT_QUEUE.unwrap(), v.as_mut_ptr(), QUEUE_WAIT_TICKS);
        let res = unsafe { xQueueReceive(EVENT_QUEUE.unwrap(), buffer, QUEUE_WAIT_TICKS) };
        // let val = v.assume_init() as i32;
        //let val = std::ptr::read(result);

        if res == 1 {
            println!("Mensaje recibido {}", val);
        } else {
            println!("Tiempo de espera expirado");
        }
    }
}
