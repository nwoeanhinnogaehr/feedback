extern crate ladspa;
extern crate mio;

use std::default::Default;
use std::thread;
use std::sync::mpsc::{self, sync_channel};
use std::mem;
use std::io::{Read, ErrorKind};

use mio::*;
use mio::tcp::{TcpStream, TcpListener};

use ladspa::{Port, PortDescriptor, PortConnection};
use ladspa::{PluginDescriptor, Plugin};
use ladspa::{PROP_NONE};
use ladspa::{HINT_INTEGER};
use ladspa::{DefaultValue};
use ladspa::Data;

const BUFFER_SIZE: usize = 1024;
const BASE_PORT: u16 = 21300;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct Packet {
    position: usize,
    data: [Data; BUFFER_SIZE],
}

impl Packet {
    pub fn parse(bytes: [u8; BUFFER_SIZE*4]) -> Packet {
        Packet {
            position: 0,
            data: unsafe { mem::transmute(bytes) },
        }
    }

    pub fn read(&mut self) -> Data {
        if self.position >= BUFFER_SIZE {
            return 0_f32;
        }
        let data = self.data[self.position];
        self.position += 1;
        data
    }

    pub fn active(&self) -> bool {
        self.position < BUFFER_SIZE
    }
}

struct Transmitter {
    sample_rate: u64,
    channel: u16,
}

impl Transmitter {
    pub fn new(desc: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        Box::new(Transmitter {
            sample_rate: sample_rate,
            channel: 0,
        })
    }

    fn init_client(&mut self) {
        //TODO
    }

    fn kill_client(&mut self) {
        //TODO
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
    //TODO
}

struct Receiver {
    sample_rate: u64,
    channel: u16,
    packet_rx: Option<mpsc::Receiver<Packet>>,
    active_packets: Vec<Packet>,
    notify_tx: Option<Sender<<PacketReceiver as Handler>::Message>>,
}

impl Receiver {
    pub fn new(desc: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        println!("receiver::new");
        Box::new(Receiver {
            sample_rate: sample_rate,
            channel: 0,
            packet_rx: None,
            active_packets: Vec::new(),
            notify_tx: None,
        })
    }

    fn init_server(&mut self) {
        let (data_tx, data_rx) = sync_channel(16);
        self.packet_rx = Some(data_rx);

        let channel = self.channel;
        let mut event_loop = EventLoop::new().unwrap();
        self.notify_tx = Some(event_loop.channel());
        thread::spawn(move || {
            let addr = format!("127.0.0.1:{}", BASE_PORT + channel).parse().unwrap();
            let server = TcpListener::bind(&addr).unwrap();
            event_loop.register(&server, SERVER).unwrap();
            event_loop.run(&mut PacketReceiver {
                server: server,
                data_tx: data_tx,
            }).unwrap();
        });
    }

    fn kill_server(&mut self) {
        self.notify_tx.as_ref().unwrap().send(()).unwrap();
    }
}

impl Plugin for Receiver {
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        let inputl = ports[0].unwrap_audio();
        let inputr = ports[1].unwrap_audio();
        let mut outputl = ports[2].unwrap_audio_mut();
        let mut outputr = ports[3].unwrap_audio_mut();

        let channel = *ports[4].unwrap_control() as u16;

        if channel != self.channel {
            self.channel = channel;
            println!("set channel {}", self.channel);
            self.kill_server();
            self.init_server();
        }

        let recv = match self.packet_rx.as_ref() {
            Some(rx) => rx,
            None => panic!("packet_rx is None!"),
        };

        loop {
            let packet = match recv.try_recv() {
                Ok(packet) => packet,
                Err(_) => break,
            };
            self.active_packets.push(packet);
            println!("have data!");
        }

        for i in 0..sample_count {
            outputl[i] = inputl[i];
            outputr[i] = inputr[i];
            for packet in self.active_packets.iter_mut() {
                let val = packet.read();
                outputl[i] += val;
                outputr[i] += val;
            }
        }

