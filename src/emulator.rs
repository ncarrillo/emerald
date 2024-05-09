use std::{collections::HashMap, fs};

use goblin::elf::Elf;

use crate::{
    context::Context,
    hw::sh4::{bus::CpuBus, cpu::{Cpu, Float32}},
    scheduler::Scheduler,
};

pub struct Emulator {
    pub cpu: Cpu,
    pub scheduler: Scheduler,
}

//pub const IP_BIN: &[u8] = include_bytes!("../roms/IP/IP.BIN");
//pub const _256_BIN: &[u8] = include_bytes!("../roms/rotozoomer/roto.BIN");

// reicast dump of ram when pc = png.cdi entry point. helps smooth over some differences until we can boot the full bios
pub const REF_RAM: &[u8] = include_bytes!("../ref-ram.bin");

impl Emulator {
    pub fn new() -> Self {
        Emulator {
            cpu: Cpu::new(),
            scheduler: Scheduler::new(),
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

        let mut i = 0;
        for ref_byte in REF_RAM.iter() {
            bus.write_8(0x0c000000 + i as u32, *ref_byte, context);
            i += 1;
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
        cpu.set_fpu_register_by_index(11, Float32 { u: 0x3f800000 });
        cpu.set_fpu_register_by_index(9, Float32 { u: 0x80000000 });
        cpu.set_fpu_register_by_index(8, Float32 { u: 0x80000000 });
        cpu.set_fpu_register_by_index(7, Float32 { u: 0x3f800000 });
        cpu.set_fpu_register_by_index(6, Float32 { u: 0x41840000 });
        cpu.set_fpu_register_by_index(5, Float32 { u: 0x3fe66666 });
        cpu.set_fpu_register_by_index(4, Float32 { u: 0x3f266666 });
        cpu.set_register_by_index(4, 0x8c010000);
        cpu.registers.sr = 0x600000f0;
        cpu.registers.pr = 0x8c00e09c;
        cpu.registers.vbr = 0x8c00f400;
        cpu.registers.fpscr = 0x00040001;
        bus.holly.sb.registers.ffst_cnt.set(245277);

        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL

    /*     for i in 0..16 {
            bus.write_16(
                0x8C0000E0 + 2 * i,
                bus.read_16(0x800000FE - 2 * i, false, context),
                context,
            );
        }

        // system ram seems to have set up the bios to these values
        bus.write_32(0xac000074, 0x31, context);
        bus.write_32(0xac00002c, 0x16, context);
        bus.write_16(0x8c0090d8, 0x5113, context);
        bus.write_16(0x8c00940a, 0x000b, context);
        bus.write_16(0x8c00940c, 0x09, context);*/

        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL  
        bus.write_32(0x005F8060, 0x00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 0x00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8050, 0x00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 0x00200000, context); // FB_R_SOF2

        Ok(symbol_map)
    }

    // stub to load IP.bin
    pub fn _load_ip(_256_BIN: &mut Cpu, _: &mut Context, _: &mut CpuBus) {
        /*  let offset: u32 = 0xAC008000;
        let size = IP_BIN.len();

        for i in 0..size {
            bus.write_8(
                (offset as u32).wrapping_add(i as u32),
                IP_BIN[i],
                context,
            );
        }

        // set pc
        cpu.registers.pc = 0xAC008300;
        cpu.registers.current_pc = 0xAC008300;
        cpu.registers.pending_pc = cpu.registers.pc.wrapping_add(2);

        // bios leaves gprs and status registers in this state
        cpu.registers.r0_bank0 = 0xac0005d8;
        cpu.registers.r1_bank0 = 0x9;
        cpu.registers.r2_bank0 = 0xac00940c;
        cpu.registers.r4_bank0 = 0xac008300;
        cpu.registers.r5_bank0 = 0xf4000000;
        cpu.registers.r6_bank0 = 0xf4002000;
        cpu.registers.r7_bank0 = 0x00000044;
        cpu.registers.r15 = 0x8d000000;
        cpu.registers.sr = 0x400000f1;
        cpu.registers.fpscr = 0x00040001;

        // bios touches some timer register and the interrupt status for normal interrupts register
        bus.tmu.registers.tstr = 1;
        bus.holly.sb.registers.istnrm = 0x4030;

        // Copy subroutine to RAM. Some of it will be overwritten, I'm trying to work out what's important and what's not.
        for i in (0..16) {
            bus.write_16(
                0x8C0000E0 + 2 * i,
                bus.read_16(0x800000FE - 2 * i, false, false),
                false,
            );
        }

        // system ram seems to have set up the bios to these values
        bus.write_32(0xac000074, 0x31, context);
        bus.write_32(0xac00002c, 0x16, context);
        bus.write_16(0x8c0090d8, 0x5113, context);
        bus.write_16(0x8c00940a, 0x000b, context);
        bus.write_16(0x8c00940c, 0x09, context);

        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL
        bus.write_32(0x005F8060, 0x00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 0x00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8050, 0x00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 0x00200000, context); // FB_R_SOF2*/
    }

    pub fn _load_rom(_: &mut Cpu, _: &mut Context, _: &mut CpuBus) {
        /*         let offset: u32 = 0xac010000;
        let size = _256_BIN.len();

        for i in 0..size {
            bus.write_8(
                (offset as u32).wrapping_add(i as u32),
                _256_BIN[i],
                context,
            );
        }

        cpu.registers.pc = 0x8c010000;
        cpu.registers.current_pc = 0x8c010000;
        cpu.registers.pending_pc = cpu.registers.pc.wrapping_add(2);
        cpu.registers.r15 = 0x8c00d400;

        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL

        // Copy subroutine to RAM. Some of it will be overwritten, I'm trying to work out what's important and what's not.
        for i in (0..16) {
            bus.write_16(
                0x8C0000E0 + 2 * i,
                bus.read_16(0x800000FE - 2 * i, false, false),
                false,
            );
        }

        // system ram seems to have set up the bios to these values
        bus.write_32(0xac000074, 0x31, context);
        bus.write_32(0xac00002c, 0x16, context);
        bus.write_16(0x8c0090d8, 0x5113, context);
        bus.write_16(0x8c00940a, 0x000b, context);
        bus.write_16(0x8c00940c, 0x09, context);

        bus.write_32(0x005F8048, 6, context); // FB_W_CTRL
        bus.write_32(0x005F8060, 0x00600000, context); // FB_W_SOF1
        bus.write_32(0x005F8064, 0x00600000, context); // FB_W_SOF2
        bus.write_32(0x005F8044, 0x0080000D, context); // FB_R_CTRL
        bus.write_32(0x005F8050, 0x00200000, context); // FB_R_SOF1
        bus.write_32(0x005F8054, 0x00200000, context); // FB_R_SOF2

        /*bus.write_32(0x005F8048, 6, self.context);          // FB_W_CTRL
                bus.write_32(0x005F8060, 0x00600000, self.context); // FB_W_SOF1
                bus.write_32(0x005F8064, 0x00600000, self.context); // FB_W_SOF2
                bus.write_32(0x005F8050, 0x00200000, self.context); // FB_R_SOF1
                bus.write_32(0x005F8054, 0x00200000, self.context); // FB_R_SOF2
        */
        println!("emulator: loaded 256b.bin to ram+0x1000");*/
    }
}
