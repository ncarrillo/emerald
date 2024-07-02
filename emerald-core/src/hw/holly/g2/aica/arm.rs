// repurposed, stripped down gba core :-)
use super::arm_bus::ArmBus;
use crate::{
    hw::extensions::{BarrelShifter, BitManipulation, ShiftType},
    scheduler::Scheduler,
};

#[derive(PartialEq, Eq)]
pub struct CpuRegisters {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r4: u32,
    pub r5: u32,
    pub r6: u32,
    pub r7: u32,
    pub r8: u32,
    pub r8_fiq: u32,
    pub r9: u32,
    pub r9_fiq: u32,
    pub r10: u32,
    pub r10_fiq: u32,
    pub r11: u32,
    pub r11_fiq: u32,
    pub r12: u32,
    pub r12_fiq: u32,
    pub r13_fiq: u32,
    pub r13_irq: u32,
    pub r13_svc: u32,
    pub r13: u32,
    pub r14_fiq: u32,
    pub r14_irq: u32,
    pub r14_svc: u32,
    pub r14: u32,
    pub r15: u32,
    pub cpsr: u32,
    pub spsr_fiq: u32,
    pub spsr_irq: u32,
    pub spsr_svc: u32,
}

impl Default for CpuRegisters {
    fn default() -> Self {
        Self {
            r0: 0,
            r1: 0,
            r2: 0,
            r3: 0,
            r4: 0,
            r5: 0,
            r6: 0,
            r7: 0,
            r8: 0,
            r8_fiq: 0,
            r9: 0,
            r9_fiq: 0,
            r10: 0,
            r10_fiq: 0,
            r11: 0,
            r11_fiq: 0,
            r12: 0,
            r12_fiq: 0,
            r13_fiq: 0,
            r13_irq: 0x03007fa0,
            r13_svc: 0x03007fe0,
            r13: 0x03007f00,
            r14_fiq: 0,
            r14_irq: 0,
            r14_svc: 0,
            r14: 0,
            r15: 0,
            cpsr: 0x0000001F,
            spsr_fiq: 0,
            spsr_irq: 0,
            spsr_svc: 0,
        }
    }
}

type ArmInstructionHandler<'b> = fn(&mut Cpu, u32, &'b mut ArmBus);

