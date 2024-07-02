use std::{collections::HashMap, fs};

use goblin::elf::Elf;

use crate::{
    context::Context,
    hw::sh4::{bus::CpuBus, cpu::Cpu},
};

pub struct Emulator {
    pub cpu: Cpu,
    pub state: EmulatorState,
}

pub const IP_BIN: &[u8] = include_bytes!("../roms/IP/IP.BIN");

// reicast dump of ram when pc = png.cdi entry point. helps smooth over some differences until we can boot the full bios
pub const REF_RAM: &[u8] = include_bytes!("../ref-ram.bin");

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EmulatorState {
    Paused,
    Running
}

impl Emulator {
    pub fn new() -> Self {
        Emulator {
            cpu: Cpu::new(),
            state: EmulatorState::Running,
        }
    }

    pub fn load_elf(
        elf_path: &str,
        cpu: &mut Cpu,
        context: &mut Context,
        bus: &mut CpuBus,
    ) -> Result<HashMap<u32, String>, ()> {
        let buffer = fs::read(elf_path).unwrap();
        let elf = Elf::parse(&buffer).unwrap();

        let offset: u32 = 0xAC008000;
        let size = IP_BIN.len();

        for i in 0..size {
            bus.write_8((offset as u32).wrapping_add(i as u32), IP_BIN[i], context);
        }

        let mut i = 0;
        for ref_byte in REF_RAM.iter() {
            bus.write_8(0x0c000000 + i as u32, *ref_byte, context);
            i += 1;
        }

        bus.write_32(0x8c03a044, 0x01, context);

        for i in 0..16 {
            bus.write_16(
                0x8C0000E0 + 2 * i,
                bus.read_16(0x800000FE - 2 * i, false, context),
                context,
            );
        }

        bus.write_32(0xA05F74E4, 0x001FFFFF, context);

        unsafe {
            std::ptr::copy_nonoverlapping(
                crate::hw::holly::g1::boot_rom::BIOS_DATA
                    .as_ptr()
                    .add(0x00000100),
                bus.system_ram.as_mut_ptr().add(0x00000100),
                0x00004000 - 0x00000100,
            );

            std::ptr::copy_nonoverlapping(
                crate::hw::holly::g1::boot_rom::BIOS_DATA
                    .as_ptr()
                    .add(0x00008000),
                bus.system_ram.as_mut_ptr().add(0x00008000),
                0x00200000 - 0x00008000,
            );
        }

        // Copy a portion of the flash ROM to RAM.
        for i in 0..8 {
            bus.write_8(
                0x8C000068 + i,
                bus.read_8(0x0021A056 + i, true, context),
                context,
            );
        }

        for i in 0..5 {
            bus.write_8(
                0x8C000068 + 8 + i,
                bus.read_8(0x0021A000 + i, true, context),
                context,
            );
        }

        let mut idx = 0_u32;
        for val in [
            0x00, 0x00, 0x89, 0xFC, 0x5B, 0xFF, 0x01, 0x00, 0x00, 0x7D, 0x0A, 0x62, 0x61,
        ] {
            bus.write_8(0x8C000068 + 13 + idx, val, context);
            idx += 1;
        }

        bus.write_32(0x8C000000, 0x00090009, context);
        bus.write_32(0x8C000004, 0x001B0009, context);
        bus.write_32(0x8C000008, 0x0009AFFD, context);
        // ??
        bus.write_16(0x8C00000C, 0, context);
        bus.write_16(0x8C00000E, 0, context);
        // RTE - Some interrupts jump there instead of having their own RTE, I have NO idea why.
        bus.write_32(0x8C000010, 0x00090009, context); // nop nop
        bus.write_32(0x8C000014, 0x0009002B, context); // rte nop
                                                       // RTS
        bus.write_32(0x8C000018, 0x00090009, context);
        bus.write_32(0x8C00001C, 0x0009000B, context);

        // ??
        bus.write_8(0x8C00002C, 0x16, context);
        bus.write_32(0x8C000064, 0x8c008100, context);
        bus.write_16(0x8C000090, 0, context);
        bus.write_16(0x8C000092, -128 as i16 as u16, context);

        for (addr, val) in [
            (0x8C0000AC, 0xA05F7000),
            (0x8C0000A8, 0xA0200000),
            (0x8C0000A4, 0xA0100000),
            (0x8C0000A0, 0x00000000),
            (0x8C00002C, 0x00000000),
            (0x8CFFFFF8, 0x8C000128),
        ] {
            bus.write_32(addr, val, context);
        }

        for (addr, val) in [
            (0xAC0090D8, 0x5113),
            (0xAC00940A, 0x000B),
            (0xAC00940C, 0x0009),
        ] {
            bus.write_16(addr, val, context);
        }

        // place each loadable segment into RAM
        for ph in elf.program_headers.iter() {
            if ph.p_type == goblin::elf::program_header::PT_LOAD {
                let segment_data =
                    &buffer[ph.p_offset as usize..(ph.p_offset + ph.p_filesz) as usize];
                let mut offset = 0_u32;

                for b in 0..ph.p_memsz {
                    bus.write_8((ph.p_vaddr + b) as u32, 0, context);
                }

                for b in segment_data {
                    bus.write_8((ph.p_vaddr as u32) + offset, *b, context);
                    offset += 1;
                }
            }
        }

        // create a symbol table map
        let mut symbol_map = HashMap::new();
        for sym in &elf.syms {
            if let Some(name) = elf.strtab.get_at(sym.st_name) {
                let addr = sym.st_value as u32;
                symbol_map.insert(addr & 0x1FFFFFFF, name.to_string());
            }
        }

        // panic!("");

        // set some initial conditions (taken from Deecy)

        cpu.registers.current_pc = 0x8c010000;
        cpu.set_register_by_index(15, 0x8c00f400);
        cpu.set_register_by_index(0, 0x8c010000);
        cpu.set_banked_register_by_index(0, 0x600000f0);
        cpu.set_banked_register_by_index(1, 0x00000808);
        cpu.set_banked_register_by_index(2, 0x8c00e070);
        cpu.set_fr_register_by_index(11, f32::from_bits(0x3f800000));
        cpu.set_fr_register_by_index(9, f32::from_bits(0x80000000));
        cpu.set_fr_register_by_index(8, f32::from_bits(0x80000000));
        cpu.set_fr_register_by_index(7, f32::from_bits(0x3f800000));
        cpu.set_fr_register_by_index(6, f32::from_bits(0x41840000));
        cpu.set_fr_register_by_index(5, f32::from_bits(0x3fe66666));
        cpu.set_fr_register_by_index(4, f32::from_bits(0x3f266666));
        cpu.set_register_by_index(4, 0x8c010000);
        cpu.registers.sr = 0x600000f0;
        cpu.registers.pr = 0x8c00e09c;
        cpu.registers.vbr = 0x8c00f400;
        cpu.registers.fpscr = 0x00040001;
        bus.holly.sb.registers.ffst_cnt.set(245277);

        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL
        bus.write_32(0x005F8060, 0x00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 0x00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8050, 0x00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 0x00200000, context); // FB_R_SOF2

        println!("loading elf..");
        Ok(symbol_map)
    }
}
