// holly block

use std::{cmp::min, ops::DerefMut, sync::Arc};

use g2::aica::{arm::Cpu, Aica};
use pvr::framebuffer::{FbReadCtrl, FbSize, FbWriteCtrl, FbXClip, FbYClip, Framebuffer};

use self::{
    g1::{gdrom::GdromEventData, G1Bus},
    maple::Maple,
    pvr::Pvr,
    sb::SystemBlock,
    spg::{Spg, SpgEventData},
};
use crate::{
    context::Context,
    hw::{
        extensions::{BitManipulation, SliceExtensions},
        holly::g1::gdrom::GdromState,
    },
    scheduler::{ScheduledEvent, Scheduler},
};

use super::sh4::{bus::PhysicalAddress, dmac::Dmac, intc::InterruptKind};
pub mod g1;
pub mod g2;
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
    FrameReady(u32),
    SpgEvent(SpgEventData),
    GdromEvent(GdromEventData),
    MapleDMA,
    Rtc,
    Ch2DMA,
    GdromDMA,
    AicaDMA,
    VBlank,
}

#[derive(Clone, Debug)]
pub struct HollyRegisters {
    pub border_col: u32,
    pub video_cfg: u32,
    pub fb_render_modulo: u32,
    pub spansort_cfg: u32,
    pub fog_table_col: u32,
    pub fog_vertex_col: u32,
    pub fog_density: u32,
    pub fog_table: [u32; 0x1fc],
    pub hpos_irq: u32,

    pub ta_itp_current: u32,
    pub ta_opb_start: u32,
    pub ta_isp_base: u32,
    pub ta_ol_limit: u32,
    pub ta_list_cont: u32,
    pub ta_isp_limit: u32,
    pub ta_next_opb: u32,
    pub tilebuf_size: u32,
    pub ta_opb_cfg: u32,
    pub ta_next_opb_init: u32,
    pub ta_list_init: u32,