        self.active_packets.retain(|x| x.active());
    }

    fn activate(&mut self) {
        println!("activate {}", self.channel);
        self.init_server();
    }

    fn deactivate(&mut self) {
        println!("deactivate {}", self.channel);
        self.kill_server();
    }
}

struct PacketReceiver {
    server: TcpListener,
    data_tx: mpsc::SyncSender<Packet>,
}

impl Handler for PacketReceiver {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Self>, token: mio::Token, events: mio::EventSet) {
        match token {
            SERVER => {
                // Only receive readable events
                assert!(events.is_readable());

                println!("the server socket is ready to accept a connection");
                match self.server.accept() {
                    Ok(Some(mut socket)) => {
                        let tx = self.data_tx.clone();
                        loop {
                            let mut bytes = [0; BUFFER_SIZE*4];
                            let res = socket.read(&mut bytes);
                            match res {
                                Ok(size) => {
                                    if size == 0 {
                                        println!("zero buffer");
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if e.kind() == ErrorKind::WouldBlock {
                                        continue;
                                    }
                                    panic!(e);
                                }
                            }
                            let packet = Packet::parse(bytes);
                            tx.send(packet).unwrap();
                            println!("recv data!");
                        }
                    }
                    Ok(None) => {
                        println!("the server socket wasn't actually ready");
                    }
                    Err(e) => {
                        println!("listener.accept() errored: {}", e);
                        event_loop.shutdown();
                    }
                }
            },
            _ => panic!("Received unknown token"),
        }
    }
    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        event_loop.shutdown();
    }
}

#[no_mangle]
pub extern fn get_ladspa_descriptor(index: u64) -> Option<PluginDescriptor> {
    match index {
        0 => Some(PluginDescriptor {
            unique_id: 5877,
            label: "feedback_tx",
            properties: PROP_NONE,
            name: "Feedback Transmitter",
            maker: "Noah Weninger",
            copyright: "None",
            ports: vec![
                Port {
                    name: "Left Audio In",
                    desc: PortDescriptor::AudioInput,
                    ..Default::default()
                },
                Port {
                    name: "Right Audio In",
                    desc: PortDescriptor::AudioInput,
                    ..Default::default()
                },
                Port {
                    name: "Left Audio Out",
                    desc: PortDescriptor::AudioOutput,
                    ..Default::default()
                },
                Port {
                    name: "Right Audio Out",
                    desc: PortDescriptor::AudioOutput,
                    ..Default::default()
                },
                Port {
                    name: "Channel",
                    desc: PortDescriptor::ControlInput,
                    hint: Some(HINT_INTEGER),
                    default: Some(DefaultValue::Value0),
                    lower_bound: Some(0_f32),
                    upper_bound: Some(255_f32),
                }
            ],
            new: Transmitter::new,
        }),
        1 => Some(PluginDescriptor {
            unique_id: 5878,
            label: "feedback_rx",
            properties: PROP_NONE,
            name: "Feedback Receiver",
            maker: "Noah Weninger",
            copyright: "None",
            ports: vec![
                Port {
                    name: "Left Audio In",
                    desc: PortDescriptor::AudioInput,
                    ..Default::default()
                },
                Port {
                    name: "Right Audio In",
                    desc: PortDescriptor::AudioInput,
                    ..Default::default()
                },
                Port {
                    name: "Left Audio Out",
                    desc: PortDescriptor::AudioOutput,
                    ..Default::default()
                },
                Port {
                    name: "Right Audio Out",
                    desc: PortDescriptor::AudioOutput,
                    ..Default::default()
                },
                Port {
                    name: "Channel",
                    desc: PortDescriptor::ControlInput,
                    hint: Some(HINT_INTEGER),
                    default: Some(DefaultValue::Value0),
                    lower_bound: Some(0_f32),
                    upper_bound: Some(255_f32),
                }
            ],
            new: Receiver::new,
        }),
        _ => None
    }
}
