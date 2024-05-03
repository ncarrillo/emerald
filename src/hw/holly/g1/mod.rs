use self::{boot_rom::BootROM, gdrom::Gdrom};
use crate::hw::sh4::bus::PhysicalAddress;

pub mod boot_rom;
pub mod gdrom;
pub mod gdi;

pub struct G1Bus {
    boot_rom: BootROM,
    pub gd_rom: Gdrom,
}

impl G1Bus {
    pub fn new() -> Self {
        Self {
            boot_rom: BootROM::new(),
            gd_rom: Gdrom::new()
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
        match addr.0 {
            0x005f7018..=0x005f709c => self.gd_rom.write_32(addr, value),
            _ => println!(
                "g1: unknown mmio write (32-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x005f7018..=0x005f709c => self.gd_rom.write_16(addr, value),
            _ => println!(
                "g1: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
        match addr.0 {
            // gd-rom
            0x005f7018..=0x005f709c => self.gd_rom.write_8(addr, value),
            _ => println!(
                "g1: unknown mmio write (8-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            _ => panic!("g1: unknown mmio read (32-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
        match addr.0 {
            0x005f7018..=0x005f709c => self.gd_rom.read_16(addr),
            _ => panic!("g1: unknown mmio read (16-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            0..=0x0023ffff => self.boot_rom.read_8(addr),
            0x005f7018..=0x005f709c => self.gd_rom.read_8(addr),
            _ => panic!("g1: unknown mmio read (8-bit) @ 0x{:08x}", addr.0),
        }
    }
}
