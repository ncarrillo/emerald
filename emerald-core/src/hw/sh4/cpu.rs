use crate::hw::extensions::BitManipulation;
// dreamcast sh-4 cpu
use crate::Context;
use crate::CpuBus;
use ::lending_iterator::prelude::*;
use std::f128;
use std::{collections::HashMap, fmt};

use super::bus::LogicalAddress;
use super::bus::PhysicalAddress;
use super::decoder::build_opcode_lut;
use super::decoder::DecodedInstruction;

pub struct CachedBlockIterator<'a> {
    block: &'a mut CachedBlock,
}

struct CachedBlockIteratorState {
    current_index: usize,
}

#[gat]
impl<'a> LendingIterator for CachedBlockIterator<'a> {
    type Item<'next> = &'next DecodedInstruction;

    fn next(&mut self) -> Option<&DecodedInstruction> {
        let state = self.block.iterator_state.as_mut()?;
        if state.current_index < self.block.instructions.len() {
            state.current_index += 1;
            Some(&self.block.instructions[state.current_index])
        } else {
            self.block.iterator_state = None;
            None
        }
    }
}

pub struct CachedBlockManager {
    blocks: Vec<CachedBlock>,
    builder: Option<CachedBlockBuilder>,
}

impl CachedBlockManager {
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            builder: None,
        }
    }

    pub fn invalidate_block(&mut self) {}

    pub fn exec_block(&mut self, block: &mut CachedBlock, max: usize) {
        // we should not be building a block while executing a block
        assert!(self.builder.is_none());

        for _ in 0..max {
            if let Some(instr) = block.instructions().next() {
                panic!("cbm: should be executing {:#?}", instr.disassembly);
            }
        }
        // executes a block for up to max cycles
    }

    pub fn alloc_block(&mut self) {
        self.builder = Some(CachedBlockBuilder::new());
    }

    pub fn record_instr_for_block(&mut self) {
        assert!(
            self.builder.is_some(),
            "must be building a block to record an instruction."
        );
    }
    pub fn find_block(&self, address: PhysicalAddress) -> Option<&CachedBlock> {
        self.blocks
            .binary_search_by(|block| {
                if address < block.start {
                    std::cmp::Ordering::Greater
                } else if address > block.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
            .map(|index| &self.blocks[index])
    }
}

pub struct CachedBlock {
    instructions: Vec<DecodedInstruction>,
    iterator_state: Option<CachedBlockIteratorState>,
    start: PhysicalAddress,
    end: PhysicalAddress,
}

impl CachedBlock {
    pub fn instructions(&mut self) -> CachedBlockIterator {
        CachedBlockIterator { block: self }
    }
}

pub struct CachedBlockBuilder {
    pending_instructions: Vec<DecodedInstruction>,
    start_pc: PhysicalAddress,
    end_pc: PhysicalAddress,
}

impl CachedBlockBuilder {
    const MAX_BLOCK_SIZE: usize = 256;

    pub fn new() -> Self {
        CachedBlockBuilder {
            pending_instructions: Vec::with_capacity(Self::MAX_BLOCK_SIZE),
            start_pc: PhysicalAddress(0),
            end_pc: PhysicalAddress(0),
        }
    }

    pub fn add_instruction_to_block(
        &mut self,
        pc: PhysicalAddress,
        instruction: DecodedInstruction,
    ) {
        assert!(
            pc >= self.start_pc,
            "pc must be greater than or equal to start of the block"
        );

        self.start_pc = std::cmp::min(self.start_pc, pc);
        self.end_pc = std::cmp::max(self.end_pc, pc);
        self.pending_instructions.push(instruction);
    }

    pub fn finalize_block(mut self) -> CachedBlock {
        assert!(
            self.pending_instructions.len() > 0,
            "pending block must have instructions"
        );

        CachedBlock {
            instructions: std::mem::take(&mut self.pending_instructions),
            start: self.start_pc,
            end: self.end_pc,
            iterator_state: Some(CachedBlockIteratorState { current_index: 0 }),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CpuState {
    Running,
    Sleeping,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union FpuBank {
    pub fr: [f32; 16],
    pub dr: [f64; 8],
}

impl Default for FpuBank {
    fn default() -> Self {
        FpuBank { fr: [0.0; 16] }
    }
}

impl FpuBank {
    pub fn get_fr(&self) -> [f32; 16] {
        unsafe { self.fr }
    }

    pub fn set_fr(&mut self, value: [f32; 16]) {
        self.fr = value;
    }

    pub fn get_dr(&self) -> [f64; 8] {
        unsafe { self.dr }
    }

    pub fn set_dr(&mut self, value: [f64; 8]) {
        self.dr = value;
    }
}

impl fmt::Debug for FpuBank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { write!(f, "FpuBank {{ fr: {:?}, dr: {:?} }}", &self.fr, &self.dr) }
    }
}

pub struct Cpu {
    pub registers: CpuRegisters,
    pub current_opcode: u16,
    pub cyc: u64,
    pub symbols_map: HashMap<u32, String>,
    pub state: CpuState,
    pub opcode_lut: Vec<DecodedInstruction>,
    pub cached_block_builder: CachedBlockBuilder,
}

#[derive(Copy, Clone, Default, Debug)]
#[repr(C)]
pub struct CpuRegisters {
    pub current_pc: u32,

    pub r: [u32; 16],
    pub r_bank: [u32; 8],

    // control registers
    pub sr: u32,
    pub gbr: u32,
    pub vbr: u32,
    pub dbr: u32,
    pub ssr: u32,
    pub spc: u32,
    pub sgr: u32,

    // system registers
    pub pr: u32,
    pub macl: u32,
    pub mach: u32,

    // fpu registers
    pub fpul: u32,
    pub fpscr: u32,

    pub fpu_banks: [FpuBank; 2],
}

impl CpuRegisters {
    pub fn new() -> Self {
        Self {
            current_pc: 0xa0000000,
            sr: 0x700000F0,
            fpscr: 0x4001,
            ..Default::default()
        }
    }
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            cyc: 0,
            registers: CpuRegisters::new(),
            current_opcode: 0,
            symbols_map: HashMap::new(),
            state: CpuState::Running,
            opcode_lut: build_opcode_lut(),
            cached_block_builder: CachedBlockBuilder::new(),
        }
    }

    pub fn swap_register_banks(&mut self) {
        for i in 0..8 {
            let temp = self.registers.r[i];
            self.registers.r[i] = self.registers.r_bank[i];
            self.registers.r_bank[i] = temp;
        }
    }

    pub fn swap_fpu_register_banks(&mut self) {
        self.registers.fpu_banks.swap(0, 1);
    }

    pub fn set_register_by_index(&mut self, index: usize, value: u32) {
        self.registers.r[index] = value;
    }

    pub fn set_banked_register_by_index(&mut self, index: usize, value: u32) {
        self.registers.r_bank[index & 0x7] = value;
    }

    pub fn set_fr_register_by_index(&mut self, index: usize, value: f32) {
        unsafe { self.registers.fpu_banks[0].fr[index] = value };
    }

    pub fn get_fr_register_by_index(&self, index: usize) -> f32 {
        self.registers.fpu_banks[0].get_fr()[index]
    }

    pub fn get_xf_register_by_index(&self, index: usize) -> f32 {
        self.registers.fpu_banks[1].get_fr()[index]
    }

    pub fn set_xf_register_by_index(&mut self, index: usize, value: f32) {
        unsafe { self.registers.fpu_banks[1].fr[index] = value };
    }

    pub fn get_dr_register_by_index(&self, index: usize) -> f64 {
        assert!(index < 8);

        self.registers.fpu_banks[0].get_dr()[index]
    }

    pub fn set_dr_register_by_index(&mut self, index: usize, value: f64) {
        assert!(index < 8);

        unsafe { self.registers.fpu_banks[0].dr[index] = value };
    }

    pub fn get_xd_register_by_index(&self, index: usize) -> f64 {
        self.registers.fpu_banks[1].get_dr()[index]
    }

    pub fn set_xd_register_by_index(&mut self, index: usize, value: f64) {
        unsafe { self.registers.fpu_banks[1].dr[index] = value };
    }

    pub fn get_register_by_index(&self, index: usize) -> u32 {
        self.registers.r[index]
    }

    pub fn get_banked_register_by_index(&self, index: usize) -> u32 {
        self.registers.r_bank[index & 0x7]
    }

    pub fn get_sr(&self) -> u32 {
        self.registers.sr
    }

    pub fn get_ssr(&self) -> u32 {
        self.registers.ssr
    }

    fn get_spc(&self) -> u32 {
        self.registers.spc
    }

    #[inline]
    fn get_gbr(&self) -> u32 {
        self.registers.gbr
    }

    fn get_vbr(&self) -> u32 {
        self.registers.vbr
    }

    fn get_pr(&self) -> u32 {
        self.registers.pr
    }

    pub fn get_fpscr(&self) -> u32 {
        self.registers.fpscr & 0x003FFFFF
    }

    pub fn swap_banks_if_needed(&mut self, old_sr: u32) {
        if old_sr.check_bit(29) != self.registers.sr.check_bit(29) {
            self.swap_register_banks();
        }
    }

    pub fn set_sr(&mut self, mut value: u32) {
        value &= 0x700083F3;

        let old_sr = self.registers.sr;
        self.registers.sr = value;
        self.swap_banks_if_needed(old_sr);
    }

    fn get_dbr(&self) -> u32 {
        self.registers.dbr
    }

    pub fn set_dbr(&mut self, value: u32) {
        self.registers.dbr = value;
    }

    pub fn set_pr(&mut self, value: u32) {
        self.registers.pr = value;
    }

    fn get_mach(&self) -> u32 {
        self.registers.mach
    }

    fn get_macl(&self) -> u32 {
        self.registers.macl
    }

    pub fn set_macl(&mut self, value: u32) {
        self.registers.macl = value;
    }

    pub fn set_mach(&mut self, value: u32) {
        self.registers.mach = value;
    }

    pub fn set_gbr(&mut self, value: u32) {
        self.registers.gbr = value;
    }

    pub fn set_vbr(&mut self, value: u32) {
        self.registers.vbr = value;
    }

    pub fn set_fpscr(&mut self, mut value: u32) {
        value &= 0x003FFFFF;

        if value.check_bit(21) != self.registers.fpscr.check_bit(21) {
            self.swap_fpu_register_banks();
        }

        self.registers.fpscr = value;
    }

    pub fn set_ssr(&mut self, value: u32) {
        self.registers.ssr = value; // & 0x700083F3;
    }

    pub fn set_spc(&mut self, value: u32) {
        self.registers.spc = value;
    }

    pub fn set_sgr(&mut self, value: u32) {
        self.registers.sgr = value;
    }

    pub fn set_fpul(&mut self, value: u32) {
        self.registers.fpul = value;
    }

    pub fn get_fpul(&self) -> u32 {
        self.registers.fpul
    }

    pub fn process_interrupts(&mut self, bus: &mut CpuBus, context: &mut Context, _: u64) {
        let imask = (self.get_sr() & 0xF0) >> 4;
        let int_index = (bus.intc.registers.interrupt_requests.trailing_zeros() as usize) & 0x3f;
        let interrupt = bus.intc.prioritized_interrupts[int_index];
        let level = bus.intc.interrupt_levels[interrupt as usize] as usize;

        if !self.get_sr().check_bit(28) || self.state != CpuState::Running {
            if bus.intc.registers.interrupt_requests != 0 {
                if level > imask as usize {
                    let intevt_table = [
                        0x1C0, // NMI
                        0x200, // IRL0
                        0x220, // IRL1
                        0x240, // IRL2
                        0x260, // IRL3
                        0x280, // IRL4
                        0x2A0, // IRL5
                        0x2C0, // IRL6
                        0x2E0, // IRL7
                        0x300, // IRL8
                        0x320, // IRL9
                        0x340, // IRL10
                        0x360, // IRL11
                        0x380, // IRL12
                        0x3A0, // IRL13
                        0x3C0, // IRL14
                        0x600, // Hitachi
                        0x620, // GPIO
                        0x640, // DMTE0
                        0x660, // DMTE1
                        0x680, // DMTE2
                        0x6A0, // DMTE3
                        0x6C0, // DMAE
                        0x400, // TUNI0
                        0x420, // TUNI1
                        0x440, // TUNI2
                        0x460, // TICPI2
                        0x480, // ATI
                        0x4A0, // PRI
                        0x4C0, // CUI
                        0x4E0, // SCI1_ERI
                        0x500, // SCI1_RXI
                        0x520, // SCI1_TXI
                        0x540, // SCI1_TEI
                        0x700, // SCIF_ERI
                        0x720, // SCIF_RXI
                        0x740, // SCIF_TXI
                        0x760, // SCIF_TEI
                        0x560, // ITI
                        0x580, // RCMI
                        0x5A0, // ROVI
                    ];

                    bus.ccn.registers.intevt = intevt_table[interrupt as usize];
                    bus.intc.registers.interrupt_requests &= !(1_u64 << int_index as u8);

                    #[cfg(feature = "log_ints")]
                    println!(
                        "{:08x} firing interrupt for {:#?} {:04x} {:08x} @ cycle {}",
                        self.registers.current_pc,
                        interrupt,
                        self.current_opcode,
                        self.get_vbr() + 0x600,
                        self.cyc
                    );

                    self.state = CpuState::Running;
                    self.set_spc(self.registers.current_pc);
                    self.set_ssr(self.get_sr());
                    self.set_sgr(self.get_register_by_index(15));
                    self.set_sr(self.get_sr().set_bit(28).set_bit(29).set_bit(30));
                    self.registers.current_pc = self.get_vbr() + 0x600;
                } else {
                }
            }
        }
    }

    pub fn exec_in_test(&mut self, bus: &mut CpuBus, context: &mut Context) -> Result<(), ()> {
        self.current_opcode = bus.read_16(self.registers.current_pc, true, context);

        let decoded = self.opcode_lut[self.current_opcode as usize];
        if true {
            println!(
                "\t{:08x} ({:04x}): {}",
                self.registers.current_pc, self.current_opcode, decoded.disassembly
            );
        }

        if decoded.disassembly == "unk" {
            return Err(());
        }

        (decoded.handler)(self, &decoded, bus, context);
        context.cyc = context.cyc + 1;
        Ok(())
    }

    pub fn exec_delay_slot_in_test(
        &mut self,
        bus: &mut CpuBus,
        context: &mut Context,
    ) -> Result<(), ()> {
        context.cyc = context.cyc + 1;
        self.current_opcode = bus.read_16(self.registers.current_pc, true, context);

        let decoded = self.opcode_lut[self.current_opcode as usize];
        if true {
            println!(
                "\t{:08x}: {} (delay slot)",
                self.registers.current_pc, decoded.disassembly
            );
        }

        if decoded.disassembly == "unk" {
            return Err(());
        }

        (decoded.handler)(self, &decoded, bus, context);

        Ok(())
    }

    pub fn exec_next_opcode(&mut self, bus: &mut CpuBus, context: &mut Context, cyc: u64) {
        if self.state == CpuState::Running {
            self.cyc = cyc;
            context.cyc = cyc;

            let opcode = bus.read_16(self.registers.current_pc, true, context);
            self.current_opcode = opcode;

            let decoded = self.opcode_lut[opcode as usize];

            if !context.tracing && self.registers.current_pc == 0x8c010c30 {
                #[cfg(feature = "trace_instrs")]
                {
                    context.tracing = true;
                }
            }


            #[cfg(feature = "trace_instrs")]
            if context.tracing {
                unsafe {
                    println!("{:08x} {:04x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}",
                    self.registers.current_pc, opcode,
                    self.get_register_by_index(0),
                    self.get_register_by_index(1),
                    self.get_register_by_index(2), self.get_register_by_index(3),
                    self.get_register_by_index(4), self.get_register_by_index(5),
                    self.get_register_by_index(6), self.get_register_by_index(7),
                    self.get_register_by_index(8), self.get_register_by_index(9),
                    self.get_register_by_index(10), self.get_register_by_index(11),
                    self.get_register_by_index(12), self.get_register_by_index(13),
                    self.get_register_by_index(14), self.get_register_by_index(15),
                    f32::to_bits(self.get_fr_register_by_index(0)),
                    f32::to_bits(self.get_fr_register_by_index(1)),
                    f32::to_bits(self.get_fr_register_by_index(2)),
                    f32::to_bits(self.get_fr_register_by_index(3)),
                    f32::to_bits(self.get_fr_register_by_index(4)),
                    f32::to_bits(self.get_fr_register_by_index(5)),
                    f32::to_bits(self.get_fr_register_by_index(6)),
                    f32::to_bits(self.get_fr_register_by_index(7)),
                    f32::to_bits(self.get_fr_register_by_index(8)),
                    f32::to_bits(self.get_fr_register_by_index(9)),
                    f32::to_bits(self.get_fr_register_by_index(10)),
                    f32::to_bits(self.get_fr_register_by_index(11)),
                    f32::to_bits(self.get_fr_register_by_index(12)),
                    f32::to_bits(self.get_fr_register_by_index(13)),
                    f32::to_bits(self.get_fr_register_by_index(14)),
                    f32::to_bits(self.get_fr_register_by_index(15)),
                    self.get_sr(), self.get_fpscr())
                };
            }

            // log some well known pc addresses in the bios to help getting the bios running
            #[cfg(feature = "log_bios")]
            {
                // 8c00b6b0
                let subroutine = match self.registers.current_pc {
                    0x80000000 => "bios_entry".to_owned(),
                    0x8c000c3e => "set_interrupts()".to_owned(),
                    0x8c00b500 => "init_machine()".to_owned(),
                    0x8c000d1c => "load_boot_file()".to_owned(),
                    0x80000116 => "system_reset()".to_owned(),
                    0x8c008300 => "IP.bin".to_owned(),
                    0x8c000120 => "boot2()".to_owned(),
                    //     0x0c000600 => "irq_handler()".to_owned(),
                    0x8c002ff4 => match self.get_register_by_index(4) {
                        16 => "CMD_PIOREAD".to_owned(),
                        17 => "CMD_DMAREAD".to_owned(),
                        18 => "CMD_GETTOC".to_owned(),
                        19 => "CMD_GETTOC2".to_owned(),
                        20 => "CMD_PLAY".to_owned(),
                        24 => "CMD_INIT".to_owned(),
                        35 => "CMD_GETTRACKS".to_owned(),
                        _ => format!("syscall CMD_{}unk()", self.get_register_by_index(4)), //.to_owned(),
                    },
                    0x8c001c34 | 0x8c001ca8 => "gd_get_toc()".to_owned(),
                    0x8c003570 => "gd_cmd_main_loop()".to_owned(),
                    0x8c0011ec => format!("gd_do_cmd({:08x})", self.get_register_by_index(6)),
                    0x8c0029a8 => "cdrom_response_loop()".to_owned(),
                    0x8c000e7c => "exec_gdcmd()".to_owned(),
                    0x8c000800 => {
                        format!("sysDoBiosCall({})", self.get_register_by_index(4) as i32)
                    }
                    0x8c000590 => "check_iso_pvd".to_owned(),
                    0x8c003450 => "gdc_reset()".to_owned(),
                    0x8c001890 => format!("gdc_init_system()"),
                    0x8c000420 => "boot3()".to_owned(),
                    0x8c000ae4 => "boot4()".to_owned(),
                    0x8c002b4c => "dispatch_gdrom_cmd()".to_owned(),
                    0x8c000990 => "syBtCheckDisk()".to_owned(),
                    0x8c0002c8 => "syBtExit()".to_owned(),
                    0x8c000820 => "boot5()".to_owned(),
                    0x8c000772 => "wait_timer()".to_owned(),
                    0x8c00095c => "check_gdrive_stat()".to_owned(),
                    0x8c000d02 => "check_disc()".to_owned(),
                    0x8c00cb2a => "wait_for_new_frame()".to_owned(),
                    0x8c184000 => "bios_anim_begin".to_owned(),
                    0x8c00ca78 => format!(
                        "bios_anim_state_machine({}, {}, {})",
                        self.get_register_by_index(4),
                        self.get_register_by_index(5),
                        self.get_register_by_index(6)
                    ),
                    0x8c00c000 => {
                        format!("bios_anim({:08x})", self.get_register_by_index(4))
                    }
                    _ => "".to_owned(),
                };

                if subroutine != "" {
                    println!(
                        "{:08x}: bios: {} @ cyc {}",
                        self.registers.current_pc, subroutine, cyc
                    );
                }
            }

            // KOS symbol mapping to help with debugging
            #[cfg(feature = "log_kos")]
            if let Some(sym) = self
                .symbols_map
                .get(&(self.registers.current_pc & 0x1FFFFFFF))
            {
                println!(
                    "{:08x}: calling {} @ cyc {}",
                    self.registers.current_pc, sym, cyc
                );
            }

            if false  {
                #[cfg(feature = "log_instrs")]
                println!(
                    "{:08x} {:04x}: {}",
                    self.registers.current_pc,
                    opcode,
                    decoded.disasm()
                );
            }

            // execute the decoded instruction
            (decoded.handler)(self, &decoded, bus, context);

            //   #[cfg(feature = "log_instrs2")]
            // writeln!(lock, "{:08x} {:04x}, {}", opcode, self.registers.current_pc, decoded.disassembly).unwrap();
        } else {
            self.process_interrupts(bus, context, 0);
        }
    }

    pub fn symbolicate(&self, addr: u32) -> String {
        // log some well known pc addresses in the bios to help getting the bios running
        #[cfg(feature = "log_bios")]
        {
            let subroutine = match self.registers.current_pc {
                0x80000000 => "bios_entry".to_owned(),
                0x8c000c3e => "set_interrupts()".to_owned(),
                0x8c00b500 => "init_machine()".to_owned(),
                0x8c000d1c => "load_boot_file()".to_owned(),
                0x80000116 => "system_reset()".to_owned(),
                0x8c008300 => "IP.bin".to_owned(),
                0x8c000120 => "boot2()".to_owned(),
                //     0x0c000600 => "irq_handler()".to_owned(),
                0x8c002ff4 => match self.get_register_by_index(4) {
                    16 => "CMD_PIOREAD".to_owned(),
                    17 => "CMD_DMAREAD".to_owned(),
                    18 => "CMD_GETTOC".to_owned(),
                    19 => "CMD_GETTOC2".to_owned(),
                    20 => "CMD_PLAY".to_owned(),
                    24 => "CMD_INIT".to_owned(),
                    35 => "CMD_GETTRACKS".to_owned(),
                    _ => format!("syscall CMD_{}unk()", self.get_register_by_index(4)), //.to_owned(),
                },
                0x8c001c34 | 0x8c001ca8 => "gd_get_toc()".to_owned(),
                0x8c003570 => "gd_cmd_main_loop()".to_owned(),
                0x8c0011ec => format!("gd_do_cmd({:08x})", self.get_register_by_index(6)),
                0x8c0029a8 => "cdrom_response_loop()".to_owned(),
                0x8c000e7c => "exec_gdcmd()".to_owned(),
                0x8c000800 => {
                    format!("sysDoBiosCall({})", self.get_register_by_index(4) as i32)
                }
                0x8c000590 => "check_iso_pvd".to_owned(),
                0x8c003450 => "gdc_reset()".to_owned(),
                0x8c001890 => format!("gdc_init_system()"),
                0x8c000420 => "boot3()".to_owned(),
                0x8c000ae4 => "boot4()".to_owned(),
                0x8c002b4c => "dispatch_gdrom_cmd()".to_owned(),
                0x8c000990 => "syBtCheckDisk()".to_owned(),
                0x8c0002c8 => "syBtExit()".to_owned(),
                0x8c000820 => "boot5()".to_owned(),
                0x8c000772 => "wait_timer()".to_owned(),
                0x8c00095c => "check_gdrive_stat()".to_owned(),
                0x8c000d02 => "check_disc()".to_owned(),
                0x8c00cb2a => "wait_for_new_frame()".to_owned(),
                0x8c184000 => "bios_anim_begin".to_owned(),
                0x8c00ca78 => format!(
                    "bios_anim_state_machine({}, {}, {})",
                    self.get_register_by_index(4),
                    self.get_register_by_index(5),
                    self.get_register_by_index(6)
                ),
                0x8c00c000 => {
                    format!("bios_anim({:08x})", self.get_register_by_index(4))
                }
                _ => "".to_owned(),
            };
        }

        // KOS symbol mapping to help with debugging
        if let Some(sym) = self
            .symbols_map
            .get(&(self.registers.current_pc & 0x1FFFFFFF))
        {
            return sym.clone();
        }

        return format!("0x{:08x}", addr);
    }
    pub fn step(&mut self, bus: &mut CpuBus, context: &mut Context, cyc: u64) {
        self.exec_next_opcode(bus, context, cyc);
    }

    pub fn delay_slot(&mut self, bus: &mut CpuBus, context: &mut Context) {
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
        self.exec_next_opcode(bus, context, self.cyc);
    }

    pub fn clrs(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        self.set_sr(self.get_sr().clear_bit(1));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn unk(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        println!(
            "{:08x}: unimplemented instruction {:04x}",
            self.registers.current_pc, self.current_opcode
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn rotcl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let mut sr = self.get_sr();
        let mut rn = self.get_register_by_index(rn_idx);

        let temp = if (rn & 0x80000000) != 0 { 1 } else { 0 };

        rn = rn.wrapping_shl(1);

        if sr.check_bit(0) {
            rn |= 0x00000001;
        } else {
            rn &= 0xFFFFFFFE;
        }

        sr = sr.eval_bit(0, temp != 0);
        self.set_sr(sr);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stc_rmbank(
        &mut self,
        instruction: &DecodedInstruction,
        _: &mut CpuBus,
        _: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        self.set_register_by_index(rn_idx, self.get_banked_register_by_index(rm_idx & 0x7));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_rmbank(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        self.set_register_by_index(rn_idx, rn);
        bus.write_32(rn, self.get_banked_register_by_index(rm_idx & 0x7), context);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcrn_rmbank(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let rn = self.get_register_by_index(rn_idx);
        self.set_banked_register_by_index(rm_idx & 0x7, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_rmbank(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let rn = self.get_register_by_index(rn_idx);
        self.set_banked_register_by_index(rm_idx & 0x7, bus.read_32(rn, context));
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn orm(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let r0 = self.get_register_by_index(0);
        let mut temp = bus.read_8(self.get_gbr() + r0, false, context) as i32;
        temp |= 0x000000FF & instruction.opcode.d8() as i32;
        bus.write_8(self.get_gbr() + r0, temp as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn rotcr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let sr = self.get_sr();
        let mut rn = self.get_register_by_index(rn_idx);

        let temp = if (rn & 0x00000001) == 0 { 0 } else { 1 };

        rn >>= 1;

        if sr.check_bit(0) {
            rn |= 0x80000000;
        } else {
            rn &= 0x7FFFFFFF;
        }

        self.set_sr(sr.eval_bit(0, temp == 1));
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn subc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let sr = self.get_sr();

        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let tmp0 = rn as u64;
        let tmp1 = rn.wrapping_sub(rm) as u64;
        rn = tmp1.wrapping_sub(if sr.check_bit(0) { 1 } else { 0 }) as u32;

        self.set_sr(sr.eval_bit(0, tmp0 < tmp1));

        if tmp1 < rn as u64 {
            self.set_sr(self.get_sr().set_bit(0));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn macl(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let mut rm = self.get_register_by_index(rm_idx);
        let mut rn = self.get_register_by_index(rn_idx);

        let mut tempn = bus.read_32(rn, context) as i32;
        rn += 4;
        let mut tempm = bus.read_32(rm, context) as i32;
        rm += 4;

        let fnlml = if ((tempn ^ tempm) as i32) < 0 { -1 } else { 0 };

        if tempn < 0 {
            tempn = 0 - tempn;
        }

        if tempm < 0 {
            tempm = 0 - tempm;
        }

        let mut temp1 = tempn as u32;
        let mut temp2 = tempm as u32;

        let rnl = temp1 & 0x0000ffff;
        let rnh = (temp1 >> 16) & 0x0000ffff;

        let rml = temp2 & 0x0000ffff;
        let rmh = (temp2 >> 16) & 0x0000ffff;

        let temp0 = rml * rnl;
        temp1 = rmh * rnl;
        temp2 = rml * rnh;
        let temp3 = rmh * rnh;

        let mut res2 = 0;
        let res1 = temp1 + temp2;

        if res1 < temp1 {
            res2 += 0x00010000;
        }

        temp1 = (res1 << 16) & 0xffff0000;
        let mut res0 = temp0 + temp1;
        if res0 < temp0 {
            res2 += 1;
        }

        res2 = res2 + ((res1 >> 16) & 0x0000ffff) + temp3;

        if fnlml < 0 {
            res2 = !res2;

            if res0 == 0 {
                res2 += 1;
            } else {
                res0 = !res0 + 1;
            }
        }

        let s = self.get_sr().check_bit(1);

        if s {
            res0 = self.get_macl() + res0;

            if self.get_macl() > res0 {
                res2 += 1;
            }

            res2 += self.get_mach() & 0x0000ffff;

            if ((res2 as i32) < 0) && (res2 < 0xffff8000) {
                res2 = 0xffff8000;
                res0 = 0x00000000;
            }

            self.set_mach((res2 & 0x0000ffff) | (self.get_mach() & 0xffff0000));
            self.set_macl(res0);
        } else {
            res0 = self.get_macl() + res0;

            if self.get_macl() > res0 {
                res2 += 1;
            }

            res2 += self.get_mach();
            self.set_mach(res2);
            self.set_macl(res0);
        }

        self.set_register_by_index(rn_idx, rn);
        self.set_register_by_index(rm_idx, rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn addc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let sr = self.get_sr();

        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        // fixme: wrapping adds
        let tmp0 = rn;
        let tmp1 = rn.wrapping_add(rm);
        rn = (tmp1.wrapping_add(if sr.check_bit(0) { 1 } else { 0 })) as u32;

        self.set_sr(sr.eval_bit(0, tmp0 > tmp1));

        if tmp1 > rn as u32 {
            self.set_sr(self.get_sr().set_bit(0));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let sr = self.get_sr();
        let rn = if sr.check_bit(0) {
            0x00000001
        } else {
            0x00000000
        };

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sleep(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        self.state = CpuState::Sleeping;
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn div1(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let mut sr = self.get_sr();

        let old_q = sr.check_bit(8);
        let mut q = (0x80000000 & self.get_register_by_index(rn_idx)) != 0;
        self.set_register_by_index(rn_idx, self.get_register_by_index(rn_idx) << 1);
        self.set_register_by_index(
            rn_idx,
            self.get_register_by_index(rn_idx) | if sr.check_bit(0) { 1 } else { 0 },
        );

        let m = sr.check_bit(9);

        let tmp0 = self.get_register_by_index(rn_idx);
        let tmp2 = self.get_register_by_index(rm_idx);
        let tmp1: bool;

        if !old_q {
            if !m {
                self.set_register_by_index(
                    rn_idx,
                    self.get_register_by_index(rn_idx).wrapping_sub(tmp2),
                );
                tmp1 = self.get_register_by_index(rn_idx) > tmp0;
                q = if !q { tmp1 } else { !tmp1 };
            } else {
                self.set_register_by_index(
                    rn_idx,
                    self.get_register_by_index(rn_idx).wrapping_add(tmp2),
                );
                tmp1 = self.get_register_by_index(rn_idx) < tmp0;
                q = if !q { !tmp1 } else { tmp1 };
            }
        } else {
            if !m {
                self.set_register_by_index(
                    rn_idx,
                    self.get_register_by_index(rn_idx).wrapping_add(tmp2),
                );
                tmp1 = self.get_register_by_index(rn_idx) < tmp0;
                q = if !q { tmp1 } else { !tmp1 };
            } else {
                self.set_register_by_index(
                    rn_idx,
                    self.get_register_by_index(rn_idx).wrapping_sub(tmp2),
                );
                tmp1 = self.get_register_by_index(rn_idx) > tmp0;
                q = if !q { !tmp1 } else { tmp1 };
            }
        }

        sr = sr.eval_bit(0, q == m).eval_bit(8, q);

        self.set_sr(sr);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn extsw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = rm;

        if (rm & 0x00008000) == 0 {
            rn &= 0x0000FFFF;
        } else {
            rn |= 0xFFFF0000;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsfpul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_fpul(rn);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsmacl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_macl(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsmach(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_mach(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movlsg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = (0x000000FF & instruction.opcode.d8() as i32) as u32;
        let r0 = self.get_register_by_index(0);
        bus.write_32(self.get_gbr() + (disp << 2), r0, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmpstr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        let temp = rn ^ rm;
        let mut hh = (temp & 0xFF000000) >> 24;
        let hl = (temp & 0x00FF0000) >> 16;
        let lh = (temp & 0x0000FF00) >> 8;
        let ll = temp & 0x000000FF;
        hh = if hh != 0 && hl != 0 && lh != 0 && ll != 0 {
            1
        } else {
            0
        };

        self.set_sr(self.get_sr().eval_bit(0, hh == 0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmppl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn as i32) > 0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmphi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn > rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmphieq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn >= rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmpeq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn == rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmpge(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn as i32 >= rm as i32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmpgt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn as i32 > rm as i32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmpimm(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8();
        let r0 = self.get_register_by_index(0);
        let imm = if (imm & 0x80) == 0 {
            0x000000FF & (imm as i32 as u32)
        } else {
            0xFFFFFF00 | imm as i32 as u32
        };

        self.set_sr(self.get_sr().eval_bit(0, imm as u32 == r0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn cmppz(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, rn as i32 >= 0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldspr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        self.set_pr(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stsmpr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let mut rn = self.get_register_by_index(rn_idx);

        rn = rn.wrapping_sub(4);

        bus.write_32(rn, self.get_pr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn nop(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn macw(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        panic!("macw....");
    }

    pub fn dmulu2(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let mut tempn = self.get_register_by_index(rn_idx) as i32;
        let mut tempm = self.get_register_by_index(rm_idx) as i32;

        if tempn < 0 {
            tempn = 0 - tempn;
        }

        if tempm < 0 {
            tempm = 0 - tempm;
        }

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let fnlml = if ((rn ^ rm) as i32) < 0 { -1 } else { 8 };

        let temp1 = tempn as u32;
        let temp2 = tempm as i32;

        let rnl = temp1 as u32 & 0x0000FFFF;
        let rnh = (temp1 as u32 >> 16) & 0x0000FFFF;

        let rml = temp2 as u32 & 0x0000FFFF;
        let rmh = (temp2 as u32 >> 16) & 0x0000FFFF;

        let temp0: u32 = rml * rnl;
        let mut temp1: u32 = rmh * rnl;
        let temp2: u32 = rml * rnh;
        let temp3: u32 = rmh * rnh;

        let mut res2 = 0;
        let res1 = temp1 + temp2;

        if res1 < temp1 {
            res2 += 0x0001000;
        }

        temp1 = (res1 << 16) & 0xffff0000;
        let mut res0 = temp0.wrapping_add(temp1);

        if res0 < temp0 {
            res2 += 1;
        }

        res2 = res2 + ((res1 >> 16) & 0x0000ffff) + temp3;

        if fnlml < 0 {
            res2 = !res2;
            if res0 == 0 {
                res2 += 1;
            } else {
                res0 = (!res0) + 1;
            }
        }

        self.set_mach(res2);
        self.set_macl(res0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn dmulu(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);

        let val = rn as u64 * rm as u64;

        let bytes = u64::to_le_bytes(val);
        self.set_mach(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]));
        self.set_macl(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn dt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(1);

        self.set_sr(self.get_sr().eval_bit(0, rn == 0));
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn rotr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let mut rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x00000001) != 0));

        rn >>= 1;

        if self.get_sr().check_bit(0) {
            rn |= 0x80000000;
        } else {
            rn &= 0x7FFFFFFF;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn rotl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let mut rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x80000000) != 0));

        rn <<= 1;

        if self.get_sr().check_bit(0) {
            rn |= 0x00000001;
        } else {
            rn &= 0xFFFFFFFE;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shar(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let mut rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn & 1) != 0));

        let temp = if (rn & 0x80000000) == 0 { 0 } else { 1 };

        rn = rn >> 1;

        if temp == 1 {
            rn |= 0x80000000
        } else {
            rn &= 0x7FFFFFFF
        };

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn addi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8() as u32;
        let imm = if (imm & 0x80) == 0 {
            0x000000FF & (imm as i32 as u32)
        } else {
            0xFFFFFF00 | imm as i32 as u32
        };

        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx) as i32 as u32;
        let val = rn.wrapping_add(imm as u32);
        self.set_register_by_index(rn_idx, val);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn xori(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8() as u32;
        let imm = 0x000000FF & imm;

        let rn = self.get_register_by_index(0);
        self.set_register_by_index(0, (rn ^ imm as i32 as u32) as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn add(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        self.set_register_by_index(rn_idx, rn.wrapping_add(rm as u32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        self.set_register_by_index(rn_idx, rn.wrapping_sub(rm as u32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn negc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let temp = 0_u32.wrapping_sub(rm);

        let mut sr = self.get_sr();
        let rn = temp - (if sr.check_bit(0) { 1 } else { 0 });

        sr = sr.eval_bit(0, 0 < temp);

        if temp < rn {
            sr = sr.set_bit(0);
        }

        self.set_register_by_index(rn_idx, rn);
        self.set_sr(sr);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn neg(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm = instruction.opcode.m();
        let rn = instruction.opcode.n();

        self.set_register_by_index(
            rn as usize,
            0_u32.wrapping_sub(self.get_register_by_index(rm as usize)),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn extub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rm_idx) & 0x000000FF;
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn extuw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rm_idx) & 0x0000FFFF;
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn extsb(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = rm;

        if (rm & 0x00000080) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        self.set_register_by_index(rn_idx, rn as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn xor(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm = instruction.opcode.m();
        let rn = instruction.opcode.n();

        self.set_register_by_index(
            rn as usize,
            self.get_register_by_index(rn as usize) ^ self.get_register_by_index(rm as usize),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn not(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm = instruction.opcode.m();
        let rn = instruction.opcode.n();

        self.set_register_by_index(rn as usize, !self.get_register_by_index(rm as usize));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ori(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8() as u32;
        let imm = 0x000000FF & imm;

        let rn = self.get_register_by_index(0);
        self.set_register_by_index(0, (rn | imm as i32 as u32) as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn and(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        let val = rn & rm;

        self.set_register_by_index(rn_idx, val);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn or(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        let val = rn | rm;

        self.set_register_by_index(rn_idx, val);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn lds_fpscr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_fpscr(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn andi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8() as u32;
        let imm = 0x000000FF & imm;

        let rn = self.get_register_by_index(0);
        self.set_register_by_index(0, (rn & imm as i32 as u32) as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movws4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000F & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        bus.write_16(
            rm.wrapping_add((disp << 1) as u32),
            self.get_register_by_index(0) as u16,
            context,
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movws0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        let r0 = self.get_register_by_index(0);

        bus.write_16(rn.wrapping_add(r0), rm as u16, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwsg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x000000FF & instruction.opcode.d8() as u32;
        bus.write_16(
            self.get_gbr().wrapping_add((disp << 1) as u32),
            self.get_register_by_index(0) as u16,
            context,
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movws(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        bus.write_16(rn, rm as u16, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwl4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000F & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let mut r0 = bus.read_16(rm.wrapping_add((disp << 1) as u32), false, context) as u32;

        if (r0 & 0x8000) == 0 {
            r0 &= 0x0000ffff;
        } else {
            r0 |= 0xffff0000;
        }

        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwlg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x000000FF & instruction.opcode.d8() as u32;
        let mut r0 = bus.read_16(self.get_gbr() + (disp << 1), false, context) as u32;

        if (r0 & 0x8000) == 0 {
            r0 &= 0x0000FFFF;
        } else {
            r0 |= 0xFFFF0000;
        }

        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbsg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = (0x000000FF & instruction.opcode.d8()) as u32;
        let r0 = self.get_register_by_index(0);
        bus.write_8(self.get_gbr().wrapping_add(disp), r0 as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movblg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = (0x000000FF & instruction.opcode.d8()) as u32;
        let mut r0 = bus.read_8(self.get_gbr().wrapping_add(disp), false, context) as u32;
        if (r0 & 0x80) == 0 {
            r0 &= 0x000000ff;
        } else {
            r0 |= 0xffffff00;
        }

        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwp(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_16(rm, false, context) as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        if rn_idx != rm_idx {
            self.set_register_by_index(rm_idx, rm.wrapping_add(2));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwm(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(2);

        bus.write_16(rn, rm as u16, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwi(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x000000FF & instruction.opcode.d8() as u32;
        let rn_idx = instruction.opcode.n();
        let mut rn = bus.read_16(
            self.registers
                .current_pc
                .wrapping_add(4 + (disp << 1) as u32),
            false,
            context,
        ) as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbl(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_8(rm, false, context) as u32;

        if (rn & 0x80) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbm(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(1);

        bus.write_8(rn, rm as u8, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movllg(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = (0x000000FF & instruction.opcode.d8()) as u32;
        let r0 = bus.read_32(self.get_gbr().wrapping_add(disp << 2), context);
        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movcal(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let r0 = self.get_register_by_index(0);
        let rn = self.get_register_by_index(rn_idx);

        bus.write_32(rn, r0, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movll(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let rn = bus.read_32(rm, context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movll0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let addr = rm.wrapping_add(self.get_register_by_index(0));
        let rn = bus.read_32(addr, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn mova(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let disp = 0x000000ff & instruction.opcode.d8() as u32;
        let val = (self.registers.current_pc & 0xfffffffc).wrapping_add(4 + (disp << 2) as u32);
        self.set_register_by_index(0, val);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbp(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_8(rm, false, context) as u32;

        if (rn & 0x80) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        if rm_idx != rn_idx {
            self.set_register_by_index(rm_idx, rm.wrapping_add(1));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbl0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let r0 = self.get_register_by_index(0);
        let mut rn = bus.read_8(r0.wrapping_add(rm), false, context) as u32;

        if (rn & 0x80) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn tas(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        let mut temp = bus.read_8(rn, false, context);
        self.set_sr(self.get_sr().eval_bit(0, temp == 0));
        temp |= 0x00000080;
        bus.write_8(rn, temp, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stsm_fpscr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_fpscr() & 0x003FFFFF, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stsmmach(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_mach(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stsmmacl(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_macl(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbl4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000F & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let mut r0 = bus.read_8(rm.wrapping_add(disp as u32), false, context) as u32;

        if (r0 & 0x80) == 0 {
            r0 &= 0x000000ff;
        } else {
            r0 |= 0xffffff00;
        }

        self.set_register_by_index(0, r0 as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwl(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_16(rm, false, context) as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movwl0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_16(
            rm.wrapping_add(self.get_register_by_index(0)),
            false,
            context,
        ) as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        self.set_register_by_index(rn_idx, rn as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbs(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        bus.write_8(rn, rm as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbs0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        bus.write_8(
            rn.wrapping_add(self.get_register_by_index(0)),
            rm as u8,
            context,
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movbs4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000F & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let addr = rm.wrapping_add(disp as u32);
        bus.write_8(addr, self.get_register_by_index(0) as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movll4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000F & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let addr = self
            .get_register_by_index(rm_idx)
            .wrapping_add((disp as u32) << 2);

        let rn = bus.read_32(addr, context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movls4(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x0000000f & instruction.opcode.d4() as i32;
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let addr = self
            .get_register_by_index(rn_idx)
            .wrapping_add((disp << 2) as u32);
        let val = self.get_register_by_index(rm_idx);
        bus.write_32(addr, val, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movls0(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        bus.write_32(
            self.get_register_by_index(rn_idx)
                .wrapping_add(self.get_register_by_index(0)),
            self.get_register_by_index(rm_idx),
            context,
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shll(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x80000000) != 0));
        self.shift_logical(rn_idx, 1, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shld(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = self.get_register_by_index(rn_idx);
        let sgn = rm & 0x80000000;

        if sgn == 0 {
            rn <<= rm & 0x1F;
        } else if (rm & 0x1F) == 0 {
            rn = 0;
        } else {
            rn = rn >> ((!rm & 0x1F) + 1);
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shad(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = self.get_register_by_index(rn_idx);
        let sgn = rm & 0x80000000;

        if sgn == 0 {
            rn <<= rm & 0x1F;
        } else if (rm & 0x1F) == 0 {
            rn = ((rn as i32) >> 31) as u32;
        } else {
            rn = ((rn as i32) >> ((!rm & 0x1f) + 1)) as u32;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shll2(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.shift_logical(rn_idx, 2, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shll8(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn = instruction.opcode.n();
        self.shift_logical(rn, 8, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shll16(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.shift_logical(rn_idx, 16, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shlr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, (rn & 1) != 0));
        self.set_register_by_index(rn_idx, rn >> 1);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shlr2(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn = instruction.opcode.n();
        self.shift_logical(rn, 2, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shlr8(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn = instruction.opcode.n();
        self.shift_logical(rn, 8, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shlr16(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn = instruction.opcode.n();
        self.shift_logical(rn, 16, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn swapw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);

        let temp = (rm >> 16) & 0x0000FFFF;
        let mut rn = rm << 16;
        rn |= temp;

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn swapb(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();
        let rm = self.get_register_by_index(rm_idx);
        let temp0 = rm & 0xFFFF0000;
        let temp1 = (rm & 0x000000FF) << 8;
        let mut rn = (rm & 0x0000FF00) >> 8;
        rn = rn | temp1 | temp0;

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stc_sr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_register_by_index(rn_idx, self.get_sr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stc_gbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_gbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stc_vbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_vbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stc_dbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_dbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldc_sr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(rn & 0x700083F3);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
        self.process_interrupts(bus, context, 0);
    }

    pub fn ldc_gbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_gbr(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldc_spc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_spc(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldc_vbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_vbr(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldc_dbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_dbr(rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_dbr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_dbr(bus.read_32(rn, context));

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_vbr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        let val = bus.read_32(rn, context);
        self.set_vbr(val);
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_spc(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        let val = bus.read_32(rn, context);
        self.set_spc(val);

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_ssr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.registers.ssr, context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_fpul(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_fpul(), context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_sr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_sr(), context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_gbr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_gbr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_vbr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_vbr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn stcm_spc(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_spc(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldc_ssr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_ssr(rn);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_ssr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_ssr(bus.read_32(rn, context));

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldcm_sr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let val = bus.read_32(rn, context) & 0x700083F3;

        let old_sr = self.registers.sr;
        self.registers.sr = val;
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);

        self.swap_banks_if_needed(old_sr);
        self.process_interrupts(bus, context, self.cyc);
    }

    pub fn ldsm_pr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let pr = bus.read_32(rn, context);
        self.set_pr(pr);

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsm_mach(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_mach(bus.read_32(rn, context));
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsm_gbr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_gbr(bus.read_32(rn, context));

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsm_macl(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_macl(bus.read_32(rn, context));

        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsm_fpscr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_fpscr(bus.read_32(rn, context));
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ldsm_fpul(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_fpul(bus.read_32(rn, context));
        self.set_register_by_index(rn_idx, rn.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sts_fpscr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_fpscr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sts_macl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_macl());

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sts_mach(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_mach());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sts_pr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_pr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sts_fpul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        self.set_register_by_index(rn_idx, self.get_fpul());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn jmp(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.delay_slot(bus, context);
        self.registers.current_pc = rn;
    }

    pub fn jsr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        self.set_pr(self.registers.current_pc + 4);
        self.delay_slot(bus, context);
        self.registers.current_pc = rn;
    }

    pub fn rts(&mut self, _: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let pr = self.get_pr();
        self.delay_slot(bus, context);
        self.registers.current_pc = pr;
    }

    pub fn rte(&mut self, _: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let spc = self.get_spc();
        let ssr = self.get_ssr();
        let old_sr = self.registers.sr & 0x700083F3;

        self.registers.sr = ssr & 0x700083F3;
        self.delay_slot(bus, context);
        self.swap_banks_if_needed(old_sr);
        self.registers.current_pc = spc;

        self.process_interrupts(bus, context, 0);
    }

    pub fn braf(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let pc = self.registers.current_pc.wrapping_add(4 + rn as u32);
        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn bra(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let mut disp = instruction.opcode.d12() as i32 as u32;
        if (disp & 0x800) == 0 {
            disp = 0x00000FFF & disp;
        } else {
            disp = 0xFFFFF000 | disp;
        }

        let pc = self
            .registers
            .current_pc
            .wrapping_add(4_u32.wrapping_add((disp << 1) as u32));
        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn bsrf(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        self.set_pr(self.registers.current_pc.wrapping_add(4));
        let rn = self.get_register_by_index(rn_idx);
        let pc = self.registers.current_pc.wrapping_add(4 + rn as u32);

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn bsr(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let mut disp = instruction.opcode.d12() as i32 as u32;
        if (disp & 0x800) == 0 {
            disp = 0x00000FFF & disp;
        } else {
            disp = 0xFFFFF000 | disp;
        }

        self.set_pr(self.registers.current_pc + 4);
        let pc = self
            .registers
            .current_pc
            .wrapping_add(4 + (disp << 1) as u32);

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn branch_if_true(
        &mut self,
        instruction: &DecodedInstruction,
        _: &mut CpuBus,
        _: &mut Context,
    ) {
        let d = instruction.opcode.d8() as i32 as u32;
        let disp = if (d & 0x80) == 0 {
            0x000000FF & d
        } else {
            0xFFFFFF00 | d
        } as i32;

        let sr = self.get_sr();
        if sr.check_bit(0) {
            self.registers.current_pc = self
                .registers
                .current_pc
                .wrapping_add(4 + (disp << 1) as u32);
        } else {
            self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
        }
    }

    pub fn branch_if_false(
        &mut self,
        instruction: &DecodedInstruction,
        _: &mut CpuBus,
        _: &mut Context,
    ) {
        let mut disp = instruction.opcode.d8() as i32 as u32;
        if (disp & 0x80) == 0 {
            disp = 0x000000FF & disp;
        } else {
            disp = 0xFFFFFF00 | disp;
        }

        let sr = self.get_sr();
        if !sr.check_bit(0) {
            self.registers.current_pc = self
                .registers
                .current_pc
                .wrapping_add(4 + (disp << 1) as u32);
        } else {
            self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
        }
    }

    pub fn branch_if_false_delayed(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let mut disp = instruction.opcode.d8() as i32 as u32;
        if (disp & 0x80) == 0 {
            disp = 0x000000FF & disp;
        } else {
            disp = 0xFFFFFF00 | disp;
        }

        let sr = self.get_sr();
        let pc = self.registers.current_pc.wrapping_add(if !sr.check_bit(0) {
            4 + (disp << 1)
        } else {
            4
        });

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn div0u(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let mut sr = self.get_sr();
        sr = sr.clear_bit(0).clear_bit(8).clear_bit(9);
        self.set_sr(sr);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn div0s(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let mut sr = self.get_sr();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        sr = sr.eval_bit(8, (rn & 0x80000000) != 0);
        sr = sr.eval_bit(9, (rm & 0x80000000) != 0);

        let m = if sr.check_bit(8) { 1 } else { 0 };
        let q = if sr.check_bit(9) { 1 } else { 0 };

        sr = sr.eval_bit(0, (m ^ q) != 0);

        self.set_sr(sr);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn branch_if_true_delayed(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let mut disp = instruction.opcode.d8() as i32 as u32;
        if (disp & 0x80) == 0 {
            disp = 0x000000FF & disp;
        } else {
            disp = 0xFFFFFF00 | disp;
        }

        let sr = self.get_sr();
        let pc = self.registers.current_pc.wrapping_add(if sr.check_bit(0) {
            4 + (disp << 1)
        } else {
            4
        });

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    pub fn pref(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rn_idx = instruction.opcode.n();
        let addr = self.get_register_by_index(rn_idx);

        if (addr & 0xEC000000 == 0xE0000000) {
            let sq = addr.check_bit(5);
            let sq_base = if sq {
                bus.ccn.registers.qacr1
            } else {
                bus.ccn.registers.qacr0
            };
            let ext_addr = (addr & 0x03ffffe0) | ((sq_base & 0x1c) << 24);
            let sq_idx = if sq { 1 } else { 0 };

            for i in 0..8 {
                bus.write_32(
                    (ext_addr + (4 * i)) as u32,
                    bus.store_queues[sq_idx][i as usize],
                    context,
                );
            }
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn tst(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        self.set_sr(self.get_sr().eval_bit(
            0,
            self.get_register_by_index(rn_idx) & self.get_register_by_index(rm_idx) == 0,
        ));

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn tsti(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = 0x000000ff & instruction.opcode.d8() as i32;
        self.set_sr(
            self.get_sr()
                .eval_bit(0, self.get_register_by_index(0) & imm as u32 == 0),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn sett(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        self.set_sr(self.get_sr().set_bit(0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn clrt(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        self.set_sr(self.get_sr().clear_bit(0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn mov(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let val = self.get_register_by_index(rm_idx);

        self.set_register_by_index(rn_idx, val);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let imm = instruction.opcode.d8();
        let rn_idx = instruction.opcode.n();

        let imm = if (imm & 0x80) == 0 {
            0x000000FF & imm
        } else {
            0xFFFFFF00 | imm
        };

        self.set_register_by_index(rn_idx as usize, imm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movlm(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        let rm = self.get_register_by_index(rm_idx);

        bus.write_32(rn, rm, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movlp(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        self.set_register_by_index(rn_idx, bus.read_32(rm, context));

        if rm_idx != rn_idx {
            self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movli(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let disp = 0x000000FF & instruction.opcode.d8() as u32;
        let rn_idx = instruction.opcode.n();
        let addr = (self.registers.current_pc & 0xfffffffc).wrapping_add(4 + (disp << 2) as u32);

        self.set_register_by_index(rn_idx, bus.read_32(addr, context));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn movls(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        bus.write_32(rn, rm, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn xtrct(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let high = (rm << 16) & 0xFFFF0000;
        let low = (rn >> 16) & 0x0000FFFF;
        rn = high | low;

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn mul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        let result = (rn as i32).wrapping_mul(rm as i32) as u32;
        self.set_macl(result);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn muls(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm = instruction.opcode.m();
        let rn = instruction.opcode.n();
        let result = (self.get_register_by_index(rn) as i16 as i32 as i64)
            * (self.get_register_by_index(rm) as i16 as i32 as i64);
        self.set_macl(result as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn mulu(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm = instruction.opcode.m();
        let rn = instruction.opcode.n();
        let result = (self.get_register_by_index(rn) as u16 as u64)
            * (self.get_register_by_index(rm) as u16 as u64);
        self.set_macl(result as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn shift_logical(&mut self, rn: usize, amount: i32, shift_direction: ShiftDirection) {
        let val = self.get_register_by_index(rn as usize);

        let shifted = if shift_direction == ShiftDirection::Left {
            val << amount
        } else {
            val >> amount
        };

        self.set_register_by_index(rn as usize, shifted);
    }
}

type InstructionHandler = fn(&mut Cpu, &DecodedInstruction, &mut CpuBus, &mut Context) -> ();

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShiftDirection {
    Left,
    Right,
}
