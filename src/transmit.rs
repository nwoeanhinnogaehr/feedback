use std::thread;
use std::sync::mpsc::{self, sync_channel};
use std::io::{Write, ErrorKind};

use mio::*;
use mio::tcp::{TcpStream, Shutdown};

use ladspa::{PluginDescriptor, Plugin, PortConnection, Data};
use ladspa::{Port, PortDescriptor};
use ladspa::{PROP_NONE, HINT_INTEGER, DefaultValue};

use super::BASE_PORT;
use super::packet::{BUFFER_SIZE, BYTE_BUFFER_SIZE, Packet};

const CLIENT: Token = Token(1);

pub struct Transmitter {
    channel: u16,
    data_tx: Option<mpsc::SyncSender<Packet>>,
    notify_tx: Option<Sender<<PacketTransmitter as Handler>::Message>>,
    lbuffer: Vec<Data>,
    rbuffer: Vec<Data>,
    time: u64,
}

impl Transmitter {
    pub fn new(_: &PluginDescriptor, _: u64) -> Box<Plugin + Send> {
        Box::new(Transmitter {
            channel: 0,
            data_tx: None,
            notify_tx: None,
            lbuffer: Vec::new(),
            rbuffer: Vec::new(),
            time: 0,
        })
    }

    pub fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor {
            unique_id: 5877,
            label: "feedback_tx",
            properties: PROP_NONE,
            name: "Feedback Transmitter",
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
                        },
                        Port {
                            name: "Dry",
                            desc: PortDescriptor::ControlInput,
                            hint: None,
                            default: Some(DefaultValue::Value1),
                            lower_bound: Some(0_f32),
                            upper_bound: Some(1_f32),
                        },
                        Port {
                            name: "Send",
                            desc: PortDescriptor::ControlInput,
                            hint: None,
                            default: Some(DefaultValue::Value1),
                            lower_bound: Some(0_f32),
                            upper_bound: Some(1_f32),
                        }],
            new: Transmitter::new,
        }
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
            client.set_nodelay(true).unwrap();
            event_loop.register(&client, CLIENT).unwrap();
            event_loop.run(&mut PacketTransmitter {
                          socket: client,
                          data_rx: data_rx,
                      })
                      .unwrap();
        });
    }

    fn kill_client(&mut self) {
        let _ = self.notify_tx.as_ref().unwrap().send(());
    }

    fn restart_client(&mut self) {
        self.kill_client();
        self.init_client();
    }

    fn set_channel(&mut self, channel: u16) {
        if channel != self.channel {
            self.channel = channel;
            println!("set channel {}", self.channel);
            self.restart_client();
            return;
        }
    }
}

impl Plugin for Transmitter {
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        let inputl = ports[0].unwrap_audio();
        let inputr = ports[1].unwrap_audio();
        let mut outputl = ports[2].unwrap_audio_mut();
        let mut outputr = ports[3].unwrap_audio_mut();

        let channel = *ports[4].unwrap_control() as u16;
        let dry = ports[5].unwrap_control();
        let wet = ports[6].unwrap_control();

        self.set_channel(channel);

        let mut need_reboot = false;
        let mut i = 0;
        while i < sample_count {
            while self.lbuffer.len() < BUFFER_SIZE && i < sample_count {
                self.lbuffer.push(inputl[i] * (*wet));
                self.rbuffer.push(inputr[i] * (*wet));

                outputl[i] = inputl[i] * (*dry);
                outputr[i] = inputr[i] * (*dry);

                i += 1;
            }

            if self.lbuffer.len() == BUFFER_SIZE {
                let packet = Packet::new(&self.lbuffer, &self.rbuffer, self.time);
                self.time += BUFFER_SIZE as u64;

                need_reboot |= self.data_tx.as_ref().unwrap().send(packet).is_err();

                self.lbuffer.clear();
                self.rbuffer.clear();
            }
        }
        if need_reboot {
            println!("transmit failed, rebooting");
            self.restart_client();
        }
    }

    fn activate(&mut self) {
        println!("activate {}", self.channel);
        self.lbuffer.clear();
        self.rbuffer.clear();
        self.time = 0;
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
                println!("client accept");
                'outer: loop {
                    assert!(events.is_writable());
                    let packet = match self.data_rx.recv() {
                        Ok(p) => p,
                        Err(_) => {
                            println!("err recieving packet from ladspa, channel is dead!");
                            event_loop.shutdown();
                            break;
                        }
                    };
                    let mut write_offset = 0;
                    loop {
                        match self.socket.write(&packet.as_bytes()[write_offset..]) {
                            Ok(num_written) => {
                                // println!("client wrote {}", num_written);
                                write_offset += num_written;
                                assert!(write_offset <= BYTE_BUFFER_SIZE);
                                if write_offset == BYTE_BUFFER_SIZE {
                                    break;
                                }
                                if num_written == 0 {
                                    println!("wrote zero bytes");
                                    event_loop.shutdown();
                                    break 'outer;
                                }
                            }
                            Err(e) => {
                                if e.kind() == ErrorKind::WouldBlock {
                                    continue;
                                }
                                println!("error writing to socket: {}", e);
                                event_loop.shutdown();
                                break 'outer;
                            }
                        }
                    }
                }
            }
            _ => panic!("Received unknown token"),
        }
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, _: Self::Message) {
        let _ = self.socket.shutdown(Shutdown::Both);
        event_loop.shutdown();
    }
}
