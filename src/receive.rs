use std::thread;
use std::sync::mpsc::{self, channel, TryRecvError};
use std::io::{Read, ErrorKind};
use std::collections::HashMap;
use std::time::Duration;

use mio::*;
use mio::tcp::TcpListener;

use ladspa::{PluginDescriptor, Plugin, PortConnection};
use ladspa::{Port, PortDescriptor};
use ladspa::{PROP_NONE, HINT_INTEGER, DefaultValue};

use super::BASE_PORT;
use super::packet::{BYTE_BUFFER_SIZE, Packet};

type ClientPacket = (u64, Packet);

const SERVER: Token = Token(0);

pub struct Receiver {
    sample_rate: u64,
    channel: u16,
    packet_rx: Option<mpsc::Receiver<ClientPacket>>,
    active_packets: Vec<ClientPacket>,
    notify_tx: Option<Sender<<PacketReceiver as Handler>::Message>>,
    client_time_map: HashMap<u64, u64>,
}

impl Receiver {
    pub fn new(_: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        println!("receiver::new");
        Box::new(Receiver {
            sample_rate: sample_rate,
            channel: 0,
            packet_rx: None,
            active_packets: Vec::new(),
            notify_tx: None,
            client_time_map: HashMap::new(),
        })
    }

    pub fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor {
            unique_id: 5878,
            label: "feedback_rx",
            properties: PROP_NONE,
            name: "Feedback Receiver",
            maker: "Noah Weninger",
            copyright: "None",
            ports: vec![Port {
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
            }],
            new: Receiver::new,
        }
    }

    fn init_server(&mut self) {
        let (data_tx, data_rx) = channel();
        self.packet_rx = Some(data_rx);

        let channel = self.channel;
        let mut event_loop = EventLoop::new().unwrap();
        self.notify_tx = Some(event_loop.channel());
        thread::spawn(move || {
            let addr = format!("127.0.0.1:{}", BASE_PORT + channel).parse().unwrap();
            let server;
            loop {
                match TcpListener::bind(&addr) {
                    Ok(s) => {
                        server = s;
                        break;
                    }
                    Err(_) => {}
                }
            }
            event_loop.register(&server, SERVER).unwrap();
            event_loop.run(&mut PacketReceiver {
                          server: server,
                          data_tx: data_tx,
                          client_id: 0,
                      })
                      .unwrap();
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
            return;
        }

        loop {
            let packet = match self.packet_rx.as_ref().unwrap().try_recv() {
                Ok(packet) => packet,
                Err(TryRecvError::Disconnected) => {
                    println!("ladspa packet receive failed, dead channel!");
                    self.kill_server();
                    self.init_server();
                    break;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
            };
            self.active_packets.push(packet);
        }

        for i in 0..sample_count {
            outputl[i] = inputl[i];
            outputr[i] = inputr[i];
        }

        let mut read_clients = Vec::new();
        for &(ref client_id, ref packet) in &self.active_packets {
            let client_time = self.client_time_map.get(client_id).map(|x| *x).unwrap_or(0);
            for i in 0..sample_count {
                let (l, r) = packet.read(client_time + i as u64);
                outputl[i] += l;
                outputr[i] += r;
            }
            read_clients.push(*client_id);
        }
        for client_id in read_clients {
            let client_time = self.client_time_map.get(&client_id).map(|x| *x).unwrap_or(0);
            self.client_time_map.insert(client_id, client_time + sample_count as u64);
        }

        let mut i = self.active_packets.len();
        while i > 0 {
            i -= 1;
            let complete = {
                let (ref client_id, ref packet) = self.active_packets[i];
                let client_time = self.client_time_map[client_id];
                packet.complete(client_time)
            };
            if complete {
                self.active_packets.remove(i);
            }
        }
    }

    fn activate(&mut self) {
        println!("activate {}", self.channel);
        self.client_time_map.clear();
        self.init_server();
    }

    fn deactivate(&mut self) {
        println!("deactivate {}", self.channel);
        self.kill_server();
    }
}

struct PacketReceiver {
    server: TcpListener,
    data_tx: mpsc::Sender<ClientPacket>,
    client_id: u64,
}

impl Handler for PacketReceiver {
    type Timeout = ();
        type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        match token {
            SERVER => {
                // Only receive readable events
                assert!(events.is_readable());

                println!("server wait");
                match self.server.accept() {
                    Ok(Some(mut socket)) => {
                        socket.set_nodelay(true).unwrap();
                        let tx = self.data_tx.clone();
                        let mut buf = [0; BYTE_BUFFER_SIZE];
                        let mut buf_pos = 0;
                        let client_id = self.client_id;
                        println!("server accept client {}", client_id);
                        self.client_id += 1;
                        loop {
                            let res = socket.read(&mut buf[buf_pos..]);
                            match res {
                                Ok(num_read) => {
                                    //println!("server read {}", num_read);
                                    // if we got a length zero read, the connection is done.
                                    if num_read == 0 {
                                        println!("read zero bytes");
                                        return;
                                    }

                                    // check if we've filled the buffer
                                    buf_pos += num_read;
                                    if buf_pos != BYTE_BUFFER_SIZE {
                                        continue;
                                    }
                                    buf_pos = 0;
                                }
                                Err(e) => {
                                    if e.kind() == ErrorKind::WouldBlock {
                                        //TODO this is a quick hack to reduce CPU usage
                                        thread::sleep(Duration::from_millis(1));
                                        continue;
                                    }
                                    panic!(e);
                                }
                            }
                            let packet = Packet::parse(&buf[..]);
                            if let Err(_) = tx.send((client_id, packet)) {
                                println!("send packet to ladspa error! channel is dead.");
                                return;
                            }
                        }
                    }
                    Ok(None) => {
                        println!("the server socket wasn't actually ready");
                    }
                    Err(e) => {
                        println!("listener.accept() errored: {}", e);
                        // event_loop.shutdown();
                    }
                }
            }
            _ => panic!("Received unknown token"),
        }
    }
    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        event_loop.shutdown();
    }
}
