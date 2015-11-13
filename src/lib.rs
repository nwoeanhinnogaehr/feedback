extern crate ladspa;
extern crate mio;

mod receive;
mod transmit;

use std::default::Default;
use std::mem;

use ladspa::{Port, PortDescriptor};
use ladspa::{PluginDescriptor};
use ladspa::{PROP_NONE};
use ladspa::{HINT_INTEGER};
use ladspa::{DefaultValue};
use ladspa::Data;

use receive::Receiver;
use transmit::Transmitter;

const BUFFER_SIZE: usize = 1024;
const BYTE_BUFFER_SIZE: usize = BUFFER_SIZE*8;
const BASE_PORT: u16 = 21300;

struct Packet {
    position: usize,
    data: [(Data, Data); BUFFER_SIZE],
}

impl Packet {
    fn parse(bytes: [u8; BYTE_BUFFER_SIZE]) -> Packet {
        Packet {
            position: 0,
            data: unsafe { mem::transmute(bytes) },
        }
    }

    fn read(&mut self) -> (Data, Data) {
        if self.position >= BUFFER_SIZE {
            return (0_f32, 0_f32);
        }
        let data = self.data[self.position];
        self.position += 1;
        data
    }

    fn active(&self) -> bool {
        self.position < BUFFER_SIZE
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
