use std::thread;
use std::sync::mpsc::{self, sync_channel};
use std::io::Write;

use mio::*;
use mio::tcp::{TcpStream, Shutdown};

use ladspa::{PluginDescriptor, Plugin, PortConnection, Data};

use super::BASE_PORT;
use super::packet::{BUFFER_SIZE, BYTE_BUFFER_SIZE, Packet};

const CLIENT: Token = Token(1);

pub struct Transmitter {
    sample_rate: u64,
    channel: u16,
    data_tx: Option<mpsc::SyncSender<Packet>>,
    notify_tx: Option<Sender<<PacketTransmitter as Handler>::Message>>,
    lbuffer: Vec<Data>,
    rbuffer: Vec<Data>,
    time: u64,
}

impl Transmitter {
    pub fn new(_: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
        Box::new(Transmitter {
            sample_rate: sample_rate,
            channel: 0,
            data_tx: None,
            notify_tx: None,
            lbuffer: Vec::new(),
            rbuffer: Vec::new(),
            time: 0,
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
}

impl Plugin for Transmitter {
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        println!("sample count {}", sample_count);
        let inputl = ports[0].unwrap_audio();
        let inputr = ports[1].unwrap_audio();
        let mut outputl = ports[2].unwrap_audio_mut();
        let mut outputr = ports[3].unwrap_audio_mut();

        let channel = *ports[4].unwrap_control() as u16;

        let mut need_reboot = false;

        if channel != self.channel {
            self.channel = channel;
            println!("set channel {}", self.channel);
            need_reboot = true;
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
                let packet = Packet::new(&self.lbuffer, &self.rbuffer, self.time);
                self.time += BUFFER_SIZE as u64;

                need_reboot |= self.data_tx.as_ref().unwrap().send(packet).is_err();

                self.lbuffer.clear();
                self.rbuffer.clear();
            }
        }
        if need_reboot {
            self.kill_client();
            self.init_client();
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
                loop {
                    assert!(events.is_writable());
                    let packet = match self.data_rx.recv() {
                        Ok(p) => p,
                        Err(_) => {
                            println!("err recieving packet from ladspa, channel is dead!");
                            event_loop.shutdown();
                            break;
                        }
                    };
                    match self.socket.write(&packet.as_bytes()[..]) {
                        Ok(num_written) => {
                            //println!("client wrote {}", num_written);
                            if num_written != BYTE_BUFFER_SIZE {
                                println!("incorrect write size: {}", num_written);
                                event_loop.shutdown();
                                break;
                            }
                        }
                        Err(_) => {
                            println!("err writing packet to network!");
                            event_loop.shutdown();
                            break;
                        }
                    }
                }
            }
            _ => panic!("Received unknown token"),
        }
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        let _ = self.socket.shutdown(Shutdown::Both);
        event_loop.shutdown();
    }
}
