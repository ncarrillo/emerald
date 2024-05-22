use super::bus::PhysicalAddress;

#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub struct DmacRegisters {
    pub sar0: u32,
    pub dar0: u32,
    pub dmatcr0: u32,
    pub chcr0: u32,
    pub sar1: u32,
    pub dar1: u32,
    pub dmatcr1: u32,
    pub chcr1: u32,
    pub sar2: u32,
    pub dar2: u32,
    pub dmatcr2: u32,
    pub chcr2: u32,
    pub sar3: u32,
    pub dar3: u32,
    pub dmatcr3: u32,
    pub chcr3: u32,
    pub dmaor: u32,
}

#[derive(Copy, Clone, Debug)]
pub struct Dmac {
    pub registers: DmacRegisters,
}

impl Dmac {
    pub fn new() -> Self {
        Self {
            registers: DmacRegisters {
                ..Default::default()
            },
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
        match addr.0 {
            0x1fa00000 => self.registers.sar0 = value,
            0x1fa00004 => self.registers.dar0 = value,
            0x1fa00008 => self.registers.dmatcr0 = value,
            0x1fa0000c => self.registers.chcr0 = value,
            0x1fa00010 => self.registers.sar1 = value,
            0x1fa00014 => self.registers.dar1 = value,
            0x1fa00018 => self.registers.dmatcr1 = value,
            0x1fa0001c => self.registers.chcr1 = value,
            0x1fa00020 => self.registers.sar2 = value,
            0x1fa00024 => self.registers.dar2 = value,
            0x1fa00028 => self.registers.dmatcr2 = value,
            0x1fa0002c => self.registers.chcr2 = value,
            0x1fa00030 => self.registers.sar3 = value,
            0x1fa00034 => self.registers.dar3 = value,
            0x1fa00038 => self.registers.dmatcr3 = value,
            0x1fa0003c => self.registers.chcr3 = value,
            0x1fa00040 => self.registers.dmaor = value,
            _ => println!(
                "dmac: unknown mmio write (32-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            0x1FA0002C => self.registers.chcr2,
            0x1fa0001c => self.registers.chcr1,
            0x1fa0003c => self.registers.chcr3,
            _ => panic!("dmac: unknown mmio read (8-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            _ => panic!("dmac: unknown mmio read (8-bit) @ 0x{:08x}", addr.0),
        }
    }
}
