#![feature(convert, slice_bytes, clone_from_slice)]

extern crate ladspa;
extern crate mio;
extern crate time;
extern crate rustc_serialize;
extern crate rmp_serialize as msgpack;

mod receive;
mod transmit;

use std::default::Default;
use std::mem;
use std::slice::{self, bytes};

use ladspa::{Port, PortDescriptor};
use ladspa::{PluginDescriptor};
use ladspa::{PROP_NONE};
use ladspa::{HINT_INTEGER};
use ladspa::{DefaultValue};
use ladspa::Data;

use receive::Receiver;
use transmit::Transmitter;

use rustc_serialize::{Decodable, Encodable};
use msgpack::{Decoder, Encoder};

const BUFFER_SIZE: usize = 1024;
const BYTE_BUFFER_SIZE: usize = 10249; // not sure how to find this other than by running and testing it out!
const BASE_PORT: u16 = 21300;

//TODO timestamps
#[derive(RustcEncodable, RustcDecodable)]
struct Packet {
    position: usize,
    ldata: Vec<Data>,
    rdata: Vec<Data>,
    timestamp: u64,
}

impl Packet {
    fn parse(bytes: &[u8]) -> Packet {
        let mut decoder = Decoder::new(bytes);
        let packet = Decodable::decode(&mut decoder).unwrap();
        packet
    }

    fn new(ldata: &[Data], rdata: &[Data], time: u64) -> Packet {
        assert_eq!(ldata.len(), BUFFER_SIZE);
        assert_eq!(rdata.len(), BUFFER_SIZE);

        let mut packet = Packet {
            position: 0,
            ldata: vec![0f32; BUFFER_SIZE],
            rdata: vec![0f32; BUFFER_SIZE],
            timestamp: time,
        };
        (&mut packet.ldata[..]).clone_from_slice(ldata);
        (&mut packet.rdata[..]).clone_from_slice(rdata);

        packet
    }

    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.encode(&mut Encoder::new(&mut bytes));
        bytes
    }

    fn get_ldata(&self) -> &[Data] {
        &self.ldata[..]
    }

    fn get_rdata(&self) -> &[Data] {
        &self.rdata[..]
    }

    fn read(&mut self) -> (Data, Data) {
        if self.position >= BUFFER_SIZE {
            return (0_f32, 0_f32);
        }
        let ldata = self.ldata[self.position];
        let rdata = self.rdata[self.position];
        self.position += 1;
        (ldata, rdata)
    }

    fn active(&self) -> bool {
        self.position < BUFFER_SIZE
    }
}

#[test]
fn test_packet() {
    let ldata = vec![1.0; BUFFER_SIZE];
    let rdata = vec![2.0; BUFFER_SIZE];
    let new = Packet::new(&ldata, &rdata, 0);
    assert_eq!(new.get_ldata(), ldata.as_slice());
    assert_eq!(new.get_rdata(), rdata.as_slice());
    let parsed = Packet::parse(&new.as_bytes()[..]);
    assert_eq!(parsed.get_ldata(), ldata.as_slice());
    assert_eq!(parsed.get_rdata(), rdata.as_slice());
    assert_eq!(&new.as_bytes()[..], &parsed.as_bytes()[..]);
    println!("{}", new.as_bytes().len());
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
