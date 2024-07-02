pub mod display_list;
pub mod framebuffer;
pub mod raster;
pub mod ta;
pub mod texture_cache;

use std::sync::{Arc, RwLock};

use display_list::{DisplayList, DisplayListBuilder, VertexDefinition};
use serde::{Deserialize, Serialize};
use ta::{ParameterControlWord, ParameterType, PolyParam, PvrListType, VertexParam, VertexType};

use crate::{
    hw::{extensions::BitManipulation, holly::HollyEventData, sh4::bus::PhysicalAddress},
    scheduler::Scheduler,
};

use self::texture_cache::{TextureAtlas, TextureId};

#[derive(Copy, Default, Clone, Debug)]
pub struct PvrRegisters {
    pub isp_backgnd_t: u32,
    pub isp_feed_cfg: u32,
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

pub struct Pvr {
    pub registers: PvrRegisters,
    pub parameter_buffer: [u32; 16],
    pub parameter_cursor: usize,
    pub context: DrawingContext,
    pub vram: Arc<RwLock<Vec<u8>>>,
    pub pram: Arc<RwLock<Vec<u8>>>,
    pub dlb: [DisplayListBuilder; 5],
    pub texture_atlas: Arc<RwLock<TextureAtlas>>,
    pub wireframe: bool,
}

pub struct DrawingContext {
    list_type: Option<PvrListType>,
}

impl DrawingContext {
    pub fn new() -> Self {
        Self { list_type: None }
    }
}

impl Pvr {
    pub fn new() -> Self {
        Self {
            registers: PvrRegisters::new(),
            vram: Arc::new(RwLock::new(vec![0; 0x3fffffff])),
            pram: Arc::new(RwLock::new(vec![0; 4096])),
            context: DrawingContext::new(),
            parameter_buffer: [0; 16],
            parameter_cursor: 0,
            dlb: [
                DisplayListBuilder::new(),
                DisplayListBuilder::new(),
                DisplayListBuilder::new(),
                DisplayListBuilder::new(),
                DisplayListBuilder::new(),
            ],
            texture_atlas: Arc::new(RwLock::new(TextureAtlas::new(4096, 4096))),
            wireframe: false,
        }
    }

    pub fn build_bg_verts(&self) -> [VertexDefinition; 4] {
        // render the backdrop. this is done by pulling 3 verts from isp_backgnd_t
        let tag_addr = (self.registers.isp_backgnd_t & 0xFFFFF8) >> 3;
        let tag_offset = self.registers.isp_backgnd_t & 0x3;
        let tag_skip = (self.registers.isp_backgnd_t & 0x7000000) >> 24;
        let backgnd_addr = self.registers.param_base + 4 * tag_addr;

        // needed to derive the vert from the offset
        let skipped_vert_size = 4 * (3 + 2 * tag_skip);
        let vert_index = backgnd_addr + 12 + tag_offset * skipped_vert_size;
        let vert_size = 4 * (3 + 1);

        let is_auto_sort = !self.registers.isp_feed_cfg.check_bit(0);

        // fixme: fix offsets if the primitive is textured
        let mut vertices = [VertexDefinition::default(); 4];

        for i in 0..3 {
            let vram = &self.vram.read().unwrap();

            let x = f32::from_bits(u32::from_le_bytes([
                vram[(vert_index + i * vert_size) as usize],
                vram[(vert_index + i * vert_size + 1) as usize],
                vram[(vert_index + i * vert_size + 2) as usize],
                vram[(vert_index + i * vert_size + 3) as usize],
            ]));

            let y = f32::from_bits(u32::from_le_bytes([
                vram[(vert_index + i * vert_size + 4) as usize],
                vram[(vert_index + i * vert_size + 5) as usize],
                vram[(vert_index + i * vert_size + 6) as usize],
                vram[(vert_index + i * vert_size + 7) as usize],
            ]));

            let z = f32::from_bits(u32::from_le_bytes([
                vram[(vert_index + i * vert_size + 8) as usize],
                vram[(vert_index + i * vert_size + 9) as usize],
                vram[(vert_index + i * vert_size + 10) as usize],
                vram[(vert_index + i * vert_size + 11) as usize],
            ]));

            let color = u32::from_le_bytes([
                vram[(vert_index + i * vert_size + 12) as usize],
                vram[(vert_index + i * vert_size + 13) as usize],
                vram[(vert_index + i * vert_size + 14) as usize],
                vram[(vert_index + i * vert_size + 15) as usize],
            ]);

            vertices[i as usize] = VertexDefinition {
                x,
                y,
                z: f32::INFINITY,
                u: 0.,
                v: 0.,
                color,
                end_of_strip: false,
            };
        }

        let a = &vertices[0];
        let b = &vertices[1];
        let c = &vertices[2];

        let dx = b.x + c.x - a.x;
        let dy = b.y + c.y - a.y;
        let dz = b.z + c.z - a.z;

        let color_d = a.color;
        // fixme: revisit all of this bc its likely wrong
        let d = VertexDefinition {
            x: dx,
            y: dy,
            z: f32::INFINITY,
            u: 0.0,
            v: 0.0,
            color: color_d,
            end_of_strip: true,
        };

        vertices[3] = d;
        vertices
    }

