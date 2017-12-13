#![allow(unused_imports)]

extern crate jack;
extern crate rimd;

use jack::prelude::{AsyncClient, Client, ClosureProcessHandler, JackControl, MidiInPort,
                    MidiInSpec, MidiOutPort, MidiOutSpec, PortFlags, ProcessScope, RawMidi, client_options};
use rimd::MidiMessage;
use std::io;

static DEFAULT_OUTPUT_PORT: &'static str = "alsa_midi:Scarlett 2i4 USB MIDI 1 (in)";

// Checksum Algorithm:
// checksum = 0x80 - (sum of address and data bytes) % 128

#[derive(Clone, Copy)]
enum ProgramState {
  Initial,
  ConnectedPorts,
  LoadedRules,
}

#[derive(Debug)]
struct RolandSysEx {}

// Enable M-FX, 40 4X 22 01, X = "Part Number"

impl RolandSysEx {
  fn enable_mfx(part: u8) -> Vec<u8> {
    let checksum = 0x80 - (0x40 + 0x40 + part + 0x22) % 0x80;
    vec![0x40, 0x40 + part, 0x22, 0x01, checksum]
  }
}

#[derive(Debug)]
enum MFxSetMode {
  Distortion,   // P-06: Distortion
}

impl MFxSetMode {
  fn value(&self) -> &[u8; 3] {
    match *self {
      MFxSetMode::Distortion => &[0x03, 0x00, 0x01],
    }
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
          vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x41, 0x22, 0x01, 0x5c, 0xf7],         // Enable M-FX for A01
          vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x42, 0x22, 0x01, 0x5b, 0xf7],         // Enable M-FX for A02
          vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x11, 0x2b, 0xf7],   // Set M-FX to P-06: Distortion
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
