#![allow(unused_imports)]
#![allow(dead_code)]

extern crate hex_slice;
extern crate jack;
extern crate rimd;

use hex_slice::AsHex;
use jack::prelude::{AsyncClient, Client, ClosureProcessHandler, JackControl, MidiInPort,
                    MidiInSpec, MidiOutPort, MidiOutSpec, PortFlags, ProcessScope, RawMidi, client_options};
use rimd::{MidiMessage, Status};
use std::io;

mod sysex;
mod mfx;

use sysex::*;
use mfx::*;

static DEFAULT_OUTPUT_PORT: &'static str = "alsa_midi:Scarlett 2i4 USB MIDI 1 (in)";

#[derive(Clone, Copy)]
enum ProgramState {
  Initial,
  ConnectedPorts,
  LoadedRules,
  WriteMessage,
  Disabled,
}

#[derive(Clone, Copy)]
enum DeviceModel {
  Roland,
  Launchpad,
}

fn main() {
  // open client
  let (client, _status) = Client::new("rust_jack_show_midi", client_options::NO_START_SERVER).unwrap();

  let mut current_state = ProgramState::Initial;

  // Disable Roland stuff for Launchpad stuff
  let device_model = DeviceModel::Launchpad;

  // process logic
  let mut maker = client.register_port("midi_out", MidiOutSpec::default()).unwrap();
  let shower = client.register_port("midi_in", MidiInSpec::default()).unwrap();

  let ports = client.ports(None, None, PortFlags::empty());
  println!("{:#?}", ports);

  // Get name of input/output ports to connect it to system output later
  let shower_info = shower.clone_unowned();
  let shower_name = shower_info.name();
  let maker_info = maker.clone_unowned();
  let maker_name = maker_info.name();

  //let output_system_port = DEFAULT_OUTPUT_PORT;
  let output_system_port = "alsa_midi:Launchpad MK2 MIDI 1 (in)";
  //let output_system_port = "MIDI monitor:midi_in";

  let sysex = RolandSysEx::new(0x10);
  let rules = [
    vec![0xf0, 0x7e, 0x7f, 0x09, 0x01, 0xf7],   // Turn General MIDI System On
    sysex.enable_mfx(0x01, true),               // Enable M-FX for A01
    sysex.enable_mfx(0x02, true),               // Enable-M-FX for A02
    //sysex.set_mfx_type(MFXType::Distortion),   // Set M-FX to P-06: Distortion
    sysex.set_mfx_type(MFXType::LoFi2),         // Set M-FX to P34: Lo-Fi 2
  ];

  let color = 0x64;
  let msg = b"hello world!";
  let scrolling_text = {
    let mut buf = vec![0xf0, 0x00, 0x20, 0x29, 0x02, 0x18, 0x14, color, 0x00];
    buf.extend(msg);
    buf.push(0xf7);
    buf
  };

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
        match device_model {
          DeviceModel::Roland => {
            for rule in &rules {
              let raw_msg = RawMidi {
                time: 0,
                bytes: rule,
              };
              put_p.write(&raw_msg).unwrap();

              let msg = MidiMessage::from_bytes(raw_msg.bytes.to_vec());
              println!("{}: {:x}\tchannel: {:?}", msg.status(), msg.data.as_hex(), msg.channel());
            }
          },
          DeviceModel::Launchpad => {
            put_p.write(&RawMidi {
              time: 0,
              bytes: &scrolling_text,
            }).unwrap();
          },
        };
        current_state = ProgramState::LoadedRules;
      },
      _ => (),
    }

    for e in show_p.iter() {
      let mut bytes = e.bytes.to_vec();

      let msg = MidiMessage::from_bytes(e.bytes.to_vec());
      println!("{}: {:x}\tchannel: {:?}", msg.status(), msg.data.as_hex(), msg.channel());

      let status = msg.status();
      if status == Status::NoteOn {
        bytes[0] = status as u8 | 2u8;
        bytes[2] = 16;
      }

      put_p.write(&RawMidi {
        time: e.time,
        bytes: &bytes,
      }).unwrap();
    }

    JackControl::Continue
  };

  // activate
  let process = ClosureProcessHandler::new(cback);
  let active_client = AsyncClient::new(client, (), process).unwrap();

  active_client.connect_ports_by_name(maker_name, output_system_port).unwrap();
  println!("Connected to {}", output_system_port);

  active_client.connect_ports_by_name("alsa_midi:Launchpad MK2 MIDI 1 (out)", shower_name).unwrap();

  // wait
  println!("Press any key to quit");
  let mut user_input = String::new();
  io::stdin().read_line(&mut user_input).ok();

  // optional deactivation
  active_client.deactivate().unwrap();
}