    pub fn receive_ta_data(
        &mut self,
        scheduler: &mut Scheduler,
        mut addr: PhysicalAddress,
        data: u32,
    ) {
        match addr.0 {
            0x10000000..=0x10FFFFFF => {
                self.parameter_buffer[self.parameter_cursor] = data;
                self.parameter_cursor += 1;
                self.handle_cmd(scheduler);
            }
            0x11000000..=0x117FFFFF => {
                let base_index = (addr.0 - 0x11000000) as usize;
                let mut vram = self.vram.write().unwrap();
                vram[base_index] = (data & 0x000000FF) as u8;
                vram[base_index + 1] = ((data >> 8) & 0x000000FF) as u8;
                vram[base_index + 2] = ((data >> 16) & 0x000000FF) as u8;
                vram[base_index + 3] = ((data >> 24) & 0x000000FF) as u8;
            }
            _ => {
                println!("got a non-Polygon PVR SQ write. {:08x}", addr.0);
            }
        }
    }

    pub fn handle_cmd(&mut self, scheduler: &mut Scheduler) {
        if self.parameter_cursor % 8 != 0 {
            return;
        }

        let data = &self.parameter_buffer[0..self.parameter_cursor];
        let pcw = ParameterControlWord::new(data[0]);

        if self.context.list_type.is_none() {
            self.context.list_type = Some(pcw.list_type());
            self.dlb[pcw.list_type() as usize] = DisplayListBuilder::new();
        }

        let list_type = self.context.list_type.unwrap() as usize;

        match pcw.para_type() {
            ParameterType::UserTileClip => {}
            ParameterType::ObjectList => {}
            ParameterType::Sprite | ParameterType::PolyOrVol => {
                self.dlb[list_type].push_poly(PolyParam::new8([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]));
            }
            ParameterType::Vertex => {
                self.dlb[list_type].push_vert(VertexParam::new_short([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]));
            }
            ParameterType::EndOfList => {
                if let Some(list_type) = self.context.list_type {
                    scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                        // fixme: what even is this timing?
                        deadline: 200,
                        event_data: HollyEventData::RaiseInterruptNormal {
                            istnrm: match list_type {
                                PvrListType::Opaque => 0.set_bit(7),
                                PvrListType::OpaqueModVol => 0.set_bit(8),
                                PvrListType::Translucent => 0.set_bit(9),
                                PvrListType::TranslucentModVol => 0.set_bit(10),
                                PvrListType::PunchThrough => 0.set_bit(21),
                                _ => 0,
                            },
                        },
                    });
                } else {
                    panic!("got EOL without a list_type set..");
                }

                self.context.list_type = None;
            }
            ParameterType::Reserved0 | ParameterType::Reserved1 => {
                println!("pvr: got a reserved parameter type");
            }
        }

        self.parameter_cursor = 0;
    }
}