    pub unk_reg_5f8110: u32,
    pub unk_reg_5f8080: u32,
    pub unk_reg_5f8084: u32,
    pub sync_cfg: u32,
    pub sync_width: u32,
    pub hborder: u32,
    pub vborder: u32,
    pub fb_render_addr1: u32,
    pub fb_render_addr2: u32,
    pub shadow: u32,
    pub ob_cfg: u32,
    pub ta_luminance: u32,
    pub object_clip: u32,
    pub bgplane_z: u32,
    pub clamp_max: u32,
    pub pt_alpha_ref: u32,
    pub clamp_min: u32,
    pub tsp_cfg: u32,
    pub scaler_cfg: u32,
    pub sdram_cfg: u32,
    pub sdram_refresh: u32,
    pub vo_startx: u32,
    pub vo_starty: u32,
    pub pal_ram_ctrl: u32,
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
    pub maple: Maple,
    pub pvr: Pvr,
    pub framebuffer: Framebuffer,
    pub aica: Aica,
    pub arm7tdmi: Cpu,
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
            framebuffer: Default::default(),
            aica: Aica::new(),
            arm7tdmi: Cpu::new(),
            cyc: 0,
        }
    }

    pub fn init(&mut self, scheduler: &mut Scheduler) {
        self.spg.init(scheduler);
        self.aica.rtc.init(scheduler);
    }

    pub fn on_scheduled_event(
        &mut self,
        scheduler: &mut Scheduler,
        dmac: &mut Dmac, // less than ideal, for ch2 dma
        ram: &mut [u8],
        target: u64,
        overrun: u64,
        event: HollyEventData,
    ) {
        match event {
            HollyEventData::SpgEvent(spg_event) => self.spg.on_scheduled_event(
                scheduler,
                &mut self.sb,
                self.framebuffer.registers.read_ctrl.raw,
                target,
                overrun,
                spg_event,
            ),
            HollyEventData::Rtc => self.aica.rtc.on_scheduled_event(scheduler),
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
                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::LowerExternalInterrupt { istext } => {
                self.sb.registers.istext &= !istext;
                self.dispatch_sh4_interrupt(scheduler);
            }
            HollyEventData::FrameReady(_) => {}
            HollyEventData::AicaDMA => {
                if self.sb.registers.ad_en == 0 {
                    return;
                }

                let start_addr = self.sb.registers.ad_star;
                let aica_addr = self.sb.registers.ad_stag;

                let len = self.sb.registers.ad_len & 0x7FFFFFFF;
                let direction = self.sb.registers.ad_dir;

                println!("got an aica dma request: start_addr: {:#x}, aica_addr: {:#x}, len: {:#x}, direction: {:#x}", start_addr, aica_addr, len, direction);

                if direction == 0 {
                    let src = &mut ram[(start_addr as usize - 0x0c000000)..];
                    for i in 0..len / 4 {
                        self.aica.write_aica_wave_32(
                            PhysicalAddress(aica_addr as u32 + i),
                            u32::from_le_bytes([
                                src[i as usize],
                                src[i as usize + 1],
                                src[i as usize + 2],
                                src[i as usize + 3],
                            ]),
                        )
                    }
                } else {
                    panic!("aica dma: got direction = 1");
                }

                self.sb.registers.ad_st = 0;
                self.sb.registers.ad_len = 0;
                self.sb.registers.ad_star += len;
                self.sb.registers.ad_stag += len;

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 70000,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0_u32.set_bit(15),
                    },
                });

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 70000, // fixme: timing
                    event_data: HollyEventData::RaiseInterruptExternal {
                        istext: 0.set_bit(1),
                    },
                });
            }
            HollyEventData::MapleDMA => {
                let start = (self.sb.registers.mdstar - 0x0c000000) as usize;
                self.maple
                    .perform_maple_transfer(start, scheduler, &mut ram[0..]);
                self.sb.registers.mdst = 0;
            }
            HollyEventData::VBlank => {}
            HollyEventData::GdromDMA => {
                if self.sb.registers.gd_en == 0x1 {
                    let dest_addr = (self.sb.registers.gd_star & 0x1FFFFFE0) as usize;
                    let len = self.sb.registers.gd_len as usize;
                    let direction = self.sb.registers.gd_dir;

                    #[cfg(feature = "log_io")]
                    println!("performing gd-dma {:08x} {}", dest_addr, len);

                    if direction == 0 {
                        unimplemented!();
                    }

                    self.sb.registers.gd_st = 1;
                    self.sb.registers.gd_lend = 0;
                    self.sb.registers.gd_stard = dest_addr as u32;

                    {
                        let mut output_fifo = self.g1_bus.gd_rom.output_fifo.borrow_mut();
                        let output_fifo = output_fifo.deref_mut();
                        let mut i = 0;
                        while let Some(b) = output_fifo.pop() {
                            ram[(dest_addr as usize + i) - 0x0C000000 as usize] = b;
                            i += 1;
                        }
                        output_fifo.clear();
                    }

                    dmac.registers.dar0 = dest_addr as u32;
                    self.sb.registers.gd_st = 0;
                    self.sb.registers.gd_lend += len as u32;
                    self.sb.registers.gd_stard += len as u32;

                    self.g1_bus
                        .gd_rom
                        .transition(scheduler, GdromState::FinishedProcessingPacket);
                    scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                        deadline: 20000,
                        event_data: HollyEventData::RaiseInterruptExternal {
                            istext: 0.set_bit(14),
                        },
                    });
                }
            }
            HollyEventData::Ch2DMA => {
                dmac.registers.dar2 = self.sb.registers.c2dstat;

                let mut src = dmac.registers.sar2;
                let dst = self.sb.registers.c2dstat;
                let mut len = self.sb.registers.c2dlen as usize;

                #[cfg(feature = "log_dma")]
                println!(
                    "ch2: pvr dma from {:08x} to {:08x} len (in bytes) {:08x} {}",
                    src,
                    dst,
                    len,
                    scheduler.now()
                );

                let ram_size = 16 * 1024 * 1024;
                let ram_mask = ram_size - 1;
                let mut ram_offset = (src & ram_mask);

                match dst {
                    0x10000000..=0x10FFFFFF => {
                        while len > 0 {
                            let sys_buf = &mut ram
                                [ram_offset as usize..(ram_offset as usize + len)]
                                .as_u32_slice_mut();
                            for &word in sys_buf.iter() {
                                self.pvr
                                    .receive_ta_data(scheduler, PhysicalAddress(dst), word);
                            }
                            src += len as u32;
                            break;
                        }
                    }
                    0x11000000..=0x11FFFFE0 => {
                        self.sb.registers.c2dstat += len as u32;

                        assert!(self.sb.registers.lmmode0 == 0);

                        let dst = (dst & 0xFFFFFF) | 0xa4000000;

                        let dst = dst & 0x1FFFFFFF;

                        while len > 0 {
                            let sys_buf = &mut ram
                                [ram_offset as usize..(ram_offset as usize + len)]
                                .as_u32_slice_mut();

                            let mut i = 0 as u32;
                            for &data in sys_buf.iter() {
                                let base_index = (dst & 0x3FFFFFF) + (i as u32 * 4) as u32;
                                let vram = &mut self.pvr.vram.write().unwrap();

                                // self.framebuffer.notify_write(dst + (i as u32 * 4));
                                vram[base_index as usize] = (data & 0x000000FF) as u8;
                                vram[base_index as usize + 1] = ((data >> 8) & 0x000000FF) as u8;
                                vram[base_index as usize + 2] = ((data >> 16) & 0x000000FF) as u8;
                                vram[base_index as usize + 3] = ((data >> 24) & 0x000000FF) as u8;
                                i += 1;
                            }

                            src += len as u32;

                            break;
                        }

                        // fixme: this invalidation should probasbly be done for all xferred addresses here..
                        self.pvr.texture_atlas.write().unwrap().notify_write(dst);
                    }
                    _ => {
                        panic!("got an unk dma to {:08x}", dst);
                        //  src += len as u32;
                        // }
                    }
                }

                dmac.registers.sar2 = src;
                dmac.registers.dmatcr2 = 0;
                dmac.registers.chcr2 &= 0xFFFFFFFE;

                self.sb.registers.c2dst = 0;
                self.sb.registers.c2dlen = 0;

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 20000,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(19),
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
            scheduler.schedule(ScheduledEvent::SH4Event {
                deadline: 0,
                event_data: crate::hw::sh4::SH4EventData::RaiseIRL {
                    irl_number: sh4_interrupt_line,
                },
            });
        }
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            // aica wave ram + mirror
            0x00800000..=0x00FFFFFF => self.aica.read_aica_wave_32(addr),

            // aica registers
            0x00700000..=0x0070FFFF => self.aica.read_aica_register_32(addr),
            0x00900000..=0x0090FFFF => self.aica.read_aica_register_32(addr),

            0x005F8004 => 0x11, // revision
            0x005f8144 => 0,    // TA_LIST_INIT always reads 0
            0x005f7018..=0x005f709c => self.g1_bus.read_32(addr),
            0x005f6800..=0x005f7cf8 => self.sb.read_32(addr),
            0x005f810c => {
                let line = self.spg.current_scanline & 0x3FF;
                (line as u32).eval_bit(13, self.spg.in_vblank)
            }
            0x005f80dc => self.registers.vborder,
            0x005f8000 => 0x17fd11db, // manufacturer id??
            0x005f8040 => self.registers.border_col,
            0x005f80d0 => self.registers.sync_cfg,
            0x005f80e8 => self.registers.video_cfg,
            0x005f8044 => self.framebuffer.registers.read_ctrl.raw,
            0x005F8128 => self.registers.ta_isp_base,
            0x005f8138 => self.registers.ta_itp_current,
            0x005f8134 => self.registers.ta_next_opb,
            0x005f80d8 => self.spg.registers.load,
            _ => {
                println!("holly: unimplemented read (32-bit) @ 0x{:08x}", addr.0);
                0
            }
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16, context: &mut Context) {
        match addr.0 {
            // aica wave ram + mirror
            0x00800000..=0x00FFFFFF => self.aica.write_aica_wave_16(addr, value),
            0x02800000..=0x02FFFFFF => self.aica.write_aica_wave_16(addr, value),

            0x005f7018..=0x005f709c => self.g1_bus.write_16(addr, value, context),
            0x005f6800..=0x005f7cf8 => self.sb.write_16(addr, value),
            _ => {
                panic!("holly: unimplemented write (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32, context: &mut Context) {
        match addr.0 {
            // aica wave ram + mirror
            0x00800000..=0x00FFFFFF => self.aica.write_aica_wave_32(addr, value),

            // aica register + mirrors
            0x00700000..=0x00707fff => self.aica.write_aica_register_32(addr, value),
            0x00900000..=0x00907fff => self.aica.write_aica_register_32(addr, value),

            0x005F8000 => {} // ID
            0x005F8004 => {} // revision
            0x005f7018..=0x005f709c => self.g1_bus.write_32(addr, value),
            0x005f6800..=0x005f7cf8 => self.sb.write_32(addr, value, context),
            0x005f8008 => {} // fixme: reset
            0x005f8030 => self.registers.spansort_cfg = value,
            0x005f8040 => self.registers.border_col = value,
            0x005f8044 => {
                self.framebuffer.registers.read_ctrl = FbReadCtrl::from_raw(value & 0x00FFFF7F);
            }
            0x005f8048 => {
                self.framebuffer.registers.write_ctrl = FbWriteCtrl::from_raw(value & 0x00FFFF0F);
            }
            0x005f804c => self.registers.fb_render_modulo = value,
            0x005f8050 => {
                self.framebuffer.registers.base_address = value;
            }
            0x005f8054 => {
                self.framebuffer.registers.base_address2 = value;
            }
            0x005f805c => {
                let current = self.framebuffer.registers.read_size.raw;
                self.framebuffer.registers.read_size = FbSize::from_raw(value);

                if current != value {
                    self.framebuffer.invalidate_watches();
                }
            }
            0x005f8060 => self.registers.fb_render_addr1 = value,
            0x005f8014 => {
                context.scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::FrameReady(self.pvr.registers.param_base),
                });
            }
            0x005f8064 => self.registers.fb_render_addr2 = value,
            0x005f8068 => self.framebuffer.registers.x_clip = FbXClip::from_raw(value),
            0x005f806c => self.framebuffer.registers.y_clip = FbYClip::from_raw(value),
            0x005f8074 => self.registers.shadow = value,
            0x005f8078 => self.registers.object_clip = value,
            0x005f807c => self.registers.ob_cfg = value,
            0x005f8080 => self.registers.unk_reg_5f8080 = value,
            0x005f8084 => self.registers.unk_reg_5f8084 = value,
            0x005f8088 => self.registers.bgplane_z = value,
            0x005f808c => self.pvr.registers.isp_backgnd_t = value,
            0x005f80a0 => self.registers.sdram_refresh = value,
            0x005f80a8 => self.registers.sdram_cfg = value,
            0x005f80b0 => self.registers.fog_table_col = value,
            0x005f80b4 => self.registers.fog_vertex_col = value,
            0x005f80b8 => self.registers.fog_density = value,
            0x005f80bc => self.registers.clamp_max = value,
            0x005f80c0 => self.registers.clamp_min = value,
            0x005f80c8 => self.registers.hpos_irq = value,
            0x005f80cc => self.spg.registers.vblank_int = value,
            0x005f80d0 => {
                self.registers.sync_cfg = value;
                self.framebuffer.interlaced = value.check_bit(4);
            }
            0x005f80d4 => self.registers.hborder = value,
            0x005f80d8 => self.spg.registers.load = value,
            0x005f80dc => self.registers.vborder = value,
            0x005f80e0 => self.registers.sync_width = value,
            0x005f80e4 => self.registers.tsp_cfg = value,
            0x005f80e8 => self.registers.video_cfg = value,
            0x005f80ec => self.registers.vo_startx = value,
            0x005f80f0 => self.registers.vo_starty = value,
            0x005f80f4 => self.registers.scaler_cfg = value,
            0x005f8110 => self.registers.unk_reg_5f8110 = value,
            0x005f8118 => self.registers.ta_luminance = value,
            0x005f811c => self.registers.pt_alpha_ref = value,
            0x005f8124 => self.registers.ta_opb_start = value,
            0x005f8128 => self.registers.ta_isp_base = value,
            0x005f812c => self.registers.ta_ol_limit = value,
            0x005f8130 => self.registers.ta_isp_limit = value,
            0x005f813c => self.registers.tilebuf_size = value,
            0x005f8140 => self.registers.ta_opb_cfg = value,
            0x005f8144 => {
                self.registers.ta_list_init = value;

                if (self.registers.ta_list_cont & 0x80000000 == 0) {
                    self.registers.ta_next_opb = self.registers.ta_next_opb_init;
                    self.registers.ta_itp_current = self.registers.ta_isp_base;
                }
            }
            0x005f8160 => self.registers.ta_list_cont = value,
            0x005f8164 => self.registers.ta_next_opb_init = value,
            0x005f8090 | 0x005f8098 => self.pvr.registers.isp_feed_cfg = value,
            0x005F8108 => {
                self.registers.pal_ram_ctrl = value;
                println!("PAL_RAM_CTRL set to {:08x}", value);
            }
            0x005f8020 => self.pvr.registers.param_base = value, // param base
            0x005f802c => {
                self.pvr.registers.region_base = value;
            } // region base
            0x005f8200..=0x005f83ff => {
                self.registers.fog_table[((addr.0 - 0x005f8200) / 4) as usize] = value
            }
            _ => {
                println!(
                    "holly: unimplemented write (32-bit) @ 0x{:08x} with 0x{:08x}",
                    addr.0, value
                );
            }
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress, context: &mut Context) -> u16 {
        match addr.0 {
            // aica wave ram + mirror
            0x00800000..=0x00FFFFFF => self.aica.read_aica_wave_16(addr),
            0x02800000..=0x02FFFFFF => self.aica.read_aica_wave_16(addr),

            // gd-rom
            0x005f7018..=0x005f709c => self.g1_bus.read_16(addr, context),

            0x005f80d8 => self.spg.registers.load as u16,
            _ => {
                panic!("holly: unimplemented read (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress, context: &mut Context) -> u8 {
        match addr.0 {
            0..=0x0023ffff => self.g1_bus.read_8(addr, context), // bios + flash
            0x05000000..=0x05800000 => {
                self.pvr.vram.read().unwrap()[(addr.0 - 0x05000000) as usize]
            } // vram
            0x07000000..=0x07800000 => {
                self.pvr.vram.read().unwrap()[(addr.0 - 0x07000000) as usize]
            } // vram mirror
            0x04000000..=0x04800000 => {
                self.pvr.vram.read().unwrap()[(addr.0 - 0x04000000) as usize]
            } // vram 64-bit
            0x005f7018..=0x005f709c => self.g1_bus.read_8(addr, context), // gdrom
            _ => panic!("holly: unimplemented read (8-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8, context: &mut Context) {
        match addr.0 {
            0..=0x0023ffff => {} // bios + flash ?? fixme: figure this out
            0x05000000..=0x05800000 => {
                self.framebuffer.notify_write(addr.0, value);
                self.pvr.vram.write().unwrap()[(addr.0 - 0x05000000) as usize] = value;
                self.pvr.texture_atlas.write().unwrap().notify_write(addr.0);
            } // vram
            0x005F9000..=0x005F9FFF => {
                self.pvr.pram.write().unwrap()[(addr.0 - 0x005F9000) as usize] = value;
                self.pvr
                    .texture_atlas
                    .write()
                    .unwrap()
                    .notify_paletted_write(addr.0);
            }
            0x04000000..=0x04800000 => {
                self.framebuffer.notify_write(addr.0, value);
                self.pvr.vram.write().unwrap()[(addr.0 - 0x04000000) as usize] = value;
                self.pvr.texture_atlas.write().unwrap().notify_write(addr.0);
            } // vram 64-bit
            0x005f7018..=0x005f709c => self.g1_bus.write_8(addr, value, context), // gd-rom
            _ => {
                panic!(
                    "holly: unimplemented write (8-bit) @ 0x{:08x} with {:08x}",
                    addr.0, value
                );
            }
        }
    }
}
