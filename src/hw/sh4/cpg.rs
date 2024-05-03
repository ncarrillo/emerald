// clock pulse generator
use super::bus::PhysicalAddress;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct CpgRegisters {
    stbcr: u8,
    stbcr2: u8,
    wtcnt: u16,
    wtcsr: u16,
}

pub struct Cpg {
    pub registers: CpgRegisters,
}

impl Cpg {
    pub fn new() -> Self {
        Self {
            registers: CpgRegisters {
                ..Default::default()
            },
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
        match addr.0 {
            0x1fc00004 => self.registers.stbcr = value,
            0x1fc00009 => {
                let upper = ((value & 0xFF) as u16) << 8;
                self.registers.wtcnt = (self.registers.wtcnt & 0x00FF) | upper;
            }
            0x1fc0000c => {
                let lower = (value & 0xFF) as u16;
                self.registers.wtcsr = (self.registers.wtcsr & 0xFF00) | lower;
            }
            0x1fc0000d => {
                let upper = ((value & 0xFF) as u16) << 8;
                self.registers.wtcsr = (self.registers.wtcsr & 0x00FF) | upper;
            }
            0x1fc00010 => self.registers.stbcr2 = value,
            _ => println!(
                "cpg: unknown mmio write (8-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            )
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x1fc00008 => {
                let lower = (value & 0xFF) as u16;
                self.registers.wtcnt = (self.registers.wtcnt & 0xFF00) | lower;
            },
            _ => {
                #[cfg(feature = "log_io")]
                println!(
                    "cpg: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:04x}",
                    addr.0, value
                );
            }
        }
    }
}