pub struct Cpu {
    pub registers: CpuRegisters,
    pub running: bool,
    pipeline: [u32; 2],
    flushed: bool,
    cyc: u64,
    pub trace: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            registers: Default::default(),
            pipeline: [0; 2],
            flushed: false,
            running: false,
            cyc: 0,
            trace: false,
        }
    }

    fn decode_arm(&self, opcode: u32) -> Option<ArmInstructionHandler> {
        if Self::is_branch_and_branch_with_link(opcode) {
            Some(Self::branch_and_branch_with_link)
        } else if Self::is_software_interrupt(opcode) {
            Some(Self::software_interrupt)
        } else if Self::is_block_data_transfer(opcode) {
            Some(Self::block_data_transfer)
        } else if Self::is_single_data_swap(opcode) {
            Some(Self::single_data_swap)
        } else if Self::is_multiply(opcode) {
            Some(Self::multiply)
        } else if Self::is_single_data_transfer(opcode) {
            Some(Self::single_data_transfer)
        } else if Self::is_halfword_data_transfer_register(opcode) {
            Some(Self::data_transfer_register)
        } else if Self::is_halfword_data_transfer_immediate(opcode) {
            Some(Self::data_transfer_immediate)
        } else if Self::is_psr_transfer_msr(opcode) {
            Some(Self::psr_transfer)
        } else if Self::is_psr_transfer_mrs(opcode) {
            Some(Self::psr_transfer)
        } else if Self::is_data_processing(opcode) {
            Some(Self::data_processing)
        } else {
            None
        }
    }

    pub fn current_pc_arm(&self) -> u32 {
        self.registers.r15 - 8
    }

    pub fn set_spsr(&mut self, spsr: u32) {
        match self.registers.cpsr & 0x1F {
            0x11 => self.registers.spsr_fiq = spsr,
            0x12 => self.registers.spsr_irq = spsr,
            0x13 => self.registers.spsr_svc = spsr,
            _ => {}
        }
    }

    pub fn get_spsr(&self) -> u32 {
        if (self.registers.cpsr & 0x1f) == 0x11 {
            return self.registers.spsr_fiq;
        } else if (self.registers.cpsr & 0x1f) == 0x12 {
            return self.registers.spsr_irq;
        } else if (self.registers.cpsr & 0x1f) == 0x13 {
            return self.registers.spsr_svc;
        } else {
            return self.registers.cpsr;
        }
    }

    #[inline(always)]
    pub fn get_register_by_index(&self, index: usize) -> u32 {
        match index {
            0 => self.registers.r0,
            1 => self.registers.r1,
            2 => self.registers.r2,
            3 => self.registers.r3,
            4 => self.registers.r4,
            5 => self.registers.r5,
            6 => self.registers.r6,
            7 => self.registers.r7,
            8 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r8_fiq,
            8 => self.registers.r8,
            9 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r9_fiq,
            9 => self.registers.r9,
            10 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r10_fiq,
            10 => self.registers.r10,
            11 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r11_fiq,
            11 => self.registers.r11,
            12 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r12_fiq,
            12 => self.registers.r12,
            13 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r13_fiq,
            13 if (self.registers.cpsr & 0x1f) == 0x12 => self.registers.r13_irq,
            13 if (self.registers.cpsr & 0x1f) == 0x13 => self.registers.r13_svc,
            13 => self.registers.r13,
            14 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r14_fiq,
            14 if (self.registers.cpsr & 0x1f) == 0x12 => self.registers.r14_irq,
            14 if (self.registers.cpsr & 0x1f) == 0x13 => self.registers.r14_svc,
            14 => self.registers.r14,
            15 => self.registers.r15,
            _ => self.registers.r0,
        }
    }

    #[inline(always)]
    pub fn get_user_register_by_index(&self, index: usize) -> u32 {
        match index {
            0 => self.registers.r0,
            1 => self.registers.r1,
            2 => self.registers.r2,
            3 => self.registers.r3,
            4 => self.registers.r4,
            5 => self.registers.r5,
            6 => self.registers.r6,
            7 => self.registers.r7,
            8 => self.registers.r8,
            9 => self.registers.r9,
            10 => self.registers.r10,
            11 => self.registers.r11,
            12 => self.registers.r12,
            13 => self.registers.r13,
            14 => self.registers.r14,
            15 => self.registers.r15,
            _ => self.registers.r0,
        }
    }

    #[inline(always)]
    pub fn set_register_by_index(&mut self, index: usize, value: u32) {
        match index {
            0 => self.registers.r0 = value,
            1 => self.registers.r1 = value,
            2 => self.registers.r2 = value,
            3 => self.registers.r3 = value,
            4 => self.registers.r4 = value,
            5 => self.registers.r5 = value,
            6 => self.registers.r6 = value,
            7 => self.registers.r7 = value,
            8 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r8_fiq = value,
            8 => self.registers.r8 = value,
            9 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r9_fiq = value,
            9 => self.registers.r9 = value,
            10 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r10_fiq = value,
            10 => self.registers.r10 = value,
            11 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r11_fiq = value,
            11 => self.registers.r11 = value,
            12 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r12_fiq = value,
            12 => self.registers.r12 = value,
            13 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r13_fiq = value,
            13 if (self.registers.cpsr & 0x1f) == 0x12 => self.registers.r13_irq = value,
            13 if (self.registers.cpsr & 0x1f) == 0x13 => self.registers.r13_svc = value,
            13 => self.registers.r13 = value,
            14 if (self.registers.cpsr & 0x1f) == 0x11 => self.registers.r14_fiq = value,
            14 if (self.registers.cpsr & 0x1f) == 0x12 => self.registers.r14_irq = value,
            14 if (self.registers.cpsr & 0x1f) == 0x13 => self.registers.r14_svc = value,
            14 => self.registers.r14 = value,
            15 => self.registers.r15 = value,
            _ => (),
        }
    }

    #[inline(always)]
    pub fn set_user_register_by_index(&mut self, index: usize, value: u32) {
        match index {
            0 => self.registers.r0 = value,
            1 => self.registers.r1 = value,
            2 => self.registers.r2 = value,
            3 => self.registers.r3 = value,
            4 => self.registers.r4 = value,
            5 => self.registers.r5 = value,
            6 => self.registers.r6 = value,
            7 => self.registers.r7 = value,
            8 => self.registers.r8 = value,
            9 => self.registers.r9 = value,
            10 => self.registers.r10 = value,
            11 => self.registers.r11 = value,
            12 => self.registers.r12 = value,
            13 => self.registers.r13 = value,
            14 => self.registers.r14 = value,
            15 => self.registers.r15 = value,
            _ => (),
        }
    }

    pub fn flush_pipeline<'b>(&mut self, bus: &ArmBus<'b>) {
        let pc = self.registers.r15 & !0x03;
        self.registers.r15 = pc + 8;
        self.pipeline[0] = bus.fetch_32(pc);
        self.pipeline[1] = bus.fetch_32(pc + 4);
        self.flushed = true;
    }

    pub fn set_mode(&mut self, mode: u32) {
        self.registers.cpsr &= !0x1F;
        self.registers.cpsr |= mode & 0x1F;
    }

    pub fn reset<'b>(&mut self, enable: bool, bus: &ArmBus<'b>) {
        if (!self.running && enable) {
            // set r14_svc to current pc
            self.registers.r14_svc = self.registers.r15;

            self.set_mode(0x13);
            self.registers.cpsr = (self.registers.cpsr.clear_bit(5).set_bit(6).set_bit(7));
            self.registers.r15 = 0x00;
            self.flush_pipeline(bus);
        }

        self.running = enable;

        if !self.running {}
    }

    pub fn step<'b>(&mut self, bus: &mut ArmBus<'b>) {
        self.cyc += 1;

        while self.cyc >= 8 {
            self.cyc -= 8;

            let instruction = self.pipeline[0];
            self.pipeline[0] = self.pipeline[1];
            self.pipeline[1] = bus.fetch_32(self.registers.r15);

            if let Some(decoded) = self.decode_arm(instruction) {
                if false {
                    println!("arm: {:08x}: {:x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}", self.current_pc_arm(), instruction, self.get_register_by_index(0), self.get_register_by_index(1), self.get_register_by_index(2), self.get_register_by_index(3), self.get_register_by_index(4), self.get_register_by_index(5), self.get_register_by_index(6), self.get_register_by_index(7), self.get_register_by_index(8), self.get_register_by_index(9), self.get_register_by_index(10), self.get_register_by_index(11), self.get_register_by_index(12), self.get_register_by_index(13), self.get_register_by_index(14), self.get_register_by_index(15));
                }

                if self.running {
                    decoded(self, instruction, bus)
                }
            } else {
                println!(
                    "arm7: WARNING: unknown instruction {:08x} @ {:08x}!",
                    instruction,
                    self.current_pc_arm()
                );
            }

            if !self.flushed {
                self.registers.r15 += 4;
            }

            self.flushed = false;
        }
    }

    pub fn is_single_data_swap(opcode: u32) -> bool {
        (opcode & 0xFB00FF0) == 0x1000090
    }

    pub fn is_data_processing(opcode: u32) -> bool {
        let data_processing_format = 0b0000_0000_0000_0000_0000_0000_0000_0000;
        let format_mask = 0b0000_1100_0000_0000_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == data_processing_format
    }

    pub fn is_branch_and_branch_with_link(opcode: u32) -> bool {
        let branch_format = 0b0000_1010_0000_0000_0000_0000_0000_0000;
        let branch_with_link_format = 0b0000_1011_0000_0000_0000_0000_0000_0000;
        let format_mask = 0b0000_1111_0000_0000_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == branch_format || extracted_format == branch_with_link_format
    }

    pub fn is_psr_transfer_msr(opcode: u32) -> bool {
        let msr_format = 0b0000_0001_0010_0000_1111_0000_0000_0000;
        let format_mask = 0b0000_1101_1011_0000_1111_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == msr_format
    }

    pub fn is_psr_transfer_mrs(opcode: u32) -> bool {
        let mrs_format = 0b0000_0001_0000_1111_0000_0000_0000_0000;
        let format_mask = 0b0000_1111_1011_1111_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == mrs_format
    }

    pub fn is_block_data_transfer(opcode: u32) -> bool {
        let block_data_transfer_format = 0b0000_1000_0000_0000_0000_0000_0000_0000;
        let format_mask = 0b0000_1110_0000_0000_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == block_data_transfer_format
    }

    pub fn is_single_data_transfer(opcode: u32) -> bool {
        let single_data_transfer_format = 0b0000_0100_0000_0000_0000_0000_0000_0000;
        let format_mask = 0b0000_1100_0000_0000_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == single_data_transfer_format
    }

    pub fn is_halfword_data_transfer_register(opcode: u32) -> bool {
        let halfword_data_transfer_register_format = 0b0000_0000_0000_0000_0000_0000_1001_0000;
        let format_mask = 0b0000_1110_0100_0000_0000_1111_1001_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == halfword_data_transfer_register_format
    }

    pub fn is_halfword_data_transfer_immediate(opcode: u32) -> bool {
        let halfword_data_transfer_immediate_format = 0b0000_0000_0100_0000_0000_0000_1001_0000;
        let format_mask = 0b0000_1110_0100_0000_0000_0000_1001_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == halfword_data_transfer_immediate_format
    }

    pub fn is_software_interrupt(opcode: u32) -> bool {
        let software_interrupt_format = 0b0000_1111_0000_0000_0000_0000_0000_0000;
        let format_mask = 0b0000_1111_0000_0000_0000_0000_0000_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == software_interrupt_format
    }

    pub fn is_multiply(opcode: u32) -> bool {
        let multiply_format = 0b0000_0000_0000_0000_0000_0000_1001_0000;
        let format_mask = 0b0000_1111_1000_0000_0000_0000_1111_0000;
        let extracted_format = opcode & format_mask;

        extracted_format == multiply_format
    }

    pub fn multiply<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let multiply_opcode = (opcode & 0x1E00000) >> 21;
        let set_condition_codes = opcode.check_bit(20);

        let destination_register_index = ((opcode & 0xF0000) >> 16) as usize;
        let accumulate_register_index = ((opcode & 0xF000) >> 12) as usize;

        let operand_register_index = ((opcode & 0xF00) >> 8) as usize;
        let operand_register2_index = (opcode & 0xF) as usize;

        let accumulate_register = self.get_register_by_index(accumulate_register_index);

        let operand_register = self.get_register_by_index(operand_register_index);
        let operand_register2 = self.get_register_by_index(operand_register2_index);
        let (mut n, mut z, mut c, mut v) = (
            self.registers.cpsr.check_bit(31),
            self.registers.cpsr.check_bit(30),
            self.registers.cpsr.check_bit(29),
            self.registers.cpsr.check_bit(28),
        );

        match multiply_opcode {
            0x00 => {
                // mul
                let result = operand_register2.wrapping_mul(operand_register);
                z = result == 0;
                n = (result as i32) < 0;

                self.set_register_by_index(destination_register_index, result);
            }
            0x01 => {
                // mla
                let full_result = (operand_register2 as u64)
                    .wrapping_mul(operand_register as u64)
                    .wrapping_add(accumulate_register as u64);
                let result = full_result as u32;
                z = result == 0;
                n = (result as i32) < 0;

                //self.add_internal_cycle();
            }
            _ => {
                panic!("arm7: invalid mul type");
            }
        }

        if set_condition_codes {
            self.registers.cpsr = if n {
                self.registers.cpsr.set_bit(31)
            } else {
                self.registers.cpsr.clear_bit(31)
            };
            self.registers.cpsr = if z {
                self.registers.cpsr.set_bit(30)
            } else {
                self.registers.cpsr.clear_bit(30)
            };
            self.registers.cpsr = if c {
                self.registers.cpsr.set_bit(29)
            } else {
                self.registers.cpsr.clear_bit(29)
            };
            self.registers.cpsr = if v {
                self.registers.cpsr.set_bit(28)
            } else {
                self.registers.cpsr.clear_bit(28)
            };
        }

        if destination_register_index == 15 {
            self.flush_pipeline(bus);
        }
    }

    pub fn branch_and_branch_with_link<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        // Calculate the offset
        let offset = (((opcode & 0xFFFFFF) << 8) as i32) >> 6;
        let is_branch_with_link = opcode.check_bit(24);

        if is_branch_with_link {
            self.set_register_by_index(14, self.current_pc_arm() + 4);
        }

        self.registers.r15 = ((self.registers.r15 as i32).wrapping_add(offset)) as u32;
        self.flush_pipeline(bus);
    }

    pub fn software_interrupt<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        let swi_opcode = opcode & 0xFFFFFF;

        self.registers.spsr_svc = self.registers.cpsr;
        self.registers.r14_svc = self.registers.r15.wrapping_sub(8);
        self.registers.cpsr = self.registers.cpsr.clear_bit(5);
        self.registers.cpsr = self.registers.cpsr.set_bit(7);

        self.set_mode(0x13); // swi mode
        self.registers.r15 = 0x08; // swi vector
        self.flush_pipeline(bus);
    }

    pub fn block_data_transfer<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let mut pre = opcode.check_bit(24); // pre/post
        let add = opcode.check_bit(23); // up/down
        let writeback = opcode.check_bit(21); // write back
        let load = opcode.check_bit(20); // load / store
        let base_register_index = ((opcode & 0xF0000) >> 16) as usize;

        let mut rlist = opcode & 0xFFFF;
        let mut transferring_pc = (rlist & (1 << 15)) != 0;
        let mut first = 0;
        let mut bytes = 0u32;

        let user_mode = opcode.check_bit(22) && (!transferring_pc || !load);

        let mut address = self.get_register_by_index(base_register_index);

        if load {
            //self.add_internal_cycle();
        }

        if rlist != 0 {
            for i in (0..16).rev() {
                if !rlist.check_bit(i) {
                    continue;
                }
                first = i;
                bytes += 4;
            }
        } else {
            rlist = 1 << 15;
            first = 15;
            transferring_pc = true;
            bytes = 64;
        }

        let mut base_new = address;

        if !add {
            pre = !pre;
            address = address.wrapping_sub(bytes);
            base_new = base_new.wrapping_sub(bytes);
        } else {
            base_new = base_new.wrapping_add(bytes);
        }

        for i in first..16 {
            if !rlist.check_bit(i) {
                continue;
            }

            if pre {
                address = address.wrapping_add(4);
            }

            if load {
                let value = bus.read_32(address);

                if writeback && i == first {
                    self.set_register_by_index(base_register_index, base_new)
                }

                if !user_mode {
                    self.set_register_by_index(i, value)
                } else {
                    self.set_user_register_by_index(i, value)
                }
            } else {
                let value = if i == 15 {
                    self.registers.r15.wrapping_add(4)
                } else {
                    if !user_mode {
                        self.get_register_by_index(i)
                    } else {
                        self.get_user_register_by_index(i)
                    }
                };

                bus.write_32(address, value);
                if writeback && i == first {
                    self.set_register_by_index(base_register_index, base_new)
                }
            }

            if !pre {
                address = address.wrapping_add(4);
            }
        }

        if load && transferring_pc {
            if user_mode {
                let spsr = self.get_spsr();
                self.registers.cpsr = spsr;
            }
            self.flush_pipeline(bus);
        }
    }

    pub fn data_processing<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let data_processing_opcode = (opcode & 0x1E00000) >> 21;
        let update_condition_flags = opcode.check_bit(20);
        let is_second_op_immediate = opcode.check_bit(25);

        let operand_one_index = ((opcode & 0xF0000) >> 16) as usize;
        let mut operand1 = self.get_register_by_index(operand_one_index);

        let destination_register_index = ((opcode & 0xF000) >> 12) as usize;

        let mut operand2: u32;
        let mut carry = self.registers.cpsr.check_bit(29);
        let shift_by_register = opcode.check_bit(4);

        let pc_adjustment = if !is_second_op_immediate && shift_by_register {
            12
        } else {
            8
        };

        if operand_one_index == 15 {
            operand1 = self.current_pc_arm() + pc_adjustment;
        }

        let ignore_pipeline_flush = matches!(data_processing_opcode, 0x0B | 0x09 | 0x08 | 0x0A); // cmn, teq, tst, cmp

        if is_second_op_immediate {
            // when I=1 (immediate as 2nd operand)
            let second_op_immediate_value = opcode & 0xFF;
            let rotate_amount = ((opcode & 0xF00) >> 8) as usize;

            operand2 = if rotate_amount > 0 {
                second_op_immediate_value.ror(rotate_amount * 2, &mut carry, false)
            } else {
                second_op_immediate_value
            };
        } else {
            let second_operand_register_index = (opcode & 0xF) as usize;
            operand2 = self.get_register_by_index(second_operand_register_index);

            if second_operand_register_index == 15 {
                operand2 = self.current_pc_arm() + pc_adjustment;
            }

            // when I=0 (register as 2nd operand)
            let shift_register_index = ((opcode & 0xF00) >> 8) as usize;
            let shift_amount = if shift_by_register {
                self.get_register_by_index(shift_register_index) & 0xFF
            } else {
                ((opcode & 0xF80) >> 7) as u32
            };
            let shift_direction =
                ShiftType::from_u32((opcode & 0x60) >> 5).unwrap_or(ShiftType::Lsl);

            if !shift_by_register {
                //self.add_internal_cycle();
            }

            operand2 = operand2.barrel_shift(
                shift_direction,
                shift_amount as usize,
                &mut carry,
                !shift_by_register,
            );
        }

        let mut n = self.registers.cpsr.check_bit(31);
        let mut z = self.registers.cpsr.check_bit(30);
        let mut c = self.registers.cpsr.check_bit(29);
        let mut v = self.registers.cpsr.check_bit(28);

        match data_processing_opcode {
            0xA => {
                // cmp
                let result = operand1.wrapping_sub(operand2);
                n = (result >> 31) != 0;
                z = result == 0;
                c = operand1 >= operand2;
                v = ((operand1 ^ operand2) & (operand1 ^ result) & 0x80000000) != 0;
            }
            0x00 => {
                // and
                let result = operand1 & operand2;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x01 => {
                // xor
                let result = operand1 ^ operand2;
                n = (result as i32) < 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x02 => {
                // subs
                let result = (operand1 as i32).wrapping_sub(operand2 as i32) as u32;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = operand1 >= operand2;
                v = ((operand1 ^ operand2) & 0x80000000 != 0)
                    && ((operand1 ^ result) & 0x80000000 != 0);
                self.set_register_by_index(destination_register_index, result);
            }
            0x03 => {
                // rsb
                let result = (operand2 as i32).wrapping_sub(operand1 as i32) as u32;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = operand2 >= operand1;
                v = ((operand2 ^ operand1) & 0x80000000 != 0)
                    && ((operand2 ^ result) & 0x80000000 != 0);
                self.set_register_by_index(destination_register_index, result);
            }
            0x04 => {
                // add
                let result = operand1.wrapping_add(operand2);
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = operand1 as u64 + operand2 as u64 > u32::MAX as u64;
                v = (((operand1 ^ operand2) & 0x80000000 == 0)
                    && ((operand1 ^ result) & 0x80000000 != 0));
                self.set_register_by_index(destination_register_index, result);
            }
            0x0e => {
                // bic
                let result = operand1 & !operand2;
                n = (result as i32) < 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x05 => {
                // adc
                let result64 = operand1 as u64 + operand2 as u64 + (if carry { 1 } else { 0 });
                let result32 = result64 as u32;
                n = result32 & 0x80000000 != 0;
                z = result32 == 0;
                c = result64 > u32::MAX as u64;
                v = (((operand1 ^ operand2) & 0x80000000 == 0)
                    && ((operand1 ^ result32) & 0x80000000 != 0));
                self.set_register_by_index(destination_register_index, result32);
            }
            0x06 => {
                // sbc
                let borrow_in = if carry { 0 } else { 1 };
                let operand_with_borrow = operand2 as u64 + borrow_in;
                let (result_with_borrow, did_underflow) =
                    (operand1 as u64).overflowing_sub(operand_with_borrow);
                let result = result_with_borrow as u32;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = !did_underflow;
                v = ((operand1 ^ operand2) & (operand1 ^ result) & 0x80000000) != 0;
                self.set_register_by_index(destination_register_index, result);
            }
            0x07 => {
                // rsc
                let borrow_in = if carry { 0 } else { 1 };
                let (operand1, operand2) = (operand2, operand1);
                let operand_with_borrow = operand2 as u64 + borrow_in;
                let result_with_borrow = operand1 as u64 - operand_with_borrow;
                let result = result_with_borrow as u32;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = result_with_borrow <= u32::MAX as u64;
                v = ((operand1 ^ operand2) & (operand1 ^ result) & 0x80000000) != 0;
                self.set_register_by_index(destination_register_index, result);
            }
            0x08 => {
                // tst
                let result = operand1 & operand2;
                n = (result as i32) < 0;
                z = result == 0;
                c = carry;
            }
            0x0c => {
                // or
                let result = operand1 | operand2;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x0f => {
                // mvn
                let result = !operand2;
                n = (result >> 31) != 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x0b => {
                // cmn
                let result = operand1.wrapping_add(operand2);
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = operand1 as u64 + operand2 as u64 > u32::MAX as u64;
                v = ((!(operand1 ^ operand2) & (operand1 ^ result) & 0x80000000) != 0);
            }
            0x0d => {
                // mov
                let result = operand2;
                n = result & 0x80000000 != 0;
                z = result == 0;
                c = carry;
                self.set_register_by_index(destination_register_index, result);
            }
            0x09 => {
                // teq
                let result = operand1 ^ operand2;
                n = (result as i32) < 0;
                z = result == 0;
                c = carry;
            }
            _ => {
                panic!("wtf")
            }
        }

        if destination_register_index == 15 {
            if update_condition_flags {
                self.registers.cpsr = self.get_spsr();
                self.set_mode(self.registers.cpsr & 0x1F);
            }
            if !ignore_pipeline_flush {
                self.flush_pipeline(bus);
            }
        }

        if update_condition_flags && destination_register_index != 15 {
            self.registers.cpsr = if n {
                self.registers.cpsr.set_bit(31)
            } else {
                self.registers.cpsr.clear_bit(31)
            };
            self.registers.cpsr = if z {
                self.registers.cpsr.set_bit(30)
            } else {
                self.registers.cpsr.clear_bit(30)
            };
            self.registers.cpsr = if c {
                self.registers.cpsr.set_bit(29)
            } else {
                self.registers.cpsr.clear_bit(29)
            };
            self.registers.cpsr = if v {
                self.registers.cpsr.set_bit(28)
            } else {
                self.registers.cpsr.clear_bit(28)
            };
        }
    }

    // validated
    pub fn single_data_transfer<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let base_register_index = ((opcode & 0xF0000) >> 16) as usize;
        let destination_register_index = ((opcode & 0xF000) >> 12) as usize;
        let mut base_register = self.get_register_by_index(base_register_index);

        let shifted = opcode.check_bit(25);
        let load = opcode.check_bit(20);

        let mut offset = opcode & 0xFFF;
        let transferring_byte = opcode.check_bit(22);
        let writeback = opcode.check_bit(21);

        if shifted {
            let mut carry = self.registers.cpsr.check_bit(29);
            let shift_direction = ((opcode & 0x60) >> 5) as u32;
            let shift_amount = ((opcode & 0xF80) >> 7) as u32;

            offset = self.get_register_by_index((opcode & 0xF) as usize);
            let shift_type =
                ShiftType::from_u32(shift_direction).expect("inexpected shift type value");
            offset = offset.barrel_shift(shift_type, shift_amount as usize, &mut carry, true);
        }

        let increment_base_address = opcode.check_bit(23);

        if !increment_base_address {
            offset = -(offset as i32) as u32;
        }

        let effective_address = base_register.wrapping_add(offset);

        let pre_index_base_address = opcode.check_bit(24);
        if pre_index_base_address {
            base_register = effective_address;
        }

        if load {
            // self.add_internal_cycle();

            let data = if transferring_byte {
                bus.read_8(base_register) as u32
            } else {
                self.load_and_rotate_32(base_register, bus)
            };

            self.set_register_by_index(destination_register_index, data);
            if destination_register_index == 15 {
                self.flush_pipeline(bus);
            }
        } else {
            let value = if destination_register_index == 15 {
                self.current_pc_arm() + 12
            } else {
                self.get_register_by_index(destination_register_index)
            };

            if transferring_byte {
                bus.write_8(base_register, (value & 0xFF) as u8);
            } else {
                bus.write_32(base_register & !0x3, value);
            }
        }

        if (!load || base_register_index != destination_register_index)
            && (!pre_index_base_address || writeback)
        {
            self.set_register_by_index(base_register_index, effective_address);
            if base_register_index == 15 {
                self.flush_pipeline(bus);
            }
        }
    }

    pub fn load_and_rotate_32(&mut self, addr: u32, bus: &mut ArmBus) -> u32 {
        if (addr & 0x3) != 0 {
            let rotation = ((addr & 0x3) << 3) as usize;
            let mut value = bus.read_32(addr & !0x3);
            let mut carry = self.registers.cpsr.check_bit(29);
            let v = value.ror(rotation, &mut carry, false);

            v
        } else {
            bus.read_32(addr)
        }
    }

    // validated
    pub fn single_data_swap<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let base_register_index = (opcode >> 16) & 0b1111;
        let source_register_index = opcode & 0b1111;
        let destination_register_index = (opcode >> 12) & 0b1111;
        let is_byte_sized = opcode.check_bit(22);
        let base_register = self.get_register_by_index(base_register_index as usize);

        //self.add_internal_cycle();

        if is_byte_sized {
            let t = bus.read_8(base_register);
            bus.write_8(
                base_register,
                self.get_register_by_index(source_register_index as usize) as u8,
            );
            self.set_register_by_index(destination_register_index as usize, t as u32);
        } else {
            let t = self.load_and_rotate_32(base_register, bus);
            bus.write_32(
                base_register & !0x3,
                self.get_register_by_index(source_register_index as usize),
            );
            self.set_register_by_index(destination_register_index as usize, t);
        }

        if destination_register_index == 15 {
            self.flush_pipeline(bus);
        }
    }

    // validated
    pub fn data_transfer_register<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let pre_index_base_address = opcode.check_bit(24); // pre/post
        let increment_base_address = opcode.check_bit(23); // up/down
        let writeback = opcode.check_bit(21); // write back
        let load = opcode.check_bit(20); // load / store

        let base_register_index = (opcode & 0xF0000) >> 16; // base register
        let source_dest_register_index = (opcode & 0xF000) >> 12; // source (or dest) register depending on the l/s bit
        let offset_register_index = opcode & 0xF; // R0-15 corresponding to the offset register
        let offset_register = self.get_register_by_index(offset_register_index as usize);
        let load_type = (opcode & 0x60) >> 5;

        self.data_transfer_common(
            opcode,
            base_register_index,
            source_dest_register_index,
            offset_register,
            load,
            increment_base_address,
            pre_index_base_address,
            writeback,
            load_type,
            bus,
        );
    }

    // validated
    pub fn data_transfer_immediate<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let pre_index_base_address = opcode.check_bit(24); // pre/post
        let increment_base_address = opcode.check_bit(23); // up/down
        let writeback = opcode.check_bit(21); // write back
        let load = opcode.check_bit(20); // load / store

        let base_register_index = (opcode & 0xF0000) >> 16; // base register
        let source_dest_register_index = (opcode & 0xF000) >> 12; // source (or dest) register depending on the l/s bit

        let offset = (opcode & 0xF) | ((opcode >> 4) & 0xF0);
        let load_type = (opcode & 0x60) >> 5;

        self.data_transfer_common(
            opcode,
            base_register_index,
            source_dest_register_index,
            offset,
            load,
            increment_base_address,
            pre_index_base_address,
            writeback,
            load_type,
            bus,
        );
    }

    // validated
    pub fn data_transfer_common(
        &mut self,
        opcode: u32,
        base_register_index: u32,
        source_dest_register_index: u32,
        mut offset: u32,
        load: bool,
        increment_base_address: bool,
        pre_index_base_address: bool,
        writeback: bool,
        load_type: u32,
        bus: &mut ArmBus,
    ) {
        let mut base_register = self.get_register_by_index(base_register_index as usize);
        if base_register_index == 15 {
            base_register = self.current_pc_arm() + 8;
        }

        if !increment_base_address {
            offset = -(offset as i32) as u32;
        }

        let mut effective_address = base_register;
        if pre_index_base_address {
            effective_address = effective_address.wrapping_add(offset);
        }

        if load {
            //self.add_internal_cycle();
        }

        if load {
            match load_type {
                0x01 => {
                    let value = self.load_and_rotate_16(effective_address, bus);
                    self.set_register_by_index(source_dest_register_index as usize, value as u32);
                }
                0x02 => {
                    let single_value = bus.read_8(effective_address);
                    let value = single_value as i8 as i32 as u32;
                    self.set_register_by_index(source_dest_register_index as usize, value);
                }
                0x03 => {
                    let value = self.load_signed_half_word(effective_address, bus);
                    self.set_register_by_index(source_dest_register_index as usize, value);
                }
                _ => {
                    panic!(
                        "arm7: bad load type {:08x} for {:08x} {:08x}",
                        load_type,
                        opcode,
                        self.current_pc_arm()
                    );
                }
            }

            if source_dest_register_index == 15 {
                self.flush_pipeline(bus);
            }
        } else {
            let value = if source_dest_register_index == 15 {
                self.current_pc_arm() + 0x12
            } else {
                self.get_register_by_index(source_dest_register_index as usize)
            };
            match load_type {
                0x01 => {
                    bus.write_16(effective_address & !0x1, value as u16);
                }
                _ => {
                    panic!(
                        "arm7: bad load type {:08x} for {:08x} {:08x}",
                        load_type,
                        opcode,
                        self.current_pc_arm()
                    );
                }
            }
        }

        if (!load || base_register_index != source_dest_register_index)
            && (!pre_index_base_address || writeback)
        {
            let new_value = self
                .get_register_by_index(base_register_index as usize)
                .wrapping_add(offset);
            self.set_register_by_index(base_register_index as usize, new_value);

            if base_register_index == 15 {
                self.flush_pipeline(bus);
            }
        }
    }

    pub fn load_and_rotate_16(&mut self, addr: u32, bus: &mut ArmBus) -> u32 {
        let result: u32;

        if (addr & 0x1) != 0 {
            let rotation = (addr & 0x01) << 3;
            let value = bus.read_16(addr & !0x01) as u32;
            let mut carry = false;
            result = value.ror(rotation as usize, &mut carry, false);
        } else {
            // address is aligned, just read the halfword
            result = bus.read_16(addr) as u32;
        }

        result
    }

    pub fn load_signed_half_word(&mut self, addr: u32, bus: &mut ArmBus) -> u32 {
        if (addr & 0x1) != 0 {
            let byte_value = bus.read_8(addr);
            let signed_byte = byte_value as i8;
            let int_value = signed_byte as i32;
            int_value as u32
        } else {
            let halfword_value = bus.read_16(addr & !0x1);
            let signed_halfword = halfword_value as i16;
            let int_value = signed_halfword as i32;
            int_value as u32
        }
    }

    // validated
    pub fn psr_transfer<'b>(&mut self, opcode: u32, bus: &'b mut ArmBus) {
        if !self.check_conditions(opcode) {
            return;
        }

        let immediate = opcode.check_bit(25);
        let targeting_cpsr = !opcode.check_bit(22);
        let is_load = !opcode.check_bit(21);
        let destination_register_index = ((opcode & 0xF000) >> 12) as usize;

        let mut operand2: u32 = 0;
        if immediate {
            let shift_amount = ((opcode & 0xF00) >> 8) as usize;
            let immediate_value = opcode & 0xFF;
            let mut shift_carry = self.registers.cpsr.check_bit(29);

            operand2 = immediate_value.ror(shift_amount * 2, &mut shift_carry, false);
        } else {
            let source_register_index = (opcode & 0xF) as usize;
            operand2 = self.get_register_by_index(source_register_index);
        }

        if is_load {
            let psr = if targeting_cpsr {
                self.registers.cpsr
            } else {
                self.get_spsr()
            };
            self.set_register_by_index(destination_register_index, psr);
        } else {
            // MSR
            if targeting_cpsr {
                let update_flags = opcode.check_bit(19);
                let update_control = opcode.check_bit(16);

                let flags_mask: u32 = if update_flags { 0xF0000000 } else { 0x0 };
                let control_mask: u32 = if update_control { 0x0FFFFFFF } else { 0x0 };

                self.registers.cpsr = (self.registers.cpsr & !flags_mask & !control_mask)
                    | (operand2 & (flags_mask | control_mask));
            } else {
                let update_flags = opcode.check_bit(19);
                let update_control = opcode.check_bit(16);

                let flags_mask: u32 = if update_flags { 0xF0000000 } else { 0x0 };
                let control_mask: u32 = if update_control { 0x0FFFFFFF } else { 0x0 };
                let spsr = self.get_spsr();
                self.set_spsr(
                    (spsr & !flags_mask & !control_mask) | (operand2 & (flags_mask | control_mask)),
                );
            }
        }
    }

    #[inline(always)]
    fn check_conditions(&self, opcode: u32) -> bool {
        // extract the condition code from the opcode (bits 31 to 28)
        let condition_code = (opcode >> 28) & 0xF;

        // extract the condition flags from the CPSR
        let n = self.registers.cpsr.check_bit(31);
        let z = self.registers.cpsr.check_bit(30);
        let c = self.registers.cpsr.check_bit(29);
        let v = self.registers.cpsr.check_bit(28);

        // check the condition code against the flags
        match condition_code {
            0x0 => z,              // EQ: Z set (equal)
            0x1 => !z,             // NE: Z clear (not equal)
            0x2 => c,              // CS/HS: C set (unsigned higher or same)
            0x3 => !c,             // CC/LO: C clear (unsigned lower)
            0x4 => n,              // MI: N set (negative)
            0x5 => !n,             // PL: N clear (positive or zero)
            0x6 => v,              // VS: V set (overflow)
            0x7 => !v,             // VC: V clear (no overflow)
            0x8 => c && !z,        // HI: C set and Z clear (unsigned higher)
            0x9 => !c || z,        // LS: C clear or Z set (unsigned lower or same)
            0xA => n == v,         // GE: N equals V (greater or equal, signed)
            0xB => n != v,         // LT: N not equal to V (less than, signed)
            0xC => !z && (n == v), // GT: Z clear, and N equals V (greater than, signed)
            0xD => z || (n != v),  // LE: Z set or N not equal to V (less than or equal, signed)
            0xE => true,           // AL: Always (unconditional)
            _ => false,
        }
    }
}
