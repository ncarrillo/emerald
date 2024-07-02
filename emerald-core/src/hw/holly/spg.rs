// sync pulse generator

use std::cmp::{max, min};

use crate::{hw::extensions::BitManipulation, scheduler::Scheduler};

use super::{sb::SystemBlock, HollyEventData};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpgEventData {
    Sync,
}

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct SpgRegisters {
    pub vblank: u32,
    pub hblank: u32,
    pub width: u32,
    pub load: u32,
    pub vblank_int: u32,
}

pub struct Spg {
    pub registers: SpgRegisters,
    pub in_vblank: bool,
    pub cycles_this_scanline: u64,
    pub current_scanline: u64,
}

impl Spg {
    pub fn new() -> Self {
        Self {
            in_vblank: false,
            cycles_this_scanline: 0,
            current_scanline: 0,
            registers: SpgRegisters {
                vblank: 0x01500104,
                vblank_int: 0x01500104,
                hblank: 0x007E0345,
                width: 0x03f1933f,
                load: 0x01060359,
                ..Default::default()
            },
        }
    }

    pub fn on_scheduled_event(
        &mut self,
        scheduler: &mut Scheduler,
        _: &mut SystemBlock,
        r_ctrl: u32, // fixme: remove
        target: u64,
        overrun: u64,
        event_data: SpgEventData,
    ) {
        match event_data {
            SpgEventData::Sync => { // ported from reicast in an attempt to be a bit more accurate here

                self.cycles_this_scanline += target;
                let (_, _, cycles_per_scanline) = self.recalc_freq(r_ctrl);

                let vcount = ((self.registers.load & 0x3FF0000) >> 16) as u64;

                let vstart = (self.registers.vblank & 0x3FF) as u64;
                let vend = ((self.registers.vblank & 0x3FF0000) >> 16) as u64;
                let vblank_int_in = (self.registers.vblank_int & 0x3FF) as u64;
                let vblank_int_out = ((self.registers.vblank_int & 0x3FF0000) >> 16) as u64;

                while self.cycles_this_scanline >= cycles_per_scanline {
                    self.current_scanline = self.current_scanline + 1;

                    if self.current_scanline > vcount {
                        self.current_scanline = 0;
                    }

                    self.cycles_this_scanline -= cycles_per_scanline;

                    // fixme: is this right?
                    if self.current_scanline == 0 {
                        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                            deadline: 0,
                            event_data: HollyEventData::RaiseInterruptNormal {
                                istnrm: 0.set_bit(5),
                            },
                        });
                    }

                    if vblank_int_in == self.current_scanline {
                        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                            deadline: 0,
                            event_data: HollyEventData::RaiseInterruptNormal {
                                istnrm: 0.set_bit(3),
                            },
                        });

                        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                            deadline: 0,
                            event_data: HollyEventData::VBlank,
                        });
                    }

                    if vblank_int_out == self.current_scanline {
                        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                            deadline: 0,
                            event_data: HollyEventData::RaiseInterruptNormal {
                                istnrm: 0.set_bit(4),
                            },
                        });
                    }

                    if vstart == self.current_scanline {
                        self.in_vblank = true;
                    }

                    if vend == self.current_scanline {
                        self.in_vblank = false;
                    }
                }

                let min_scanline = self.current_scanline + 1;
                let mut min_active = vcount;

                if min_scanline < vblank_int_in {
                    min_active = min(min_active, vblank_int_in);
                }

                if min_scanline < vblank_int_out {
                    min_active = min(min_active, vblank_int_out);
                }

                if min_scanline < vstart {
                    min_active = min(min_active, vstart);
                }

                if min_scanline < vend {
                    min_active = min(min_active, vend);
                }

                if min_scanline < vcount {
                    min_active = min(min_active, vcount);
                }

                min_active = max(min_active, min_scanline);
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: max(
                        0,
                        (((min_active - self.current_scanline) * cycles_per_scanline) - overrun)
                            as i32,
                    ) as u64,
                    event_data: super::HollyEventData::SpgEvent(SpgEventData::Sync),
                });
            }
        }
    }

    pub fn init(&mut self, scheduler: &mut Scheduler) {
        let (_, _, cycles_per_scanline) = self.recalc_freq(0);
        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: cycles_per_scanline,
            event_data: super::HollyEventData::SpgEvent(SpgEventData::Sync),
        });
    }

    pub fn recalc_freq(&mut self, r_ctrl: u32) -> (u64, u64, u64) {
        // revisit and understand this when I have time
        let vclk = if r_ctrl.check_bit(23) { 1 } else { 2 };

        const PIXEL_CLOCK: u64 = 54 * 1000 * 1000 / 2;
        let pixel_clock = PIXEL_CLOCK / vclk;

        const SH4_CLOCK: u64 = 200 * 1000 * 1000;

        let hcount = (self.registers.load & 0x3FF) as u64;
        let cycles_per_scanline = (SH4_CLOCK * (hcount + 1) / pixel_clock) as u32;

        let vblank_in_freq = ((self.registers.vblank & 0x3ff) as u32 * cycles_per_scanline) as u64;
        let vblank_out_freq =
            (((self.registers.vblank & 0x3ff0000) >> 16) as u32 * cycles_per_scanline) as u64;

        // vblank in/out frequency
        (
            vblank_in_freq,
            vblank_in_freq + vblank_out_freq,
            cycles_per_scanline as u64,
        )
    }
}
