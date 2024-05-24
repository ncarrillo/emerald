use std::collections::HashMap;

pub mod texture_cache;

use crate::{
    hw::{
        extensions::BitManipulation,
        holly::{pvr::texture_cache::Texture, HollyEventData},
        sh4::{bus::PhysicalAddress, intc},
    },
    scheduler::Scheduler,
};

use self::texture_cache::{TextureAtlas, TextureId};

#[derive(Copy, Clone, Debug)]
pub enum DisplayListItem {
    Clear(VertexColor),
    Triangle {
        verts: [Vertex; 3],
        texture_id: Option<TextureId>,
    },
    Quad {
        verts: [Vertex; 4],
    },
}

pub struct TextureDefinition {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

#[derive(Copy, Default, Clone, Debug)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub color: VertexColor,
}

#[derive(Copy, Default, Clone, Debug)]
pub struct VertexColor {
    pub raw: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Copy, Default, Clone, Debug)]
pub struct PvrRegisters {
    pub isp_backgnd_t: u32,
    pub region_base: u32,
    pub param_base: u32,
}

impl PvrRegisters {
    pub fn new() -> Self {
        PvrRegisters {
            ..Default::default()
        }
    }
}

impl VertexColor {
    fn from_color(color: u32) -> VertexColor {
        VertexColor {
            raw: color,
            r: ((color >> 16) & 0xFF) as f32,
            g: ((color >> 8) & 0xFF) as f32,
            b: (color & 0xFF) as f32,
        }
    }
}

pub struct Pvr {
    pub registers: PvrRegisters,
    pub pending_data: Vec<u32>,
    pub context: DrawingContext,
    pub vram: Vec<u8>,
    pub starting_offset: u32,
    pub depth: u32,
    pub width: usize,
    pub height: usize,
    pub pending_display_list: HashMap<u32, Vec<DisplayListItem>>,
    pub last_rendered_list: Vec<DisplayListItem>,
    pub texture_atlas: TextureAtlas,
}

#[derive(Copy, Clone, Debug)]
pub enum PolygonType {
    PolygonType0(PolygonType0),
    Sprite,
}

impl PolygonType {
    pub fn texture_control(&self) -> u32 {
        match self {
            PolygonType::PolygonType0(pt0) => pt0.texture_control,
            PolygonType::Sprite => 0, // fixme
        }
    }

