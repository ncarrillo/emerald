use crate::scheduler::Scheduler;

use super::Aica;

pub struct ArmBus<'a> {
    pub aica: &'a mut Aica,
}

impl<'a> ArmBus<'a> {
    pub const MASK: usize = 0x1FFFFF;
    const EXTERNAL_THRESHOLD: usize = 0x800000;

    pub fn read_32(&self, physical_addr: u32) -> u32 {
        let physical_addr = physical_addr as usize;

        match physical_addr {
            0..Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                let mut value = 0;
                for i in 0..4 {
                    value |= (self.aica.wave_ram[addr_base + i] as u32) << (i * 8);
                }

                value
            }
            _ => self
                .aica
                .read_aica_register_32(crate::hw::sh4::bus::PhysicalAddress(physical_addr as u32)),
        }
    }

    pub fn fetch_32(&self, physical_addr: u32) -> u32 {
        let physical_addr = physical_addr as usize;

        let addr_base = physical_addr & Self::MASK as usize;
        let mut value = 0;
        for i in 0..4 {
            value |= (self.aica.wave_ram[addr_base + i] as u32) << (i * 8);
        }

        value
    }

    pub fn read_16(&self, physical_addr: u32) -> u16 {
        let physical_addr = physical_addr as usize;
        match physical_addr {
            0..Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                let mut value = 0;
                for i in 0..2 {
                    value |= (self.aica.wave_ram[addr_base + i] as u16) << (i * 8);
                }

                value
            }
            _ => panic!(
                "fixme: external memory read from register space at addr {:08x}",
                physical_addr
            ),
        }
    }

    pub fn read_8(&self, physical_addr: u32) -> u8 {
        let physical_addr = physical_addr as usize;
        match physical_addr {
            0..=Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                let value = self.aica.wave_ram[addr_base];
                value
            }
            _ => self
                .aica
                .read_aica_register_32(crate::hw::sh4::bus::PhysicalAddress(physical_addr as u32))
                as u8,
        }
    }

    pub fn write_32(&mut self, physical_addr: u32, value: u32) {
        let physical_addr = physical_addr as usize;
        match physical_addr {
            0..=Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                for i in 0..4 {
                    self.aica.wave_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                }

                if physical_addr == 0x00000020 {
                    panic!("write32 to 20 with {:08x}??", value);
                }
            }
            _ => self.aica.write_aica_register_32(
                crate::hw::sh4::bus::PhysicalAddress(physical_addr as u32),
                value,
            ),
        }
    }

    pub fn write_16(&mut self, physical_addr: u32, value: u16) {
        let physical_addr = physical_addr as usize;
        match physical_addr {
            0..=Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                for i in 0..2 {
                    self.aica.wave_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                }
            }
            _ => println!(
                "fixme: external memory write to register space at addr {:08x}",
                physical_addr
            ),
        }
    }

    pub fn write_8(&mut self, physical_addr: u32, value: u8) {
        let physical_addr = physical_addr as usize;
        match physical_addr {
            0..=Self::EXTERNAL_THRESHOLD => {
                let addr_base = physical_addr & Self::MASK as usize;
                self.aica.wave_ram[addr_base] = value;
            }
            _ => println!(
                "fixme: external memory write to register space at addr {:08x}",
                physical_addr
            ),
        }
    }
}
