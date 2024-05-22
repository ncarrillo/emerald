// timers

use super::{bus::PhysicalAddress, intc::InterruptKind};
use crate::{context::Context, hw::extensions::BitManipulation};

#[derive(Copy, Default, Clone, Debug, Eq, PartialEq)]
pub struct TmuRegisters {
    tocr: u8,
    pub tstr: u8,
    tcor0: u32,
    tcor1: u32,
    tcor2: u32,
    tcnt0: u32,
    tcnt1: u32,
    tcnt2: u32,
    tcr0: u16,
    tcr1: u16,
    tcr2: u16,

    channel_0_cycles: u64,
    channel_1_cycles: u64,
    channel_2_cycles: u64,
}

pub struct Tmu {
    pub registers: TmuRegisters,
}

impl Tmu {
    pub fn new() -> Self {
        Self {
            registers: TmuRegisters {
                tcr0: 0x2, // fixme: should be to load_elf
                tcr1: 0x100,
                ..Default::default()
            },
        }
    }

    pub fn tick(&mut self, context: &mut Context) {
        if self.registers.tstr.check_bit(0) {
            self.registers.channel_0_cycles += 1;
            let scale = match self.registers.tcr0 & 0x7 {
                0 => 4,
                1 => 16,
                2 => 64,
                3 => 256,
                4 => 1024,
                _ => panic!("wtf"),
            };

            while self.registers.channel_0_cycles >= scale {
                self.registers.channel_0_cycles -= scale;
                if self.registers.tcnt0 == 0 {
                    self.registers.tcnt0 = self.registers.tcor0;

                    // signal underflow
                    self.registers.tcr0 = self.registers.tcr0.set_bit(8);

                    if self.registers.tcr0.check_bit(5) {
                        context
                            .scheduler
                            .schedule(crate::scheduler::ScheduledEvent::SH4Event {
                                deadline: 200,
                                event_data: crate::hw::sh4::SH4EventData::RaiseIRL {
                                    irl_number: InterruptKind::TUNI0 as usize,
                                },
                            })
                    }
                } else {
                    self.registers.tcnt0 = self.registers.tcnt0.wrapping_sub(1);
                }
            }
        }

        if self.registers.tstr.check_bit(1) {
            self.registers.channel_1_cycles += 1;
            let scale = match self.registers.tcr1 & 0x7 {
                0 => 4,
                1 => 16,
                2 => 64,
                3 => 256,
                4 => 1024,
                _ => unreachable!(),
            };

            while self.registers.channel_1_cycles >= scale {
                self.registers.channel_1_cycles -= scale;
                if self.registers.tcnt1 == 0 {
                    self.registers.tcnt1 = self.registers.tcor1;

                    // signal underflow
                    self.registers.tcr1 = self.registers.tcr1.set_bit(8);
                    if self.registers.tcr1.check_bit(5) {
                        panic!("tmu: timer1 expected an interrupt");
                    }
                } else {
                    self.registers.tcnt1 = self.registers.tcnt1.wrapping_sub(1);
                }
            }
        }

        if self.registers.tstr.check_bit(2) {
            self.registers.channel_2_cycles += 1;
            let scale = match self.registers.tcr2 & 0x7 {
                0 => 4,
                1 => 16,
                2 => 64,
                3 => 256,
                4 => 1024,
                _ => unreachable!(),
            };

            while self.registers.channel_2_cycles >= scale {
                self.registers.channel_2_cycles -= scale;
                if self.registers.tcnt2 == 0 {
                    self.registers.tcnt2 = self.registers.tcor2;

                    // signal underflow
                    self.registers.tcr2 = self.registers.tcr2.set_bit(8);

                    if self.registers.tcr2.check_bit(5) {
                        context
                            .scheduler
                            .schedule(crate::scheduler::ScheduledEvent::SH4Event {
                                deadline: 200,
                                event_data: crate::hw::sh4::SH4EventData::RaiseIRL {
                                    irl_number: InterruptKind::TUNI2 as usize,
                                },
                            })
                    }
                } else {
                    self.registers.tcnt2 = self.registers.tcnt2.wrapping_sub(1);
                }
            }
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
        match addr.0 {
            0x1fd80008 => self.registers.tcor0 = value,
            0x1fd80014 => self.registers.tcor1 = value,
            0x1fd80018 => self.registers.tcnt1 = value,
            0x1fd80020 => self.registers.tcor2 = value,
            0x1fd80024 => self.registers.tcnt2 = value,
            0x1fd8000c => self.registers.tcnt0 = value,
            _ => panic!(
                "tmu: unknown mmio write (32-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x1fd80010 => {
                self.registers.tcr0 = value;
            }
            0x1fd8001c => self.registers.tcr1 = value,
            0x1fd80028 => self.registers.tcr2 = value,
            _ => panic!(
                "tmu: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
        match addr.0 {
            0x1fd80000 => self.registers.tocr = value,
            0x1fd80004 => self.registers.tstr = value,
            _ => panic!(
                "tmu: unknown mmio write (8-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        match addr.0 {
            0x1fd8000c => self.registers.tcnt0,
            0x1fd80024 => self.registers.tcnt2,
            _ => {
                panic!("tmu: unknown mmio read (32-bit) @ 0x{:08x}", addr.0);
                0
            }
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
        match addr.0 {
            0x1fd80010 => self.registers.tcr0,
            0x1fd8001c => self.registers.tcr1,
            0x1fd80028 => self.registers.tcr2,
            _ => {
                panic!("tmu: unknown mmio read (16-bit) @ 0x{:08x}", addr.0);
                0
            }
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            0x1fd80004 => self.registers.tstr,
            _ => {
                //#[cfg(feature = "log_io")]
                panic!("tmu: unknown mmio read (8-bit) @ 0x{:08x}", addr.0);
            }
        }
    }
}