    pub fn tsp(&self) -> u32 {
        match self {
            PolygonType::PolygonType0(pt0) => pt0.tsp_instruction,
            PolygonType::Sprite => 0, // fixme
        }
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct PolygonType0 {
    parameter_control_word: u32,
    isp_tsp_instruction: u32,
    tsp_instruction: u32,
    texture_control: u32,
    _ignored: u64,
    data_size: u32,
    next_address: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ColorType {
    Packed,
    Floating,
    IntensityMode1,
    IntensityMode2,
}

pub struct VertexParameterType {
    parameter_control_word: u32,
}

#[derive(Copy, Clone, Debug)]
pub enum TextureFormat {
    Rgb1555,
    Rgb565,
    Rgb4444,
    Yuv422,
    BumpMap,
    Palette4bpp,
    Palette8bpp,
}

#[derive(Copy, Clone, Debug)]
pub enum FramebufferFormat {
    Rgb0555,
    Rgb565,
    Rgb888,
    Rgb0888,
}

pub struct DrawingContext {
    list_type: u32,
    textured: bool,
    pub polygon_type: Option<PolygonType>,
    pub strip_count: usize,
    verts: Vec<Vertex>,
    pub display_list_id: u32,
    texture_id: Option<TextureId>,
    texture_format: Option<TextureFormat>,
    pending_sprite_words: Vec<u32>, // some vertex data is more than 32 bytes?
}

impl DrawingContext {
    pub fn new() -> Self {
        Self {
            list_type: 0,
            verts: vec![],
            polygon_type: None,
            strip_count: 0,
            textured: false,
            texture_format: None,
            pending_sprite_words: vec![],
            display_list_id: 0,
            texture_id: None,
        }
    }
}

impl Pvr {
    pub fn new() -> Self {
        Self {
            registers: PvrRegisters::new(),
            pending_data: vec![],
            vram: vec![0; 0x3fffffff],
            context: DrawingContext::new(),
            starting_offset: 0,
            width: 0,
            height: 0,
            depth: 0,
            pending_display_list: HashMap::new(),
            last_rendered_list: vec![],
            texture_atlas: TextureAtlas::new(1024, 1024),
        }
    }

    fn average_color(colors: [u32; 3]) -> u32 {
        let (mut total_r, mut total_g, mut total_b, mut total_a) = (0, 0, 0, 0);

        for color in colors.iter() {
            total_a += (*color >> 24) & 0xFF;
            total_r += (*color >> 16) & 0xFF;
            total_g += (*color >> 8) & 0xFF;
            total_b += *color & 0xFF;
        }

        let avg_a = (total_a / 3) as u32;
        let avg_r = (total_r / 3) as u32;
        let avg_g = (total_g / 3) as u32;
        let avg_b = (total_b / 3) as u32;

        (avg_a << 24) | (avg_r << 16) | (avg_g << 8) | avg_b
    }

    pub fn receive_ta_fifo_dma_data(&mut self, scheduler: &mut Scheduler, data: &mut [u32]) {
        self.handle_cmd(scheduler, data);
    }

    pub fn render(&mut self, display_list_id: u32) {
        self.texture_atlas.increment_generation();
        let display_list = self
            .pending_display_list
            .remove(&display_list_id)
            .unwrap_or_else(Vec::new);

        // render the backdrop. this is done by pulling 3 verts from isp_backgnd_t
        let tag_addr = (self.registers.isp_backgnd_t & 0xFFFFF8) >> 3;
        let tag_offset = self.registers.isp_backgnd_t & 0x3;
        let tag_skip = (self.registers.isp_backgnd_t & 0x7000000) >> 24;
        let backgnd_addr = self.registers.param_base + 4 * tag_addr;

        let isp_tsp = u32::from_le_bytes([
            self.vram[backgnd_addr as usize],
            self.vram[(backgnd_addr + 1) as usize],
            self.vram[(backgnd_addr + 2) as usize],
            self.vram[(backgnd_addr + 3) as usize],
        ]);

        let tsp = u32::from_le_bytes([
            self.vram[(backgnd_addr + 4) as usize],
            self.vram[(backgnd_addr + 5) as usize],
            self.vram[(backgnd_addr + 6) as usize],
            self.vram[(backgnd_addr + 7) as usize],
        ]);

        let texture_control = u32::from_le_bytes([
            self.vram[(backgnd_addr + 8) as usize],
            self.vram[(backgnd_addr + 9) as usize],
            self.vram[(backgnd_addr + 10) as usize],
            self.vram[(backgnd_addr + 11) as usize],
        ]);

        // needed to derive the vert from the offset
        let skipped_vert_size = 4 * (3 + 2 * tag_skip);
        let vert_index = backgnd_addr + 12 + tag_offset * skipped_vert_size;
        let vert_size = 4 * (3 + 1);
        let is_backgnd_offset = isp_tsp.check_bit(25);
        let is_backgnd_textured = isp_tsp.check_bit(26);

        // fixme: fix offsets if the primitive is textured

        let mut backgnd_verts = vec![];
        for i in 0..3 {
            let x = f32::from_bits(u32::from_le_bytes([
                self.vram[(vert_index + i * vert_size) as usize],
                self.vram[(vert_index + i * vert_size + 1) as usize],
                self.vram[(vert_index + i * vert_size + 2) as usize],
                self.vram[(vert_index + i * vert_size + 3) as usize],
            ]));

            let y = f32::from_bits(u32::from_le_bytes([
                self.vram[(vert_index + i * vert_size + 4) as usize],
                self.vram[(vert_index + i * vert_size + 5) as usize],
                self.vram[(vert_index + i * vert_size + 6) as usize],
                self.vram[(vert_index + i * vert_size + 7) as usize],
            ]));

            let z = f32::from_bits(u32::from_le_bytes([
                self.vram[(vert_index + i * vert_size + 8) as usize],
                self.vram[(vert_index + i * vert_size + 9) as usize],
                self.vram[(vert_index + i * vert_size + 10) as usize],
                self.vram[(vert_index + i * vert_size + 11) as usize],
            ]));

            let color = u32::from_le_bytes([
                self.vram[(vert_index + i * vert_size + 12) as usize],
                self.vram[(vert_index + i * vert_size + 13) as usize],
                self.vram[(vert_index + i * vert_size + 14) as usize],
                self.vram[(vert_index + i * vert_size + 15) as usize],
            ]);

            backgnd_verts.push(Vertex {
                x: x,
                y: y,
                z: 0.,
                u: 0.,
                v: 0.,
                color: VertexColor::from_color(color),
            });
        }

        let a = &backgnd_verts[0];
        let b = &backgnd_verts[1];
        let c = &backgnd_verts[2];

        let dx = b.x + c.x - a.x;
        let dy = b.y + c.y - a.y;

        let color_d =
            VertexColor::from_color(Self::average_color([a.color.raw, b.color.raw, c.color.raw]));

        // fixme: revisit all of this bc its likely wrong
        let d = Vertex {
            x: dx,
            y: dy,
            z: 0.0,
            u: 0.0,
            v: 0.0,
            color: color_d,
        };

        // idk?
        backgnd_verts.push(d);

        self.rasterize_textured_triangle(
            [backgnd_verts[0], backgnd_verts[1], backgnd_verts[2]],
            None,
        );

        self.rasterize_textured_triangle(
            [backgnd_verts[1], backgnd_verts[2], backgnd_verts[3]],
            None,
        );

        // after rendering the backdrop, render the current  display list
        for dli in display_list.clone() {
            match dli {
                DisplayListItem::Triangle { verts, texture_id } => {
                    self.rasterize_textured_triangle(verts, texture_id);
                }
                DisplayListItem::Quad { verts } => {
                    self.rasterize_textured_triangle([verts[0], verts[1], verts[2]], None);
                    self.rasterize_textured_triangle([verts[1], verts[2], verts[3]], None);
                }
                _ => {}
            }
        }

        self.last_rendered_list = display_list;
    }

    pub fn receive_ta_data(&mut self, scheduler: &mut Scheduler, addr: PhysicalAddress, data: u32) {
        match addr.0 {
            0x10000000..=0x107FFFFF => {
                self.pending_data.push(data);
                if (self.pending_data.len() % 8) == 0 {
                    let data_to_process = std::mem::take(&mut self.pending_data);
                    self.handle_cmd(scheduler, &data_to_process);
                }
            }
            0x11000000..=0x117FFFFF => {
                // Calculate the base index for the write
                let base_index = (addr.0 - 0x11000000) as usize;

                // Extract and write each byte of the u32 to consecutive bytes in the array
                self.vram[base_index] = (data & 0x000000FF) as u8; // Least significant byte
                self.vram[base_index + 1] = ((data >> 8) & 0x000000FF) as u8; // Second byte
                self.vram[base_index + 2] = ((data >> 16) & 0x000000FF) as u8; // Third byte
                self.vram[base_index + 3] = ((data >> 24) & 0x000000FF) as u8; // Most significant byte
            }
            _ => {
                panic!("got a non-Polygon PVR SQ write.");
            }
        }
    }

    pub fn get_vertex_type(&self, data: &[u32], obj_control: u16) {
        let textured = obj_control.check_bit(3);
        let col_type = match (obj_control & 0x30) >> 4 {
            0 => ColorType::Packed,
            1 => ColorType::Floating,
            2 => ColorType::IntensityMode1,
            3 => ColorType::IntensityMode2,
            _ => unreachable!(),
        };
    }

    fn get_texel(&mut self, texture_id: TextureId, tx: f32, ty: f32) -> u32 {
        let (texture_metadata, texture_data) =
            self.texture_atlas.get_texture_slice(texture_id).unwrap();

        // Ensure tx and ty are correctly normalized and mapped
        let tx =
            (tx * texture_metadata.width as f32).clamp(0.0, texture_metadata.width as f32 - 1.0);
        let ty =
            (ty * texture_metadata.height as f32).clamp(0.0, texture_metadata.height as f32 - 1.0);

        // Convert to integer coordinates
        let tx = tx as u32;
        let ty = ty as u32;

        // Calculate the index in the texture data array
        let texel_index = (ty as usize * texture_metadata.width as usize) + tx as usize;

        // Ensure the index is within bounds
        assert!(texel_index < texture_data.len() / 4);

        let bytes = &texture_data[texel_index * 4..texel_index * 4 + 4];
        let texel = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        Self::rgb8888_to_rgb565(texel) as u32
    }

    fn rgb565_to_rgb555(rgb565: u16) -> u16 {
        let r = (rgb565 >> 11) & 0x1F;
        let g = (rgb565 >> 5) & 0x3F;
        let b = rgb565 & 0x1F;

        let g_5bit = (g >> 1) & 0x1F;
        (r << 10) | (g_5bit << 5) | b
    }

    fn rgb8888_to_rgb555(color: u32) -> u16 {
        let r = (((color >> 16) & 0xFF) as f32) / 255.0;
        let g = (((color >> 8) & 0xFF) as f32) / 255.0;
        let b = ((color & 0xFF) as f32) / 255.0;
        let r = (r * 31.) as u16 & 0x1F;
        let g = (g * 31.) as u16 & 0x1F;
        let b = (b * 31.) as u16 & 0x1F;

        ((r << 10) | (g << 5) | b) as u16
    }

    fn rgb8888_to_rgb565(color: u32) -> u16 {
        let r = (((color >> 24) & 0xFF) as f32) / 255.0;
        let g = (((color >> 16) & 0xFF) as f32) / 255.0;
        let b = (((color >> 8) & 0xFF) as f32) / 255.0;

        let r = (r * 31.0) as u16 & 0x1F;
        let g = (g * 63.0) as u16 & 0x3F;
        let b = (b * 31.0) as u16 & 0x1F;

        ((r << 11) | (g << 5) | b) as u16
    }

    fn rgb4444_to_rgb565(color: u16) -> u16 {
        let r = (color >> 12) & 0x0F;
        let g = (color >> 8) & 0x0F;
        let b = (color >> 4) & 0x0F;
        let r = (r * 255 / 15) * 31 / 255;
        let g = (g * 255 / 15) * 63 / 255;
        let b = (b * 255 / 15) * 31 / 255;

        return 0;
    }

    fn rasterize_textured_triangle(
        &mut self,
        mut verts: [Vertex; 3],
        texture_id: Option<TextureId>,
    ) {
        let mut a = verts[0];
        let (mut b, mut c) = (verts[1], verts[2]);

        let drawing_area_top = 0.0;
        let drawing_area_right = 640 as f32;
        let drawing_area_bottom = 480 as f32;

        let xmin = f32::max(Self::min3f(a.x, b.x, c.x), 0.0) as u32;
        let ymin = f32::max(Self::min3f(a.y, b.y, c.y), drawing_area_top) as u32;
        let xmax = f32::min(Self::max3f(a.x, b.x, c.x), drawing_area_right) as u32;
        let ymax = f32::min(Self::max3f(a.y, b.y, c.y), drawing_area_bottom) as u32;

        let mut area = Self::edge_function(a, b, c);

        // change the winding order
        if area < 0.0 {
            verts.swap(1, 2);
            a = verts[0];
            b = verts[1];
            c = verts[2];

            area = -area;
        }

        for iy in ymin..=ymax {
            for ix in xmin..=xmax {
                let p = Vertex {
                    x: ix as f32,
                    y: iy as f32,
                    z: 1.0,
                    u: 0.0,
                    v: 0.0,
                    color: a.color,
                };

                let w0 = Self::edge_function(b, c, p) / area;
                let w1 = Self::edge_function(c, a, p) / area;
                let w2 = Self::edge_function(a, b, p) / area;

                if (w0 < 0.) || (w1 < 0.) || (w2 < 0.) {
                    continue;
                }

                if area == 0. {
                    continue;
                }

                let mut final_pixel = if !texture_id.is_some() {
                    let r = a.color.r as f32 * w0 + b.color.r as f32 * w1 + c.color.r as f32 * w2;
                    let g = a.color.g as f32 * w0 + b.color.g as f32 * w1 + c.color.g as f32 * w2;
                    let b = a.color.b as f32 * w0 + b.color.b as f32 * w1 + c.color.b as f32 * w2;
                    let rgb =
                        (((r * 1.) as u32) << 16) | (((g * 1.) as u32) << 8) | (b * 1.) as u32;

                    // fixme: formats
                    Self::rgb8888_to_rgb565(rgb)
                } else {
                    let tx = (a.u * w0 + b.u * w1 + c.u * w2) as f32;
                    let ty = (a.v * w0 + b.v * w1 + c.v * w2) as f32;
                    let texel = self.get_texel(texture_id.unwrap(), tx, ty) as u16;

                    texel
                };

                if final_pixel != 0 {
                    let i_buf = (self.starting_offset + iy * (self.width as u32) * 2 + ix) as usize;
                    let pixels = u16::to_le_bytes(final_pixel);

                    self.vram[(i_buf)] = pixels[1];
                    self.vram[(i_buf) + 1] = pixels[0];
                }
            }
        }
    }

    fn edge_function(a: Vertex, b: Vertex, c: Vertex) -> f32 {
        ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x))
    }

    fn min3f(x: f32, y: f32, z: f32) -> f32 {
        x.min(y).min(z)
    }

    fn max3f(x: f32, y: f32, z: f32) -> f32 {
        x.max(y).max(z)
    }

    fn write_vram_16(&mut self, addr: usize, value: u16) {
        self.vram[addr] = (value & 0xFF) as u8;
        self.vram[addr + 1] = (value >> 8) as u8;
    }

    fn write_vram_32(&mut self, addr: usize, value: u32) {
        self.write_vram_16(addr, (value & 0xffff) as u16);
        self.write_vram_16(addr + 2, (value >> 16) as u16);
    }

    pub fn get_polygon_type(&self, data: &[u32], obj_control: u16) -> PolygonType {
        let textured = obj_control.check_bit(3);
        let col_type = match (obj_control & 0x30) >> 4 {
            0 => ColorType::Packed,
            1 => ColorType::Floating,
            2 => ColorType::IntensityMode1,
            3 => ColorType::IntensityMode2,
            _ => unreachable!(),
        };

        match (textured, col_type) {
            _ => PolygonType::PolygonType0(unsafe { *(data.as_ptr() as *const PolygonType0) }),
            //   _ => panic!("unexpected polygon type")
        }
    }

    pub fn handle_cmd(&mut self, scheduler: &mut Scheduler, data: &[u32]) {
        let pcw = data[0];
        let parameter_type = (pcw & 0xE0000000) >> 29;
        let list_type = (pcw & 0x7000000) >> 24;
        let obj_control = ((pcw & 0xffff) & 0b00000000_0_1_11_1_1_0_1) as u16;
        let group_control = ((pcw & 0xff0000) >> 16) as u16;

        match parameter_type {
            0x01 => {
                #[cfg(feature = "log_pvr")]
                println!("pvr: received user tile clip");
            }
            0x02 => {
                #[cfg(feature = "log_pvr")]
                println!("pvr: received object list");
            }
            0x04 => {
                self.context.list_type = list_type;
                self.context.strip_count = match (((group_control & 0xc) >> 2) as usize) {
                    0 => 1,
                    1 => 2,
                    2 => 4,
                    3 => 6,
                    _ => unreachable!(),
                };

                self.context.polygon_type = Some(self.get_polygon_type(data, obj_control));
                self.context.textured = obj_control.check_bit(3);

                if self.context.textured {
                    let format =
                        (self.context.polygon_type.unwrap().texture_control() & 0x38000000) >> 27;
                    self.context.texture_format = Some(match format {
                        0 | 7 => TextureFormat::Rgb1555,
                        1 => TextureFormat::Rgb565,
                        2 => TextureFormat::Rgb4444,
                        3 => TextureFormat::Yuv422,
                        4 => TextureFormat::BumpMap,
                        5 => TextureFormat::Palette4bpp,
                        6 => TextureFormat::Palette8bpp,
                        _ => unreachable!("unexpected format {}", format),
                    });
                }

                if self.context.textured {
                    let texture_addr = ((self.context.polygon_type.unwrap().texture_control()
                        & 0x1FFFFF) as usize)
                        * 8;
                    let tsp = self.context.polygon_type.unwrap().tsp();
                    let width = match tsp & 0x7 {
                        0 => 8,
                        1 => 16,
                        2 => 32,
                        3 => 64,
                        4 => 128,
                        5 => 256,
                        6 => 512,
                        7 => 1024,
                        _ => unreachable!("{}", tsp & 0x7),
                    };

                    let height = match (tsp & 0x38) >> 3 {
                        0 => 8,
                        1 => 16,
                        2 => 32,
                        3 => 64,
                        4 => 128,
                        5 => 256,
                        6 => 512,
                        7 => 1024,
                        _ => unreachable!(),
                    };

                    let texture_size = (width * height * 4) as usize; // Assuming 4 bytes per pixel (RGBA8888)
                    let texture_data = &self.vram[texture_addr..texture_addr + texture_size];
                    let texture = Texture {
                        width: width as u32,
                        height: width as u32,
                        data: texture_data.to_vec(),
                    };

                    let texture_id = self
                        .texture_atlas
                        .upload_texture(texture, self.context.texture_format.unwrap())
                        .unwrap();

                    self.context.texture_id = Some(texture_id);
                } else {
                    self.context.texture_id = None;
                }

                #[cfg(feature = "log_pvr")]
                {
                    println!("pvr: received polygon start with list type {}", list_type);

                    if self.context.textured {
                        let texture_addr = ((self.context.polygon_type.unwrap().texture_control()
                            & 0x1FFFFF) as usize)
                            * 8;

                        let tsp = self.context.polygon_type.unwrap().tsp();
                        let width = match tsp & 0x7 {
                            0 => 8,
                            1 => 16,
                            2 => 32,
                            3 => 64,
                            4 => 128,
                            5 => 256,
                            6 => 512,
                            7 => 1024,
                            _ => unreachable!("{}", tsp & 0x7),
                        };

                        let height = match (tsp & 0x38) >> 3 {
                            0 => 8,
                            1 => 16,
                            2 => 32,
                            3 => 64,
                            4 => 128,
                            5 => 256,
                            6 => 512,
                            7 => 1024,
                            _ => unreachable!(),
                        };
                        println!(
                            "\t- texture @ 0x{:08x}, size={}x{}, format={:#?}",
                            texture_addr, width, height, self.context.texture_format
                        );
                    } else {
                    }
                }
            }
            0x05 => {
                #[cfg(feature = "log_pvr")]
                println!("pvr: received sprite list start");
                self.context.polygon_type = Some(PolygonType::Sprite);
            }
            0x07 => {
                match self.context.polygon_type.unwrap() {
                    PolygonType::PolygonType0(pt0) => {
                        self.get_vertex_type(data, obj_control);
                        self.context.verts.push(Vertex {
                            x: f32::from_bits(data[1]),
                            y: f32::from_bits(data[2]),
                            z: f32::from_bits(data[3]),
                            u: f32::from_bits(data[4]),
                            v: f32::from_bits(data[5]),
                            color: VertexColor::from_color(data[6]),
                        });

                        #[cfg(feature = "log_pvr")]
                        println!("pvr: received vertex");

                        let tsp = self.context.polygon_type.unwrap().tsp();
                        let width = match tsp & 0x7 {
                            0 => 8,
                            1 => 16,
                            2 => 32,
                            3 => 64,
                            4 => 128,
                            5 => 256,
                            6 => 512,
                            7 => 1024,
                            _ => unreachable!("{}", tsp & 0x7),
                        };

                        let height = match (tsp & 0x38) >> 3 {
                            0 => 8,
                            1 => 16,
                            2 => 32,
                            3 => 64,
                            4 => 128,
                            5 => 256,
                            6 => 512,
                            7 => 1024,
                            _ => unreachable!(),
                        };

                        if pcw.check_bit(28) {
                            let data_to_process = std::mem::take(&mut self.context.verts);

                            // Process the vertices into individual triangles
                            if data_to_process.len() >= 3 {
                                // Iterate from the third vertex to the end
                                for i in 2..data_to_process.len() {
                                    let triangle = (
                                        data_to_process[i - 2],
                                        data_to_process[i - 1],
                                        data_to_process[i],
                                    );

                                    let v1 = triangle.0;
                                    let v2 = triangle.1;
                                    let v3 = triangle.2;

                                    #[cfg(feature = "log_pvr")]
                                    println!("pvr: inserted tri into DL");

                                    let display_list_item = DisplayListItem::Triangle {
                                        verts: [v1, v2, v3],
                                        texture_id: self.context.texture_id,
                                    };

                                    let texture_addr =
                                        ((self.context.polygon_type.unwrap().texture_control()
                                            & 0x1FFFFF)
                                            as usize)
                                            * 8;

                                    let key = self.context.display_list_id;
                                    self.pending_display_list
                                        .entry(key)
                                        .or_insert_with(Vec::new)
                                        .push(display_list_item);
                                }
                            } else {
                                println!("pvr: not enough data to process");
                            }
                        }
                    }
                    PolygonType::Sprite => {
                        self.context.pending_sprite_words.push(data[0]);
                        self.context.pending_sprite_words.push(data[1]);
                        self.context.pending_sprite_words.push(data[2]);
                        self.context.pending_sprite_words.push(data[3]);
                        self.context.pending_sprite_words.push(data[4]);
                        self.context.pending_sprite_words.push(data[5]);
                        self.context.pending_sprite_words.push(data[6]);
                        self.context.pending_sprite_words.push(data[7]);

                        if self.context.pending_sprite_words.len() == 16 {
                            let sprite_data =
                                std::mem::take(&mut self.context.pending_sprite_words);

                            let mut idx = 0;
                            for w in &sprite_data {
                                idx += 1
                            }

                            let ax = f32::from_bits(sprite_data[1]);
                            let ay = f32::from_bits(sprite_data[2]);
                            let az = f32::from_bits(sprite_data[3]);
                            let bx = f32::from_bits(sprite_data[4]);
                            let by = f32::from_bits(sprite_data[5]);
                            let bz = f32::from_bits(sprite_data[6]);
                            let cx = f32::from_bits(sprite_data[7]);
                            let cy = f32::from_bits(sprite_data[9]);
                            let cz = f32::from_bits(sprite_data[10]);
                            let dx = f32::from_bits(sprite_data[11]);
                            let dy = f32::from_bits(sprite_data[12]);

                            let v1 = Vertex {
                                x: ax,
                                y: ay,
                                z: az,
                                u: 0.,
                                v: 0.,
                                color: VertexColor::from_color(0xbaba),
                            };

                            let v2 = Vertex {
                                x: bx,
                                y: by,
                                z: bz,
                                u: 0.,
                                v: 0.,
                                color: VertexColor::from_color(0xbaba),
                            };

                            let v3 = Vertex {
                                x: cx,
                                y: cy,
                                z: cz,
                                u: 0.,
                                v: 0.,
                                color: VertexColor::from_color(0xbaba),
                            };

                            let v4 = Vertex {
                                x: dx,
                                y: dy,
                                z: cz,
                                u: 0.,
                                v: 0.,
                                color: VertexColor::from_color(0xceba),
                            };

                            let display_list_item = DisplayListItem::Quad {
                                verts: [v1, v2, v3, v4],
                            };

                            let key = self.context.display_list_id;
                            self.pending_display_list
                                .entry(key)
                                .or_insert_with(Vec::new)
                                .push(display_list_item);

                        } else {
                        }
                    }
                }
            }
            0 => {
                #[cfg(feature = "log_pvr")]
                println!("pvr: received end of list");
                self.context.verts.clear();

                // fix me: set the right istnrm bit depending on the type of list
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 200,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: match self.context.list_type {
                            0 => 0.set_bit(7),  // opaque
                            1 => 0.set_bit(8),  // opaque modifier vol
                            2 => 0.set_bit(9),  // transluscent
                            3 => 0.set_bit(10), // transluscent modifier vol
                            4 => 0.set_bit(21), // punch through
                            _ => unimplemented!(),
                        },
                    },
                });
            }
            _ => panic!("pvr: unhandled parameter type {:08x}!", parameter_type),
        }
    }
}
