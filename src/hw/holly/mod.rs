// holly block

use self::{
    g1::{gdrom::GdromEventData, G1Bus},
    maple::Maple,
    pvr::Pvr,
    sb::SystemBlock,
    spg::{Spg, SpgEventData},
};
use crate::{
    context::Context,
    hw::extensions::{BitManipulation, SliceExtensions},
    scheduler::{ScheduledEvent, Scheduler},
};

use super::sh4::{bus::PhysicalAddress, dmac::Dmac, intc::InterruptKind};
pub mod g1;
pub mod maple;
pub mod pvr;
pub mod sb;
pub mod spg;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum HollyEventData {
    RaiseInterruptNormal { istnrm: u32 },
    RaiseInterruptExternal { istext: u32 },
    LowerExternalInterrupt { istext: u32 },
    RecalculateInterrupts,
    FrameEnd,
    SpgEvent(SpgEventData),
    GdromEvent(GdromEventData),
    MapleDMA,
    Ch2DMA,
}

#[derive(Clone, Debug)]
pub struct HollyRegisters {
    pub border_col: u32,
    pub fb_clip_x: u32,
    pub fb_clip_y: u32,
    pub video_cfg: u32,
    pub fb_r_ctrl: u32,
    pub fb_w_ctrl: u32,
    pub fb_render_modulo: u32,
    pub spansort_cfg: u32,
    pub fog_table_col: u32,
    pub fog_vertex_col: u32,
    pub fog_density: u32,
    pub fog_table: [u32; 0x1fc],
    pub vpos_irq: u32,
    pub hpos_irq: u32,
    pub ta_opb_start: u32,
    pub ta_ob_start: u32,
    pub ta_opb_end: u32,
    pub ta_ob_end: u32,
    pub tilebuf_size: u32,
    pub ta_opb_cfg: u32,
    pub ta_opl_init: u32,
    pub ta_init: u32,
    pub unk_reg_5f8110: u32,
    pub unk_reg_5f8080: u32,
    pub unk_reg_5f8084: u32,
    pub sync_cfg: u32,
    pub sync_load: u32,
    pub sync_width: u32,
    pub hborder: u32,
    pub vborder: u32,
    pub fb_render_addr1: u32,
    pub fb_render_addr2: u32,
    pub fb_display_addr1: u32,
    pub fb_display_addr2: u32,
    pub fb_display_size: u32,
    pub shadow: u32,
    pub ob_cfg: u32,
    pub ta_luminance: u32,
    pub object_clip: u32,
    pub bgplane_z: u32,
    pub isp_background_t: u32,
    pub clamp_max: u32,
    pub clamp_min: u32,
    pub tsp_cfg: u32,
    pub scaler_cfg: u32,
    pub sdram_cfg: u32,
    pub sdram_refresh: u32,
    pub vo_startx: u32,
    pub vo_starty: u32,
    pub isp_speed_cfg: u32,
}

impl Default for HollyRegisters {
    fn default() -> HollyRegisters {
        unsafe { std::mem::zeroed() }
    }
}

pub struct Holly {
    pub registers: HollyRegisters,
    pub spg: Spg,
    pub sb: SystemBlock,
    pub g1_bus: G1Bus,
    pub cyc: u64,
    pub frame_cyc: u64,
    pub last_line: u32,
    pub maple: Maple,
    pub pvr: Pvr,
}

impl Holly {
    pub fn new() -> Self {
        Self {
            registers: Default::default(),
            spg: Spg::new(),
            sb: SystemBlock::new(),
            g1_bus: G1Bus::new(),
            maple: Maple::new(),
            pvr: Pvr::new(),
            last_line: 0,
            cyc: 0,
            frame_cyc: 0,
        }
    }

    pub fn init(&mut self, scheduler: &mut Scheduler) {
        self.spg.init(scheduler);
        self.registers.fb_display_size = 0x00177e7f;
        scheduler.schedule(ScheduledEvent::HollyEvent {
            deadline: 3333333,
            event_data: HollyEventData::FrameEnd,
        })
    }

