// sync pulse generator

use crate::{hw::extensions::BitManipulation, scheduler::Scheduler};

use super::{sb::SystemBlock, HollyEventData};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpgEventData {
    VBlankIn,
    VBlankOut,
    HBlank,
}

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct SpgRegisters {
    pub vblank: u32,
    pub hblank: u32,
    pub width: u32,
    pub load: u32,
}

pub struct Spg {
    pub registers: SpgRegisters,
    pub in_vblank: bool,
}

impl Spg {
    pub fn new() -> Self {
        Self {
            in_vblank: false,
            registers: SpgRegisters {
                vblank: 0x00280208,
                hblank: 0x007e0345,
                width: 0x03f1933f,
                load: 0x020C0359,
                ..Default::default()
            },
        }
    }

    pub fn on_scheduled_event(
        &mut self,
        scheduler: &mut Scheduler,
        _: &mut SystemBlock,
        event_data: SpgEventData,
    ) {
        // println!("got {:#?}", event_data);
        match event_data {
            SpgEventData::VBlankIn => {
                let (vblank_in_freq, _, _) = self.recalc_freq();
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: vblank_in_freq,
                    event_data: super::HollyEventData::SpgEvent(SpgEventData::VBlankIn),
                });
                self.in_vblank = true;

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(3),
                    },
                });
            }
            SpgEventData::HBlank => {
                let (_, _, cycles_per_scanline) = self.recalc_freq();
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: cycles_per_scanline,
                    event_data: super::HollyEventData::SpgEvent(SpgEventData::HBlank),
                });

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(5),
                    },
                });
            }
            SpgEventData::VBlankOut => {
                let (_, vblank_out_freq, _) = self.recalc_freq();

                self.in_vblank = false;
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: vblank_out_freq,
                    event_data: super::HollyEventData::SpgEvent(SpgEventData::VBlankOut),
                });
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(4),
                    },
                });
            }
        }
    }

    pub fn init(&mut self, scheduler: &mut Scheduler) {
        let (vblank_in_freq, vblank_out_freq, cycles_per_scanline) = self.recalc_freq();

        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: vblank_in_freq,
            event_data: super::HollyEventData::SpgEvent(SpgEventData::VBlankIn),
        });

        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: vblank_out_freq,
            event_data: super::HollyEventData::SpgEvent(SpgEventData::VBlankOut),
        });

        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: cycles_per_scanline,
            event_data: super::HollyEventData::SpgEvent(SpgEventData::HBlank),
        });
    }

    pub fn recalc_freq(&mut self) -> (u64, u64, u64) {
        const CYCLES_PER_FRAME: u64 = 3333333;

        let vcount = ((self.registers.load & 0x3ff0000) >> 16) as u64;
        let cycles_per_scanline = CYCLES_PER_FRAME / vcount;

        let vblank_in_freq = ((self.registers.vblank & 0x3ff) as u64 * cycles_per_scanline) as u64;
        let vblank_out_freq =
            (((self.registers.vblank & 0x3ff0000) >> 16) as u64 * cycles_per_scanline) as u64;

        // vblank in/out frequency
        (
            vblank_in_freq,
            vblank_in_freq + vblank_out_freq,
            cycles_per_scanline,
        )
    }
}
