#![feature(convert, clone_from_slice)]

extern crate ladspa;
extern crate mio;
extern crate time;
extern crate rustc_serialize;
extern crate rmp_serialize as msgpack;

mod receive;
mod transmit;
mod packet;

use std::default::Default;

use ladspa::{Port, PortDescriptor};
use ladspa::PluginDescriptor;
use ladspa::{PROP_NONE, HINT_INTEGER, DefaultValue};

use receive::Receiver;
use transmit::Transmitter;

const BASE_PORT: u16 = 21300;

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
