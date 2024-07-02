#[derive(Copy, Clone, Debug, Default)]
pub struct FbLineStride {
    pub fb_line_stride: u32,
    pub raw: u32,
}

impl FbLineStride {
    pub fn new() -> Self {
        FbLineStride {
            fb_line_stride: 0,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbLineStride {
            fb_line_stride: data & 0x1ff,
            raw: data,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FramebufferDepth {
    Fbde_0555 = 0, // 0555, lower 3 bits on fb_concat
    Fbde_565 = 1,  // 565, lower 3 bits on fb_concat, [1:0] for G
    Fbde_888 = 2,  // 888, packed
    Fbde_C888 = 3, // C888, first byte used for chroma
}

impl FramebufferDepth {
    pub fn from_u32(data: u32) -> Self {
        match data {
            0 => FramebufferDepth::Fbde_0555,
            1 => FramebufferDepth::Fbde_565,
            2 => FramebufferDepth::Fbde_888,
            3 => FramebufferDepth::Fbde_C888,
            _ => panic!("Invalid framebuffer depth"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FbYClip {
    pub fb_y_clip_max: u32,
    pub fb_y_clip_min: u32,
    pub raw: u32,
}

impl FbYClip {
    pub fn new() -> Self {
        FbYClip {
            fb_y_clip_max: 0,
            fb_y_clip_min: 0,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbYClip {
            fb_y_clip_max: (data >> 16) & 0x3f,
            fb_y_clip_min: data & 0x3ff,
            raw: data,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FbXClip {
    pub fb_x_clip_max: u32,
    pub fb_x_clip_min: u32,
    pub raw: u32,
}

impl FbXClip {
    pub fn new() -> Self {
        FbXClip {
            fb_x_clip_max: 0,
            fb_x_clip_min: 0,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbXClip {
            fb_x_clip_max: (data >> 16) & 0x1f,
            fb_x_clip_min: data & 0x7ff,
            raw: data,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FbSize {
    pub x_size: u32,
    pub y_size: u32,
    pub modulus: u32,
    pub raw: u32,
}

impl FbSize {
    pub fn new() -> Self {
        FbSize {
            x_size: 0,
            y_size: 0,
            modulus: 0,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbSize {
            x_size: data & 0x3ff,
            y_size: (data >> 10) & 0x3ff,
            modulus: (data >> 20) & 0x3,
            raw: data,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FbWriteCtrl {
    pub fb_alpha_threshold: u32,
    pub fb_kval: u32,
    pub fb_dither: u32,
    pub fb_packmode: u32,
    pub raw: u32,
}

impl FbWriteCtrl {
    pub fn new() -> Self {
        FbWriteCtrl {
            fb_alpha_threshold: 0,
            fb_kval: 0,
            fb_dither: 0,
            fb_packmode: 0,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbWriteCtrl {
            fb_alpha_threshold: (data >> 16) & 0xff,
            fb_kval: (data >> 8) & 0xff,
            fb_dither: (data >> 4) & 0xf,
            fb_packmode: data & 0xf,
            raw: data,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FbReadCtrl {
    pub vclk_div: u8,
    pub fb_strip_buf_en: bool,
    pub fb_strip_size: u8,
    pub fb_chroma_threshold: u8,
    pub reserved: u8,
    pub fb_concat: u8,
    pub fb_depth: u8,
    pub fb_line_double: bool,
    pub fb_enable: bool,
    pub raw: u32,
}

impl FbReadCtrl {
    pub fn new() -> Self {
        FbReadCtrl {
            vclk_div: 0,
            fb_strip_buf_en: false,
            fb_strip_size: 0,
            fb_chroma_threshold: 0,
            reserved: 0,
            fb_concat: 0,
            fb_depth: 0,
            fb_line_double: false,
            fb_enable: false,
            raw: 0,
        }
    }

    pub fn from_raw(data: u32) -> Self {
        FbReadCtrl {
            vclk_div: ((data >> 24) & 0xff) as u8,
            fb_strip_buf_en: ((data >> 23) & 0x1) != 0,
            fb_strip_size: ((data >> 16) & 0x3f) as u8,
            fb_chroma_threshold: ((data >> 8) & 0xff) as u8,
            reserved: ((data >> 7) & 0x1) as u8,
            fb_concat: ((data >> 4) & 0x7) as u8,
            fb_depth: ((data >> 2) & 0x3) as u8,
            fb_line_double: ((data >> 1) & 0x1) != 0,
            fb_enable: (data & 0x1) != 0,
            raw: data,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FramebufferRegisters {
    pub read_ctrl: FbReadCtrl,
    pub write_ctrl: FbWriteCtrl,
    pub read_size: FbSize,
    pub base_address: u32,
    pub base_address2: u32,

    pub x_clip: FbXClip,
    pub y_clip: FbYClip,
    pub line_stride: FbLineStride,
}

impl Default for FramebufferRegisters {
    fn default() -> Self {
        FramebufferRegisters {
            read_ctrl: FbReadCtrl::from_raw(0x005F8044),
            read_size: FbSize::from_raw(0x00177e7f),
            write_ctrl: FbWriteCtrl::from_raw(0x005F8048),
            base_address: 0x005F8050, // fixme: is this right? update: no it is not
            base_address2: 0x005F8050, // fixme: is this right? update: no it is not
            x_clip: FbXClip::from_raw(0x005F8068),
            y_clip: FbYClip::from_raw(0x005F806C),
            line_stride: FbLineStride::from_raw(0x005F804C),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Framebuffer {
    pub registers: FramebufferRegisters,
    pub dirty: bool,
    pub watch_start: u32,
    pub watch_end: u32,
    pub watch_start1: u32,
    pub watch_end1: u32,
    pub interlaced: bool,
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer {
            dirty: false,
            registers: Default::default(),
            watch_start: 0,
            watch_end: 0,
            watch_start1: 0,
            watch_end1: 0,
            interlaced: false,
        }
    }

    pub fn get_parameters(&self) -> (u32, u32) {
        let size = (((self.registers.read_size.x_size + self.registers.read_size.modulus)
            * (self.registers.read_size.y_size + 1))
            * 4);

        match false {
            true => (self.registers.base_address2, size),
            false => (self.registers.base_address, size),
        }
    }

    pub fn invalidate_watches(&mut self) {
        let vram_mask = (8 * 1024 * 1024) - 1;
        let size = (((self.registers.read_size.x_size + self.registers.read_size.modulus)
            * (self.registers.read_size.y_size + 1))
            * 4);

        self.watch_start = self.registers.base_address & vram_mask;
        self.watch_end = self.watch_start + size;

        self.watch_start1 = self.registers.base_address2 & vram_mask;
        self.watch_end1 = self.watch_start1 + size;
        self.dirty = false;
    }

    pub fn notify_write(&mut self, addr: u32, value: u8) {
        let vram_mask = (8 * 1024 * 1024) - 1;
        let addr = addr & vram_mask;

        if !self.dirty
            && ((addr >= self.watch_start && addr < self.watch_end)
                || (addr >= self.watch_start1 && addr < self.watch_end1))
        {
            self.dirty = true;
        }
    }

    pub fn render_framebuffer(&self, vram: &[u8]) -> (Vec<u8>, u32, u32) {
        let fb_r_ctrl = self.registers.read_ctrl;
        let fb_r_size = self.registers.read_size;
        let fb_r_sof1 = self.registers.base_address;

        if fb_r_size.x_size == 0 || fb_r_size.y_size == 0 {
            return (Vec::new(), 0, 0);
        }

        let mut result = Vec::new();

        let mut width = (fb_r_size.x_size + 1) << 1; // in 16-bit words
        let height = 480; //fb_r_size.y_size + 1;
        let mut modulus = (fb_r_size.modulus - 1) << 1;

        let bpp: usize;
        match Some(FramebufferDepth::from_u32(fb_r_ctrl.fb_depth as u32)) {
            Some(FramebufferDepth::Fbde_0555) | Some(FramebufferDepth::Fbde_565) => {
                bpp = 2;
            }
            Some(FramebufferDepth::Fbde_888) => {
                bpp = 3;
                width = (width * 2) / 3; // in pixels
                modulus = (modulus * 2) / 3; // in pixels
            }
            Some(FramebufferDepth::Fbde_C888) => {
                bpp = 4;
                width /= 2; // in pixels
                modulus /= 2; // in pixels
            }
            _ => {
                panic!("Invalid framebuffer format");
            }
        }

        let vram_mask = (8 * 1024 * 1024) - 1;
        let mut addr = fb_r_sof1;

        match Some(FramebufferDepth::from_u32(fb_r_ctrl.fb_depth as u32)) {
            Some(FramebufferDepth::Fbde_0555) => {
                // 555 RGB
                for _ in 0..height {
                    for _ in 0..width {
                        let src =
                            u16::from_le_bytes([vram[addr as usize], vram[(addr + 1) as usize]]);
                        result.push(
                            ((((src >> 0) & 0x1F) << 3) + fb_r_ctrl.fb_concat as u16)
                                .try_into()
                                .unwrap(),
                        );
                        result.push(
                            ((((src >> 5) & 0x1F) << 3) + fb_r_ctrl.fb_concat as u16)
                                .try_into()
                                .unwrap(),
                        );
                        result.push(
                            ((((src >> 10) & 0x1F) << 3) + fb_r_ctrl.fb_concat as u16)
                                .try_into()
                                .unwrap(),
                        );
                        result.push(0xFF);
                        addr += bpp as u32;
                    }
                    addr += modulus as u32 * bpp as u32;
                }
            }
            Some(FramebufferDepth::Fbde_565) => {
                // 565 RGB
                for _ in 0..height {
                    for _ in 0..width {
                        let src =
                            u16::from_le_bytes([vram[addr as usize], vram[(addr + 1) as usize]]);

                        result.push(
                            ((((src >> 0) & 0x1F) << 3) + fb_r_ctrl.fb_concat as u16)
                                .try_into()
                                .unwrap(),
                        );
                        result.push(
                            ((((src >> 5) & 0x3F) << 2) + (fb_r_ctrl.fb_concat as u16 >> 1))
                                .try_into()
                                .unwrap(),
                        );
                        result.push(
                            ((((src >> 11) & 0x1F) << 3) + fb_r_ctrl.fb_concat as u16)
                                .try_into()
                                .unwrap(),
                        );
                        result.push(0xFF);
                        addr += bpp as u32;
                    }
                    addr += modulus as u32 * bpp as u32;
                }

                // panic!("");
            }
            Some(FramebufferDepth::Fbde_888) => {
                for _ in 0..height {
                    for _ in 0..width {
                        if addr & 1 != 0 {
                            let src = u32::from_le_bytes([
                                vram[(addr - 1) as usize],
                                vram[addr as usize],
                                vram[(addr + 1) as usize],
                                vram[(addr + 2) as usize],
                            ]);
                            result.push((src >> 16) as u8);
                            result.push((src >> 8) as u8);
                            result.push(src as u8);
                        } else {
                            let src = u32::from_le_bytes([
                                vram[addr as usize],
                                vram[(addr + 1) as usize],
                                vram[(addr + 2) as usize],
                                vram[(addr + 3) as usize],
                            ]);
                            result.push((src >> 24) as u8);
                            result.push((src >> 16) as u8);
                            result.push((src >> 8) as u8);
                        }
                        result.push(0xFF);
                        addr += bpp as u32;
                    }
                    addr += modulus as u32 * bpp as u32;
                }
            }
            Some(FramebufferDepth::Fbde_C888) => {
                // 0888 RGB
                for _ in 0..height {
                    for _ in 0..width {
                        let src = u32::from_le_bytes([
                            vram[addr as usize],
                            vram[(addr + 1) as usize],
                            vram[(addr + 2) as usize],
                            vram[(addr + 3) as usize],
                        ]);
                        result.push((src & 0xFF) as u8);
                        result.push(((src >> 8) & 0xFF) as u8);
                        result.push(((src >> 16) & 0xFF) as u8);
                        result.push(0xFF);
                        addr += bpp as u32;
                    }
                    addr += modulus as u32 * bpp as u32;
                }
            }
            _ => {}
        }

        (result, width, height)
    }
}
