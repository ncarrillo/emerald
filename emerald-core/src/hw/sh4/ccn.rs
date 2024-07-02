// cache and TLB controller

use super::bus::PhysicalAddress;
use crate::hw::extensions::BitManipulation;

#[derive(Copy, Default, Clone, Debug, Eq, PartialEq)]
pub struct CcnRegisters {
    pub pteh: u32,
    pub ptel: u32,
    pub ttb: u32,
    pub tea: u32,
    pub mmucr: u32,
    pub basra: u8,
    pub basrb: u8,
    pub ccr: u32,
    pub tra: u32,
    pub expevt: u32,
    pub intevt: u32,
    pub ptea: u32,
    pub qacr0: u32,
    pub qacr1: u32,
}

pub struct Ccn {
    pub registers: CcnRegisters,
    operand_cache_ram: Vec<u8>,
}

impl Ccn {
    pub fn new() -> Self {
        Self {
            operand_cache_ram: vec![0; 8 * 1024],
            registers: Default::default(),
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
        match addr.0 {
            0x1f000000 => self.registers.pteh = value,
            0x1f000004 => self.registers.ptel = value,
            0x1f000008 => self.registers.ttb = value,
            0x1f00000c => self.registers.tea = value,
            0x1f000010 => self.registers.mmucr = value,
            0x1f00001c => self.registers.ccr = value,
            0x1f000020 => self.registers.tra = value,
            0x1f000024 => self.registers.expevt = value & 0xFFFF,
            0x1f000028 => self.registers.intevt = value & 0xFFFF,
            0x1f000034 => self.registers.ptea = value & 0x0000000F,
            0x1f000038 => self.registers.qacr0 = value,
            0x1f00003c => self.registers.qacr1 = value,
            _ => panic!(
                "ccn: unknown mmio write @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn write_oc_32(&mut self, addr: PhysicalAddress, value: u32) {
        let index = if !self.registers.ccr.check_bit(7) {
            (((addr.0 & 0x2000) >> 1) | (addr.0 & 0xfff)) as usize
        } else {
            (((addr.0 & 0x02000000) >> 13) | (addr.0 & 0xfff)) as usize
        };

        let bytes = value.to_le_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            self.operand_cache_ram[index + offset] = byte;
        }
    }

    pub fn write_oc_16(&mut self, addr: PhysicalAddress, value: u16) {
        let index = if !self.registers.ccr.check_bit(7) {
            (((addr.0 & 0x2000) >> 1) | (addr.0 & 0xfff)) as usize
        } else {
            (((addr.0 & 0x02000000) >> 13) | (addr.0 & 0xfff)) as usize
        };

        let bytes = value.to_le_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            self.operand_cache_ram[index + offset] = byte;
        }
    }

    pub fn write_oc_8(&mut self, addr: PhysicalAddress, value: u8) {
        let index = if !self.registers.ccr.check_bit(7) {
            (((addr.0 & 0x2000) >> 1) | (addr.0 & 0xfff)) as usize
        } else {
            (((addr.0 & 0x02000000) >> 13) | (addr.0 & 0xfff)) as usize
        };

        let bytes = value.to_le_bytes();
        for (offset, &byte) in bytes.iter().enumerate() {
            self.operand_cache_ram[index + offset] = byte;
        }
    }

    pub fn read_oc_32(&self, addr: PhysicalAddress) -> u32 {
        let index = if !self.registers.ccr.check_bit(7) {
            (((addr.0 & 0x2000) >> 1) | (addr.0 & 0xfff)) as usize
        } else {
            (((addr.0 & 0x02000000) >> 13) | (addr.0 & 0xfff)) as usize
        };

        let bytes = [
            self.operand_cache_ram[index],
            self.operand_cache_ram[index + 1],
            self.operand_cache_ram[index + 2],
            self.operand_cache_ram[index + 3],
        ];

        u32::from_le_bytes(bytes)
    }

    pub fn read_oc_8(&self, addr: PhysicalAddress) -> u8 {
        let index = if !self.registers.ccr.check_bit(7) {
            (((addr.0 & 0x2000) >> 1) | (addr.0 & 0xfff)) as usize
        } else {
            (((addr.0 & 0x02000000) >> 13) | (addr.0 & 0xfff)) as usize
        };

        self.operand_cache_ram[index]
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            0x1f000024 => self.registers.expevt,
            // bits 3 and 11 always return 0 when read for ccr
            0x1f00001c => self.registers.ccr.clear_bit(11).clear_bit(3),
            0x1f000028 => self.registers.intevt,
            0x1f000030 => 0x040205c1,
            0x1f000010 => self.registers.mmucr,
            _ => panic!("ccn: unknown mmio read (32-bit) @ 0x{:08x}", addr.0),
        }
    }
}