    pub fn on_scheduled_event(
        &mut self,
        scheduler: &mut Scheduler,
        dmac: &mut Dmac, // less than idea, for ch2 dma
        ram: &mut [u8],
        event: HollyEventData,
    ) {
        match event {
            HollyEventData::SpgEvent(spg_event) => {
                self.spg
                    .on_scheduled_event(scheduler, &mut self.sb, spg_event)
            }
            HollyEventData::GdromEvent(gdrom_event) => {
                self.g1_bus
                    .gd_rom
                    .on_scheduled_event(scheduler, &mut self.sb, gdrom_event)
            }
            HollyEventData::RecalculateInterrupts => {
                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::RaiseInterruptNormal { istnrm } => {
                self.sb.registers.istnrm |= istnrm;

                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::RaiseInterruptExternal { istext } => {
                self.sb.registers.istext |= istext;
                self.sb.registers.istnrm = self.sb.registers.istnrm.eval_bit(30, self.sb.registers.istext != 0);

                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::LowerExternalInterrupt { istext } => {
                self.sb.registers.istext = istext;
                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::FrameEnd => {
                self.frame_cyc = scheduler.now();
                scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 3333333,
                    event_data: HollyEventData::FrameEnd,
                });
            }
            HollyEventData::MapleDMA => {
                // perform maple DMA
                let start = (self.sb.registers.mdstar - 0x0c000000) as usize;
                self.maple.perform_maple_transfer(start, scheduler,  &mut ram[0..]);
                self.sb.registers.mdst = 0;
            }
            HollyEventData::Ch2DMA => {
                let source_addr = dmac.registers.sar2 & 0x1fffffff;
                let dest_addr = self.sb.registers.c2dstat & 0x1fffffff;

                let mut dmac_len = dmac.registers.dmatcr2 as usize;
                let len = self.sb.registers.c2dlen as usize;

                assert_eq!(dmac_len, (len / 32) as usize); // spec says these should line up, except c2dlen is in bytes

                let mut ram_offset = (source_addr - 0x0c000000) as usize;

                while dmac_len > 0 {
                    let dma_data = ram[ram_offset..ram_offset + 32].as_u32_slice_mut();

                    // println!();
                    // Pvr::dump_ram_to_console(source_addr, &dma_data[0..].as_u8_slice_mut());
                    // println!();

                    match dest_addr {
                        0x10000000..=0x107FFFE0 => {
                            self.pvr.receive_ta_fifo_dma_data(scheduler, dma_data)
                        }
                        _ => {}/*panic!(
                            "holly: got ch2 dma to an unimplemented addr {:08x}",
                            dest_addr
                        ),*/ 
                    }

                    ram_offset += 32;
                    dmac_len -= 1;
                }

                self.sb.registers.c2dst = 0;
                self.sb.registers.c2dlen = 0;
                dmac.registers.sar2 += len as u32;
                dmac.registers.dmatcr2 = 0;

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 200,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(3).set_bit(4),
                    },
                });
            }
        }
    }

    // fixme: move to system block?
    pub fn dispatch_sh4_interrupt(&mut self, scheduler: &mut Scheduler) {
        //let is_level_9 =
        let sh4_interrupt_line = match () {
            _ if ((self.sb.registers.istnrm & self.sb.registers.iml6nrm) != 0
                || (self.sb.registers.istext & self.sb.registers.iml6ext) != 0
                || ((self.sb.registers.isterr & self.sb.registers.iml6err) != 0)) =>
            {
                InterruptKind::IRL9 as usize
            }
            _ if ((self.sb.registers.istnrm & self.sb.registers.iml4nrm) != 0
                || (self.sb.registers.istext & self.sb.registers.iml4ext) != 0
                || ((self.sb.registers.isterr & self.sb.registers.iml4err) != 0)) =>
            {
                InterruptKind::IRL11 as usize
            }
            _ if ((self.sb.registers.istnrm & self.sb.registers.iml2nrm) != 0
                || (self.sb.registers.istext & self.sb.registers.iml2ext) != 0
                || ((self.sb.registers.isterr & self.sb.registers.iml2err) != 0)) =>
            {
                InterruptKind::IRL13 as usize
            }
            _ => 0,
        };

        if sh4_interrupt_line != 0 {
            //println!("raising IRL @ {}", sh4_interrupt_line);
            scheduler.schedule(ScheduledEvent::SH4Event {
                deadline: 0,
                event_data: crate::hw::sh4::SH4EventData::RaiseIRL {
                    irl_number: sh4_interrupt_line,
                },
            });
        }
    }

    // fixme: taken from jsmoo
    fn holly_get_spg_line(&self) -> u32 {
        let vcount = ((self.spg.registers.load & 0x3ff0000) >> 16) as u64;
        const CYCLES_PER_FRAME: u64 = 3333333;
        let cycles_per_scanline = CYCLES_PER_FRAME / vcount;

        let cycle_num = (self.cyc.wrapping_sub(self.frame_cyc)) as u32;
        return cycle_num / cycles_per_scanline as u32;
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            0x005F8004 => 0x11, // revision
            0x005f8144 => 0,    // TA_LIST_INIT always reads 0
            0x005f7018..=0x005f709c => self.g1_bus.read_32(addr),
            0x005f6800..=0x005f7cf8 => self.sb.read_32(addr),
            0x005f810c => {
                let line = self.holly_get_spg_line() & 0x3FF;
                line.eval_bit(13, self.spg.in_vblank)
            }
            0x005f80dc => self.registers.vborder,
            0x005f8000 => 0x17fd11db, // manufacturer id??
            0x005f8040 => self.registers.border_col,
            0x005f80d0 => self.registers.sync_cfg,
            0x005f80e8 => self.registers.video_cfg,
            0x005f8044 => self.registers.fb_r_ctrl,
            _ => {
                #[cfg(feature = "log_io")]
                println!("holly: unimplemented read (32-bit) @ 0x{:08x}", addr.0);
                0
            }
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x005f7018..=0x005f709c => self.g1_bus.write_16(addr, value),
            0x005f6800..=0x005f7cf8 => self.sb.write_16(addr, value),
            _ => {
                panic!("holly: unimplemented write (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32, context: &mut Context) {
        match addr.0 {
            0x005f7018..=0x005f709c => self.g1_bus.write_32(addr, value),
            0x005f6800..=0x005f7cf8 => self.sb.write_32(addr, value, context),
            0x005f8008 => {} // fixme: reset
            0x005f8030 => self.registers.spansort_cfg = value,
            0x005f8040 => self.registers.border_col = value,
            0x005f8044 => self.registers.fb_r_ctrl = value & 0x00FFFF7F,
            0x005f8048 => self.registers.fb_w_ctrl = value & 0x00FFFF0F,
            0x005f804c => self.registers.fb_render_modulo = value,
            0x005f8050 => {
                self.registers.fb_display_addr1 = value;
                self.pvr.starting_offset = self.registers.fb_display_addr1;
            }
            0x005f8054 => self.registers.fb_display_addr2 = value,
            0x005f805c => self.registers.fb_display_size = value,
            0x005f8060 => self.registers.fb_render_addr1 = value,
            0x005f8014 => {
                self.pvr.starting_offset = self.registers.fb_display_addr1;
                self.pvr.depth = (self.registers.fb_r_ctrl & 0xC) >> 2;
            },
            0x005f8064 => self.registers.fb_render_addr2 = value,
            0x005f8068 => self.registers.fb_clip_x = value,
            0x005f806c => self.registers.fb_clip_y = value,
            0x005f8074 => self.registers.shadow = value,
            0x005f8078 => self.registers.object_clip = value,
            0x005f807c => self.registers.ob_cfg = value,
            0x005f8080 => self.registers.unk_reg_5f8080 = value,
            0x005f8084 => self.registers.unk_reg_5f8084 = value,
            0x005f8088 => self.registers.bgplane_z = value,
            0x005f808c => self.registers.isp_background_t = value,
            0x005f80a0 => self.registers.sdram_refresh = value,
            0x005f80a8 => self.registers.sdram_cfg = value,
            0x005f80b0 => self.registers.fog_table_col = value,
            0x005f80b4 => self.registers.fog_vertex_col = value,
            0x005f80b8 => self.registers.fog_density = value,
            0x005f80bc => self.registers.clamp_max = value,
            0x005f80c0 => self.registers.clamp_min = value,
            0x005f80c8 => self.registers.hpos_irq = value,
            0x005f80cc => self.registers.vpos_irq = value,
            0x005f80d0 => self.registers.sync_cfg = value,
            0x005f80d4 => self.registers.hborder = value,
            0x005f80d8 => self.registers.sync_load = value,
            0x005f80dc => self.registers.vborder = value,
            0x005f80e0 => self.registers.sync_width = value,
            0x005f80e4 => self.registers.tsp_cfg = value,
            0x005f80e8 => self.registers.video_cfg = value,
            0x005f80ec => self.registers.vo_startx = value,
            0x005f80f0 => self.registers.vo_starty = value,
            0x005f80f4 => self.registers.scaler_cfg = value,
            0x005f8110 => self.registers.unk_reg_5f8110 = value,
            0x005f8118 => self.registers.ta_luminance = value,
            0x005f8124 => self.registers.ta_opb_start = value,
            0x005f8128 => self.registers.ta_ob_start = value,
            0x005f812c => self.registers.ta_opb_end = value,
            0x005f8130 => self.registers.ta_ob_end = value,
            0x005f813c => self.registers.tilebuf_size = value,
            0x005f8140 => self.registers.ta_opb_cfg = value,
            0x005f8144 => self.registers.ta_init = value,
            0x005f8164 => self.registers.ta_opl_init = value,
            0x005f8098 => self.registers.isp_speed_cfg = value,
            0x005f8020 => {}, // param base
            0x005f802c => {}, // region base
            0x005f8200..=0x005f83ff => {
                self.registers.fog_table[((addr.0 - 0x005f8200) / 4) as usize] = value
            }
            _ => {
                panic!(
                    "holly: unimplemented write (32-bit) @ 0x{:08x} with 0x{:08x}",
                    addr.0, value
                );
            }
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
        match addr.0 {
            // gd-rom
            0x005f7018..=0x005f709c => self.g1_bus.read_16(addr),
            _ => {
                panic!("holly: unimplemented read (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            0..=0x0023ffff => self.g1_bus.read_8(addr), // bios + flash
            0x05000000..=0x05800000 => self.pvr.vram[(addr.0 - 0x05000000) as usize], // vram
            0x04000000..=0x04800000 => {
                self.pvr.vram[(addr.0 - 0x04000000) as usize]
            }, // vram 64-bit
            0x005f7018..=0x005f709c => self.g1_bus.read_8(addr), // gdrom
            _ => panic!("holly: unimplemented read (8-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
        match addr.0 {
            0x05000000..=0x05800000 => self.pvr.vram[(addr.0 - 0x05000000) as usize] = value, // vram
            0x04000000..=0x04800000 => self.pvr.vram[(addr.0 - 0x04000000) as usize] = value, // vram 64-bit
            0x005f7018..=0x005f709c => self.g1_bus.write_8(addr, value),                  // gd-rom
            _ => {
                panic!(
                    "holly: unimplemented write (8-bit) @ 0x{:08x} with {:08x}",
                    addr.0, value
                );
            }
        }
    }
}
