use std::thread;
use std::sync::mpsc::{self, sync_channel};
use std::io::{Read, ErrorKind};

use mio::*;
use mio::tcp::TcpStream;

use ladspa::{PluginDescriptor, Plugin, PortConnection};

use super::{BUFFER_SIZE, BYTE_BUFFER_SIZE, BASE_PORT};
use super::Packet;

const CLIENT: Token = Token(1);

pub struct Transmitter {
    sample_rate: u64,
    channel: u16,
    packet_tx: Option<mpsc::SyncSender<Packet>>,
    notify_tx: Option<Sender<<PacketTransmitter as Handler>::Message>>,
}

impl Transmitter {
    pub fn new(desc: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        Box::new(Transmitter {
            sample_rate: sample_rate,
            channel: 0,
            packet_tx: None,
            notify_tx: None,
        })
    }

    fn init_client(&mut self) {
        let (data_tx, data_rx) = sync_channel(16);
        self.packet_tx = Some(data_tx);

        let channel = self.channel;
        let mut event_loop = EventLoop::new().unwrap();
        self.notify_tx = Some(event_loop.channel());
        thread::spawn(move || {
            let addr = format!("127.0.0.1:{}", BASE_PORT + channel).parse().unwrap();
            let client = TcpStream::connect(&addr).unwrap();
            event_loop.register(&client, CLIENT).unwrap();
            event_loop.run(&mut PacketTransmitter {
                socket: client,
                data_rx: data_rx,
            }).unwrap();
        });
    }

    fn kill_client(&mut self) {
        self.notify_tx.as_ref().unwrap().send(()).unwrap();
    }
}

impl Plugin for Transmitter {
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        let inputl = ports[0].unwrap_audio();
        let inputr = ports[1].unwrap_audio();
        let mut outputl = ports[2].unwrap_audio_mut();
        let mut outputr = ports[3].unwrap_audio_mut();

        let channel = *ports[4].unwrap_control() as u16;

        if channel != self.channel {
            self.channel = channel;
            println!("set channel {}", self.channel);
            self.kill_client();
            self.init_client();
        }

        //TODO
    }

    fn activate(&mut self) {
        println!("activate {}", self.channel);
        self.init_client();
    }

    fn deactivate(&mut self) {
        println!("deactivate {}", self.channel);
        self.kill_client();
    }
}

struct PacketTransmitter {
    socket: TcpStream,
    data_rx: mpsc::Receiver<Packet>,
}

impl Handler for PacketTransmitter {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        event_loop.shutdown();
    }
}

