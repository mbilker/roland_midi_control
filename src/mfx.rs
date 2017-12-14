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
