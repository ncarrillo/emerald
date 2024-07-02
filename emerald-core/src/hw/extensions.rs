pub trait BitManipulation {
    fn set_bit(self, index: usize) -> Self;
    fn clear_bit(self, index: usize) -> Self;
    fn check_bit(self, index: usize) -> bool;
    fn eval_bit(self, index: usize, val: bool) -> Self;
    fn toggle_bit(self, index: usize) -> Self;
}

pub trait SliceExtensions {
    fn as_u32_slice_mut<'b>(self) -> &'b mut [u32];
}

pub trait SliceExtensions8 {
    fn as_u8_slice_mut<'b>(self) -> &'b mut [u8];
}

impl SliceExtensions for &mut [u8] {
    fn as_u32_slice_mut<'b>(self) -> &'b mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.as_mut_ptr() as *mut u32, self.len() / 4) }
    }
}

impl SliceExtensions8 for &mut [u32] {
    fn as_u8_slice_mut<'b>(self) -> &'b mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.as_mut_ptr() as *mut u8, self.len() * 4) }
    }
}

impl BitManipulation for u8 {
    fn set_bit(self, index: usize) -> Self {
        self | (1 << index)
    }

    fn clear_bit(self, index: usize) -> Self {
        self & !(1 << index)
    }

    fn check_bit(self, index: usize) -> bool {
        (self & (1 << index)) != 0
    }

    fn eval_bit(self, index: usize, val: bool) -> Self {
        if val {
            self.set_bit(index)
        } else {
            self.clear_bit(index)
        }
    }

    fn toggle_bit(self, index: usize) -> Self {
        self.eval_bit(index, !self.check_bit(index))
    }
}

impl BitManipulation for u16 {
    fn set_bit(self, index: usize) -> Self {
        self | (1 << index)
    }

    fn clear_bit(self, index: usize) -> Self {
        self & !(1 << index)
    }

    fn check_bit(self, index: usize) -> bool {
        (self & (1 << index)) != 0
    }

    fn eval_bit(self, index: usize, val: bool) -> Self {
        if val {
            self.set_bit(index)
        } else {
            self.clear_bit(index)
        }
    }

    fn toggle_bit(self, index: usize) -> Self {
        self.eval_bit(index, !self.check_bit(index))
    }
}

impl BitManipulation for u32 {
    fn set_bit(self, index: usize) -> Self {
        self | (1 << index)
    }

    fn clear_bit(self, index: usize) -> Self {
        self & !(1 << index)
    }

    fn check_bit(self, index: usize) -> bool {
        (self & (1 << index)) != 0
    }

    fn eval_bit(self, index: usize, val: bool) -> Self {
        if val {
            self.set_bit(index)
        } else {
            self.clear_bit(index)
        }
    }

    fn toggle_bit(self, index: usize) -> Self {
        if self.check_bit(index) {
            self.clear_bit(index)
        } else {
            self.set_bit(index)
        }
    }
}

pub trait BarrelShifter {
    fn barrel_shift(
        &self,
        shift_type: ShiftType,
        amount: usize,
        carry: &mut bool,
        immediate: bool,
    ) -> u32;
    fn lsl(&self, amount: usize, carry: &mut bool) -> u32;
    fn lsr(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32;
    fn asr(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32;
    fn ror(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32;
    fn add_signed_offset(&self, offset: i32) -> u32;
}

impl BarrelShifter for u32 {
    fn barrel_shift(
        &self,
        shift_type: ShiftType,
        amount: usize,
        carry: &mut bool,
        immediate: bool,
    ) -> u32 {
        match shift_type {
            ShiftType::Lsl => self.lsl(amount, carry),
            ShiftType::Lsr => self.lsr(amount, carry, immediate),
            ShiftType::Asr => self.asr(amount, carry, immediate),
            ShiftType::Ror => self.ror(amount, carry, immediate),
        }
    }

    fn lsl(&self, amount: usize, carry: &mut bool) -> u32 {
        if amount == 0 {
            return *self;
        }

        if amount >= 32 {
            if amount > 32 {
                *carry = false;
            } else {
                *carry = (*self & 1) != 0;
            }
            return 0;
        }

        *carry = ((*self << (amount - 1)) >> 31) != 0;
        *self << amount
    }

    fn lsr(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32 {
        let mut amount = amount;
        if amount == 0 {
            if immediate {
                amount = 32;
            } else {
                return *self;
            }
        }

        if amount >= 32 {
            if amount > 32 {
                *carry = false;
            } else {
                *carry = (*self >> 31) != 0;
            }
            return 0;
        }

        *carry = (*self >> (amount - 1)) & 1 != 0;
        *self >> amount
    }

    fn asr(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32 {
        let mut amount = amount;
        if amount == 0 {
            if immediate {
                amount = 32;
            } else {
                return *self;
            }
        }

        let msb = *self >> 31;

        if amount >= 32 {
            *carry = msb != 0;
            let val = 0xFFFFFFFF * msb;
            return val;
        }

        *carry = ((*self >> (amount - 1)) & 1) != 0;
        (*self >> amount) | ((0xFFFFFFFF * msb) << (32 - amount))
    }

    fn ror(&self, amount: usize, carry: &mut bool, immediate: bool) -> u32 {
        // ROR #0 equals to RRX #1
        let mut amount = amount;
        let mut val = *self;

        if amount != 0 || !immediate {
            if amount == 0 {
                return val;
            }

            amount %= 32;
            val = (val >> amount) | (val << (32 - amount));
            *carry = (val >> 31) != 0;
            val
        } else {
            let lsb = (val & 1) != 0;
            val = (val >> 1) | ((if *carry { 1 } else { 0 }) << 31);
            *carry = lsb;

            val
        }
    }

    fn add_signed_offset(&self, offset: i32) -> u32 {
        if offset >= 0 {
            self.wrapping_add(offset as u32)
        } else {
            self.wrapping_sub(offset.abs() as u32)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ShiftType {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

impl ShiftType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(ShiftType::Lsl),
            1 => Some(ShiftType::Lsr),
            2 => Some(ShiftType::Asr),
            3 => Some(ShiftType::Ror),
            _ => None,
        }
    }
}
