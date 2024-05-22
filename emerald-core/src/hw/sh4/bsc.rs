// bus state controller 
use super::bus::PhysicalAddress;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct BscRegisters {
  bcr1: u32,
  bcr2: u16,
  rfcr: u16,
  rtcor: u16,
  rtcsr: u16,
  pdtra: u16,
  pdtrb: u16,
  pctra: u32,
  pctrb: u32,

  mcr: u32,
  pcr: u16,
  gpioic: u16,
  wcr1: u32,
  wcr2: u32,
  wcr3: u32,
  sdmr2: Vec<u8>,
  sdmr3: Vec<u8>
}

pub struct Bsc {
    registers: BscRegisters,
}

impl Bsc {
  pub fn new() -> Self {
    Self {
      registers: BscRegisters {
        sdmr2: vec![0; 65535],
        sdmr3: vec![0; 65535],
        ..Default::default()
      }
    }
  }

  pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
    match addr.0 {
      0x1f800000 => self.registers.bcr1 = value,
      0x1f800008 => self.registers.wcr1 = value,
      0x1f800010 => self.registers.wcr3 = value,
      0x1f800014 => self.registers.mcr = value,
      0x1f80002c => {
        // fixme: Reicast doesnt store the full 32-bits but I cant tell why
        self.registers.pctra = value & 0xffff;
      },
      0x1f800040 => self.registers.pctrb = value,
      0x1f80000c => self.registers.wcr2 = value,
      _ => panic!("bsc: unknown mmio write (32-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }

  pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
    match addr.0 {
      0x1f940000..=0x1f94ffff => self.registers.sdmr3[(addr.0 - 0x1f940000) as usize] = value,
      _ => panic!("bsc: unknown mmio write (8-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }

  pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
    match addr.0 {
      0x1f800004 => self.registers.bcr2 = value,
      0x1f800018 => self.registers.pcr = value,
      0x1f80001c => self.registers.rtcsr = value,
      0x1f800024 => self.registers.rtcor = value,
      0x1f800028 => self.registers.rfcr = value,
      0x1f800030 => self.registers.pdtra = value,
      0x1f800044 => self.registers.pdtrb = value,
      0x1f800048 => self.registers.gpioic = value,
      _ => panic!("bsc: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }
  
  pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
    match addr.0 {
      0x1f800028 => self.registers.rfcr,
      0x1f800030 => { // pdtra
        // got this from jsmoo-emu which got it from deecy which got it from flycast which got it from chankast
        let tpctra = self.registers.pctra;
        let tpdtra = self.registers.pdtra;

        let mut tfinal = 0;

        // magic values
        if (tpctra & 0xf) == 0x8 {
            tfinal = 3;
        }
        else if (tpctra & 0xf) == 0xB {
            tfinal = 3;
        }
        else {
            tfinal = 0;
        }

        if (tpctra & 0xf) == 0xB && (tpdtra & 0xf) == 2 {
            tfinal = 0;
        }
        else if (tpctra & 0xf) == 0xC && (tpdtra & 0xf) == 2 {
            tfinal = 3;
        }

        let cable_type = 0; //vga
        tfinal |= cable_type << 8;
        
        return tfinal;
      },
      _ => panic!("bsc: unknown mmio read (16-bit) @ 0x{:08x}", addr.0)
    }
  }
  pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
    match addr.0 {
      0x1f80002c => self.registers.pctra,
      _ => panic!("bsc: unknown mmio read (32-bit) @ 0x{:08x}", addr.0)
    }
  }

  pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
    match addr.0 {
      _ => panic!("bsc: unknown mmio read (8-bit) @ 0x{:08x}", addr.0)
    }
  }
}
