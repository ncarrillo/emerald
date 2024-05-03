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
