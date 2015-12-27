#![feature(clone_from_slice)]
#![cfg_attr(test, feature(convert))]

extern crate ladspa;
extern crate mio;
extern crate rustc_serialize;
extern crate bincode;

mod receive;
mod transmit;
mod packet;

#[cfg(test)]
mod test;

use ladspa::PluginDescriptor;

use receive::Receiver;
use transmit::Transmitter;

const BASE_PORT: u16 = 21300;

#[no_mangle]
pub extern "C" fn get_ladspa_descriptor(index: u64) -> Option<PluginDescriptor> {
    match index {
        0 => Some(Transmitter::get_descriptor()),
        1 => Some(Receiver::get_descriptor()),
        _ => None,
    }
}
