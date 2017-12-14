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

#[derive(Copy, Clone, Debug)]
pub enum MFXType {
  Thru,           // P-00: Thru
  StereoEQ,       // P-01: Stereo-EQ
  Spectrum,       // P-02: Spectrum
  Enhancer,       // P-03: Enhancer
  Humanizer,      // P-04: Humanizer
  Overdrive,      // P-05: Overdrive
  Distortion,     // P-06: Distortion
  LoFi1,          // P-33: Lo-Fi 1
  LoFi2,          // P-34: Lo-Fi 2
}

impl MFXType {
  pub fn value(&self) -> &[u8; 2] {
    match *self {
      MFXType::Thru       => &[0x00, 0x00],
      MFXType::StereoEQ   => &[0x01, 0x00],
      MFXType::Spectrum   => &[0x01, 0x01],
      MFXType::Enhancer   => &[0x01, 0x02],
      MFXType::Humanizer  => &[0x01, 0x03],
      MFXType::Overdrive  => &[0x01, 0x10],
      MFXType::Distortion => &[0x01, 0x11],
      MFXType::LoFi1      => &[0x01, 0x72],
      MFXType::LoFi2      => &[0x01, 0x73],
    }
  }
}

#[derive(Copy, Clone, Debug)]
pub struct RolandSysEx {
  pub device_id: u8,
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

  // Enable/Disable M-FX for part, 0x40 0x4X 0x22 0x01, X = "Part Number"
  pub fn enable_mfx(&self, part: u8, enable: bool) -> Vec<u8> {
    let value = match enable {
      true  => 0x01,
      false => 0x00,
    };
    self.data(&[0x40, 0x40 + part, 0x22], &[value])
  }

  // Set M-FX to type, 0x40 0x03 0x00 + mode value
  pub fn set_mfx_type(&self, mode: MFXType) -> Vec<u8> {
    self.data(&[0x40, 0x03, 0x00], mode.value())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_set_mfx() {
    assert_eq!(RolandSysEx::new(0x10).enable_mfx(0x01, true), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x41, 0x22, 0x01, 0x5c, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).enable_mfx(0x02, true), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x42, 0x22, 0x01, 0x5b, 0xf7]);
  }

  #[test]
  fn test_set_mfx_type() {
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Thru), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x00, 0x00, 0x3d, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::StereoEQ), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x00, 0x3c, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Spectrum), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x01, 0x3b, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Enhancer), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x02, 0x3a, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Humanizer), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x03, 0x39, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Overdrive), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x10, 0x2c, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::Distortion), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x11, 0x2b, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::LoFi1), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x72, 0x4a, 0xf7]);
    assert_eq!(RolandSysEx::new(0x10).set_mfx_type(MFXType::LoFi2), vec![0xf0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x03, 0x00, 0x01, 0x73, 0x49, 0xf7]);
  }
}
