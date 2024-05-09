use crate::hw::extensions::BitManipulation;

// dreamcast sh-4 cpu
use crate::Context;
use once_cell::sync::OnceCell;
use std::{collections::HashMap, ffi::CStr, fmt};
use crate::CpuBus;
use std::mem;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CpuState {
    Running,
    Sleeping
}

#[repr(C)]
#[derive(Debug)]
pub struct KThread {
    context: IrqContext,
    t_list: ListEntry,
    thdq: TailQEntry,
    timerq: TailQEntry,
    tid: Tid,
    prio: Prio,
    flags: u32,
    state: i32,
    wait_obj: *mut std::ffi::c_void,
    wait_msg: *const std::os::raw::c_char,
    wait_callback: Option<extern "C" fn(*mut std::ffi::c_void)>,
    wait_timeout: u64,
    label: [u8; KTHREAD_LABEL_SIZE],
}

#[repr(transparent)]
#[derive(Debug)]
struct ListEntry(u8);
#[repr(transparent)]
#[derive(Debug)]
struct TailQEntry(u8);

type Tid = u64;
type Prio = u32;

const KTHREAD_LABEL_SIZE: usize = 32;

#[repr(C)]
#[derive(Debug)]
pub struct IrqContext {
    pc: u32,
    pr: u32,
    gbr: u32,
    vbr: u32,
    mach: u32,
    macl: u32,
    sr: u32,
    fpul: u32,
    fr: [u32; 16],
    frbank: [u32; 16],
    r: [u32; 16],
    fpscr: u32,
}

pub struct Cpu {
    pub registers: CpuRegisters,
    pub current_opcode: u16,
    pub cyc: u64,
    pub symbols_map: HashMap<u32, String>,
    pub state: CpuState
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Float32 {
    pub u: u32,
    f: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Float64 {
    u: [u32; 2],
    f: f64,
}

impl Default for Float32 {
    fn default() -> Self {
        Self { u: 0 }
    }
}

impl fmt::Debug for Float32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl Default for Float64 {
    fn default() -> Self {
        Self { u: [0, 0] }
    }
}

impl fmt::Debug for Float64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[derive(Copy, Clone, Default, Debug)]
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
    fpul: Float32,

    pub fr0_bank0: Float32,
    pub fr1_bank0: Float32,
    pub fr2_bank0: Float32,
    pub fr3_bank0: Float32,
    pub fr4_bank0: Float32,
    pub fr5_bank0: Float32,
    pub fr6_bank0: Float32,
    pub fr7_bank0: Float32,
    pub fr8_bank0: Float32,
    pub fr9_bank0: Float32,
    pub fr10_bank0: Float32,
    pub fr11_bank0: Float32,
    pub fr12_bank0: Float32,
    pub fr13_bank0: Float32,
    pub fr14_bank0: Float32,
    pub fr15_bank0: Float32,

    pub fr0_bank1: Float32,
    pub fr1_bank1: Float32,
    pub fr2_bank1: Float32,
    pub fr3_bank1: Float32,
    pub fr4_bank1: Float32,
    pub fr5_bank1: Float32,
    pub fr6_bank1: Float32,
    pub fr7_bank1: Float32,
    pub fr8_bank1: Float32,
    pub fr9_bank1: Float32,
    pub fr10_bank1: Float32,
    pub fr11_bank1: Float32,
    pub fr12_bank1: Float32,
    pub fr13_bank1: Float32,
    pub fr14_bank1: Float32,
    pub fr15_bank1: Float32,

    pub fpscr: u32,
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

#[macro_export]
macro_rules! generate_instructions {
    ($instructions:expr, $pattern:expr, $func:expr, $format:expr) => {{
        let mut base_opcode = 0u16;
        let mut val = 0x8000u16;
        let mut presence = [false; 4];
        let mut masks = [0u16; 4];
        let mut counts = [0; 4];

        for (_, chr) in $pattern.chars().enumerate() {
            match chr {
                '1' => base_opcode |= val,
                'n' => {
                    masks[0] |= val;
                    presence[0] = true;
                }
                'm' => {
                    masks[1] |= val;
                    presence[1] = true;
                }
                'i' => {
                    masks[2] |= val;
                    presence[2] = true;
                    counts[2] += 1;
                }
                'd' => {
                    masks[3] |= val;
                    presence[3] = true;
                    counts[3] += 1;
                }
                _ => (),
            }
            val >>= 1;
        }

        let max_values = [
            16,
            16,
            // i and d adjust their range dynamically
            1 << counts[2],
            1 << counts[3],
        ];

        for n in 0..if presence[0] { 16 } else { 1 } {
            for m in 0..if presence[1] { 16 } else { 1 } {
                for i in 0..if presence[2] { max_values[2] } else { 1 } {
                    for d in 0..if presence[3] { max_values[3] } else { 1 } {
                        let mut opcode = base_opcode;
                        if presence[0] {
                            opcode |= n << masks[0].trailing_zeros();
                        }
                        if presence[1] {
                            opcode |= m << masks[1].trailing_zeros();
                        }
                        if presence[2] {
                            opcode |= i << masks[2].trailing_zeros();
                        }
                        if presence[3] {
                            opcode |= d << masks[3].trailing_zeros();
                        }

                        let rm = if presence[1] { Some(m as usize) } else { None };
                        let rn = if presence[0] { Some(n as usize) } else { None };
                        let imm = if presence[2] { Some(i as u32) } else { None };
                        let disp = if presence[3] { Some(d as i32) } else { None };

                        $instructions.insert(
                            opcode,
                            DecodedInstruction {
                                rm: if presence[1] { Some(m as usize) } else { None },
                                rn: if presence[0] { Some(n as usize) } else { None },
                                imm: if presence[2] { Some(i as u32) } else { None },
                                displacement: if presence[3] { Some(d as i32) } else { None },
                                disassembly: (|| {
                                    let rm = format!("r{}", rm.unwrap_or(0));
                                    let rn = format!("r{}", rn.unwrap_or(0));
                                    let imm = format!("#{}", imm.unwrap_or(0));

                                    // 4 bit zero extended displacement
                                    let disp4 = format!("{}", disp.unwrap_or(0) & 0x0000000f);

                                    // 8 bit zero extended displacement
                                    let disp8 = format!("{}", disp.unwrap_or(0) & 0x000000ff);
                                    
                                    $format.to_owned()
                                       // .replace("PC", &pc)
                                        .replace("disp4", &disp4)
                                        .replace("disp8", &disp8)
                                        .replace("Rm", &rm)
                                        .replace("Rn", &rn)
                                        .replace("#imm", &imm)
                                })(),
                                func: $func,
                            },
                        );
                    }
                }
            }
        }
    }};
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            cyc: 0,
            registers: CpuRegisters::new(),
            current_opcode: 0,
            symbols_map: HashMap::new(),
            state: CpuState::Running
        }
    }

    pub fn swap_register_banks(&mut self) {
        for i in 0..8 {
            let temp = self.registers.r[i];
            self.registers.r[i] = self.registers.r_bank[i];
            self.registers.r_bank[i] = temp;
        }
    }

    pub fn set_register_by_index(&mut self, index: usize, value: u32) {
        self.registers.r[index] = value;
    }

    pub fn set_banked_register_by_index(&mut self, index: usize, value: u32) {
        self.registers.r_bank[index & 0x7] = value;
    }

