use crate::{
    hw::{extensions::BitManipulation, holly::HollyEventData},
    scheduler::Scheduler,
};

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

impl VertexColor {
    pub fn from_color(color: u32) -> Self {
        let r = ((color >> 16) & 0xFF) as f32 / 255.;
        let g = ((color >> 8) & 0xFF) as f32 / 255.;
        let b = (color & 0xFF) as f32 / 255.;
        VertexColor {
            raw: color,
            r,
            g,
            b,
        }
    }
}

pub struct Pvr {
    pub pending_data: Vec<u32>,
    pub context: DrawingContext,
    pub vram: Vec<u8>,
    pub starting_offset: u32,
    pub depth: u32,
}

#[derive(Copy, Clone, Debug)]
pub enum PolygonType {
    PolygonType0(PolygonType0),
}

impl PolygonType {
    pub fn texture_control(&self) -> u32 {
        match self {
            PolygonType::PolygonType0(pt0) => pt0.texture_control,
        }
    }

    pub fn tsp(&self) -> u32 {
        match self {
            PolygonType::PolygonType0(pt0) => pt0.tsp_instruction,
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

#[derive(Copy, Clone, Debug)]
pub enum ColorType {
    Packed,
    Floating,
    IntensityMode1,
    IntensityMode2,
}

pub struct VertexParameterType {
    parameter_control_word: u32,
}

pub struct DrawingContext {
    list_type: u32,
    polygon_type: Option<PolygonType>,
    verts: Vec<Vertex>,
}

impl DrawingContext {
    pub fn new() -> Self {
        Self {
            list_type: 0,
            verts: vec![],
            polygon_type: None,
        }
    }
}

impl Pvr {
    pub fn new() -> Self {
        Self {
            pending_data: vec![],
            vram: vec![0; 0x3fffffff],
            context: DrawingContext::new(),
            starting_offset: 0,
            depth: 0,
        }
    }

    pub fn receive_ta_fifo_dma_data(&mut self, scheduler: &mut Scheduler, data: &mut [u32]) {
        //  panic!("pvr: received TA fifo data from dma {}", data.len() % 8);
        self.handle_cmd(scheduler, data);
    }

    pub fn receive_ta_data(&mut self, scheduler: &mut Scheduler, data: u32) {
        self.pending_data.push(data);

        if self.pending_data.len() == 8 {
            let data_to_process = std::mem::take(&mut self.pending_data);
            self.handle_cmd(scheduler, &data_to_process);
        }
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

    fn get_texel(&self, tx: u32, ty: u32, texture_width: u32) -> u16 {
        let tx = tx % texture_width;
        let ty = ty % texture_width;
        let offset = (self.context.polygon_type.unwrap().texture_control() & 0x1FFFFF) as usize;

        let texel_address = offset*2 + (ty as usize * texture_width as usize) + (tx as usize);
        self.read_vram_16(texel_address)
    }

    fn read_vram_16(&self, addr: usize) -> u16 {
        let lower = self.vram[addr] as u16;
        let upper = self.vram[addr + 1] as u16;

        (upper << 8) | lower
    }

    fn rgb565_to_rgb555(rgb565: u16) -> u16 {
        let r = (rgb565 >> 11) & 0x1F;
        let g = (rgb565 >> 5) & 0x3F;
        let b = rgb565 & 0x1F;

        let g_5bit = (g >> 1) & 0x1F;
        (r << 10) | (g_5bit << 5) | b
    }

    fn rasterize_textured_triangle(
        &mut self,
        mut verts: [Vertex; 3],
        texture_width: u32,
        texture_height: u32,
    ) {
        let mut a = verts[0];
        let (mut b, mut c) = if verts[1].y < verts[2].y {
            (verts[1], verts[2])
        } else {
            (verts[2], verts[1])
        };

        if a.y > b.y {
            std::mem::swap(&mut a, &mut b);
        }
        if a.y > c.y {
            std::mem::swap(&mut a, &mut c);
        }
        if b.y > c.y {
            std::mem::swap(&mut b, &mut c);
        }

        let drawing_area_top = 0.0;
        let drawing_area_right = 640.0;
        let drawing_area_bottom = 480.0;

        let xmin = f32::max(Self::min3f(a.x, b.x, c.x), 0.0) as u32;
        let ymin = f32::max(Self::min3f(a.y, b.y, c.y), drawing_area_top) as u32;

        let xmax = f32::min(Self::max3f(a.x, b.x, c.x), drawing_area_right as f32) as u32;
        let ymax = f32::min(Self::max3f(a.y, b.y, c.y), drawing_area_bottom as f32) as u32;

        let mut area = Self::edge_function(a, b, c);
        if (area < 0.0) {
            let tmp = verts[2];
            verts[2] = verts[1];
            verts[1] = tmp;
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

                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }

                let tx =
                    ((a.u * w0 + b.u * w1 + c.u * w2) * (texture_width - 1) as f32).round() as u32;
                let ty =
                    ((a.v * w0 + b.v * w1 + c.v * w2) * (texture_height - 1) as f32).round() as u32;

                let texel = self.get_texel(tx, ty, texture_width);
                if texel != 0 {
                    let i_buf = iy * 640 + ix;
                    self.write_vram_16(
                        (self.starting_offset + i_buf) as usize,
                        Self::rgb565_to_rgb555(texel),
                    );
                }
            }
        }
    }

    fn edge_function(a: Vertex, b: Vertex, p: Vertex) -> f32 {
        -((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x))
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

    pub fn handle_cmd(&mut self, scheduler: &mut Scheduler, data: &[u32]) {
        assert!((data.len() % 8) == 0); // dmas should be at a multiple of 8 bytes

        let pcw = data[0];
        let parameter_type = (pcw & 0xE0000000) >> 29;
        let list_type = (pcw & 0x7000000) >> 24;
        let obj_control = ((pcw & 0xffff) & 0b00000000_0_1_11_1_1_0_1) as u16;

        match parameter_type {
            0x01 => {
                // user tile clip
            }
            0x02 => {
                // object List set
                //     println!("got object list");
            }
            0x04 => {
                self.context.list_type = list_type;
                self.context.polygon_type = Some(self.get_polygon_type(data, obj_control));
            }
            0x07 => {
                self.context.verts.push(Vertex {
                    x: f32::from_bits(data[1]),
                    y: f32::from_bits(data[2]),
                    z: f32::from_bits(data[3]),
                    u: f32::from_bits(data[4]),
                    v: f32::from_bits(data[5]),
                    color: VertexColor::from_color(0xffffff),
                });

                if self.context.verts.len() == 3 {
                    let data_to_process = std::mem::take(&mut self.context.verts);

                    let tsp = self.context.polygon_type.unwrap().tsp();
                    let width = match tsp & 0x7 {
                        6 => 512,
                        0 => 8,
                        5 => 256,
                        3 => 64,
                        _ => unreachable!("{}", tsp & 0x7),
                    };

                    let height = match (tsp & 0x38) >> 3 {
                        6 => 512,
                        0 => 8,
                        5 => 256,
                        3 => 64,
                        _ => unreachable!(),
                    };

                    self.rasterize_textured_triangle(
                        [data_to_process[0], data_to_process[1], data_to_process[2]],
                        width,
                        height,
                    );
                }
            }
            0 => {
                // fix me: set the right istnrm bit depending on the type of list
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: if self.context.list_type == 0 {
                            0.set_bit(7)
                        } else {
                            0.set_bit(9)
                        },
                    },
                });
            }
            _ => panic!("pvr: unhandled parameter type {:08x}!", parameter_type),
        }

        // the first thing we receive is the PCW or parameter control word
    }
}