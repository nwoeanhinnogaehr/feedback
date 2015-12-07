use ladspa::Data;

use bincode::SizeLimit;
use bincode::rustc_serialize::{encode, decode};

pub const BUFFER_SIZE: usize = 1024;
pub const BYTE_BUFFER_SIZE: usize = BUFFER_SIZE * 4 * 2 + 8 * 2 + 8; // data + data size + timestamp

#[derive(RustcEncodable, RustcDecodable, Clone)]
pub struct Packet {
    ldata: Vec<Data>,
    rdata: Vec<Data>,
    timestamp: u64,
}

impl Packet {
    pub fn parse(bytes: &[u8]) -> Packet {
        decode(bytes).unwrap()
    }

    pub fn new(ldata: &[Data], rdata: &[Data], time: u64) -> Packet {
        assert_eq!(ldata.len(), BUFFER_SIZE);
        assert_eq!(rdata.len(), BUFFER_SIZE);

        let mut packet = Packet {
            ldata: vec![0f32; BUFFER_SIZE],
            rdata: vec![0f32; BUFFER_SIZE],
            timestamp: time,
        };
        (&mut packet.ldata[..]).clone_from_slice(ldata);
        (&mut packet.rdata[..]).clone_from_slice(rdata);

        packet
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        encode(self, SizeLimit::Infinite).unwrap()
    }

    pub fn get_ldata(&self) -> &[Data] {
        &self.ldata[..]
    }

    pub fn get_rdata(&self) -> &[Data] {
        &self.rdata[..]
    }

    pub fn read(&self, time: u64) -> (Data, Data) {
        if !self.active(time) {
            return (0_f32, 0_f32);
        }
        let position = (time - self.timestamp) as usize;
        let ldata = self.ldata[position];
        let rdata = self.rdata[position];
        (ldata, rdata)
    }

    pub fn active(&self, time: u64) -> bool {
        time >= self.timestamp && !self.complete(time)
    }

    pub fn complete(&self, time: u64) -> bool {
        time >= self.timestamp + BUFFER_SIZE as u64
    }
}

#[test]
fn test_packet_serialize() {
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

#[test]
#[should_panic]
fn test_packet_wrong_size() {
    let ldata = vec![1.0; 5];
    let rdata = vec![2.0; 6];
    Packet::new(&ldata, &rdata, 0);
}

#[test]
fn test_packet_read() {
    let ldata = vec![1.0; BUFFER_SIZE];
    let rdata = vec![2.0; BUFFER_SIZE];
    let packet = Packet::new(&ldata, &rdata, 100);
    assert_eq!((0.0, 0.0), packet.read(0));
    assert_eq!((0.0, 0.0), packet.read(99));
    assert_eq!((1.0, 2.0), packet.read(100));
    assert_eq!((1.0, 2.0), packet.read(100 + BUFFER_SIZE as u64 - 1));
    assert_eq!((0.0, 0.0), packet.read(100 + BUFFER_SIZE as u64));
}

#[test]
fn test_packet_active_complete() {
    let ldata = vec![1.0; BUFFER_SIZE];
    let rdata = vec![2.0; BUFFER_SIZE];
    let packet = Packet::new(&ldata, &rdata, 100);
    assert!(!packet.active(0));
    assert!(!packet.complete(0));
    assert!(!packet.active(99));
    assert!(!packet.complete(99));
    assert!(packet.active(100));
    assert!(!packet.complete(100));
    assert!(packet.active(100 + BUFFER_SIZE as u64 - 1));
    assert!(!packet.complete(100 + BUFFER_SIZE as u64 - 1));
    assert!(!packet.active(100 + BUFFER_SIZE as u64));
    assert!(packet.complete(100 + BUFFER_SIZE as u64));
}
