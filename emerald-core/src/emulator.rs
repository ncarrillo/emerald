use std::{collections::HashMap, fs};

use goblin::elf::Elf;

use crate::{
    context::Context,
    hw::sh4::{
        bus::CpuBus,
        cpu::{Cpu, Float32},
    },
    scheduler::Scheduler,
};

pub struct Emulator {
    pub cpu: Cpu,
    pub state: EmulatorState,
}

pub const IP_BIN: &[u8] = include_bytes!("../roms/IP/IP.BIN");
pub const _256_BIN: &[u8] = include_bytes!("../roms/rotozoomer/roto.BIN");

// reicast dump of ram when pc = png.cdi entry point. helps smooth over some differences until we can boot the full bios
pub const REF_RAM: &[u8] = include_bytes!("../ref-ram.bin");

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EmulatorState {
    Paused,
    Running,
    BreakpointTripped,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Breakpoint {
    MemoryBreakpoint {
        addr: u32,
        read: bool,
        write: bool,
        fetch: bool,
    },
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
                (0x8C0000E0 + 2 * i),
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
        for i in (0..8) {
            bus.write_8(
                0x8C000068 + i,
                bus.read_8(0x0021A056 + i, true, context),
                context,
            );
        }

        for i in (0..5) {
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
        cpu.set_fr_register_by_index(11, Float32 { u: 0x3f800000 });
        cpu.set_fr_register_by_index(9, Float32 { u: 0x80000000 });
        cpu.set_fr_register_by_index(8, Float32 { u: 0x80000000 });
        cpu.set_fr_register_by_index(7, Float32 { u: 0x3f800000 });
        cpu.set_fr_register_by_index(6, Float32 { u: 0x41840000 });
        cpu.set_fr_register_by_index(5, Float32 { u: 0x3fe66666 });
        cpu.set_fr_register_by_index(4, Float32 { u: 0x3f266666 });
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

        Ok(symbol_map)
    }

    // stub to load IP.bin
    pub fn load_ip(cpu: &mut Cpu, context: &mut Context, bus: &mut CpuBus) {
        let offset: u32 = 0xAC008000;
        let size = IP_BIN.len();

        for i in 0..size {
            bus.write_8((offset as u32).wrapping_add(i as u32), IP_BIN[i], context);
        }

        for i in 0..16 {
            bus.write_16(
                (0x8C0000E0 + 2 * i),
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
        for i in (0..8) {
            bus.write_8(
                0x8C000068 + i,
                bus.read_8(0x0021A056 + i, true, context),
                context,
            );
        }

        for i in (0..5) {
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

        // set pc
        cpu.registers.current_pc = 0xAC008300;
        cpu.registers.sr = 0x400000F1;
        cpu.registers.fpscr = 0x00040001;
        cpu.registers.r[0x0] = 0xAC0005D8;
        cpu.registers.r[0x1] = 0x00000009;
        cpu.registers.r[0x2] = 0xAC00940C;
        cpu.registers.r[0x3] = 0x00000000;
        cpu.registers.r[0x4] = 0xAC008300;
        cpu.registers.r[0x5] = 0xF4000000;
        cpu.registers.r[0x6] = 0xF4002000;
        cpu.registers.r[0x7] = 0x00000070;
        cpu.registers.r[0x8] = 0x00000000;
        cpu.registers.r[0x9] = 0x00000000;
        cpu.registers.r[0xA] = 0x00000000;
        cpu.registers.r[0xB] = 0x00000000;
        cpu.registers.r[0xC] = 0x00000000;
        cpu.registers.r[0xD] = 0x00000000;
        cpu.registers.r[0xE] = 0x00000000;
        cpu.registers.r[0xF] = 0x8D000000;
        cpu.registers.gbr = 0x8C000000;
        cpu.registers.ssr = 0x40000001;
        cpu.registers.spc = 0x8C000776;
        cpu.registers.sgr = 0x8D000000;
        cpu.registers.dbr = 0x8C000010;
        cpu.registers.vbr = 0x8C000000;
        cpu.registers.pr = 0xAC00043C;
        cpu.registers.r_bank[0] = 0xDFFFFFFF;
        cpu.registers.r_bank[1] = 0x500000F1;
        cpu.registers.r_bank[2] = 0x00000000;
        cpu.registers.r_bank[3] = 0x00000000;
        cpu.registers.r_bank[4] = 0x00000000;
        cpu.registers.r_bank[5] = 0x00000000;
        cpu.registers.r_bank[6] = 0x00000000;
        cpu.registers.r_bank[7] = 0x00000000;

        unsafe {
            cpu.registers.fpul = Float32 { u: 0x00000000 };
        }

        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL
        bus.write_32(0x005F8060, 0x00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 0x00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8050, 0x00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 0x00200000, context); // FB_R_SOF2
    }

    pub fn _load_rom(cpu: &mut Cpu, context: &mut Context, bus: &mut CpuBus) {
        let offset: u32 = 0xac010000;
        let size = _256_BIN.len();

        for i in 0..size {
            bus.write_8((offset as u32).wrapping_add(i as u32), _256_BIN[i], context);
        }

        cpu.registers.current_pc = 0x8c010000;
        cpu.registers.r[15] = 0x8c00d400;

        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL

        // Copy subroutine to RAM. Some of it will be overwritten, I'm trying to work out what's important and what's not.
        for i in (0..16) {
            bus.write_16(
                0x8C0000E0 + 2 * i,
                bus.read_16(0x800000FE - 2 * i, true, context),
                context,
            );
        }

        // system ram seems to have set up the bios to these values
        bus.write_32(0xac000074, 0x31, context);
        bus.write_32(0xac00002c, 0x16, context);
        bus.write_16(0x8c0090d8, 0x5113, context);
        bus.write_16(0x8c00940a, 0x000b, context);
        bus.write_16(0x8c00940c, 0x09, context);

        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL
        bus.write_32(0x005F8060, 00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8050, 00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 00200000, context); // FB_R_SOF2

        println!("emulator: loaded 256b.bin to ram+0x1000");
    }
}
