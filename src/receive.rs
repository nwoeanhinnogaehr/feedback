use std::thread;
use std::sync::mpsc::{self, sync_channel, TryRecvError};
use std::io::{Read, ErrorKind};

use mio::*;
use mio::tcp::TcpListener;

use ladspa::{PluginDescriptor, Plugin, PortConnection};

use super::{BUFFER_SIZE, BYTE_BUFFER_SIZE, BASE_PORT};
use super::Packet;

const SERVER: Token = Token(0);

pub struct Receiver {
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
            let server;
            loop {
                match TcpListener::bind(&addr) {
                    Ok(s) => {
                        server = s;
                        break;
                    },
                    Err(_) => { },
                }
            }
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

        loop {
            if self.active_packets.len() > 128 {
                break;
            }
            let packet = match self.packet_rx.as_ref().unwrap().try_recv() {
                Ok(packet) => packet,
                Err(TryRecvError::Disconnected) => {
                    println!("ladspa packet receive failed, dead channel!");
                    self.kill_server();
                    self.init_server();
                    break;
                },
                Err(TryRecvError::Empty) => {
                    break;
                }
            };
            self.active_packets.push(packet);
        }

        for i in 0..sample_count {
            outputl[i] = inputl[i];
            outputr[i] = inputr[i];
            //TODO mix overlapping data
            if let Some(mut packet) = self.active_packets.first_mut() {
                let (l, r) = packet.read();
                outputl[i] += l;
                outputr[i] += r;
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

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        match token {
            SERVER => {
                // Only receive readable events
                assert!(events.is_readable());

                println!("the server socket is ready to accept a connection");
                match self.server.accept() {
                    Ok(Some(mut socket)) => {
                        let tx = self.data_tx.clone();
                        let mut buf = [0; BYTE_BUFFER_SIZE];
                        let mut buf_pos = 0;
                        loop {
                            let res = socket.read(&mut buf[buf_pos..]);
                            match res {
                                Ok(num_read) => {
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
                                        continue;
                                    }
                                    panic!(e);
                                }
                            }
                            let packet = Packet::parse(buf);
                            if let Err(_) = tx.send(packet) {
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
                        //event_loop.shutdown();
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