    pub fn set_fpu_register_by_index(&mut self, index: usize, value: Float32) {
        let fr = self.registers.fpscr.check_bit(21);
        if fr {
            // fpscr.fr = 1
            match index {
                0 => self.registers.fr0_bank1 = value,
                1 => self.registers.fr1_bank1 = value,
                2 => self.registers.fr2_bank1 = value,
                3 => self.registers.fr3_bank1 = value,
                4 => self.registers.fr4_bank1 = value,
                5 => self.registers.fr5_bank1 = value,
                6 => self.registers.fr6_bank1 = value,
                7 => self.registers.fr7_bank1 = value,
                8 => self.registers.fr8_bank1 = value,
                9 => self.registers.fr9_bank1 = value,
                10 => self.registers.fr10_bank1 = value,
                11 => self.registers.fr11_bank1 = value,
                12 => self.registers.fr12_bank1 = value,
                13 => self.registers.fr13_bank1 = value,
                14 => self.registers.fr14_bank1 = value,
                15 => self.registers.fr15_bank1 = value,
                _ => {}, //panic!("invalid fpu register index: {}", index),
            }
        } else {
            match index {
                0 => self.registers.fr0_bank0 = value,
                1 => self.registers.fr1_bank0 = value,
                2 => self.registers.fr2_bank0 = value,
                3 => self.registers.fr3_bank0 = value,
                4 => self.registers.fr4_bank0 = value,
                5 => self.registers.fr5_bank0 = value,
                6 => self.registers.fr6_bank0 = value,
                7 => self.registers.fr7_bank0 = value,
                8 => self.registers.fr8_bank0 = value,
                9 => self.registers.fr9_bank0 = value,
                10 => self.registers.fr10_bank0 = value,
                11 => self.registers.fr11_bank0 = value,
                12 => self.registers.fr12_bank0 = value,
                13 => self.registers.fr13_bank0 = value,
                14 => self.registers.fr14_bank0 = value,
                15 => self.registers.fr15_bank0 = value,
                _ => {}, //panic!("invalid fpu register index: {}", index),
            }
        }
    }

    fn get_fpu_register_by_index(&self, index: usize) -> Float32 {
        let fr = self.registers.fpscr.check_bit(21);

        if fr {
            // fpscr.fr = 1
            match index {
                0 => self.registers.fr0_bank1,
                1 => self.registers.fr1_bank1,
                2 => self.registers.fr2_bank1,
                3 => self.registers.fr3_bank1,
                4 => self.registers.fr4_bank1,
                5 => self.registers.fr5_bank1,
                6 => self.registers.fr6_bank1,
                7 => self.registers.fr7_bank1,
                8 => self.registers.fr8_bank1,
                9 => self.registers.fr9_bank1,
                10 => self.registers.fr10_bank1,
                11 => self.registers.fr11_bank1,
                12 => self.registers.fr12_bank1,
                13 => self.registers.fr13_bank1,
                14 => self.registers.fr14_bank1,
                15 => self.registers.fr15_bank1,
                _ => Float32 { u: 0 }, //panic!("invalid fpu register index: {}", index),
            }
        } else {
            match index {
                0 => self.registers.fr0_bank0,
                1 => self.registers.fr1_bank0,
                2 => self.registers.fr2_bank0,
                3 => self.registers.fr3_bank0,
                4 => self.registers.fr4_bank0,
                5 => self.registers.fr5_bank0,
                6 => self.registers.fr6_bank0,
                7 => self.registers.fr7_bank0,
                8 => self.registers.fr8_bank0,
                9 => self.registers.fr9_bank0,
                10 => self.registers.fr10_bank0,
                11 => self.registers.fr11_bank0,
                12 => self.registers.fr12_bank0,
                13 => self.registers.fr13_bank0,
                14 => self.registers.fr14_bank0,
                15 => self.registers.fr15_bank0,
                _ => Float32 { u: 0 }, //panic!("invalid fpu register index: {}", index),
            }
        }
    }

    fn get_register_by_index(&self, index: usize) -> u32 {
        self.registers.r[index]
    }

    fn get_banked_register_by_index(&self, index: usize) -> u32 {
        self.registers.r_bank[index & 0x7]
    }

    pub fn get_sr(&self) -> u32 {
        self.registers.sr
    }

    fn get_ssr(&self) -> u32 {
        self.registers.ssr
    }

    fn get_spc(&self) -> u32 {
        self.registers.spc
    }

    fn get_gbr(&self) -> u32 {
        self.registers.gbr
    }

    fn get_vbr(&self) -> u32 {
        self.registers.vbr
    }

    fn get_pr(&self) -> u32 {
        self.registers.pr
    }

    fn get_fpscr(&self) -> u32 {
        self.registers.fpscr
    }

    pub fn set_sr(&mut self, value: u32) {
        if value.check_bit(29) != self.registers.sr.check_bit(29) {
            self.swap_register_banks();
        }

        self.registers.sr = value;
    }

    fn get_dbr(&self) -> u32 {
        self.registers.dbr
    }

    fn set_dbr(&mut self, value: u32) {
        self.registers.dbr = value;
    }

    fn set_pr(&mut self, value: u32) {
        self.registers.pr = value;
    }

    fn get_mach(&self) -> u32 {
        self.registers.mach
    }

    fn get_macl(&self) -> u32 {
        self.registers.macl
    }

    fn set_macl(&mut self, value: u32) {
        self.registers.macl = value;
    }

    fn set_mach(&mut self, value: u32) {
        self.registers.mach = value;
    }

    fn set_gbr(&mut self, value: u32) {
        #[cfg(feature = "log_instrs")]
        println!("cpu: gbr set to {:08x} @ {:08x}", value, self.registers.current_pc);
        self.registers.gbr = value;
    }

    fn set_vbr(&mut self, value: u32) {
        self.registers.vbr = value;
    }

    fn set_fpscr(&mut self, value: u32) {
        self.registers.fpscr = value;
    }

    fn set_ssr(&mut self, value: u32) {
        self.registers.ssr = value;
    }

    fn set_spc(&mut self, value: u32) {
        self.registers.spc = value;
    }

    fn set_sgr(&mut self, value: u32) {
        self.registers.sgr = value;
    }

    fn set_fpul(&mut self, value: Float32) {
        self.registers.fpul = value;
    }

    fn get_fpul(&self) -> Float32 {
        self.registers.fpul
    }

