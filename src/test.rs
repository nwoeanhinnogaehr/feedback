// TODO
// should directly test the plugins, because why not?
// maybe ladspa should have some testing facilities built in.

use super::get_ladspa_descriptor;
use ladspa::{Data, Port, PortConnection, PortData};
use std::cell::RefCell;
use std::thread;
use std::time::Duration;

const SAMPLE_RATE: u64 = 44100;

#[derive(Debug)]
enum OwnedPortData {
    AudioInput(Vec<Data>),
    AudioOutput(Vec<Data>),
    ControlInput(Data),
    ControlOutput(Data),
}

impl PartialEq for OwnedPortData {
    fn eq(&self, other: &Self) -> bool {
        use self::OwnedPortData::*;

        match *self {
            AudioInput(ref a) | AudioOutput(ref a) => {
                match *other {
                    AudioInput(ref b) | AudioOutput(ref b) => a == b,
                    _ => false,
                }
            }
            ControlInput(ref a) | ControlOutput(ref a) => {
                match *other {
                    ControlInput(ref b) | ControlOutput(ref b) => a == b,
                    _ => false,
                }
            }
        }
    }
}

struct OwnedPortConnection {
    port: Port,
    data: OwnedPortData,
}

// TODO port is fragile
fn make_owned_port_connections(ports: &[Port],
                               size: usize,
                               port_tag: f32,
                               input_tag: f32,
                               output_tag: f32)
                               -> Vec<OwnedPortConnection> {
    use ladspa::PortDescriptor::*;

    let mut out = Vec::new();
    for port in ports {
        let data = match port.desc {
            AudioInput => OwnedPortData::AudioInput(vec![input_tag; size]),
            AudioOutput => OwnedPortData::AudioOutput(vec![output_tag; size]),
            ControlInput => OwnedPortData::ControlInput(port_tag),
            ControlOutput => OwnedPortData::ControlOutput(port_tag),
            Invalid => panic!(),
        };
        out.push(OwnedPortConnection {
            port: port.clone(),
            data: data,
        });
    }
    out
}

fn make_port_connections<'a>(owned: &'a mut [OwnedPortConnection]) -> Vec<PortConnection<'a>> {
    let mut out = Vec::new();
    for port in owned {
        let data = match port.data {
            OwnedPortData::AudioInput(ref data) => PortData::AudioInput(data),
            OwnedPortData::AudioOutput(ref mut data) => PortData::AudioOutput(RefCell::new(data)),
            OwnedPortData::ControlInput(ref data) => PortData::ControlInput(data),
            OwnedPortData::ControlOutput(ref mut data) => {
                PortData::ControlOutput(RefCell::new(data))
            }
        };

        out.push(PortConnection {
            port: port.port,
            data: data,
        });
    }
    out
}

fn borrow_port_connections<'a>(ports: &'a [PortConnection<'a>]) -> Vec<&'a PortConnection<'a>> {
    ports.iter().map(|x| x).collect()
}

#[test]
fn test_working_basic() {
    let sample_count = super::packet::BUFFER_SIZE;
    test_sample_count(sample_count, 0);
}

#[test]
fn test_working_multi_packet() {
    let sample_count = super::packet::BUFFER_SIZE * 4;
    test_sample_count(sample_count, 1);
}

fn test_sample_count(sample_count: usize, port: u8) {
    let tx_desc = get_ladspa_descriptor(0).unwrap();
    let rx_desc = get_ladspa_descriptor(1).unwrap();
    let mut tx = (tx_desc.new)(&tx_desc, SAMPLE_RATE);
    let mut rx = (rx_desc.new)(&rx_desc, SAMPLE_RATE);

    rx.activate();
    tx.activate();

    let mut tx_owned = make_owned_port_connections(&tx_desc.ports,
                                                   sample_count,
                                                   port as f32,
                                                   1.0,
                                                   0.0);
    let mut rx_owned = make_owned_port_connections(&rx_desc.ports,
                                                   sample_count,
                                                   port as f32,
                                                   0.0,
                                                   0.0);
    {
        let tx_ports = make_port_connections(&mut tx_owned);
        let rx_ports = make_port_connections(&mut rx_owned);
        let tx_ports = borrow_port_connections(&tx_ports);
        let rx_ports = borrow_port_connections(&rx_ports);

        tx.run(sample_count, &tx_ports);
        thread::sleep(Duration::from_millis(100)); // wait for recv
        rx.run(sample_count, &rx_ports);
    }

    for i in 0..2 {
        assert_eq!(tx_owned[i].data, rx_owned[i + 2].data);
    }

    rx.deactivate();
    tx.deactivate();
}
