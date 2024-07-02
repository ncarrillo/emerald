use crate::{context::Context, hw::extensions::BitManipulation};

use super::{bus::CpuBus, cpu::Cpu, decoder::DecodedInstruction};

impl Cpu {
    pub fn float(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(19) {
            let fpul = self.get_fpul();
            let float_value = fpul as i32 as f32;
            self.set_fr_register_by_index(rn_idx, float_value);
        } else {
            assert!((rn_idx & 0x1) == 0);

            let fpul = self.get_fpul();
            let val = fpul as i32 as f64;
            self.set_dr_register_by_index(rn_idx >> 1, val);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fadd(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(19) {
            let rn = self.get_fr_register_by_index(rn_idx);
            let rm = self.get_fr_register_by_index(rm_idx);
            let result = rn + rm;
            self.set_fr_register_by_index(rn_idx, result);
        } else {
            assert!((rn_idx & 0x1) == 0);
            assert!((rm_idx & 0x1) == 0);

            let rn = self.get_dr_register_by_index(rn_idx >> 1);
            let rm = self.get_dr_register_by_index(rm_idx >> 1);

            self.set_dr_register_by_index(rn_idx >> 1, rm + rn);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fsub(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(19) {
            let rn = self.get_fr_register_by_index(rn_idx);
            let rm = self.get_fr_register_by_index(rm_idx);
            self.set_fr_register_by_index(rn_idx, rn - rm)
        } else {
            assert!((rn_idx & 0x1) == 0);
            assert!((rm_idx & 0x1) == 0);

            let rn = self.get_dr_register_by_index(rn_idx >> 1);
            let rm = self.get_dr_register_by_index(rm_idx >> 1);
            self.set_dr_register_by_index(rn_idx >> 1, rn - rm);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fdiv(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(19) {
            let rn = self.get_fr_register_by_index(rn_idx);
            let rm = self.get_fr_register_by_index(rm_idx);
            self.set_fr_register_by_index(rn_idx, rn / rm);
        } else {
            assert!((rn_idx & 0x1) == 0);
            assert!((rm_idx & 0x1) == 0);

            let rn = self.get_dr_register_by_index(rn_idx >> 1);
            let rm = self.get_dr_register_by_index(rm_idx >> 1);
            self.set_dr_register_by_index(rn_idx >> 1, rn / rm);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmul(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(19) {
            let rn = self.get_fr_register_by_index(rn_idx);
            let rm = self.get_fr_register_by_index(rm_idx);

            self.set_fr_register_by_index(rn_idx, rn * rm);
        } else {
            assert!((rn_idx & 0x1) == 0);
            assert!((rm_idx & 0x1) == 0);

            let rn = self.get_dr_register_by_index(rn_idx >> 1);
            let rm = self.get_dr_register_by_index(rm_idx >> 1);
            self.set_dr_register_by_index(rn_idx >> 1, rn * rm);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fabs(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_fr_register_by_index(
            rn_idx,
            f32::from_bits(f32::to_bits(self.get_fr_register_by_index(rn_idx)) & 0x7FFFFFFF),
        );

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fsqrt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        if !self.registers.fpscr.check_bit(19) {
            let fr_value = self.get_fr_register_by_index(rn_idx);

            let result = if fr_value < 0.0 {
                std::f32::NAN
            } else {
                fr_value.sqrt()
            };

            self.set_fr_register_by_index(rn_idx, result);
        } else {
            assert!(rn_idx & 0x1 == 0);

            let dr_idx = rn_idx >> 1;
            let dr_value = self.get_dr_register_by_index(dr_idx);

            let result = if dr_value < 0.0 {
                std::f64::NAN
            } else {
                dr_value.sqrt()
            };

            self.set_dr_register_by_index(dr_idx, result);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fneg(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_fr_register_by_index(
            rn_idx,
            f32::from_bits(f32::to_bits(self.get_fr_register_by_index(rn_idx)) ^ 0x80000000),
        );

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fcnvsd(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_dr_register_by_index(rn_idx >> 1, self.get_fpul() as f64);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fcnvds(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_fpul(f32::to_bits(
            self.get_dr_register_by_index(rn_idx >> 1) as f32
        ));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fsts(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        //  assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = instruction.opcode.n();
        self.set_fr_register_by_index(rn_idx, f32::from_bits(self.get_fpul()));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ftrc(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();
        if !self.get_fpscr().check_bit(19) {
            let rn = self.get_fr_register_by_index(rn_idx);
            let mut result = rn.min(2147483520.0f32) as i32 as u32;

            if result == 0x80000000 {
                if result as i32 > 0 {
                    result -= 1;
                }
            }

            self.set_fpul(result as u32);
        } else {
            assert!((rn_idx & 0x1) == 0);

            let rn = self.get_dr_register_by_index(rn_idx >> 1);
            let result = rn as i32 as u32;
            self.set_fpul(result);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fsca(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.registers.fpscr.check_bit(19));
        assert!(instruction.opcode.n() & 1 == 0);

        let rn_idx = instruction.opcode.n();
        let pi_idx = self.get_fpul() & 0xffff;

        let rads = pi_idx as f32 / 65536.0 * (2.0 * std::f32::consts::PI);

        let sin_value = rads.sin();
        let cos_value = rads.cos();

        self.set_fr_register_by_index(rn_idx, sin_value);
        self.set_fr_register_by_index(rn_idx + 1, cos_value);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn ftrv(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.registers.fpscr.check_bit(19));

        let rn_idx = (instruction.opcode.n() & 0xC) as usize;

        let v1 = self.get_xf_register_by_index(0) * self.get_fr_register_by_index(rn_idx)
            + self.get_xf_register_by_index(4) * self.get_fr_register_by_index(rn_idx + 1)
            + self.get_xf_register_by_index(8) * self.get_fr_register_by_index(rn_idx + 2)
            + self.get_xf_register_by_index(12) * self.get_fr_register_by_index(rn_idx + 3);

        let v2 = self.get_xf_register_by_index(1) * self.get_fr_register_by_index(rn_idx)
            + self.get_xf_register_by_index(5) * self.get_fr_register_by_index(rn_idx + 1)
            + self.get_xf_register_by_index(9) * self.get_fr_register_by_index(rn_idx + 2)
            + self.get_xf_register_by_index(13) * self.get_fr_register_by_index(rn_idx + 3);

        let v3 = self.get_xf_register_by_index(2) * self.get_fr_register_by_index(rn_idx)
            + self.get_xf_register_by_index(6) * self.get_fr_register_by_index(rn_idx + 1)
            + self.get_xf_register_by_index(10) * self.get_fr_register_by_index(rn_idx + 2)
            + self.get_xf_register_by_index(14) * self.get_fr_register_by_index(rn_idx + 3);

        let v4 = self.get_xf_register_by_index(3) * self.get_fr_register_by_index(rn_idx)
            + self.get_xf_register_by_index(7) * self.get_fr_register_by_index(rn_idx + 1)
            + self.get_xf_register_by_index(11) * self.get_fr_register_by_index(rn_idx + 2)
            + self.get_xf_register_by_index(15) * self.get_fr_register_by_index(rn_idx + 3);

        self.set_fr_register_by_index(rn_idx, v1);
        self.set_fr_register_by_index(rn_idx + 1, v2);
        self.set_fr_register_by_index(rn_idx + 2, v3);
        self.set_fr_register_by_index(rn_idx + 3, v4);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmac(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.registers.fpscr.check_bit(19));

        let rn_idx = instruction.opcode.n();
        let rm_idx = instruction.opcode.m();

        let frn = self.get_fr_register_by_index(rn_idx);
        let fr0 = self.get_fr_register_by_index(0);
        let frm = self.get_fr_register_by_index(rm_idx);

        self.set_fr_register_by_index(rn_idx, frn + (fr0 * frm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn flds(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rn_idx = instruction.opcode.n();

        self.set_fpul(f32::to_bits(self.get_fr_register_by_index(rn_idx)));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fsrra(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.registers.fpscr.check_bit(19));

        let rn_idx = instruction.opcode.n();
        let fr_value = self.get_fr_register_by_index(rn_idx);

        let result = if fr_value <= 0.0 {
            std::f32::NAN
        } else {
            1.0 / fr_value.sqrt()
        };

        self.set_fr_register_by_index(rn_idx, result);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fschg(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        // do this to avoid a fpu bank switch here
        self.registers.fpscr = self.get_fpscr().toggle_bit(20);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn frchg(&mut self, _: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));
        self.set_fpscr(self.get_fpscr().toggle_bit(21));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fipr(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = (instruction.opcode.n() & 0xc) as usize;
        let rm_idx = ((instruction.opcode.m() << 2) & 0xc) as usize;

        let mut idp = self.get_fr_register_by_index(rn_idx) * self.get_fr_register_by_index(rm_idx);
        idp += self.get_fr_register_by_index((rn_idx + 1) as usize)
            * self.get_fr_register_by_index((rm_idx + 1) as usize);

        idp += self.get_fr_register_by_index((rn_idx + 2) as usize)
            * self.get_fr_register_by_index((rm_idx + 2) as usize);

        idp += self.get_fr_register_by_index((rn_idx + 3) as usize)
            * self.get_fr_register_by_index((rm_idx + 3) as usize);

        self.set_fr_register_by_index((rn_idx + 3) as usize, idp);

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fldi1(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = instruction.opcode.n();
        self.set_fr_register_by_index(rn_idx, 1.0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fldi0(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rn_idx = instruction.opcode.n();
        self.set_fr_register_by_index(rn_idx, 0.0);
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    // validated
    pub fn fmov_load(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        if !self.get_fpscr().check_bit(20) {
            let value = bus.read_32(rm, context);
            self.set_fr_register_by_index(rn_idx, f32::from_bits(value));
        } else {
            let value = bus.read_64(rm, context);
            if (rn_idx & 0x1) == 0 {
                // fmov DRm, DRn
                self.set_dr_register_by_index(rn_idx >> 1, f64::from_bits(value));
            } else {
                // fmov XDm, DRn
                self.set_xd_register_by_index(rn_idx >> 1, f64::from_bits(value));
            }
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmov_index_load(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        // fixme: ??
        //   assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rm = self.get_register_by_index(rm_idx);
        let value = bus.read_32(self.get_register_by_index(0).wrapping_add(rm), context);

        self.set_fr_register_by_index(rn_idx, f32::from_bits(value));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    // validated
    pub fn fmov_index_store(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        //   assert!(!self.get_fpscr().check_bit(20));

        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);
        let frm = self.get_fr_register_by_index(rm_idx);

        bus.write_32(
            self.get_register_by_index(0).wrapping_add(rn),
            f32::to_bits(frm),
            context,
        );
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmov(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(20) {
            let rm = self.get_fr_register_by_index(rm_idx);
            self.set_fr_register_by_index(rn_idx, rm);
        } else {
            if (rn_idx & 0x01) == 0 && (rm_idx & 0x01) == 0 {
                self.set_dr_register_by_index(
                    rn_idx >> 1,
                    self.get_dr_register_by_index(rm_idx >> 1),
                );
            } else if (rn_idx & 0x01) == 1 && (rm_idx & 0x01) == 0 {
                self.set_xd_register_by_index(
                    rn_idx >> 1,
                    self.get_dr_register_by_index(rm_idx >> 1),
                );
            } else if (rn_idx & 0x01) == 0 && (rm_idx & 0x01) == 1 {
                self.set_dr_register_by_index(
                    rn_idx >> 1,
                    self.get_xd_register_by_index(rm_idx >> 1),
                );
            } else if (rn_idx & 0x01) == 1 && (rm_idx & 0x01) == 1 {
                self.set_xd_register_by_index(
                    rn_idx >> 1,
                    self.get_xd_register_by_index(rm_idx >> 1),
                );
            }
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmov_restore(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_register_by_index(rm_idx);
        if !self.get_fpscr().check_bit(20) {
            let val = bus.read_32(rm, context);
            self.set_fr_register_by_index(rn_idx, f32::from_bits(val));
            self.set_register_by_index(rm_idx, rm.wrapping_add(4));
        } else {
            let val = bus.read_64(rm, context);

            if (rn_idx & 0x1) == 0 {
                self.set_dr_register_by_index(rn_idx >> 1, f64::from_bits(val));
            } else {
                self.set_xd_register_by_index(rn_idx >> 1, f64::from_bits(val));
            }

            self.set_register_by_index(rm_idx, rm.wrapping_add(8));
        }
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmov_store(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();
        let rn = self.get_register_by_index(rn_idx);

        if !self.get_fpscr().check_bit(20) {
            let rm = self.get_fr_register_by_index(rm_idx);
            bus.write_32(rn, f32::to_bits(rm), context);
        } else {
            if (rm_idx & 0x1) == 0 {
                let drm = self.get_dr_register_by_index(rm_idx >> 1);
                bus.write_64(rn, f64::to_bits(drm), context);
            } else {
                let xrm = self.get_xd_register_by_index(rm_idx >> 1);
                bus.write_64(rn, f64::to_bits(xrm), context);
            }
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fmov_save(
        &mut self,
        instruction: &DecodedInstruction,
        bus: &mut CpuBus,
        context: &mut Context,
    ) {
        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        if !self.get_fpscr().check_bit(20) {
            let rn = self.get_register_by_index(rn_idx).wrapping_sub(4);
            let rm = self.get_fr_register_by_index(rm_idx);

            bus.write_32(rn, f32::to_bits(rm), context);
            self.set_register_by_index(rn_idx, rn);
        } else {
            let rn = self.get_register_by_index(rn_idx).wrapping_sub(8);
            if (rm_idx & 0x1) == 0 {
                let drm = self.get_dr_register_by_index(rm_idx >> 1);
                bus.write_64(rn, f64::to_bits(drm), context);
            } else {
                let xdm = self.get_xd_register_by_index(rm_idx >> 1);
                bus.write_64(rn, f64::to_bits(xdm), context);
            }

            self.set_register_by_index(rn_idx, rn);
        }

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fcmpeq(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_fr_register_by_index(rm_idx);
        let rn = self.get_fr_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, !!(rn == rm)));

        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }

    pub fn fcmpgt(&mut self, instruction: &DecodedInstruction, _: &mut CpuBus, _: &mut Context) {
        assert!(!self.get_fpscr().check_bit(19));

        let rm_idx = instruction.opcode.m();
        let rn_idx = instruction.opcode.n();

        let rm = self.get_fr_register_by_index(rm_idx);
        let rn = self.get_fr_register_by_index(rn_idx);

        self.set_sr(self.get_sr().eval_bit(0, rn > rm));
        self.registers.current_pc = self.registers.current_pc.wrapping_add(2);
    }
}