    fn instruction_lut<'a>() -> &'static HashMap<u16, DecodedInstruction> {
        static INSTANCE: OnceCell<HashMap<u16, DecodedInstruction>> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            let mut decoding_lookup_table: HashMap<u16, DecodedInstruction> = HashMap::new();

            generate_instructions!(decoding_lookup_table, "0000000000001001", Self::nop, "nop");
            generate_instructions!(decoding_lookup_table, "0000000000011011", Self::sleep, "sleep");
            generate_instructions!(decoding_lookup_table, "0000000000001000", Self::clrt, "clrt");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1101", Self::xtrct, "xtrct Rm, Rn");

            // temp nopped
            generate_instructions!(decoding_lookup_table, "0000nnnn10100011", Self::nop, "ocbp"); // ocbp
            generate_instructions!(decoding_lookup_table, "0000nnnn10010011", Self::nop, "ocbp"); // ocbi
            generate_instructions!(decoding_lookup_table, "0000nnnn10110011", Self::nop, "???"); // ocbwb

            generate_instructions!(decoding_lookup_table, "0000nnnn01101010", Self::sts_fpscr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn01100010", Self::stsm_fpscr, "???"); 
            generate_instructions!(decoding_lookup_table, "1111nnmm11101101", Self::fipr, "fipr fvRn, fvRm"); 
            

            generate_instructions!(
                decoding_lookup_table,
                "1111nnnnmmmm0111",
                Self::fmov_index_store,
                "???"
            ); // fmov index store
            generate_instructions!(
                decoding_lookup_table,
                "1111nnnnmmmm0110",
                Self::fmov_index_load,
                "???"
            ); // fmov index load
            generate_instructions!(decoding_lookup_table, "1111nnnn10001101", Self::fldi0, "???"); // fldi0
            generate_instructions!(decoding_lookup_table, "1111nnnn10011101", Self::fldi1, "???"); // fldi1
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0101", Self::fcmpgt, "???"); // fcmpgt
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0100", Self::fcmpeq, "???"); // fcmpeq
            generate_instructions!(decoding_lookup_table, "1111nn0111111101", Self::nop, "ftrv ???"); // ftrv
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm1110", Self::fmac, "fmac ???"); // fmac
            generate_instructions!(decoding_lookup_table, "1111nnnn01101101", Self::fsqrt, "fsqrt ???"); // fsqrt
            generate_instructions!(decoding_lookup_table, "1111nnnn01111101", Self::nop, "fsrra ???"); // fsrra

            generate_instructions!(decoding_lookup_table, "0100nnnn00010000", Self::dt, "dt Rn");

            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0011", Self::mov, "mov Rn, Rn");
            generate_instructions!(decoding_lookup_table, "0000nnnn11000011", Self::movcal, "movca.l r0, @Rn");
            generate_instructions!(decoding_lookup_table, "0000nnnn00101001", Self::movt, "???");
            generate_instructions!(decoding_lookup_table, "1110nnnniiiiiiii", Self::movi, "movi #imm, Rn");
            generate_instructions!(decoding_lookup_table, "1101nnnndddddddd", Self::movli, "mov.l @(disp8+PC), Rn");
            generate_instructions!(decoding_lookup_table, "11000010dddddddd", Self::movlsg, "mov.l r0, @(disp8, gbr)");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0110", Self::movlm, "mov.l Rm, @-Rn");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0110", Self::movlp, "mov.l @Rm+, Rn");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0010", Self::movls, "mov.l Rm, @Rn");
            generate_instructions!(decoding_lookup_table, "0101nnnnmmmmdddd", Self::movll4, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm0110", Self::movls0, "???");
            generate_instructions!(decoding_lookup_table, "0001nnnnmmmmdddd", Self::movls4, "???");
            generate_instructions!(decoding_lookup_table, "11000110dddddddd", Self::movllg, "mov.l @(disp8, gbr), r0");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0001", Self::movws, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm0101", Self::movws0, "???");
            generate_instructions!(decoding_lookup_table, "10000001nnnndddd", Self::movws4, "???");
            generate_instructions!(decoding_lookup_table, "11000001dddddddd", Self::movwsg, "???");
            generate_instructions!(decoding_lookup_table, "10000101mmmmdddd", Self::movwl4, "???");
            generate_instructions!(decoding_lookup_table, "11000101dddddddd", Self::movwlg, "???");
            generate_instructions!(decoding_lookup_table, "1001nnnndddddddd", Self::movwi, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0000", Self::movbl, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0100", Self::movbm, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0010", Self::movll, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm1110", Self::movll0, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0000", Self::movbs, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm0100", Self::movbs0, "mov.b Rm, @(r0, Rn)");
            generate_instructions!(decoding_lookup_table, "10000000nnnndddd", Self::movbs4, "mov.b r0, @(disp4, Rn)");
            generate_instructions!(decoding_lookup_table, "11000111dddddddd", Self::mova, "mova @(disp8, PC), r0");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0100", Self::movbp, "mov.b @Rm+, Rn");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm1100", Self::movbl0, "mov.b @(r0, Rm), Rn");
            generate_instructions!(decoding_lookup_table, "10000100mmmmdddd", Self::movbl4, "mov.b @(disp4, Rm), r0");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0001", Self::movwl, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0101", Self::movwp, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0101", Self::movwm, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm1101", Self::movwl0, "???");
            generate_instructions!(decoding_lookup_table, "11000000dddddddd", Self::movbsg, "???");
            generate_instructions!(decoding_lookup_table, "11000100dddddddd", Self::movblg, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm1111", Self::macl, "mac.l @Rm+, @Rn+");
            generate_instructions!(decoding_lookup_table, "0100nnnn00011011", Self::tas, "???");

            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1100", Self::extub, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1101", Self::extuw, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1110", Self::extsb, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1111", Self::extsw, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1011", Self::neg, "???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1010", Self::negc, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnn01001101", Self::fneg, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1010", Self::xor, "xor Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm0111", Self::not, "not Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1011", Self::or, "or Rm, Rn");
            generate_instructions!(decoding_lookup_table, "11001111iiiiiiiif", Self::orm, "???");
            generate_instructions!(decoding_lookup_table, "11001011iiiiiiii", Self::ori, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1001", Self::and, "and Rm, Rn");
            generate_instructions!(decoding_lookup_table, "11001001iiiiiiii", Self::andi, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnnmmmm0111", Self::mul,"mul.l Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0101", Self::dmulu, "???");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm1101", Self::dmulu, "???"); // fixme: make signed
            generate_instructions!(decoding_lookup_table, "0000000000011001", Self::div0u, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm0111", Self::div0s, "???");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0100", Self::div1, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1111", Self::muls, "muls.w r0, Rn");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1110", Self::mulu, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00000000", Self::shll, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnnmmmm1101", Self::shld, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnnmmmm1100", Self::shad, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00001000", Self::shll2, "shll2 Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00011000", Self::shll8, "shll8 Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00101000", Self::shll16, "shll16 Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00000001", Self::shlr, "shlr Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00001001", Self::shlr2, "shlr2 Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00011001", Self::shlr8, "shlr8 Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00101001", Self::shlr16, "shlr16 Rn");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1001", Self::swapw,"???");
            generate_instructions!(decoding_lookup_table, "0110nnnnmmmm1000", Self::swapb, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn00011010", Self::sts_macl, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn00001010", Self::sts_mach, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00000010", Self::stsmmach, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00010010", Self::stsmmacl, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn01011010", Self::sts_fpul, "???");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1000", Self::tst, "tst Rm, Rn");
            generate_instructions!(decoding_lookup_table, "11001000iiiiiiii", Self::tsti, "???");
            generate_instructions!(decoding_lookup_table, "0000000000011000", Self::sett, "???");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm1100", Self::add, "add Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm1000", Self::sub, "sub Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm1010", Self::subc, "subc Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm1110", Self::addc, "addc Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0111nnnniiiiiiii", Self::addi, "addi #imm");
            generate_instructions!(decoding_lookup_table, "11001010iiiiiiii", Self::xori, "xori #imm");
            generate_instructions!(decoding_lookup_table, "0100nnnn00100001", Self::shar, "shar Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00000101", Self::rotr, "rotr Rn");
            generate_instructions!(decoding_lookup_table, "0000nnnn10000011", Self::pref, "pref Rn");
            generate_instructions!(decoding_lookup_table, "0010nnnnmmmm1100", Self::cmpstr, "cmp/str Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00010101", Self::cmppl, "cmp/pl Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0110", Self::cmphi, "cmp/hi Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0010", Self::cmphieq, "cmp/hs Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0000", Self::cmpeq, "cmp/eq Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0011", Self::cmpge, "cmp/ge Rm, Rn");
            generate_instructions!(decoding_lookup_table, "0011nnnnmmmm0111", Self::cmpgt, "cmp/gt Rm, Rn");
            generate_instructions!(decoding_lookup_table, "10001000iiiiiiii", Self::cmpimm, "cmp #imm, r0");
            generate_instructions!(decoding_lookup_table, "0100nnnn00010001", Self::cmppz, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00101010", Self::ldspr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00100010", Self::stsmpr, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn00000010", Self::stc_sr, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn00010010", Self::stc_gbr, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn00100010", Self::stc_vbr, "???");
            generate_instructions!(decoding_lookup_table, "0000nnnn11111010", Self::stc_dbr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00001110", Self::ldc_sr, "ldc Rm, sr");
            generate_instructions!(decoding_lookup_table, "0100mmmm00011110", Self::ldc_gbr, "ldc Rm, gbr");
            generate_instructions!(decoding_lookup_table, "0100nnnn00010011", Self::stcm_gbr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn01010010", Self::stcm_fpul, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00101110", Self::ldc_vbr, "ldc Rm, vbr");
            generate_instructions!(decoding_lookup_table, "0100mmmm11111010", Self::ldc_dbr, "ldc Rm, dbr");
            generate_instructions!(decoding_lookup_table, "0100mmmm11110110", Self::ldcm_dbr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00110111", Self::ldcm_ssr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00110011", Self::stcm_ssr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00100111", Self::ldcm_vbr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00100011", Self::stcm_vbr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm01000111", Self::ldcm_spc, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn01000011", Self::stcm_spc, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00000111", Self::ldcm_sr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00000011", Self::stcm_sr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00100110", Self::ldsm_pr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00000110", Self::ldsm_mach, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00010111", Self::ldsm_gbr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm00010110", Self::ldsm_macl, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm01010110", Self::ldsm_fpul, "ldc (r{rm_idx}), fpul");
            generate_instructions!(decoding_lookup_table, "0100mmmm01100110", Self::ldsm_fpscr, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm01101010", Self::lds_fpscr, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn00100100", Self::rotcl, "rotcl Rn");
            generate_instructions!(decoding_lookup_table, "0100nnnn00100101", Self::rotcr, "rotcr Rn");
            generate_instructions!(decoding_lookup_table, "0000nnnn1mmm0010", Self::stc_rmbank, "???");
            generate_instructions!(decoding_lookup_table, "0100nnnn1mmm0011", Self::stcm_rmbank, "???");
            generate_instructions!(decoding_lookup_table, "0100mmmm1nnn0111", Self::ldcm_rmbank, "???");
            generate_instructions!(decoding_lookup_table, "1111001111111101", Self::fschg, "fschg");
            generate_instructions!(decoding_lookup_table, "0100nnnnmmmm1111", Self::nop, "mach");


            generate_instructions!(
                decoding_lookup_table,
                "10001011dddddddd",
                Self::branch_if_false,
                "???"
            );
            generate_instructions!(decoding_lookup_table, "0000mmmm00100011", Self::braf, "???");
            generate_instructions!(
                decoding_lookup_table,
                "10001001dddddddd",
                Self::branch_if_true,
                "???"
            );
            generate_instructions!(
                decoding_lookup_table,
                "10001111dddddddd",
                Self::branch_if_false_delayed,
                "???"
            );
            generate_instructions!(
                decoding_lookup_table,
                "10001101dddddddd",
                Self::branch_if_true_delayed,
                "???"
            );
            generate_instructions!(decoding_lookup_table, "1010dddddddddddd", Self::bra, "???");
            generate_instructions!(decoding_lookup_table, "1011dddddddddddd", Self::bsr, "???");
            generate_instructions!(decoding_lookup_table, "0000mmmm00000011", Self::bsrf, "bsrf Rm");
            generate_instructions!(decoding_lookup_table, "0100mmmm00101011", Self::jmp, "jmp @Rm");
            generate_instructions!(decoding_lookup_table, "0100mmmm00001011", Self::jsr, "jsr @Rm");
            generate_instructions!(decoding_lookup_table, "0000000000001011", Self::rts, "rts");
            generate_instructions!(decoding_lookup_table, "0000000000101011", Self::rte, "rte");

            // fpu
            generate_instructions!(decoding_lookup_table, "0100mmmm01011010", Self::ldsfpul, "???");
            generate_instructions!(
                decoding_lookup_table,
                "1111nnnn00101101",
                Self::float_single,
                "???"
            );
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0000", Self::fadd, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0001", Self::fsub, "fsub fRn, fRm");
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0011", Self::fdiv, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm0010", Self::fmul, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm1000", Self::fmov_load, "???");
            
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm1100", Self::fmov, "???");


            generate_instructions!(
                decoding_lookup_table,
                "1111nnnnmmmm1001",
                Self::fmov_restore,
                "???"
            );
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm1010", Self::fmov_store, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnnmmmm1011", Self::fmov_save, "???");
            generate_instructions!(decoding_lookup_table, "1111mmmm00111101", Self::ftrc, "ftrc fRM, fpul");
            generate_instructions!(decoding_lookup_table, "1111nnn011111101", Self::fsca, "fsca fpul, dRn");
            generate_instructions!(decoding_lookup_table, "1111101111111101", Self::frchg, "???");
            generate_instructions!(decoding_lookup_table, "1111nnnn00001101", Self::fsts, "???");

            decoding_lookup_table
        })
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

                   // println!("{:08x} firing interrupt for {:#?} {:04x} @ cycle{}", self.registers.current_pc, interrupt, self.current_opcode, self.cyc);
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
    
    pub fn print_thread_context(&mut self, bus: &mut CpuBus, context: &mut Context) {
        // fixme: hard coded addr from the symbol table, do this lookup dynamically
        let data = (bus.read_32(0x8c089468, context)) as usize;

        // deref data and mask the top 3 bits to get the physical address
        // then subtract is from the sram base to get an index into sram
        let starting_idx = (data & 0x1FFFFFFF) - 0x0C000000;
        let bytes = &bus.system_ram[starting_idx..];

        // transmute the bytes into KThread reference
        let kthread: &KThread = unsafe { mem::transmute(bytes.as_ptr()) };

        // the label is stored as an array of u8 with a max length of 30, but the null terminator is often earlier
        // read up until the null terminator and disregard the rest.
        let label_bytes = &kthread.label;
        let first_null_index = label_bytes.iter().position(|&c| c == 0).unwrap_or(label_bytes.len());
        let label_slice = &label_bytes[..=first_null_index];
        
        let c_str = CStr::from_bytes_with_nul(label_slice).unwrap();
        let label_str = c_str.to_str().unwrap();
        
        println!("executing thread name: {}", label_str);
        println!("executing thread pc: {:08x}", kthread.context.pc);
        println!("executing thread sr: {:08x}", kthread.context.sr);
        println!("executing thread wait timeout: {}", kthread.wait_timeout);
        println!("executing thread wait callback: {:#?}", kthread.wait_callback);
    }

    pub fn exec_next_opcode(&mut self, bus: &mut CpuBus, context: &mut Context, cyc: u64) {
        if self.state == CpuState::Running { 
            self.cyc = cyc;
            context.cyc = cyc;

            if self.registers.current_pc == 0x8c02ce0c && context.tracing == false {//cyc >= 150000000 {
                #[cfg(feature = "trace_instrs")]
                {
                    context.tracing = true;
                }
            }

            if self.registers.current_pc == 0x8c027110 {
                println!("loading PNG texture from {:08x} to {:08x} {}x{}", self.get_register_by_index(4), self.get_register_by_index(5),  self.get_register_by_index(6),  self.get_register_by_index(7))
            }

            if self.registers.current_pc == 0x8c010800 {
                println!("cpu: reached main! congratulations :-).");
            }
            
            let opcode = bus.read_16(self.registers.current_pc, true, context);
            self.current_opcode = opcode;

            if let Some(decoded) = Self::instruction_lut().get(&opcode) {
                #[cfg(feature = "trace_instrs")]
                if context.tracing {
                    unsafe {
                        println!("{:08x} {:04x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}", 
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
                        self.get_sr(), self.get_mach())
                    };
                }

                // log some well known pc addresses in the bios to help getting the bios running
                #[cfg(feature = "log_bios")]
                {
                    let subroutine = match self.registers.current_pc & 0x1FFFFFFF {
                        0x00000000 => "bios_entry".to_owned(),
                        0x0c000c3e => "set_interrupts()".to_owned(),
                        0x0c00b500 => "init_machine()".to_owned(),
                        0x0c000d1c => "load_boot_file".to_owned(),
                        0x00000116 => "system_reset".to_owned(),
                        0x0c008300 => "IP.bin".to_owned(),
                        0x0c000120 => "boot2()".to_owned(),
                        0x0c000600 => "irq_handler".to_owned(),
                        0x0c002ff4 => match self.get_register_by_index(4) {
                            0x18 => "CMD_GETTOC".to_owned(),
                            _ => "CMD_???".to_owned()
                        },
                        0x0c001c34 | 0x8c001ca8 => "gd_get_toc".to_owned(),
                        0x0c003570 => "gd_rom_cmd_processor_thing()".to_owned(),
                        0x0c0011ec => format!("gd_do_cmd({:08x})", self.get_register_by_index(6)),
                        0x0c0029a8 => "cdrom_read_loop()".to_owned(),
                        0x0c000800 => format!("sysDoBiosCall({})", self.get_register_by_index(4) as i32),
                        0x0c003450 => "gdc_reset()".to_owned(),
                        0x0c001890 => format!("gdc_init_system() sr {:08x} imask {:08x}", self.get_sr(), (self.get_sr() & 0xF0) >> 4),
                        0x0c000420 => "boot3()".to_owned(),
                        0x0c000ae4 => "boot4()".to_owned(),
                        0x0c002b4c => "dispatch_gdrom_cmd() ??".to_owned(),
                        0x0c002ba0 => "gdrom_cmd successful?".to_owned(),
                        0x0c000990 => "syBtCheckDisk".to_owned(),
                        0x0c0002c8 => "syBtExit()".to_owned(),
                        0x0c000820 => "boot5()".to_owned(),
                        0x0c000772 => "wait_timer()".to_owned(),
                        0x0c00095c => "check_gdrive_stat()".to_owned(),
                        0x0c000d02 => "check_disc()".to_owned(),
                        0x0c00cb2a => "wait_for_new_frame".to_owned(),
                        0x0c184000 => "playing_anim".to_owned(),
                        0x0c00ca78 => format!("bios_song_and_dance({}, {}, {})", self.get_register_by_index(4), self.get_register_by_index(5), self.get_register_by_index(6)),
                        0x0c00c000 => format!("bios_anim_maybe({:08x})", self.get_register_by_index(4)),
                        0x0c00c040 => format!("abs_fn({}, {}, {})", self.get_register_by_index(4), self.get_register_by_index(5), self.get_register_by_index(6)),
                        _ => "".to_owned()
                    };

                    if subroutine != "" {
                        println!("bios: {} @ cyc {}", subroutine, cyc);
                    }
                }

                // KOS symbol mapping to help with debugging
                #[cfg(feature = "log_kos")]
                if let Some(sym) = self.symbols_map.get(&(self.registers.current_pc & 0x1FFFFFFF)) {
                    println!("{:08x}: calling {} @ cyc {}", self.registers.current_pc, sym, cyc);
                }

                // execute the decoded instruction
                (decoded.func)(self, decoded, bus, context);
                
             //   #[cfg(feature = "log_instrs2")]
               // writeln!(lock, "{:08x} {:04x}, {}", opcode, self.registers.current_pc, decoded.disassembly).unwrap();
            } else {
                println!(
                    "cpu: unimplemented opcode {:04x} @ pc {:08x} after {} instructions",
                    opcode, self.registers.current_pc, cyc / 8
                );

                self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
            }
        }
    }

    pub fn step(&mut self, bus: &mut CpuBus, context: &mut Context, cyc: u64) {
        self.exec_next_opcode(bus, context, cyc);
    }

    pub fn delay_slot(&mut self,bus: &mut CpuBus, context: &mut Context) {
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
        self.exec_next_opcode(bus, context, self.cyc);   
    }

    fn clrt(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        self.set_sr(self.get_sr().clear_bit(0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmac(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        unsafe { 
            let rn_idx = instruction.rn.unwrap();
            let res = self.get_fpu_register_by_index(rn_idx).f as u64 + (self.get_fpu_register_by_index(0).f as u64 * self.get_fpu_register_by_index(rn_idx).f as u64);
            self.set_fpu_register_by_index(rn_idx, Float32 { f: res as f32 });
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fipr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap() & 0xC;
        let n = rn_idx as i64;
        let m=((rn_idx & 0x3) << 2) as i64;
        unsafe {
            let mut idp = self.get_fpu_register_by_index((n+0) as usize).f * self.get_fpu_register_by_index((m+0) as usize).f;
            idp += self.get_fpu_register_by_index((n+1) as usize).f * self.get_fpu_register_by_index((m+1) as usize).f;
            idp += self.get_fpu_register_by_index((n+2) as usize).f * self.get_fpu_register_by_index((m+2) as usize).f;
            idp += self.get_fpu_register_by_index((n+3) as usize).f * self.get_fpu_register_by_index((m+3) as usize).f;

            self.set_fpu_register_by_index((n+3) as usize, Float32 { f: idp });
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }
    fn rotcl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let sr = self.get_sr();
        let mut rn = self.get_register_by_index(rn_idx);
        let temp = if (rn & 0x80000000) == 0 { 0 } else { 1 };

        rn <<= 1;

        if sr.check_bit(0) {
            rn |= 0x00000001;
        } else {
            rn &= 0xFFFFFFFE;
        }

        self.set_sr(sr.eval_bit(0, temp == 1));
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_rmbank(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);

        bus.write_32(rn, self.get_banked_register_by_index(rm_idx &  0x7), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stc_rmbank(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        
        self.set_register_by_index(rn_idx, self.get_banked_register_by_index(rm_idx & 0x7));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }


    fn ldcm_rmbank(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        self.set_banked_register_by_index(rn_idx & 0x7, bus.read_32(rm, context));
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fsqrt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        unsafe { 
            let rn_idx = instruction.rn.unwrap();
            let rn = self.get_fpu_register_by_index(rn_idx).f;
            self.set_fpu_register_by_index(rn_idx, Float32 { f: f32::sqrt(rn) })
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fldi1(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = instruction.rn.unwrap();
        self.set_fpu_register_by_index(rn_idx, Float32 { f: 1.0 });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fldi0(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = instruction.rn.unwrap();
        self.set_fpu_register_by_index(rn_idx, Float32 { f: 0.0 });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn orm(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let r0 = self.get_register_by_index(0);
        let mut temp = bus.read_8(self.get_gbr() + r0, context) as i32;
        temp |= 0x000000FF & instruction.imm.unwrap() as i32;
        bus.write_8(self.get_gbr() + r0, temp as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn rotcr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
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

    fn subc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let sr = self.get_sr();

        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let mut tmp0 = 0_u64;
        let mut tmp1 = 0_u64;

        tmp1 = rn.wrapping_sub(rm) as u64;
        tmp0 = rn as u64;
        rn = (tmp1.wrapping_sub((if sr.check_bit(0) { 1 } else { 0 })) as u32);

        self.set_sr(sr.eval_bit(0, tmp0 < tmp1));

        if tmp1 < rn as u64 {
            self.set_sr(self.get_sr().set_bit(0));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn macl(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
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

    fn addc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let sr = self.get_sr();

        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let tmp0 = rn;
        let tmp1 = rn.wrapping_add(rm);
        rn = (tmp1 + (if sr.check_bit(0) { 1 } else { 0 })) as u32;

        self.set_sr(sr.eval_bit(0, tmp0 > tmp1));

        if tmp1 > rn as u32 {
            self.set_sr(self.get_sr().set_bit(0));
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let sr = self.get_sr();
        let rn = if sr.check_bit(0) {
            0x00000001
        } else {
            0x00000000
        };

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sleep(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        self.state = CpuState::Sleeping;
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn div1(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let sr = self.get_sr();

        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let mut q = sr.check_bit(8);
        let old_q = q;

        let m = sr.check_bit(9);
        let mut t = sr.check_bit(0);

        let tmp0 = rn;
        let tmp2 = rm;
        let mut tmp1: bool = false;

        q = (0x80000000 & rn) != 0;
        rn <<= 1;
        rn |= if t { 1 } else { 0 };

        if !old_q {
            if !m {
                rn = rn.wrapping_sub(tmp2);
                tmp1 = rn > tmp0;

                if !q {
                    q = tmp1;
                } else {
                    q = tmp1 == false;
                }
            } else {
                rn = rn.wrapping_add(tmp2);
                tmp1 = rn < tmp0;

                if !q {
                    q = tmp1 == false;
                } else {
                    q = tmp1;
                }
            }
        } else {
            if !m {
                rn = rn.wrapping_add(tmp2);
                tmp1 = rn < tmp0;

                if !q {
                    q = tmp1;
                } else {
                    q = tmp1 == false;
                }
            } else {
                rn = rn.wrapping_sub(tmp2);
                tmp1 = rn > tmp0;

                if !q {
                    q = tmp1 == false;
                } else {
                    q = tmp1;
                }
            }
        }

        t = q == m;

        self.set_sr(sr.eval_bit(0, t).eval_bit(8, q).eval_bit(9, m));
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn extsw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = self.get_register_by_index(rn_idx);

        rn = rm;

        if (rm & 0x00008000) == 0 {
            rn &= 0x0000FFFF;
        } else {
            rn |= 0xFFFF0000;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsfpul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_fpul(Float32 { u: rm });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn frchg(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));
        self.set_fpscr(self.get_fpscr().toggle_bit(21));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movlsg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = (0x000000FF & instruction.displacement.unwrap() as i64) as u32;
        let r0 = self.get_register_by_index(0);
        bus.write_32(self.get_gbr() + (disp << 2), r0, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn float_single(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));
        let rn_idx = instruction.rn.unwrap();

        let fpul = self.get_fpul();
        unsafe { 
            self.set_fpu_register_by_index(
                rn_idx,
                Float32 { f: fpul.u as i32 as f32 },
            );
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fadd(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rn = self.get_fpu_register_by_index(rn_idx);
        let rm = self.get_fpu_register_by_index(rm_idx);
        unsafe {
            self.set_fpu_register_by_index(
                rn_idx,
                Float32 {
                    f: rn.f + rm.f,
                },
            )
        };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fsub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        
        let rn = self.get_fpu_register_by_index(rn_idx);
        let rm = self.get_fpu_register_by_index(rm_idx);
        unsafe {
            self.set_fpu_register_by_index(
                rn_idx,
                Float32 {
                    f: rn.f - rm.f,
                },
            )
        };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fdiv(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rn = self.get_fpu_register_by_index(rn_idx);
        let rm = self.get_fpu_register_by_index(rm_idx);
        unsafe { self.set_fpu_register_by_index(rn_idx, Float32 { f: rn.f / rm.f }) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rn = self.get_fpu_register_by_index(rn_idx);
        let rm = self.get_fpu_register_by_index(rm_idx);

        unsafe { self.set_fpu_register_by_index(rn_idx, Float32 { f: rn.f * rm.f }) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ftrc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_fpu_register_by_index(rm_idx);
        unsafe { self.set_fpul(Float32 { u : rm.f as i32 as u32 }) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fsca(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));
    
        let rn_idx = instruction.rn.unwrap();
        let fraction = unsafe { self.get_fpul().u as u32 & 0x0000_FFFF };
        let offset = 0x0001_0000;
        let angle = 2.0 * std::f64::consts::PI * (fraction as f64) / (offset as f64);
        self.set_fpu_register_by_index(rn_idx, Float32 { f: f32::sin(angle as f32) });
        self.set_fpu_register_by_index(rn_idx + 1, Float32 { f: f32::cos(angle as f32) });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }
    

    fn fmov_load(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
       // assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let value = bus.read_32(rm, context);

        self.set_fpu_register_by_index(rn_idx, Float32 { u: value });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov_index_load(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let value = bus.read_32(self.get_register_by_index(0).wrapping_add(rm), context);

        self.set_fpu_register_by_index(rn_idx, Float32 { u: value });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov_index_store(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let frm = self.get_fpu_register_by_index(rm_idx);

        #[cfg(feature = "log_instrs")]
        println!(
            "{:08x}: fmov.s fr{}, (r0+r{})",
            self.registers.current_pc, rm_idx, rn_idx
        );

        unsafe {
            bus.write_32(self.get_register_by_index(0).wrapping_add(rn), frm.u, context)
        };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_fpu_register_by_index(rm_idx);
        self.set_fpu_register_by_index(rn_idx, rm);

        if self.get_fpscr().check_bit(20) {
            let rm = self.get_fpu_register_by_index(rm_idx + 1);
            self.set_fpu_register_by_index(rn_idx + 1, rm);
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov_store(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
       // assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_fpu_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);
        unsafe { bus.write_32(rn, rm.u, context) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov_save(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        if self.get_fpscr().check_bit(20) {
            let rn = self.get_register_by_index(rn_idx).wrapping_sub(8);
            let rm_lo = self.get_fpu_register_by_index(rm_idx);
            let rm_hi = self.get_fpu_register_by_index(rm_idx + 1);
            
            unsafe { bus.write_64(rn, (((rm_lo.u as u64) << 32) | rm_hi.u as u64) as u64, context) };
            self.set_register_by_index(rn_idx, rn);
            
        } else {
            let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
            let rm = self.get_fpu_register_by_index(rm_idx);
            
            unsafe { bus.write_32(rn, rm.u, context) };
            self.set_register_by_index(rn_idx, rn);    
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmpstr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        let temp = rn ^ rm;
        let mut hh = (temp & 0xFF000000) >> 24;
        let hl = (temp & 0x00FF0000) >> 16;
        let lh = (temp & 0x0000FF00) >> 8;
        let ll = temp & 0x000000FF;
        hh = if (hh != 0 && hl != 0 && lh !=0 && ll != 0) { 1 } else { 0 };

        self.set_sr(self.get_sr().eval_bit(
            0,
            hh == 0,
        ));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmppl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn as i32) > 0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmphi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn > rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmphieq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn >= rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmpeq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn == rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmpge(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn as i32 >= rm as i32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmpgt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn as i32 > rm as i32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fcmpgt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_fpu_register_by_index(rm_idx);
        let rn = self.get_fpu_register_by_index(rn_idx);

        unsafe { self.set_sr(self.get_sr().eval_bit(0, rn.f > rm.f )) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fcmpeq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_fpu_register_by_index(rm_idx);
        let rn = self.get_fpu_register_by_index(rn_idx);

        unsafe { self.set_sr(self.get_sr().eval_bit(0, rn.f == rm.f )) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmpimm(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap();
        let r0 = self.get_register_by_index(0);
        let imm = if (imm & 0x80) == 0 {
            0x000000FF & (imm as i32 as u32)
        } else {
            0xFFFFFF00 | imm as i32 as u32
        };

        self.set_sr(self.get_sr().eval_bit(0, imm as u32 == r0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn cmppz(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, rn as i32 >= 0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldspr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);

        self.set_pr(rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stsmpr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let mut rn = self.get_register_by_index(rn_idx);

        #[cfg(feature = "log_instrs")]
        println!(
            "{:08x}: stsmpr pr, (r{} - 4)",
            self.registers.current_pc, rn_idx
        );

        rn = rn.wrapping_sub(4);

        bus.write_32(rn, self.get_pr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn nop(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn dmulu(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);

        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);

        let rnl = rn & 0x0000FFFF;
        let rnh = (rn >> 16) & 0x0000FFFF;

        let rml = rm & 0x0000FFFF;
        let rmh = (rm >> 16) & 0x0000FFFF;

        let temp0 = rml * rnl;
        let mut temp1 = rmh * rnl;
        let temp2 = rml * rnh;
        let temp3 = rmh * rnh;

        let mut res2 = 0;
        let res1 = temp1 + temp2;

        if res1 < temp1 {
            res2 += 0x0001000;
        }

        temp1 = (res1 << 16) & 0xffff0000;
        let res0 = temp0.wrapping_add(temp1);

        if res0 < temp0 {
            res2 += 1;
        }

        res2 = res2 + ((res1 >> 16) & 0x0000ffff) + temp3;

        self.set_mach(res2);
        self.set_macl(res0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fschg(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));
        self.set_fpscr(self.get_fpscr() ^ 0x00100000);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fsts(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        unsafe { self.set_fpu_register_by_index(rn_idx, Float32 { f: self.get_fpul().f }) };
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn dt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(1);
        
        self.set_sr(self.get_sr().eval_bit(0, rn == 0));
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn rotr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let mut rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x00000001) != 0));

        rn >>= 1;

        if (self.get_sr().check_bit(0)) {
            rn |= 0x80000000;
        } else {
            rn &= 0x7FFFFFFF;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shar(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let mut rn = self.get_register_by_index(rn_idx);
        self.set_sr(self.get_sr().eval_bit(0, (rn & 1) != 0));

        rn = (rn >> 1) | (rn & 0x80000000);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn addi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap() as u32;
        let imm = if ((imm & 0x80) == 0) {
            0x000000FF & (imm as i32 as u32)
        } else {
            0xFFFFFF00 | imm as i32 as u32
        };

        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        self.set_register_by_index(rn_idx, rn.wrapping_add(imm as u32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }


    fn xori(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap() as u32;
        let imm = if (imm & 0x80) == 0 {
            0x000000FF & (imm as i32 as u32)
        } else {
            0xFFFFFF00 | imm as i32 as u32
        };

        let rn = self.get_register_by_index(0);
        self.set_register_by_index(0, rn ^ imm as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn add(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        self.set_register_by_index(rn_idx, rn.wrapping_add(rm as u32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        self.set_register_by_index(rn_idx, rn.wrapping_sub(rm as u32));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn negc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

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

    fn neg(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm = instruction.rm.unwrap();
        let rn = instruction.rn.unwrap();

        self.set_register_by_index(
            rn as usize,
            0_u32.wrapping_sub(self.get_register_by_index(rm as usize)),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fneg(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn = instruction.rn.unwrap();
        
        unsafe {
            self.set_fpu_register_by_index(
                rn as usize,
                Float32 {
                    u: self.get_fpu_register_by_index(rn as usize).u ^ 0x80000000
                },
            )
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn extub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rm_idx) & 0x000000FF;
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn extuw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rm_idx) & 0x0000FFFF;
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn extsb(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
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

    fn xor(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm = instruction.rm.unwrap();
        let rn = instruction.rn.unwrap();

        self.set_register_by_index(
            rn as usize,
            self.get_register_by_index(rn as usize) ^ self.get_register_by_index(rm as usize),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn not(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm = instruction.rm.unwrap();
        let rn = instruction.rn.unwrap();

        self.set_register_by_index(rn as usize, !self.get_register_by_index(rm as usize));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ori(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap();
        let mut r0 = self.get_register_by_index(0);
        r0 |= 0x000000FF & (imm as i32 as u32);
        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn and(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        self.set_register_by_index(rn_idx, rn & rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn or(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        self.set_register_by_index(rn_idx, rn | rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn lds_fpscr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_fpscr(rm & 0x003FFFFF);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn fmov_restore(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        if self.get_fpscr().check_bit(20) {
            let frn = bus.read_64(rm, context);
            self.set_fpu_register_by_index(rn_idx + 0, Float32 { u: frn as u32 });
            self.set_fpu_register_by_index(rn_idx + 1, Float32 { u: ((frn & 0xffffffff00000000) >> 32) as u32 });
            self.set_register_by_index(rm_idx, rm.wrapping_add(8));
        } else {
            let frn = bus.read_32(rm, context);
            self.set_register_by_index(rm_idx, rm);
            self.set_fpu_register_by_index(rn_idx, Float32 { u: frn });  
            self.set_register_by_index(rm_idx, rm.wrapping_add(4));  
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn andi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap();
        let mut r0 = self.get_register_by_index(0);
        r0 &= 0x000000FF & (imm as i32 as u32);
        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movws4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as i32;
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        bus.write_16(
            rn.wrapping_add((disp << 1) as u32),
            self.get_register_by_index(0) as u16,
            context
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movws0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        let r0 = self.get_register_by_index(0);

        bus.write_16(rn.wrapping_add(r0), rm as u16, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movwsg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x000000FF & instruction.displacement.unwrap();
        bus.write_16(
            self.get_gbr().wrapping_add((disp << 1) as u32),
            self.get_register_by_index(0) as u16,
            context
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movws(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        bus.write_16(rn, rm as u16, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movwl4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as i32;
        let rm_idx = instruction.rm.unwrap();
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

    fn movwlg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as u32;
        let mut r0 = bus.read_16(self.get_gbr() + (disp << 1), false, context) as u32;

        if (r0 & 0x8000) == 0 {
            r0 &= 0x0000FFFF;
        }
        else { 
            r0 |= 0xFFFF0000;
        }

        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbsg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = (0x000000FF & instruction.displacement.unwrap()) as u32;
        let r0 = self.get_register_by_index(0);
        bus.write_8(self.get_gbr().wrapping_add(disp), r0 as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movblg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = (0x000000FF & instruction.displacement.unwrap()) as u32;
        let mut r0 = bus.read_8(self.get_gbr().wrapping_add(disp), context) as u32;
        if (r0 & 0x80) == 0 {
            r0 &= 0x000000ff;
        } else {
            r0 |= 0xff000000;
        }

        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movwp(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
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

    fn movwm(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(2);

        bus.write_16(rn, rm as u16, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movwi(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x000000FF & instruction.displacement.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let mut rn = bus.read_16(
            self.registers
                .current_pc
                .wrapping_add(4 + (disp << 1) as u32),
            false,
            context
        ) as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbl(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_8(rm, context) as u32;

        if (rn & 0x80) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbm(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(1);

        bus.write_8(rn, rm as u8, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movllg(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = (0x000000FF & instruction.displacement.unwrap()) as u32;
        let r0 = bus.read_32(self.get_gbr().wrapping_add(disp << 2), context);
        self.set_register_by_index(0, r0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movcal(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let r0 = self.get_register_by_index(0);
        let rn = self.get_register_by_index(rn_idx);

        bus.write_32(rn, r0, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movll(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = bus.read_32(rm, context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movll0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = bus.read_32(rm.wrapping_add(self.get_register_by_index(0)), context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn mova(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let disp = 0x000000ff & instruction.displacement.unwrap() as u32;
        let val = (self.registers.current_pc & 0xfffffffc).wrapping_add(4 + (disp << 2) as u32);
        self.set_register_by_index(0, val);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbp(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();


        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_8(rm, context) as u32;

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

    fn movbl0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let r0 = self.get_register_by_index(0);
        let mut rn = bus.read_8(r0.wrapping_add(rm), context) as u32;

        if (rn & 0x80) == 0 {
            rn &= 0x000000FF;
        } else {
            rn |= 0xFFFFFF00;
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn tas(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);

        let mut temp = bus.read_8(rn, context);
        self.set_sr(self.get_sr().eval_bit(0, temp == 0));
        temp |= 0x00000080;
        bus.write_8(rn, temp, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stsm_fpscr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_fpscr() & 0x003FFFFF, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stsmmach(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_mach(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stsmmacl(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_macl(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbl4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as i32;
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let mut r0 = bus.read_8(rm.wrapping_add(disp as u32), context) as u32;

        if (r0 & 0x80) == 0 {
            r0 &= 0x000000ff;
        } else {
            r0 |= 0xffffff00;
        }

        self.set_register_by_index(0, r0 as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movwl(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
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

    fn movwl0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rm = self.get_register_by_index(rm_idx);
        let mut rn = bus.read_16(rm.wrapping_add(self.get_register_by_index(0)), false, context)
            as u32;

        if (rn & 0x8000) == 0 {
            rn &= 0x0000ffff;
        } else {
            rn |= 0xffff0000;
        }

        self.set_register_by_index(rn_idx, rn as u32);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbs(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        bus.write_8(rn, rm as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbs0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);
        bus.write_8(rn.wrapping_add(self.get_register_by_index(0)), rm as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movbs4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as i32;
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);
        let addr = rn.wrapping_add(disp as u32);
        bus.write_8(addr, self.get_register_by_index(0) as u8, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movll4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000F & instruction.displacement.unwrap() as i32;
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let rn = bus.read_32(
            self.get_register_by_index(rm_idx)
                .wrapping_add((disp << 2) as u32),
            context
        );

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movls4(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x0000000f & instruction.displacement.unwrap() as i32;
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        bus.write_32(
            self.get_register_by_index(rn_idx)
                .wrapping_add((disp << 2) as u32),
            self.get_register_by_index(rm_idx),
            context
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movls0(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        bus.write_32(
            self.get_register_by_index(rn_idx)
                .wrapping_add(self.get_register_by_index(0)),
            self.get_register_by_index(rm_idx),
            context
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shll(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x80000000) != 0));
        self.shift_logical(rn_idx, 1, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shld(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
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

    fn shad(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let mut rn = self.get_register_by_index(rn_idx);
        let sgn = rm & 0x80000000;

        if sgn == 0 {
            rn <<= rm & 0x1F;
        } else if (rm & 0x1F) == 0 {
            if ((rn & 0x80000000) == 0) {
                rn = 0;
            } else {
                rn = 0xFFFFFFFF;
            }
        } else {
            rn = rn >> ((!rm & 0x1F) + 1);
        }

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shll2(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 2, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shll8(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 8, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shll16(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 16, ShiftDirection::Left);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shlr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, (rn & 0x00000001) != 0));
        self.shift_logical(rn_idx, 1, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shlr2(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 2, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shlr8(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 8, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shlr16(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn = instruction.rn.unwrap();
        self.shift_logical(rn, 16, ShiftDirection::Right);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn swapw(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = ((rm & 0xFFFF0000) >> 16) | ((rm & 0xFFFF) << 16);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn swapb(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let temp0 = rm & 0xFFFF0000;
        let temp1 = (rm & 0x000000FF) << 8;
        let mut rn = (rm & 0x0000FF00) >> 8;
        rn = rn | temp1 | temp0;

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stc_sr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_sr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stc_gbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_gbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stc_vbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_vbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stc_dbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_dbr());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldc_sr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);

        self.set_sr(rm & 0x700083F3);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldc_gbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_gbr(rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldc_vbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_vbr(rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldc_dbr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_dbr(rm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldcm_dbr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_dbr(bus.read_32(rm, context));

        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldcm_vbr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);

        let val = bus.read_32(rm, context);
        self.set_vbr(val);
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldcm_spc(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);

        let val = bus.read_32(rm, context);
        self.set_spc(val);

        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_ssr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_ssr(), context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_fpul(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        unsafe { bus.write_32(rn, self.get_fpul().u, context)};

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_sr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_sr(), context);

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_gbr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_gbr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_vbr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_vbr(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn stcm_spc(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        bus.write_32(rn, self.get_spc(), context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldcm_ssr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_ssr(bus.read_32(rm, context));

        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldcm_sr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_sr(bus.read_32(rm, context) & 0x700083F3);
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_pr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let pr = bus.read_32(rm, context);
        self.set_pr(pr);

        
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_mach(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_mach(bus.read_32(rm, context));
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_gbr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_gbr(bus.read_32(rm, context));

        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_macl(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_macl(bus.read_32(rm, context));

        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_fpscr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_fpscr(bus.read_32(rm, context));
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn ldsm_fpul(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_fpul(Float32 { u: bus.read_32(rm, context) });
        self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sts_fpscr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_fpscr() & 0x003FFFFF);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sts_macl(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_macl());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sts_mach(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_mach());
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sts_fpul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, unsafe { self.get_fpul().u });
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn jmp(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.delay_slot(bus, context);
        self.registers.current_pc = rm;
    }

    fn jsr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_pr(self.registers.current_pc + 4);
        self.delay_slot(bus, context);
        self.registers.current_pc = rm;
    }

    fn rts(&mut self, _: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let pr = self.get_pr();
        self.delay_slot(bus, context);
        self.registers.current_pc = pr;
    }

    fn rte(&mut self, _: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let spc = self.get_spc();
        let ssr = self.get_ssr();
        self.set_sr(ssr);
        self.delay_slot(bus, context);
       // println!("{:08x} rte returning to {:08x} @ cyc {}", self.registers.current_pc, spc, self.cyc);
        self.registers.current_pc = spc;
    }

    fn braf(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let pc = self.registers.current_pc.wrapping_add(4 + rm as u32);
        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    fn bra(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let mut disp = instruction.displacement.unwrap() as i32 as u32;
        if (disp & 0x800) == 0 {
            disp = 0x00000FFF & disp;
        } else {
            disp = 0xFFFFF000 | disp;
        }

        let pc = self.registers.current_pc.wrapping_add(4 + (disp << 1) as u32);;

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    fn bsrf(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        self.set_pr(self.registers.current_pc.wrapping_add(4));
        let rm = self.get_register_by_index(rm_idx);
        let pc = self.registers.current_pc.wrapping_add(4 + rm as u32);

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    fn bsr(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let mut disp = instruction.displacement.unwrap() as i32 as u32;
        if (disp & 0x800) == 0 {
            disp = 0x00000FFF & disp;
        } else {
            disp = 0xFFFFF000 | disp;
        }

        self.set_pr(self.registers.current_pc + 4);
        let pc = self.registers.current_pc.wrapping_add(4 + (disp << 1) as u32);

        self.delay_slot(bus, context);
        self.registers.current_pc = pc;
    }

    fn branch_if_true(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let d = instruction.displacement.unwrap() as i32 as u32;
        let disp = if (d & 0x80) == 0 {
            0x000000FF & d
        } else {
            0xFFFFFF00 | d
        };

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

    fn branch_if_false(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let mut disp = instruction.displacement.unwrap() as i32 as u32;
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

    fn branch_if_false_delayed(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let mut disp = instruction.displacement.unwrap() as i32 as u32;
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

    fn div0u(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let sr = self.get_sr();
        self.set_sr(sr.clear_bit(0).clear_bit(8).clear_bit(9));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn div0s(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let mut sr = self.get_sr();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        sr = sr.eval_bit(8, (rn & 0x80000000) != 0);
        sr = sr.eval_bit(9, (rm & 0x80000000) != 0);
        sr = sr.eval_bit(0, !sr.check_bit(8) == sr.check_bit(9));

        self.set_sr(sr);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn branch_if_true_delayed(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let mut disp = instruction.displacement.unwrap() as i32 as u32;
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

    fn pref(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rn_idx = instruction.rn.unwrap();
        let addr = self.get_register_by_index(rn_idx) & 0xffffffe0;

        if addr >= 0xe0000000 && addr <= 0xe3ffffff {
            let sq = addr.check_bit(5);
            let sq_base = if sq { bus.ccn.registers.qacr1 } else { bus.ccn.registers.qacr0 };
            let mut ext_addr = (addr & 0x03ffffe0) | ((sq_base & 0x1c) << 24);
            let sq_idx = if sq { 1 } else { 0 };

            for i in 0..8 {
                bus.write_32(ext_addr, bus.store_queues[sq_idx][i], context);
                ext_addr += 4;
            };
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn tst(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        self.set_sr(self.get_sr().eval_bit(
            0,
            self.get_register_by_index(rn_idx) & self.get_register_by_index(rm_idx) == 0,
        ));

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn tsti(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = 0x000000ff & instruction.imm.unwrap() as i32;
        self.set_sr(
            self.get_sr()
                .eval_bit(0, self.get_register_by_index(0) & imm as u32 == 0),
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn sett(&mut self, _: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        self.set_sr(self.get_sr().set_bit(0));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn mov(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        self.set_register_by_index(rn_idx, self.get_register_by_index(rm_idx));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movi(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let imm = instruction.imm.unwrap();
        let rn_idx = instruction.rn.unwrap();

        let imm = if (imm & 0x80) == 0 {
            0x000000FF & imm
        } else {
            0xFFFFFF00 | imm
        };

        self.set_register_by_index(rn_idx as usize, imm);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movlm(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
        let rm = self.get_register_by_index(rm_idx);

        bus.write_32(rn, rm, context);
        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movlp(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        self.set_register_by_index(rn_idx, bus.read_32(rm, context));

        if rm_idx != rn_idx {
            self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movli(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let disp = 0x000000FF & instruction.displacement.unwrap() as u32;
        let rn_idx = instruction.rn.unwrap();
        let addr = (self.registers.current_pc & 0xfffffffc).wrapping_add(4 + (disp << 2) as u32);
        self.set_register_by_index(rn_idx as usize, bus.read_32(addr, context));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn movls(&mut self, instruction: &DecodedInstruction, bus: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);
        bus.write_32(rn, rm, context);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn xtrct(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let mut rn = self.get_register_by_index(rn_idx);
        let rm = self.get_register_by_index(rm_idx);

        let high = (rm << 16) & 0xFFFF0000;
        let low = (rn >> 16) & 0x0000FFFF;
        rn = high | low;

        self.set_register_by_index(rn_idx, rn);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn mul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm_idx = instruction.rm.unwrap();
        let rn_idx = instruction.rn.unwrap();
        let rm = self.get_register_by_index(rm_idx);
        let rn = self.get_register_by_index(rn_idx);

        let result = ((rn as i32).wrapping_mul(rm as i32)) as u32;
        self.set_macl(result);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn muls(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm = instruction.rm.unwrap();
        let rn = instruction.rn.unwrap();
        let result = self.get_register_by_index(rn) as i16 as u32
            * self.get_register_by_index(rm) as i16 as u32;
        self.set_macl(result);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn mulu(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, context: &mut Context) {
        let rm = instruction.rm.unwrap();
        let rn = instruction.rn.unwrap();
        let result = self.get_register_by_index(rn) as u16 as u32
            * self.get_register_by_index(rm) as u16 as u32;
        self.set_macl(result);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    fn shift_logical(&mut self, rn: usize, amount: i32, shift_direction: ShiftDirection) {
        let val = self.get_register_by_index(rn as usize);

        let shifted = if shift_direction == ShiftDirection::Left {
            val << amount
        } else {
            val >> amount
        };

        self.set_register_by_index(rn as usize, shifted);
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct DecodedInstruction {
    rm: Option<usize>,
    rn: Option<usize>,
    imm: Option<u32>,
    displacement: Option<i32>,
    func: InstructionHandler,
    disassembly: String
}

type InstructionHandler = fn(&mut Cpu, &DecodedInstruction, &mut CpuBus, &mut Context) -> ();
type DisassemblyHandler = fn(&mut Cpu) -> String;


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShiftDirection {
    Left,
    Right,
}
