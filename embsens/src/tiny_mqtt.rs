use embassy_net::tcp::TcpSocket;
use embassy_net::Ipv4Address;
use embedded_io::blocking::{Read, Write};
use esp_println::println;
use esp_wifi::{compat::queue::SimpleQueue, wifi::WifiError};
use mqttrust::{
    encoding::v4::{decode_slice, encode_slice, Connect, Pid, Protocol},
    Mqtt, MqttError, Packet, Publish, QoS, Subscribe, SubscribeTopic,
};

#[derive(Debug)]
pub enum TinyMqttError {
    MqttError(MqttError),
    WifiError(WifiError),
}

impl From<MqttError> for TinyMqttError {
    fn from(e: MqttError) -> Self {
        TinyMqttError::MqttError(e)
    }
}

impl From<WifiError> for TinyMqttError {
    fn from(e: WifiError) -> Self {
        TinyMqttError::WifiError(e)
    }
}

#[derive(Copy, Clone)]
pub struct PacketBuffer {
    bytes: [u8; 1024],
}

impl PacketBuffer {
    pub fn new(packet: Packet<'_>) -> PacketBuffer {
        let mut buf = [0u8; 1024];
        encode_slice(&packet, &mut buf).ok();
        PacketBuffer { bytes: buf }
    }

    pub fn parsed(&self) -> Packet<'_> {
        // this might panic: "InvalidPid(0)" when I send s.th with QoS > 0
        decode_slice(&self.bytes).unwrap().unwrap()
    }
}

pub struct TinyMqtt<'a> {
    client_id: &'a str,
    // TCP socket for the MQTT connection
    pub socket: TcpSocket<'a>,
    // Queue of encoded packages to send
    queue: core::cell::RefCell<SimpleQueue<(usize, [u8; 1024]), 10>>,
    recv_buffer: [u8; 1024],
    recv_index: usize,
    // Queue of encoded packages to receive
    recv_queue: core::cell::RefCell<SimpleQueue<PacketBuffer, 10>>,
    timeout_secs: u16,
    last_sent_millis: u64,
    current_millis_fn: fn() -> u64,
    receive_callback: Option<&'a dyn Fn(&str, &[u8])>,
    // for TCP socket:
    // rx_buffer: [u8; 4096],
    // tx_buffer: [u8; 4096],
}

use esp_wifi::wifi::WifiDevice;
use embassy_net::Stack;

impl<'a> TinyMqtt<'a> {
    pub fn new(
        client_id: &'a str,
        socket: TcpSocket<'a>,
        current_millis_fn: fn() -> u64,
        receive_callback: Option<&'a dyn Fn(&str, &[u8])>,
        stack: &'static Stack<WifiDevice<'static>>
    ) -> TinyMqtt<'a> {
        let res = TinyMqtt {
            client_id,
            socket,
            // socket: TcpSocket::new(&stack, &mut self.rx_buffer, &mut self.tx_buffer),
            // socket: TcpSocket::new(&stack, &mut Self::rx_buffer, &mut [0u8; 4096]),
            queue: core::cell::RefCell::new(SimpleQueue::new()),
            recv_buffer: [0u8; 1024],
            recv_index: 0,
            recv_queue: core::cell::RefCell::new(SimpleQueue::new()),
            timeout_secs: 0,
            last_sent_millis: 0,
            current_millis_fn,
            receive_callback,
            // rx_buffer: [0; 4096],
            // tx_buffer: [0; 4096],
        };
        // res.socket = TcpSocket::new(&stack, &mut res.rx_buffer, &mut res.tx_buffer);

