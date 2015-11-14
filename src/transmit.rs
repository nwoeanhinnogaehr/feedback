use std::thread;
use std::sync::mpsc::{self, sync_channel, SendError};
use std::io::{Write, Read, ErrorKind};

use mio::*;
use mio::tcp::{TcpStream, Shutdown};

use ladspa::{PluginDescriptor, Plugin, PortConnection, Data};

use super::{BUFFER_SIZE, BYTE_BUFFER_SIZE, BASE_PORT};
use super::Packet;

const CLIENT: Token = Token(1);

pub struct Transmitter {
    sample_rate: u64,
    channel: u16,
    data_tx: Option<mpsc::SyncSender<Packet>>,
    notify_tx: Option<Sender<<PacketTransmitter as Handler>::Message>>,
    lbuffer: Vec<Data>,
    rbuffer: Vec<Data>,
}

impl Transmitter {
    pub fn new(desc: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        Box::new(Transmitter {
            sample_rate: sample_rate,
            channel: 0,
            data_tx: None,
            notify_tx: None,
            lbuffer: Vec::new(),
            rbuffer: Vec::new(),
        })
    }

    fn init_client(&mut self) {
        let (data_tx, data_rx) = sync_channel(16);
        self.data_tx = Some(data_tx);

        let channel = self.channel;
        let mut event_loop = EventLoop::new().unwrap();
        self.notify_tx = Some(event_loop.channel());
        thread::spawn(move || {
            let addr = format!("127.0.0.1:{}", BASE_PORT + channel).parse().unwrap();
            let client = TcpStream::connect(&addr).unwrap();
            client.set_nodelay(true);
            event_loop.register(&client, CLIENT).unwrap();
            event_loop.run(&mut PacketTransmitter {
                socket: client,
                data_rx: data_rx,
            }).unwrap();
        });
    }

    fn kill_client(&mut self) {
        self.notify_tx.as_ref().unwrap().send(());
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

        let mut i = 0;
        while i < sample_count {
            while self.lbuffer.len() < BUFFER_SIZE && i < sample_count {
                self.lbuffer.push(inputl[i]);
                self.rbuffer.push(inputr[i]);

                outputl[i] = inputl[i];
                outputr[i] = inputr[i];

                i += 1;
            }

            if self.lbuffer.len() == BUFFER_SIZE {
                let mut packet = Packet::new(&self.lbuffer, &self.rbuffer);
                //TODO this should really push them to a queue, don't want to wait here forever.
                while let Err(SendError(p)) = self.data_tx.as_ref().unwrap().send(packet) {
                    self.kill_client();
                    self.init_client();
                    packet = p;
                }

                self.lbuffer.clear();
                self.rbuffer.clear();
            }
        }
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
        match token {
            CLIENT => {
                loop {
                    assert!(events.is_writable());
                    let packet = match self.data_rx.recv() {
                        Ok(p) => p,
                        Err(_) => {
                            println!("err recieving packet from ladspa, channel is dead!");
                            break;
                        }
                    };
                    match self.socket.write(&packet.as_bytes()[..]) {
                        Ok(num_written) => assert!(num_written == BYTE_BUFFER_SIZE),
                        Err(_) => {
                            println!("err writing packet to network!");
                            event_loop.shutdown();
                            break;
                        }
                    }
                }
            },
            _ => panic!("Received unknown token"),
        }
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        self.socket.shutdown(Shutdown::Both);
        event_loop.shutdown();
    }
}

