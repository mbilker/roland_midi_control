#![allow(dead_code)]

extern crate enigo;
extern crate hex_slice;
extern crate jack;
#[macro_use] extern crate lazy_static;
extern crate rimd;

use enigo::{Enigo, Key, KeyboardControllable};
use hex_slice::AsHex;
use jack::prelude::{AsyncClient, Client, ClosureProcessHandler, JackControl, MidiInPort,
                    MidiInSpec, MidiOutPort, MidiOutSpec, PortFlags, ProcessScope, RawMidi, client_options};
use rimd::{MidiMessage, Status};

use std::collections::hash_map::HashMap;
use std::io;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

mod sysex;
mod mfx;

use sysex::*;
use mfx::*;

static DEFAULT_OUTPUT_PORT: &'static str = "alsa_midi:Scarlett 2i4 USB MIDI 1 (in)";

lazy_static! {
  static ref MIDI_KEYBOARD_MAPPING: Mutex<HashMap<u8, Key>> = {
    let mut m = HashMap::new();

    // Keypad numbers
    m.insert(0x56, Key::Layout('7'));
    m.insert(0x57, Key::Layout('8'));
    m.insert(0x58, Key::Layout('9'));
    m.insert(0x4c, Key::Layout('4'));
    m.insert(0x4d, Key::Layout('5'));
    m.insert(0x4e, Key::Layout('6'));
    m.insert(0x42, Key::Layout('1'));
    m.insert(0x43, Key::Layout('2'));
    m.insert(0x44, Key::Layout('3'));

    // P1
    m.insert(11, Key::Layout('z')); // P1 1
    m.insert(22, Key::Layout('s')); // P1 2
    m.insert(12, Key::Layout('x')); // P1 3
    m.insert(23, Key::Layout('d')); // P1 4
    m.insert(13, Key::Layout('c')); // P1 5
    m.insert(24, Key::Layout('f')); // P1 6
    m.insert(14, Key::Layout('v')); // P1 7

    // SDVX specific
    m.insert(25, Key::Layout('g'));

    // Game controls
    m.insert(0x59, Key::Backspace); // Card Insert (handled separately)
    m.insert(31, Key::Layout('q')); // P1 START
    m.insert(32, Key::Layout('t')); // P1 TT-
    m.insert(33, Key::Layout('r')); // P1 TT+
    m.insert(34, Key::Layout('w')); // EFFECT
    m.insert(35, Key::Layout('e')); // VEFX
    m.insert(21, Key::Shift); // P1 TT+/-

    Mutex::new(m)
  };
}

#[derive(Clone, Copy)]
enum ProgramState {
  Initial,
  ConnectedPorts,
  LoadedRules,
  WriteMessage,
  WaitForScollingFinish,
  SetBaseColors,
  Disabled,
}

#[derive(Clone, Copy)]
enum DeviceModel {
  Roland,
  Launchpad,
}

enum KeyboardControl {
  Down(u8),
  Up(u8),
  RawDown(Key),
  RawUp(Key),
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
  let msg = b"fizz";
  let scrolling_text = {
    let mut buf = vec![0xf0, 0x00, 0x20, 0x29, 0x02, 0x18, 0x14, color, 0x00];
    buf.extend(msg);
    buf.push(0xf7);
    buf
  };

  let base_led_states: HashMap<u8, MidiMessage> = {
    let m = MIDI_KEYBOARD_MAPPING.lock().unwrap();
    m.iter()
      .map(|(key, _value)| (*key, MidiMessage::note_on(*key, 71, 0)))
      .collect()
  };

  // Keyboard and mouse control handler
  let (sender, receiver) = mpsc::channel();
  let thread_handle = thread::spawn(move || {
    let mut enigo = Enigo::new();

    while let Ok(key) = receiver.recv() {
      let m = MIDI_KEYBOARD_MAPPING.lock().unwrap();
      match key {
        KeyboardControl::Down(key) => {
          if let Some(key) = m.get(&key) {
            enigo.key_down(*key);
          }
        },
        KeyboardControl::Up(key) => {
          if let Some(key) = m.get(&key) {
            enigo.key_up(*key);
          }
        },

        KeyboardControl::RawDown(key) => enigo.key_down(key),
        KeyboardControl::RawUp(key) => enigo.key_up(key),
      };
    }
  });

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
        current_state = ProgramState::WaitForScollingFinish;
      },
      ProgramState::WaitForScollingFinish => {
        for e in show_p.iter() {
          if e.bytes == &[0xf0, 0x00, 0x20, 0x29, 0x02, 0x18, 0x15, 0xf7] {
            current_state = ProgramState::SetBaseColors;
            break;
          }
        }
      },
      ProgramState::SetBaseColors => {
        for e in base_led_states.values() {
          put_p.write(&RawMidi {
            time: 0,
            bytes: &e.data,
          }).unwrap();
        }
        current_state = ProgramState::Disabled;
      },
      _ => (),
    }

    for e in show_p.iter() {
      let mut overwritten = false;
      let mut bytes = e.bytes.to_vec();

      let msg = MidiMessage::from_bytes(e.bytes.to_vec());
      println!("{}: {:x}\tchannel: {:?}", msg.status(), msg.data.as_hex(), msg.channel());

      let status = msg.status();
      match status {
        Status::NoteOn => {
          bytes[0] = status as u8 | 2u8;
          bytes[2] = 8;

          let event = KeyboardControl::Down(bytes[1]);
          sender.send(event).unwrap();
        },
        Status::NoteOff => {
          let event = KeyboardControl::Up(bytes[1]);
          sender.send(event).unwrap();

          if let Some(msg) = base_led_states.get(&bytes[1]) {
            overwritten = true;

            put_p.write(&RawMidi {
              time: e.time,
              bytes: msg.data.as_slice(),
            }).unwrap();
          }
        },
        _ => {},
      };

      if !overwritten {
        put_p.write(&RawMidi {
          time: e.time,
          bytes: &bytes,
        }).unwrap();
      }
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

  thread_handle.join().unwrap();
}
