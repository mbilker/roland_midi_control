#![allow(unused_imports)]

extern crate jack;
extern crate rimd;

use jack::prelude::{AsyncClient, Client, ClosureProcessHandler, JackControl, MidiInPort,
                    MidiInSpec, MidiOutPort, MidiOutSpec, PortFlags, ProcessScope, RawMidi, client_options};
use rimd::MidiMessage;
use std::io;

fn main() {
  // open client
  let (client, _status) = Client::new("rust_jack_show_midi", client_options::NO_START_SERVER).unwrap();

  let mut init_done = false;

  // process logic
  let mut maker = client.register_port("rust_midi_maker", MidiOutSpec::default()).unwrap();
  let shower = client.register_port("rust_midi_shower", MidiInSpec::default()).unwrap();

  let ports = client.ports(None, None, PortFlags::empty());
  println!("{:#?}", ports);

  //client.connect_ports_by_name(maker.name(), "alsa_midi:Scarlett 2i4 USB MIDI 1 (in)").unwrap();

  let cback = move |_: &Client, ps: &ProcessScope| -> JackControl {
    let connected_num = maker.connected_count() > 0;

    let show_p = MidiInPort::new(&shower, ps);
    let mut put_p = MidiOutPort::new(&mut maker, ps);

    if !init_done && connected_num {
      macro_rules! sysex {
        ($($x:expr),*) => {
          let msg = RawMidi {
            time: 0,
            bytes: &[ $($x,)* ],
          };
          put_p.write(&msg).unwrap();

          println!("{}", MidiMessage::from_bytes(msg.bytes.to_vec()));
        }
      }

      let rules = vec![
        vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x41, 0x22, 0x01, 0x5c, 0xf7],
        vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x11, 0x2b, 0xf7],
      ];

      //sysex!(0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x41, 0x22, 0x01, 0x5c, 0xf7);		// Enable M-FX for A01
      //sysex!(0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x11, 0x2b, 0xf7);	// Set M-FX to P-06: Distortion

      for rule in rules {
        let msg = RawMidi {
          time: 0,
          bytes: &rule,
        };
        put_p.write(&msg).unwrap();
        println!("{}", MidiMessage::from_bytes(msg.bytes.to_vec()));
      }

      init_done = true;
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

  // wait
  println!("Press any key to quit");
  let mut user_input = String::new();
  io::stdin().read_line(&mut user_input).ok();

  // optional deactivation
  active_client.deactivate().unwrap();
}
