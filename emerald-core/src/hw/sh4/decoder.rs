use super::{bus::CpuBus, cpu::Cpu};
use crate::context::Context;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct InstructionOpcode(pub u16);

impl InstructionOpcode {
    #[inline]
    pub fn m(&self) -> usize {
        ((self.0 >> 4) & 0xF) as usize
    }

    #[inline]
    pub fn d4(&self) -> usize {
        (self.0 & 0xF) as usize
    }

    #[inline]
    pub fn d8(&self) -> u32 {
        (self.0 & 0xFF) as u32
    }

    #[inline]
    pub fn d12(&self) -> u32 {
        (self.0 & 0xFFF) as u32
    }

    #[inline]
    pub fn n(&self) -> usize {
        ((self.0 >> 8) & 0xF) as usize
    }
}

#[derive(Clone, Copy)]
pub struct DecodedInstruction {
    pub opcode: InstructionOpcode,
    pub disassembly: &'static str,
    pub handler: InstructionHandler,
}

type InstructionHandler = fn(&mut Cpu, &DecodedInstruction, &mut CpuBus, &mut Context) -> ();

pub fn build_opcode_lut() -> Vec<DecodedInstruction> {
    let mut lut = vec![
        DecodedInstruction {
            opcode: InstructionOpcode(0),
            disassembly: "unk",
            handler: super::cpu::Cpu::unk,
        };
        u16::MAX as usize + 1
    ];

    let instructions = [
        (
            0b0110000000000011,
            0b0000111111110000,
            "mov Rm, Rn",
            super::cpu::Cpu::mov as InstructionHandler,
        ),
        (
            0b1110000000000000,
            0b0000111111111111,
            "movi #imm, Rn",
            super::cpu::Cpu::movi as InstructionHandler,
        ),
        (
            0b1100011100000000,
            0b0000000011111111,
            "mova @(disp8, PC), r0",
            super::cpu::Cpu::mova as InstructionHandler,
        ),
        (
            0b1001000000000000,
            0b0000111111111111,
            "movwi",
            super::cpu::Cpu::movwi as InstructionHandler,
        ),
        (
            0b1101000000000000,
            0b0000111111111111,
            "mov.l @(disp8+PC), Rn",
            super::cpu::Cpu::movli as InstructionHandler,
        ),
        (
            0b0110000000000000,
            0b0000111111110000,
            "movbl",
            super::cpu::Cpu::movbl as InstructionHandler,
        ),
        (
            0b0110000000000001,
            0b0000111111110000,
            "movwl",
            super::cpu::Cpu::movwl as InstructionHandler,
        ),
        (
            0b0110000000000010,
            0b0000111111110000,
            "movll",
            super::cpu::Cpu::movll as InstructionHandler,
        ),
        (
            0b0010000000000000,
            0b0000111111110000,
            "movbs",
            super::cpu::Cpu::movbs as InstructionHandler,
        ),
        (
            0b0010000000000001,
            0b0000111111110000,
            "movws",
            super::cpu::Cpu::movws as InstructionHandler,
        ),
        (
            0b0010000000000010,
            0b0000111111110000,
            "mov.l Rm, @Rn",
            super::cpu::Cpu::movls as InstructionHandler,
        ),
        (
            0b0110000000000100,
            0b0000111111110000,
            "mov.b @Rm+, Rn",
            super::cpu::Cpu::movbp as InstructionHandler,
        ),
        (
            0b0110000000000101,
            0b0000111111110000,
            "movwp",
            super::cpu::Cpu::movwp as InstructionHandler,
        ),
        (
            0b0110000000000110,
            0b0000111111110000,
            "mov.l @Rm+, Rn",
            super::cpu::Cpu::movlp as InstructionHandler,
        ),
        (
            0b0010000000000100,
            0b0000111111110000,
            "movbm",
            super::cpu::Cpu::movbm as InstructionHandler,
        ),
        (
            0b0010000000000101,
            0b0000111111110000,
            "movwm",
            super::cpu::Cpu::movwm as InstructionHandler,
        ),
        (
            0b0010000000000110,
            0b0000111111110000,
            "movlm",
            super::cpu::Cpu::movlm as InstructionHandler,
        ),
        (
            0b1000010000000000,
            0b0000000011111111,
            "mov.b @(disp4, Rm), r0",
            super::cpu::Cpu::movbl4 as InstructionHandler,
        ),
        (
            0b1000010100000000,
            0b0000000011111111,
            "movwl4",
            super::cpu::Cpu::movwl4 as InstructionHandler,
        ),
        (
            0b0101000000000000,
            0b0000111111111111,
            "movll4",
            super::cpu::Cpu::movll4 as InstructionHandler,
        ),
        (
            0b1000000000000000,
            0b0000000011111111,
            "movbs4",
            super::cpu::Cpu::movbs4 as InstructionHandler,
        ),
        (
            0b1000000100000000,
            0b0000000011111111,
            "movws4",
            super::cpu::Cpu::movws4 as InstructionHandler,
        ),
        (
            0b0001000000000000,
            0b0000111111111111,
            "movls4",
            super::cpu::Cpu::movls4 as InstructionHandler,
        ),
        (
            0b0000000000001100,
            0b0000111111110000,
            "mov.b @(r0, Rm), Rn",
            super::cpu::Cpu::movbl0 as InstructionHandler,
        ),
        (
            0b0000000000001101,
            0b0000111111110000,
            "movwl0",
            super::cpu::Cpu::movwl0 as InstructionHandler,
        ),
        (
            0b0000000000001110,
            0b0000111111110000,
            "movll0",
            super::cpu::Cpu::movll0 as InstructionHandler,
        ),
        (
            0b0000000000000100,
            0b0000111111110000,
            "mov.b Rm, @(r0, Rn)",
            super::cpu::Cpu::movbs0 as InstructionHandler,
        ),
        (
            0b0000000000000101,
            0b0000111111110000,
            "movws0",
            super::cpu::Cpu::movws0 as InstructionHandler,
        ),
        (
            0b0000000000000110,
            0b0000111111110000,
            "movls0",
            super::cpu::Cpu::movls0 as InstructionHandler,
        ),
        (
            0b1100010000000000,
            0b0000000011111111,
            "movblg",
            super::cpu::Cpu::movblg as InstructionHandler,
        ),
        (
            0b1100010100000000,
            0b0000000011111111,
            "movwlg",
            super::cpu::Cpu::movwlg as InstructionHandler,
        ),
        (
            0b1100011000000000,
            0b0000000011111111,
            "mov.l @(disp8, gbr), r0",
            super::cpu::Cpu::movllg as InstructionHandler,
        ),
        (
            0b1100000000000000,
            0b0000000011111111,
            "movbsg",
            super::cpu::Cpu::movbsg as InstructionHandler,
        ),
        (
            0b1100000100000000,
            0b0000000011111111,
            "movwsg",
            super::cpu::Cpu::movwsg as InstructionHandler,
        ),
        (
            0b1100001000000000,
            0b0000000011111111,
            "movlsg",
            super::cpu::Cpu::movlsg as InstructionHandler,
        ),
        (
            0b0000000000101001,
            0b0000111100000000,
            "movt",
            super::cpu::Cpu::movt as InstructionHandler,
        ),
        (
            0b0110000000001000,
            0b0000111111110000,
            "swap.b",
            super::cpu::Cpu::swapb as InstructionHandler,
        ),
        (
            0b0110000000001001,
            0b0000111111110000,
            "swap.w",
            super::cpu::Cpu::swapw as InstructionHandler,
        ),
        (
            0b0010000000001101,
            0b0000111111110000,
            "xtrct",
            super::cpu::Cpu::xtrct as InstructionHandler,
        ),
        (
            0b0011000000001100,
            0b0000111111110000,
            "add",
            super::cpu::Cpu::add as InstructionHandler,
        ),
        (
            0b0111000000000000,
            0b0000111111111111,
            "add #imm",
            super::cpu::Cpu::addi as InstructionHandler,
        ),
        (
            0b0011000000001110,
            0b0000111111110000,
            "addc",
            super::cpu::Cpu::addc as InstructionHandler,
        ),
        (
            0b1000100000000000,
            0b0000000011111111,
            "cmp/eq #imm",
            super::cpu::Cpu::cmpimm as InstructionHandler,
        ),
        (
            0b0011000000000000,
            0b0000111111110000,
            "cmp/eq",
            super::cpu::Cpu::cmpeq as InstructionHandler,
        ),
        (
            0b0011000000000010,
            0b0000111111110000,
            "cmp/hs",
            super::cpu::Cpu::cmphieq as InstructionHandler,
        ),
        (
            0b0011000000000011,
            0b0000111111110000,
            "cmp/ge",
            super::cpu::Cpu::cmpge as InstructionHandler,
        ),
        (
            0b0011000000000110,
            0b0000111111110000,
            "cmp/hi",
            super::cpu::Cpu::cmphi as InstructionHandler,
        ),
        (
            0b0011000000000111,
            0b0000111111110000,
            "cmp/gt",
            super::cpu::Cpu::cmpgt as InstructionHandler,
        ),
        (
            0b0100000000010101,
            0b0000111100000000,
            "cmp/pl",
            super::cpu::Cpu::cmppl as InstructionHandler,
        ),
        (
            0b0100000000010001,
            0b0000111100000000,
            "cmp/pz",
            super::cpu::Cpu::cmppz as InstructionHandler,
        ),
        (
            0b0010000000001100,
            0b0000111111110000,
            "cmp/str",
            super::cpu::Cpu::cmpstr as InstructionHandler,
        ),
        (
            0b0010000000000111,
            0b0000111111110000,
            "div0s",
            super::cpu::Cpu::div0s as InstructionHandler,
        ),
        (
            0b0000000000011001,
            0b0000000000000000,
            "div0u",
            super::cpu::Cpu::div0u as InstructionHandler,
        ),
        (
            0b0011000000000100,
            0b0000111111110000,
            "div1",
            super::cpu::Cpu::div1 as InstructionHandler,
        ),
        (
            0b0011000000001101,
            0b0000111111110000,
            "dmuls.l",
            super::cpu::Cpu::dmulu2 as InstructionHandler,
        ),
        (
            0b0011000000000101,
            0b0000111111110000,
            "dmulu.l",
            super::cpu::Cpu::dmulu as InstructionHandler,
        ),
        (
            0b0100000000010000,
            0b0000111100000000,
            "dt",
            super::cpu::Cpu::dt as InstructionHandler,
        ),
        (
            0b0110000000001110,
            0b0000111111110000,
            "exts.b",
            super::cpu::Cpu::extsb as InstructionHandler,
        ),
        (
            0b0110000000001111,
            0b0000111111110000,
            "exts.w",
            super::cpu::Cpu::extsw as InstructionHandler,
        ),
        (
            0b0110000000001100,
            0b0000111111110000,
            "extu.b",
            super::cpu::Cpu::extub as InstructionHandler,
        ),
        (
            0b0110000000001101,
            0b0000111111110000,
            "extu.w",
            super::cpu::Cpu::extuw as InstructionHandler,
        ),
        (
            0b0000000000001111,
            0b0000111111110000,
            "mac.l @Rm+,@Rn+",
            super::cpu::Cpu::macl as InstructionHandler,
        ),
        (
            0b0100000000001111,
            0b0000111111110000,
            "mac.w @Rm+,@Rn+",
            super::cpu::Cpu::macw as InstructionHandler,
        ),
        (
            0b0010000000001110,
            0b0000111111110000,
            "mulu.w",
            super::cpu::Cpu::mulu as InstructionHandler,
        ),
        (
            0b0010000000001111,
            0b0000111111110000,
            "muls",
            super::cpu::Cpu::muls as InstructionHandler,
        ),
        (
            0b0000000000000111,
            0b0000111111110000,
            "mull",
            super::cpu::Cpu::mul as InstructionHandler,
        ),
        (
            0b0110000000001011,
            0b0000111111110000,
            "neg",
            super::cpu::Cpu::neg as InstructionHandler,
        ),
        (
            0b0110000000001010,
            0b0000111111110000,
            "negc",
            super::cpu::Cpu::negc as InstructionHandler,
        ),
        (
            0b0011000000001000,
            0b0000111111110000,
            "sub",
            super::cpu::Cpu::sub as InstructionHandler,
        ),
        (
            0b0011000000001010,
            0b0000111111110000,
            "subc",
            super::cpu::Cpu::subc as InstructionHandler,
        ),
        (
            0b0010000000001001,
            0b0000111111110000,
            "and",
            super::cpu::Cpu::and as InstructionHandler,
        ),
        (
            0b1100100100000000,
            0b0000000011111111,
            "and #imm",
            super::cpu::Cpu::andi as InstructionHandler,
        ),
        (
            0b0110000000000111,
            0b0000111111110000,
            "not",
            super::cpu::Cpu::not as InstructionHandler,
        ),
        (
            0b0010000000001011,
            0b0000111111110000,
            "or",
            super::cpu::Cpu::or as InstructionHandler,
        ),
        (
            0b1100101100000000,
            0b0000000011111111,
            "or #imm",
            super::cpu::Cpu::ori as InstructionHandler,
        ),
        (
            0b1100111100000000,
            0b0000000011111111,
            "orm",
            super::cpu::Cpu::orm as InstructionHandler,
        ),
        (
            0b0100000000011011,
            0b0000111100000000,
            "tas.b @Rn",
            super::cpu::Cpu::tas as InstructionHandler,
        ),
        (
            0b0010000000001000,
            0b0000111111110000,
            "tst",
            super::cpu::Cpu::tst as InstructionHandler,
        ),
        (
            0b1100100000000000,
            0b0000000011111111,
            "tst #imm",
            super::cpu::Cpu::tsti as InstructionHandler,
        ),
        (
            0b0010000000001010,
            0b0000111111110000,
            "xor",
            super::cpu::Cpu::xor as InstructionHandler,
        ),
        (
            0b1100101000000000,
            0b0000000011111111,
            "xor #imm",
            super::cpu::Cpu::xori as InstructionHandler,
        ),
        (
            0b0100000000100100,
            0b0000111100000000,
            "rotcl",
            super::cpu::Cpu::rotcl as InstructionHandler,
        ),
        (
            0b0100000000100101,
            0b0000111100000000,
            "rotcr",
            super::cpu::Cpu::rotcr as InstructionHandler,
        ),
        (
            0b0100000000000100,
            0b0000111100000000,
            "rotl",
            super::cpu::Cpu::rotl as InstructionHandler,
        ),
        (
            0b0100000000000101,
            0b0000111100000000,
            "rotr",
            super::cpu::Cpu::rotr as InstructionHandler,
        ),
        (
            0b0100000000001100,
            0b0000111111110000,
            "shad",
            super::cpu::Cpu::shad as InstructionHandler,
        ),
        (
            0b0100000000100001,
            0b0000111100000000,
            "shar",
            super::cpu::Cpu::shar as InstructionHandler,
        ),
        (
            0b0100000000001101,
            0b0000111111110000,
            "shld",
            super::cpu::Cpu::shld as InstructionHandler,
        ),
        (
            0b0100000000000000,
            0b0000111100000000,
            "shll",
            super::cpu::Cpu::shll as InstructionHandler,
        ),
        (
            0b0100000000001000,
            0b0000111100000000,
            "shll2",
            super::cpu::Cpu::shll2 as InstructionHandler,
        ),
        (
            0b0100000000011000,
            0b0000111100000000,
            "shll8",
            super::cpu::Cpu::shll8 as InstructionHandler,
        ),
        (
            0b0100000000101000,
            0b0000111100000000,
            "shll16",
            super::cpu::Cpu::shll16 as InstructionHandler,
        ),
        (
            0b0100000000000001,
            0b0000111100000000,
            "shlr",
            super::cpu::Cpu::shlr as InstructionHandler,
        ),
        (
            0b0100000000001001,
            0b0000111100000000,
            "shlr2",
            super::cpu::Cpu::shlr2 as InstructionHandler,
        ),
        (
            0b0100000000011001,
            0b0000111100000000,
            "shlr8",
            super::cpu::Cpu::shlr8 as InstructionHandler,
        ),
        (
            0b0100000000101001,
            0b0000111100000000,
            "shlr16",
            super::cpu::Cpu::shlr16 as InstructionHandler,
        ),
        (
            0b1000101100000000,
            0b0000000011111111,
            "bf label:8",
            super::cpu::Cpu::branch_if_false as InstructionHandler,
        ),
        (
            0b1000111100000000,
            0b0000000011111111,
            "bf/s label:8",
            super::cpu::Cpu::branch_if_false_delayed as InstructionHandler,
        ),
        (
            0b1000100100000000,
            0b0000000011111111,
            "bt label:8",
            super::cpu::Cpu::branch_if_true as InstructionHandler,
        ),
        (
            0b1000110100000000,
            0b0000000011111111,
            "bt/s label:8",
            super::cpu::Cpu::branch_if_true_delayed as InstructionHandler,
        ),
        (
            0b1010000000000000,
            0b0000111111111111,
            "bra label:12",
            super::cpu::Cpu::bra as InstructionHandler,
        ),
        (
            0b0000000000100011,
            0b0000111100000000,
            "braf",
            super::cpu::Cpu::braf as InstructionHandler,
        ),
        (
            0b1011000000000000,
            0b0000111111111111,
            "bsr label:12",
            super::cpu::Cpu::bsr as InstructionHandler,
        ),
        (
            0b0000000000000011,
            0b0000111100000000,
            "bsrf",
            super::cpu::Cpu::bsrf as InstructionHandler,
        ),
        (
            0b0100000000101011,
            0b0000111100000000,
            "jmp",
            super::cpu::Cpu::jmp as InstructionHandler,
        ),
        (
            0b0100000000001011,
            0b0000111100000000,
            "jsr",
            super::cpu::Cpu::jsr as InstructionHandler,
        ),
        (
            0b0000000000001011,
            0b0000000000000000,
            "rts",
            super::cpu::Cpu::rts as InstructionHandler,
        ),
        (
            0b0000000001001000,
            0b0000000000000000,
            "clrs",
            super::cpu::Cpu::clrs as InstructionHandler,
        ),
        (
            0b0000000000001000,
            0b0000000000000000,
            "clrt",
            super::cpu::Cpu::clrt as InstructionHandler,
        ),
        (
            0b0100000000001110,
            0b0000111100000000,
            "ldc Rn,SR",
            super::cpu::Cpu::ldc_sr as InstructionHandler,
        ),
        (
            0b0100000001001110,
            0b0000111100000000,
            "ldc Rn,SPC",
            super::cpu::Cpu::ldc_spc as InstructionHandler,
        ),
        (
            0b0100000000000111,
            0b0000111100000000,
            "ldc.l @Rn+,SR",
            super::cpu::Cpu::ldcm_sr as InstructionHandler,
        ),
        (
            0b0100000000011110,
            0b0000111100000000,
            "ldc Rn,GBR",
            super::cpu::Cpu::ldc_gbr as InstructionHandler,
        ),
        (
            0b0100000000010111,
            0b0000111100000000,
            "ldc.l @Rn+,GBR",
            super::cpu::Cpu::ldsm_gbr as InstructionHandler,
        ),
        (
            0b0100000000101110,
            0b0000111100000000,
            "ldc Rn,VBR",
            super::cpu::Cpu::ldc_vbr as InstructionHandler,
        ),
        (
            0b0100000000100111,
            0b0000111100000000,
            "ldc.l @Rn+,VBR",
            super::cpu::Cpu::ldcm_vbr as InstructionHandler,
        ),
        (
            0b0100000000111110,
            0b0000111100000000,
            "ldc Rn,SSR",
            super::cpu::Cpu::ldc_ssr as InstructionHandler,
        ),
        (
            0b0100000000110111,
            0b0000111100000000,
            "ldc.l @Rn+,SSR",
            super::cpu::Cpu::ldcm_ssr as InstructionHandler,
        ),
        (
            0b0100000001000111,
            0b0000111100000000,
            "ldc.l @Rn+,SPC",
            super::cpu::Cpu::ldcm_spc as InstructionHandler,
        ),
        (
            0b0100000011111010,
            0b0000111100000000,
            "ldc Rn,DBR",
            super::cpu::Cpu::ldc_dbr as InstructionHandler,
        ),
        (
            0b0100000011110110,
            0b0000111100000000,
            "ldc.l @Rn+,DBR",
            super::cpu::Cpu::ldcm_dbr as InstructionHandler,
        ),
        (
            0b0100000010000111,
            0b0000111101110000,
            "ldc.l @Rn+,Rm_BANK",
            super::cpu::Cpu::ldcm_rmbank as InstructionHandler,
        ),
        (
            0b0100000010001110,
            0b0000111101110000,
            "ldc Rn,Rm_BANK",
            super::cpu::Cpu::ldcrn_rmbank as InstructionHandler,
        ),
        (
            0b0100000000001010,
            0b0000111100000000,
            "lds Rn,MACH",
            super::cpu::Cpu::ldsmach as InstructionHandler,
        ),
        (
            0b0100000000000110,
            0b0000111100000000,
            "lds.l @Rn+,MACH",
            super::cpu::Cpu::ldsm_mach as InstructionHandler,
        ),
        (
            0b0100000000011010,
            0b0000111100000000,
            "lds Rn,MACL",
            super::cpu::Cpu::ldsmacl as InstructionHandler,
        ),
        (
            0b0100000000010110,
            0b0000111100000000,
            "lds.l @Rn+,MACL",
            super::cpu::Cpu::ldsm_macl as InstructionHandler,
        ),
        (
            0b0100000000101010,
            0b0000111100000000,
            "lds Rn,PR",
            super::cpu::Cpu::ldspr as InstructionHandler,
        ),
        (
            0b0100000000100110,
            0b0000111100000000,
            "lds.l @Rn+,PR",
            super::cpu::Cpu::ldsm_pr as InstructionHandler,
        ),
        (
            0b0000000011000011,
            0b0000111100000000,
            "movca.l",
            super::cpu::Cpu::movcal as InstructionHandler,
        ),
        (
            0b0000000000001001,
            0b0000000000000000,
            "nop",
            super::cpu::Cpu::nop as InstructionHandler,
        ),
        (
            0b0000000010010011,
            0b0000111100000000,
            "ocbi",
            super::cpu::Cpu::nop as InstructionHandler,
        ),
        (
            0b0000000010100011,
            0b0000111100000000,
            "ocbp",
            super::cpu::Cpu::nop as InstructionHandler,
        ),
        (
            0b0000000010110011,
            0b0000111100000000,
            "ocbwb",
            super::cpu::Cpu::nop as InstructionHandler,
        ),
        (
            0b0000000010000011,
            0b0000111100000000,
            "pref",
            super::cpu::Cpu::pref as InstructionHandler,
        ),
        (
            0b0000000000101011,
            0b0000000000000000,
            "rte",
            super::cpu::Cpu::rte as InstructionHandler,
        ),
        (
            0b0000000000011000,
            0b0000000000000000,
            "sett",
            super::cpu::Cpu::sett as InstructionHandler,
        ),
        (
            0b0000000000011011,
            0b0000000000000000,
            "sleep",
            super::cpu::Cpu::sleep as InstructionHandler,
        ),
        (
            0b0000000000000010,
            0b0000111100000000,
            "stc SR,Rn",
            super::cpu::Cpu::stc_sr as InstructionHandler,
        ),
        (
            0b0100000000000011,
            0b0000111100000000,
            "stc.l SR,@-Rn",
            super::cpu::Cpu::stcm_sr as InstructionHandler,
        ),
        (
            0b0000000000010010,
            0b0000111100000000,
            "stc GBR,Rn",
            super::cpu::Cpu::stc_gbr as InstructionHandler,
        ),
        (
            0b0100000000010011,
            0b0000111100000000,
            "stc.l GBR,@-Rn",
            super::cpu::Cpu::stcm_gbr as InstructionHandler,
        ),
        (
            0b0000000000100010,
            0b0000111100000000,
            "stc VBR,Rn",
            super::cpu::Cpu::stc_vbr as InstructionHandler,
        ),
        (
            0b0100000000100011,
            0b0000111100000000,
            "stc.l VBR,@-Rn",
            super::cpu::Cpu::stcm_vbr as InstructionHandler,
        ),
        (
            0b0100000000110011,
            0b0000111100000000,
            "stc.l SSR,@-Rn",
            super::cpu::Cpu::stcm_ssr as InstructionHandler,
        ),
        (
            0b0100000001000011,
            0b0000111100000000,
            "stc.l SPC,@-Rn",
            super::cpu::Cpu::stcm_spc as InstructionHandler,
        ),
        (
            0b0000000011111010,
            0b0000111100000000,
            "stc DBR,Rn",
            super::cpu::Cpu::stc_dbr as InstructionHandler,
        ),
        (
            0b0000000010000010,
            0b0000111101110000,
            "stc Rm_BANK,Rn",
            super::cpu::Cpu::stc_rmbank as InstructionHandler,
        ),
        (
            0b0100000010000011,
            0b0000111101110000,
            "stc.l Rm_BANK,@-Rn",
            super::cpu::Cpu::stcm_rmbank as InstructionHandler,
        ),
        (
            0b0000000000001010,
            0b0000111100000000,
            "sts MACH,Rn",
            super::cpu::Cpu::sts_mach as InstructionHandler,
        ),
        (
            0b0100000000000010,
            0b0000111100000000,
            "sts.l MACH,@-Rn",
            super::cpu::Cpu::stsmmach as InstructionHandler,
        ),
        (
            0b0000000000011010,
            0b0000111100000000,
            "sts MACL,Rn",
            super::cpu::Cpu::sts_macl as InstructionHandler,
        ),
        (
            0b0100000000010010,
            0b0000111100000000,
            "sts.l MACL,@-Rn",
            super::cpu::Cpu::stsmmacl as InstructionHandler,
        ),
        (
            0b0000000000101010,
            0b0000111100000000,
            "sts PR,Rn",
            super::cpu::Cpu::sts_pr as InstructionHandler,
        ),
        (
            0b0100000000100010,
            0b0000111100000000,
            "sts.l PR,@-Rn",
            super::cpu::Cpu::stsmpr as InstructionHandler,
        ),
        (
            0b1111000010101101,
            0b0000111000000000,
            "fcnvsd",
            super::cpu::Cpu::fcnvsd as InstructionHandler,
        ),
        (
            0b1111000010111101,
            0b0000111000000000,
            "fcnvds",
            super::cpu::Cpu::fcnvds as InstructionHandler,
        ),
        (
            0b1111000000001100,
            0b0000111111110000,
            "fmov FRm,FRn",
            super::cpu::Cpu::fmov as InstructionHandler,
        ),
        (
            0b1111000000001000,
            0b0000111111110000,
            "fmov.s @Rm,FRn",
            super::cpu::Cpu::fmov_load as InstructionHandler,
        ),
        (
            0b1111000000001010,
            0b0000111111110000,
            "fmov.s FRm,@Rn",
            super::cpu::Cpu::fmov_store as InstructionHandler,
        ),
        (
            0b1111000000001001,
            0b0000111111110000,
            "fmov.s @Rm+,FRn",
            super::cpu::Cpu::fmov_restore as InstructionHandler,
        ),
        (
            0b1111000000001011,
            0b0000111111110000,
            "fmov.s FRm,@-Rn",
            super::cpu::Cpu::fmov_save as InstructionHandler,
        ),
        (
            0b1111000000000110,
            0b0000111111110000,
            "fmov.s @(R0,Rm),FRn",
            super::cpu::Cpu::fmov_index_load as InstructionHandler,
        ),
        (
            0b1111000000000111,
            0b0000111111110000,
            "fmov.s FRm,@(R0,Rn)",
            super::cpu::Cpu::fmov_index_store as InstructionHandler,
        ),
        (
            0b1111000010001101,
            0b0000111100000000,
            "fldi0",
            super::cpu::Cpu::fldi0 as InstructionHandler,
        ),
        (
            0b1111000010011101,
            0b0000111100000000,
            "fldi1",
            super::cpu::Cpu::fldi1 as InstructionHandler,
        ),
        (
            0b1111000000011101,
            0b0000111100000000,
            "flds",
            super::cpu::Cpu::flds as InstructionHandler,
        ),
        (
            0b1111000000001101,
            0b0000111100000000,
            "fsts",
            super::cpu::Cpu::fsts as InstructionHandler,
        ),
        (
            0b1111000001011101,
            0b0000111100000000,
            "fabs",
            super::cpu::Cpu::fabs as InstructionHandler,
        ),
        (
            0b1111000001001101,
            0b0000111100000000,
            "fneg",
            super::cpu::Cpu::fneg as InstructionHandler,
        ),
        (
            0b1111000000000000,
            0b0000111111110000,
            "fadd",
            super::cpu::Cpu::fadd as InstructionHandler,
        ),
        (
            0b1111000000000001,
            0b0000111111110000,
            "fsub",
            super::cpu::Cpu::fsub as InstructionHandler,
        ),
        (
            0b1111000000000010,
            0b0000111111110000,
            "fmul",
            super::cpu::Cpu::fmul as InstructionHandler,
        ),
        (
            0b1111000000001110,
            0b0000111111110000,
            "fmac",
            super::cpu::Cpu::fmac as InstructionHandler,
        ),
        (
            0b1111000000000011,
            0b0000111111110000,
            "fdiv",
            super::cpu::Cpu::fdiv as InstructionHandler,
        ),
        (
            0b1111000001101101,
            0b0000111100000000,
            "fsqrt",
            super::cpu::Cpu::fsqrt as InstructionHandler,
        ),
        (
            0b1111000000000100,
            0b0000111111110000,
            "fcmp/eq",
            super::cpu::Cpu::fcmpeq as InstructionHandler,
        ),
        (
            0b1111000000000101,
            0b0000111111110000,
            "fcmp/gt",
            super::cpu::Cpu::fcmpgt as InstructionHandler,
        ),
        (
            0b1111000000101101,
            0b0000111100000000,
            "float",
            super::cpu::Cpu::float as InstructionHandler,
        ),
        (
            0b1111000000111101,
            0b0000111100000000,
            "ftrc",
            super::cpu::Cpu::ftrc as InstructionHandler,
        ),
        (
            0b1111000011101101,
            0b0000111100000000,
            "fipr",
            super::cpu::Cpu::fipr as InstructionHandler,
        ),
        (
            0b1111000111111101,
            0b0000110000000000,
            "ftrv",
            super::cpu::Cpu::ftrv as InstructionHandler,
        ),
        (
            0b1111000001111101,
            0b0000111100000000,
            "fsrra",
            super::cpu::Cpu::fsrra as InstructionHandler,
        ),
        (
            0b1111000011111101,
            0b0000111000000000,
            "fsca",
            super::cpu::Cpu::fsca as InstructionHandler,
        ),
        (
            0b0100000001101010,
            0b0000111100000000,
            "lds Rn,FPSCR",
            super::cpu::Cpu::lds_fpscr as InstructionHandler,
        ),
        (
            0b0000000001101010,
            0b0000111100000000,
            "sts FPSCR,Rn",
            super::cpu::Cpu::sts_fpscr as InstructionHandler,
        ),
        (
            0b0100000001100110,
            0b0000111100000000,
            "lds.l @Rn+,FPSCR",
            super::cpu::Cpu::ldsm_fpscr as InstructionHandler,
        ),
        (
            0b0100000001100010,
            0b0000111100000000,
            "sts.l FPSCR,@-Rn",
            super::cpu::Cpu::stsm_fpscr as InstructionHandler,
        ),
        (
            0b0100000001011010,
            0b0000111100000000,
            "lds Rn,FPUL",
            super::cpu::Cpu::ldsfpul as InstructionHandler,
        ),
        (
            0b0000000001011010,
            0b0000111100000000,
            "sts FPUL,Rn",
            super::cpu::Cpu::sts_fpul as InstructionHandler,
        ),
        (
            0b0100000001010110,
            0b0000111100000000,
            "lds.l @Rn+,FPUL",
            super::cpu::Cpu::ldsm_fpul as InstructionHandler,
        ),
        (
            0b0100000001010010,
            0b0000111100000000,
            "sts.l FPUL,@-Rn",
            super::cpu::Cpu::stcm_fpul as InstructionHandler,
        ),
        (
            0b1111101111111101,
            0b0000000000000000,
            "frchg",
            super::cpu::Cpu::frchg as InstructionHandler,
        ),
        (
            0b1111001111111101,
            0b0000000000000000,
            "fschg",
            super::cpu::Cpu::fschg as InstructionHandler,
        ),
    ];

    lut[0] = {
        let (_, _, disassembly, handler) = instructions[0];
        DecodedInstruction {
            opcode: InstructionOpcode(0),
            disassembly,
            handler,
        }
    };

    for i in 1..=0x10000_u32 {
        for (code, mask, disassembly, handler) in instructions {
            // go through each item in the instruction array and see if any of the patterns match our opcode, if so add that entry for the opcode
            if (i & !mask) == code {
                lut[i as usize] = DecodedInstruction {
                    opcode: InstructionOpcode(i as u16),
                    disassembly,
                    handler,
                };

                // since only one opcode can be mapped to an instruction, break
                break;
            }
        }
    }

    lut
}