        res
    }

    pub fn connect(
        &mut self,
        addr: Ipv4Address,
        port: u16,
        keep_alive_secs: u16,
        username: Option<&'a str>,
        password: Option<&'a [u8]>,
    ) -> Result<(), TinyMqttError> {
        self.timeout_secs = keep_alive_secs;

        // self.socket.open(addr, port).unwrap();
        // self.socket.open(smoltcp::wire::IpAddress::Ipv4(addr), port).unwrap();

        let connect = Packet::Connect(Connect {
            protocol: Protocol::MQTT311,
            keep_alive: keep_alive_secs,
            client_id: "", //self.client_id(),
            clean_session: true,
            last_will: None,
            username,
            password,
        });
        // println!("MQTT Connect packet sent: {:?}", connect);
        self.send(connect)?;
        self.last_sent_millis = (self.current_millis_fn)();

        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), TinyMqttError> {
        // self.socket.disconnect();
        self.socket.close();
        Ok(())
    }

    pub fn publish_with_pid(
        &self,
        pid: Option<Pid>,
        topic_name: &str,
        payload: &[u8],
        qos: QoS,
    ) -> Result<(), MqttError> {
        let packet = Packet::Publish(Publish {
            dup: false,
            qos,
            pid: None,
            retain: false,
            topic_name,
            payload,
        });

        let mut buf = [0u8; 1024];
        let len = encode_slice(&packet, &mut buf).unwrap();

        // encode_slice doesn't fill in the PID for publish packets
        if pid.is_some() {
            let pid: u16 = pid.unwrap().into();
            let idx = len - payload.len() - 2;
            buf[idx + 0] = ((pid & 0xff00) >> 8) as u8;
            buf[idx + 1] = (pid & 0xff) as u8;
        }

        self.queue.borrow_mut().enqueue((len, buf)).unwrap();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn subscribe<'b: 'a>(
        &self,
        _pid: Option<Pid>,
        topics: &[SubscribeTopic<'_>],
    ) -> Result<(), MqttError> {
        let subscribe = Subscribe::new(topics);
        let packet = Packet::Subscribe(subscribe);

        self.send(packet)?;

        Ok(())
    }

    pub async fn poll(&mut self) -> Result<(), TinyMqttError> {
        self.poll_internal(true).await
    }

    async fn poll_internal(&mut self, drain_receive_queue: bool) -> Result<(), TinyMqttError> {
        let time = (self.current_millis_fn)();

        if time > self.last_sent_millis + ((self.timeout_secs as u64 / 2) * 1000) {
            // ping
            println!("ping");
            self.send(Packet::Pingreq)?;
            self.last_sent_millis = (self.current_millis_fn)();
        }

        println!("send_internal");
        self.send_internal().await?;

        // println!("receive_internal");
        // self.receive_internal().await?;

        // // just drain the received packets for now
        // println!("drain packets");
        // if drain_receive_queue {
        //     while let Some(received) = self.recv_queue.borrow_mut().dequeue() {
        //         if let Packet::Publish(publish) = received.parsed() {
        //             if let Some(callback) = self.receive_callback {
        //                 callback(publish.topic_name, publish.payload);
        //             }
        //         }
        //     }
        // }

        Ok(())
    }

    pub async fn receive(&mut self) -> Result<(), TinyMqttError> {
        self.receive_internal().await?;
        Ok(())
    }
    
    async fn receive_internal(&mut self) -> Result<(), TinyMqttError> {
        loop {
            let mut buffer = [0u8; 1024];
            // let len = self.socket.read(&mut buffer).unwrap();
            if self.socket.can_recv() {
                println!("can_recv true");
                // socket.read() won't block, there are data waiting to be read.
                // TODO: create Embassy task to await on read()
            } else {
                println!("can_recv false");
                // nothing received in the socket, if read() is called it will
                // block until something arrives.
                return Ok(());
            }
            let len = self.socket.read(&mut buffer).await.unwrap();
            // let state = self.socket.state();
            if len > 0 {
                println!("got {} bytes: {:02x?}", len, &buffer[..len]);
            }

            self.recv_buffer[self.recv_index..][..len].copy_from_slice(&buffer[..len]);
            self.recv_index += len;

            let data = self.recv_buffer[..len].as_ref();
            let packet = decode_slice(data);

            if let Ok(Some(packet)) = packet {
                println!("{:?}", packet);
                self.recv_index = 0;
                self.recv_queue
                    .borrow_mut()
                    .enqueue(PacketBuffer::new(packet))
                    .ok();
            } else {
                println!("Error decoding mqtt package");
            }

            if len == 0 {
                return Ok(());
            }
        }
    }

    async fn send_internal(&mut self) -> Result<(), TinyMqttError> {
        loop {
            let dq = self.queue.borrow_mut().dequeue();
            match dq {
                Some((len, buffer)) => loop {
                    // println!("try sending a buffer, len = {}", len);
                    // if self.socket.write(&buffer[..len]).is_ok() {
                    if self.socket.write(&buffer[..len]).await.is_ok() {
                        println!("fine");
                        return Ok(());
                    }
                },
                None => return Ok(()),
            }
        }
    }
}

impl<'a> Mqtt for TinyMqtt<'a> {
    fn send(&self, packet: mqttrust::Packet<'_>) -> Result<(), mqttrust::MqttError> {
        let mut buf = [0u8; 1024];
        let len = encode_slice(&packet, &mut buf).unwrap();

        // println!("add to queue: {}, {:?}", len, buf);
        self.queue.borrow_mut().enqueue((len, buf)).ok();
        Ok(())
    }

    fn client_id(&self) -> &str {
        self.client_id
    }
}
