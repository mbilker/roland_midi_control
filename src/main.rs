#![allow(unused_imports)]

extern crate jack;
extern crate rimd;

use jack::prelude::{AsyncClient, Client, ClosureProcessHandler, JackControl, MidiInPort,
                    MidiInSpec, MidiOutPort, MidiOutSpec, PortFlags, ProcessScope, RawMidi, client_options};
use rimd::MidiMessage;
use std::io;

static DEFAULT_OUTPUT_PORT: &'static str = "alsa_midi:Scarlett 2i4 USB MIDI 1 (in)";

// Checksum Algorithm = 0x80 - (sum of address and data bytes) % 0x80

// System Exclusive Message Format:
// 0xf0 = System Exclusive Message status
// 0x41 = Roland's Manufacturer ID
// 0x00 = Device ID (REPLACE AT RUNTIME)
// 0x42 = Model ID (GS)
// 0x12 = Command ID (Data Type 1)
// 0xXX = Address MSB (REPLACE AT RUNTIME)
// 0xXX = Address (REPLACE AT RUNTIME)
// 0xXX = Address LSB (REPLACE AT RUNTIME)
// ...  = Data
// 0xXX = Checksum
// 0xf7 = End Of Exclusive
static ROLAND_SYSEX_PREFIX: &'static [u8] = &[0xf0, 0x41, 0x00, 0x42, 0x12, 0x00, 0x00, 0x00];
static END_OF_EXCLUSIVE: u8 = 0xf7;

#[derive(Clone, Copy)]
enum ProgramState {
  Initial,
  ConnectedPorts,
  LoadedRules,
}

#[derive(Debug)]
enum MFXType {
  Distortion,     // P-06: Distortion
}

impl MFXType {
  fn value(&self) -> &[u8; 2] {
    match *self {
      MFXType::Distortion => &[0x01, 0x11],
    }
  }
}

#[derive(Debug)]
struct RolandSysEx {
  device_id: u8,
}

impl RolandSysEx {
  pub fn new(device_id: u8) -> Self {
    Self {
      device_id,
    }
  }

  fn data(&self, address: &[u8], data: &[u8]) -> Vec<u8> {
    let sum = address.into_iter().sum::<u8>() + data.into_iter().sum::<u8>();
    let checksum = 0x80 - sum % 0x80;

    // Allocate array with the exact size to avoid reallocation
    let len = ROLAND_SYSEX_PREFIX.len() + data.len() + 2;
    let mut msg = Vec::with_capacity(len);
    msg.extend_from_slice(ROLAND_SYSEX_PREFIX);

    msg[2] = self.device_id;

    // Copy elements 5-7, Rust ranges exclude the last element
    for i in 5..8 {
      msg[i] = address[i - 5];
    }
    msg.extend_from_slice(data);
    msg.push(checksum);
    msg.push(END_OF_EXCLUSIVE);
    msg
  }

  // Enable M-FX for part, 0x40 0x4X 0x22 0x01, X = "Part Number"
  pub fn enable_mfx(&self, part: u8) -> Vec<u8> {
    self.data(&[0x40, 0x40 + part, 0x22], &[0x01])
  }

  // Set M-FX to type, 0x40 0x03 0x00 + mode value
  pub fn set_mfx_type(&self, mode: MFXType) -> Vec<u8> {
    self.data(&[0x40, 0x03, 0x00], mode.value())
  }
}

fn main() {
  // open client
  let (client, _status) = Client::new("rust_jack_show_midi", client_options::NO_START_SERVER).unwrap();

  let mut current_state = ProgramState::Initial;

  // process logic
  let mut maker = client.register_port("midi_out", MidiOutSpec::default()).unwrap();
  let shower = client.register_port("midi_in", MidiInSpec::default()).unwrap();

  let ports = client.ports(None, None, PortFlags::empty());
  println!("{:#?}", ports);

  // Get name of output port to connect it to system output later
  let maker_info = maker.clone_unowned();
  let maker_name = maker_info.name();

  let output_system_port = DEFAULT_OUTPUT_PORT;
  //let output_system_port = "MIDI monitor:midi_in";

  let cback = move |_: &Client, ps: &ProcessScope| -> JackControl {
    let connected_num = maker.connected_count() > 0;

    let show_p = MidiInPort::new(&shower, ps);
    let mut put_p = MidiOutPort::new(&mut maker, ps);

    match current_state {
      ProgramState::Initial => {
        if connected_num {
          current_state = ProgramState::ConnectedPorts;
        }
      },
      ProgramState::ConnectedPorts => {
        let rules = vec![
          RolandSysEx::new(0x10).enable_mfx(0x01),                    // Enable M-FX for A01
          RolandSysEx::new(0x10).enable_mfx(0x02),                    // Enable M-FX for A02
          RolandSysEx::new(0x10).set_mfx_type(MFXType::Distortion),   // Set M-FX to P-06: Distortion
        ];
        let mut time = 0;
        for rule in rules {
          let msg = RawMidi {
            time: time,
            bytes: &rule,
          };
          time += 1;
          put_p.write(&msg).unwrap();
          println!("{}", MidiMessage::from_bytes(msg.bytes.to_vec()));
        }
        current_state = ProgramState::LoadedRules;
      },
      _ => (),
    }

    for e in show_p.iter() {
      let msg = MidiMessage::from_bytes(e.bytes.to_vec());
      println!("{}", msg);

      put_p.write(&e).unwrap();
    }
    JackControl::Continue
  };

  // activate
  let process = ClosureProcessHandler::new(cback);
  let active_client = AsyncClient::new(client, (), process).unwrap();

  active_client.connect_ports_by_name(maker_name, output_system_port).unwrap();
  println!("Connected to {}", output_system_port);

  //active_client.connect_ports_by_name(maker_name, "MIDI monitor:midi_in").unwrap();

  // wait
  println!("Press any key to quit");
  let mut user_input = String::new();
  io::stdin().read_line(&mut user_input).ok();

  // optional deactivation
  active_client.deactivate().unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_enable_mfx() {
    assert_eq!(RolandSysEx::new(0x10).enable_mfx(0x01), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x41, 0x22, 0x01, 0x5c, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).enable_mfx(0x02), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x42, 0x22, 0x01, 0x5b, 0xf7]);
  }

  #[test]
  fn test_set_mfx_type() {
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Distortion), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x11, 0x2b, 0xf7]);
  }
}
