// real time clock
use super::bus::PhysicalAddress;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct RtcRegisters {
  rmonar: u8,
  rcr1: u8
}

pub struct Rtc {
    pub registers: RtcRegisters,
}

impl Rtc {
  pub fn new() -> Self {
    Self {
      registers: RtcRegisters {
        ..Default::default()
      }
    }
  }

  pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
    match addr.0 {
      _ => println!("rtc: unknown mmio write (32-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }

  pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
    match addr.0 {
      0x1fc80034 => self.registers.rmonar = value,
      0x1fc80038 => self.registers.rcr1 = value,
      _ => println!("rtc: unknown mmio write (8-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }

  pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
    match addr.0 {
      _ => println!("rtc: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:08x}", addr.0, value)
    }
  }
}
