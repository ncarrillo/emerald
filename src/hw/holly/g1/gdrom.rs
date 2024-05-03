use std::cell::{Cell, RefCell};

use crate::{
    fifo::Fifo,
    hw::{
        extensions::BitManipulation,
        holly::{sb::SystemBlock, HollyEventData},
        sh4::bus::PhysicalAddress,
    },
    scheduler::Scheduler,
};

use super::gdi::GdiImage;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GdromEventData {
    AckCommandWithStat(u32),
    ProcessCommand(u8),
    ProcessSPICommand(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct Gdrom {
    pub registers: GdromRegisters,
    pub pending_cmd: Option<u8>,
    pub pending_data: Vec<u8>,
    pub pending_clear: Cell<bool>,
    pub pending_err: bool,
    pub pending_ack: Option<u32>,
    pub output_fifo: RefCell<Fifo<u8, 0xffffff>>,
    pub gdi_image: Option<GdiImage>,
}

#[derive(Clone, Default, Debug)]
pub struct GdromRegisters {
    status: Cell<u32>,
    control: u32,
    features: u32,
    sector_count: u32,
    sector_num_status: u32,
    byte_count_lo: u8,
    byte_count_hi: u8,
    sns_key: u8,
    sns_asc: u8,
}

impl GdromRegisters {
    pub fn new() -> Self {
        Self {
            sns_key: 0x6,
            sns_asc: 0,
            sector_num_status: 0x02,
            ..Default::default()
        }
    }
}

const REQ_MODE_TABLE: [u16; 16] = [
    0x0000_u16, 0x0000, 0xb400, 0x0019, 0x0800, 0x4553, 0x2020, 0x2020, 0x2020, 0x6552, 0x2076,
    0x2e36, 0x3334, 0x3939, 0x3430, 0x3830,
];

impl Gdrom {
    pub fn new() -> Self {
        Self {
            pending_cmd: None,
            pending_data: vec![],
            pending_err: false,
            pending_ack: None, // pending holly interrupt lower
            output_fifo: RefCell::new(Fifo::new()),
            gdi_image: None,
            registers: GdromRegisters::new(),
            pending_clear: Cell::new(false),
        }
    }

    pub fn set_gdi(&mut self, gdi_image: GdiImage) {
        self.gdi_image = Some(gdi_image);
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32) {
        match addr.0 {
            _ => {
                panic!(
                    "gdrom: unimplemented write (32-bit) @ 0x{:08x} with 0x{:08x}",
                    addr.0, value
                );
            }
        }
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x005f7080 => {
                // data
                // we get 6 16-bit writes which make up a total of 12 bytes for the input parameters to spi commands
                let bytes = u16::to_le_bytes(value);
                self.pending_data.push(bytes[0]);
                self.pending_data.push(bytes[1]);
            }
            _ => {
                panic!(
                    "gdrom: unimplemented write (16-bit) @ 0x{:08x} with 0x{:04x}",
                    addr.0, value
                );
            }
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
        match addr.0 {
            0x005f7080 => {
                let lo = self.output_fifo.borrow_mut().pop().unwrap();
                let hi = self.output_fifo.borrow_mut().pop().unwrap();

                if self.output_fifo.borrow().is_empty() {
                    self.registers
                        .status
                        .set(self.registers.status.get().clear_bit(3));
                }

                let val = lo as u16 | ((hi as u16) << 8);
                val
            }
            _ => {
                panic!("gdrom: unimplemented read (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            0x005f708c => (8 << 4) | self.registers.sector_num_status as u8,

            0x005f7018 => {
                let stat = self.registers.status.get().set_bit(4) as u8;
                stat
            }
            0x005f709c => {
                let status = self.registers.status.get().set_bit(4) as u8;

                #[cfg(feature = "log_io")]
                println!("got a gd-rom stat read.. {:02x}", status);

                status
            }
            0x005f7081 => 0,
            0x005f7090 => self.registers.byte_count_lo,
            0x005f7094 => self.registers.byte_count_hi,
            _ => {
                panic!("gdrom: unimplemented read (8-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn write_8(&mut self, addr: PhysicalAddress, value: u8) {
        match addr.0 {
            0x005f7018 => self.registers.control = value as u32,
            0x005f7084 => self.registers.features = value as u32,
            0x005f7088 => self.registers.sector_count = value as u32,
            0x005f7090 => self.registers.byte_count_lo = value,
            0x005f7094 => self.registers.byte_count_hi = value,
            0x005f709c => self.pending_cmd = Some(value),
            _ => {
                panic!(
                    "gdrom: unimplemented write (8-bit) @ 0x{:08x} with 0x{:02x}",
                    addr.0, value
                );
            }
        }
    }

    pub fn on_scheduled_event(
        &mut self,
        scheduler: &mut Scheduler,
        sb: &mut SystemBlock,
        event_data: GdromEventData,
    ) {
        match event_data {
            GdromEventData::ProcessCommand(cmd) => {
                self.registers.status.set(self.registers.status.get().clear_bit(6));

                self.process_cmd(cmd, scheduler)
            }
            GdromEventData::ProcessSPICommand(params) => {
                self.process_spi_cmd(&params, scheduler)
            }
            GdromEventData::AckCommandWithStat(stat) => {
                // fixme: should this be scheduled immediately?
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: HollyEventData::RaiseInterruptExternal {
                        istext: sb.registers.istext.set_bit(0),
                    },
                });

                self.registers.status.set(stat);
            }
            _ => panic!("gdrom: unimplemented event {:#?}", event_data),
        }
    }

    pub fn process_cmd(&mut self, cmd: u8, scheduler: &mut Scheduler) {

        #[cfg(feature = "log_io")]
        println!("got cmd {:02x}", cmd);

        if self.registers.sns_key != 0 {
            self.registers
                .status
                .set(self.registers.status.get().set_bit(0));
        } else {
            self.registers
                .status
                .set(self.registers.status.get().clear_bit(0));
        }

        match cmd {
            0x08 => {
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 80,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7)
                                .set_bit(6)
                                .clear_bit(3),
                        ),
                    ),
                });
            }
            0xa0 => {
                self.registers.status.set(self.registers.status.get().clear_bit(7).set_bit(3));
            }
            0xef => {
                self.registers
                    .status
                    .set(self.registers.status.get().clear_bit(0));
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 80,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7)
                                .set_bit(6)
                                .clear_bit(3),
                        ),
                    ),
                });
            }
            _ => panic!("gdrom: unimplemented command {:02x}", cmd),
        }
    }

    pub fn process_spi_cmd(&mut self, parameters: &[u8], scheduler: &mut Scheduler) {
        let cmd = parameters[0];

        #[cfg(feature = "log_io")]
        println!("gdrom: got spi cmd {:02x}", cmd);

        if self.registers.sns_key != 0 {
            self.registers
                .status
                .set(self.registers.status.get().set_bit(0));
        } else {
            self.registers
                .status
                .set(self.registers.status.get().clear_bit(0));
        }

        match cmd {
            0x00 => {
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7)
                                .set_bit(6)
                                .clear_bit(3)
                                .clear_bit(0)
                        ),
                    ),
                });
            }
            0x70 => {
                // 0x70 - undocumented SPI command
                // we can safely treat this as a nop and ack the command
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7)
                                .set_bit(6)
                                .clear_bit(3),
                        ),
                    ),
                });
            }
            0x71 => {
                // 0x71 - undocumented SPI command
                // output from a real dreamcast, passed down from emu to emu
                let output = [
                    0x0b96, 0xf045, 0xff7e, 0x063d, 0x7d4d, 0xbf10, 0x0007, 0xcf73, 0x009c, 0x0cbc,
                    0xaf1c, 0x301c, 0xa7e7, 0xa803, 0x0098, 0x0fbd, 0x5bbd, 0x50aa, 0x3923, 0x1031,
                    0x690e, 0xe513, 0xd200, 0x660d, 0xbf54, 0xfd5f, 0x7437, 0x5bf4, 0x0022, 0x09c6,
                    0xca0f, 0xe893, 0xaba4, 0x6100, 0x2e0e, 0x4be1, 0x8b76, 0xa56a, 0xe69c, 0xc423,
                    0x4b00, 0x1b06, 0x0191, 0xe200, 0xcf0d, 0x38ca, 0xb93a, 0x91e7, 0xefe5, 0x004b,
                    0x09d6, 0x68d3, 0xc43e, 0x2daf, 0x2a00, 0xf90d, 0x78fc, 0xaeed, 0xb399, 0x5a32,
                    0x00e7, 0x0a4c, 0x9722, 0x825b, 0x7a06, 0x004c, 0x0e42, 0x7857, 0xf546, 0xfc20,
                    0xcb6b, 0x5b01, 0x0086, 0x0ee4, 0x26b2, 0x71cd, 0xa5e3, 0x0633, 0x9a8e, 0x0050,
                    0x0707, 0x34f5, 0xe6ef, 0x3200, 0x130f, 0x5941, 0x0f56, 0x3802, 0x642a, 0x072a,
                    0x003e, 0x1152, 0x1d2a, 0x765f, 0xa066, 0x2fb2, 0xc797, 0x6e5e, 0xe252, 0x5800,
                    0xca09, 0xa589, 0x0adf, 0x00de, 0x0650, 0xb849, 0x00b4, 0x0577, 0xe824, 0xbb00,
                    0x910c, 0xa289, 0x628b, 0x6ade, 0x60c6, 0xe700, 0x0f0f, 0x9611, 0xd255, 0xe6bf,
                    0x0b48, 0xab5c, 0x00dc, 0x0aba, 0xd730, 0x0e48, 0x6378, 0x000c, 0x0dd2, 0x8afb,
                    0xfea3, 0x3af8, 0x88dd, 0x4ba9, 0xa200, 0x750a, 0x0d5d, 0x2437, 0x9dc5, 0xf700,
                    0x250b, 0xdbef, 0xe041, 0x3e52, 0x004e, 0x03b7, 0xe500, 0xb911, 0x5ade, 0xcf57,
                    0x1ab9, 0x7ffc, 0xee26, 0xcd7b, 0x002b, 0x084b, 0x09b8, 0x6a70, 0x009f, 0x114b,
                    0x158c, 0xa387, 0x4f05, 0x8e37, 0xde63, 0x39ef, 0x4bfc, 0xab00, 0x0b10, 0xaa91,
                    0xe10f, 0xaee9, 0x3a69, 0x03f8, 0xd269, 0xe200, 0xc107, 0x3d5c, 0x0082, 0x08a9,
                    0xc468, 0x2ead, 0x00d1, 0x0ef7, 0x47c6, 0xcdc8, 0x7c8e, 0x5c00, 0xb995, 0x00f4,
                    0x04e3, 0x005b, 0x0774, 0xc765, 0x8e84, 0xc600, 0x6107, 0x4480, 0x003f, 0x0ec8,
                    0x7872, 0xd347, 0x4dc2, 0xc0af, 0x1354, 0x0031, 0x0df7, 0xd848, 0x92e2, 0x7f9f,
                    0x442f, 0x3368, 0x0d00, 0xab10, 0xeafe, 0x198e, 0xf881, 0x7c6f, 0xe1de, 0x06b3,
                    0x4d00, 0x6611, 0x4cae, 0xb7f9, 0xee2f, 0x8eb0, 0xe17e, 0x958d, 0x006f, 0x0df4,
                    0x9d88, 0xe3ca, 0xb2c4, 0xbb47, 0x69a0, 0xf300, 0x480b, 0x4117, 0xa064, 0x710e,
                    0x0082, 0x1e34, 0x4d18, 0x8085, 0xa94c, 0x660b, 0x759b, 0x6113, 0x2770, 0x7a81,
                    0xcd02, 0xab57, 0x02df, 0x5293, 0xdf83, 0xa848, 0x9ea6, 0x6f74, 0x0389, 0x2528,
                    0x9652, 0x67ff, 0xd87a, 0xb13c, 0x462c, 0xef84, 0xc1e1, 0xc9c6, 0x96dc, 0xa9aa,
                    0x82c4, 0x2758, 0x7557, 0x3467, 0x3bfb, 0xbf25, 0x3bfb, 0x13f6, 0x96ec, 0x16e5,
                    0xfd26, 0xdaa8, 0xc61b, 0x7f50, 0xff47, 0x5508, 0xed08, 0x9300, 0xc49b, 0x6771,
                    0xa6ec, 0x16cc, 0x8720, 0x0747, 0x00a6, 0x5d79, 0xab4f, 0x6fa1, 0x6b7a, 0xc427,
                    0xa3da, 0x94c3, 0x7f4f, 0xe5f3, 0x6f1b, 0xe5cc, 0xe5f0, 0xc99d, 0xfdae, 0xac39,
                    0xe54c, 0x8358, 0x6525, 0x7492, 0x819e, 0xb6a0, 0x02a9, 0x079b, 0xe7b6, 0x5779,
                    0x4ad9, 0xface, 0x94b4, 0xcc05, 0x3c86, 0x06dd, 0xa6cd, 0x2424, 0xc1fa, 0x48f9,
                    0x0cc9, 0xc46c, 0x8296, 0xf617, 0x0931, 0xe2c4, 0xfd77, 0x46cf, 0xb218, 0x015f,
                    0xd16b, 0x567b, 0x94b8, 0xe54a, 0x196c, 0xc0f0, 0x70b6, 0xf793, 0xd1d3, 0x6e2b,
                    0x537c, 0x856d, 0x0cd1, 0x778b, 0x90ee, 0x15da, 0xe055, 0x0958, 0xfc56, 0x9f31,
                    0x46af, 0xc3cb, 0x718d, 0xf275, 0xc32c, 0xa1bb, 0xcfc4, 0x5627, 0x9b7c, 0xaffe,
                    0x4e3e, 0xcdb4, 0xaa6a, 0xf3f5, 0x22e3, 0xe182, 0x68a5, 0xdbb3, 0x9e8f, 0x7b5e,
                    0xf090, 0x3f79, 0x8c52, 0x8861, 0xae76, 0x6314, 0x0f19, 0xce1d, 0x63a1, 0xb210,
                    0xd7e2, 0xb194, 0xcb33, 0x8528, 0x9b7d, 0xf4f5, 0x5025, 0xdb9b, 0xa535, 0x9cb0,
                    0x9209, 0x31e3, 0xab40, 0xf44d, 0xe835, 0x0ab3, 0xc321, 0x9c86, 0x29cb, 0x77a4,
                    0xbc57, 0xdad8, 0x82a5, 0xe880, 0x72cf, 0xad81, 0x282e, 0xd8ff, 0xd1b6, 0x972b,
                    0xff00, 0x06e1, 0x3944, 0x4b1c, 0x19ab, 0x4d5b, 0x3ed6, 0x5c1b, 0xbb64, 0x6832,
                    0x7cf5, 0x9ec9, 0xb4e8, 0x1b29, 0x4d7f, 0x8080, 0x8b7e, 0x0a1c, 0x9ae6, 0x49bf,
                    0xc51e, 0x67b6, 0x057d, 0x90e4, 0x4b40, 0x9baf, 0xde52, 0x8017, 0x5681, 0x3aea,
                    0x8253, 0x628c, 0x96fb, 0x6f97, 0x16c1, 0xd478, 0xe77b, 0x5ab9, 0xeb2a, 0x6887,
                    0xd333, 0x4531, 0xfefa, 0x1cf4, 0x8690, 0x7773, 0xa9d9, 0x4ad1, 0xcf4a, 0x23ae,
                    0xf9db, 0xd809, 0xdc18, 0x0d6a, 0x19e4, 0x658c, 0x64c6, 0xdcc7, 0xe3a9, 0xb191,
                    0xc84c, 0x9ec1, 0x7f3b, 0xa3cb, 0xddcf, 0x1df0, 0x6e07, 0xcedc, 0xcd0d, 0x1e7e,
                    0x1155, 0xdf8b, 0xab3a, 0x3bb6, 0x526e, 0xa77f, 0xd100, 0xbe33, 0x9bf2, 0x4afc,
                    0x9dcf, 0xc68f, 0x7bc4, 0xe7da, 0x1c2a, 0x6e26,
                ];

                for b in &output {
                    let bytes = u16::to_le_bytes(*b);
                    self.output_fifo.borrow_mut().push(bytes[0]).unwrap();
                    self.output_fifo.borrow_mut().push(bytes[1]).unwrap();
                }

                let output_len = u16::to_le_bytes(output.len() as u16);
                self.registers.byte_count_lo = output_len[0];
                self.registers.byte_count_hi = output_len[1];
                self.registers.sector_num_status = 0x2;

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7) // clear bsy
                                .set_bit(6) // set drdy
                                .set_bit(3), // set drq
                        ),
                    ),
                });
            }
            0x13 => {
                // REQ_ERROR
                let len = parameters[4] as usize;

                self.pending_err = false;
                if len > 0 {
                    self.output_fifo.borrow_mut().push(0xf0).unwrap();
                    self.output_fifo.borrow_mut().push(0x00).unwrap();
                    self.output_fifo
                        .borrow_mut()
                        .push(((self.registers.sns_key as u8) << 4) | 0)
                        .unwrap();

                    self.output_fifo.borrow_mut().push(0x00).unwrap();

                    self.output_fifo.borrow_mut().push(0x00).unwrap();
                    self.output_fifo.borrow_mut().push(0x00).unwrap();
                    self.output_fifo.borrow_mut().push(0x00).unwrap();
                    self.output_fifo.borrow_mut().push(0x00).unwrap();
                    self.output_fifo
                        .borrow_mut()
                        .push(self.registers.sns_asc)
                        .unwrap();
                    self.output_fifo.borrow_mut().push(0x00).unwrap();

                    let output_len = u16::to_le_bytes(len as u16);
                    self.registers.byte_count_lo = output_len[0];
                    self.registers.byte_count_hi = output_len[1];
                }

                self.registers.sns_key = 0;
                self.registers.sns_asc = 0;
                self.registers.status.set(
                    self.registers
                        .status
                        .get()
                        .clear_bit(7) // clear bsy
                        .clear_bit(0) // clear check
                        .set_bit(6) // set drdy
                        .eval_bit(3, len > 0),
                );

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7) // clear bsy
                                .clear_bit(0)
                                .set_bit(6) // set drdy
                                .eval_bit(3, len > 0), // set drq if len > 0
                        ),
                    ),
                });
            }
            0x11 => {
                // REQ_MODE
                let start_addr = parameters[2] as usize;
                let len = parameters[4] as usize;

                if len > 0 {
                    let start_idx = start_addr / 2;
                    let end_idx = start_idx + (len / 2);

                    // use the start and end indices to read out of the REQ_MODE table
                    for b in &REQ_MODE_TABLE[start_idx..end_idx] {
                        let bytes = u16::to_le_bytes(*b);
                        self.output_fifo.borrow_mut().push(bytes[0]).unwrap();
                        self.output_fifo.borrow_mut().push(bytes[1]).unwrap();
                    }

                    let output_len = u16::to_le_bytes(len as u16);
                    self.registers.byte_count_lo = output_len[0];
                    self.registers.byte_count_hi = output_len[1];
                }

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            if len > 0 { 
                                self.registers
                                .status
                                .get()
                                .clear_bit(7) // clear bsy
                                .set_bit(6)
                                .set_bit(3) // drq
                            } else {
                                self.registers
                                .status
                                .get()
                                .clear_bit(7) // clear busy 
                                .set_bit(6) // set drdy 
                                .clear_bit(3) // clear drq 
                            }
                        ),
                    ),
                });
            }
            0x14 => {
                // REQ_TOC
                let area = parameters[1].check_bit(0) as usize; // select bit
                let len = u16::from_le_bytes([parameters[4], parameters[3]]);
                let img = self.gdi_image.as_ref().unwrap();

                let area_descriptor = img.get_descriptor_for_area(area);

                for _ in 0..area_descriptor.start_track {
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                }

                for track_id in area_descriptor.start_track..=area_descriptor.end_track {
                    self.output_fifo
                        .borrow_mut()
                        .push(((img.tracks[track_id].lba_start & 0x0000ff00) >> 8) as u8)
                        .unwrap();
                    self.output_fifo
                        .borrow_mut()
                        .push((img.tracks[track_id].lba_start & 0x000000ff) as u8)
                        .unwrap();
                    self.output_fifo
                        .borrow_mut()
                        .push(((img.tracks[track_id].control as u8) << 4) | 0)
                        .unwrap(); // fixme: adr
                    self.output_fifo
                        .borrow_mut()
                        .push(((img.tracks[track_id].lba_start & 0x00ff0000) >> 16) as u8)
                        .unwrap();
                }

                for _ in area_descriptor.end_track + 1..99 {
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                    self.output_fifo.borrow_mut().push(0xff).unwrap();
                }

                self.output_fifo.borrow_mut().push(0x00).unwrap();
                self.output_fifo.borrow_mut().push(0x00).unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push(((img.tracks[area_descriptor.start_track].control & 0xf) << 4) as u8 | 0)
                    .unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push((0x01 & 0xff) as u8)
                    .unwrap();

                self.output_fifo.borrow_mut().push(0x00).unwrap();
                self.output_fifo.borrow_mut().push(0x00).unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push(((0x0 & 0xf) << 4) as u8 | 0)
                    .unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push((0x02 & 0xff) as u8)
                    .unwrap();

                self.output_fifo
                    .borrow_mut()
                    .push(((0x3300 & 0x0000ff00) >> 8) as u8)
                    .unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push((0x1d & 0x000000ff) as u8)
                    .unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push(((0 & 0xf) << 4) as u8 | 0)
                    .unwrap();
                self.output_fifo
                    .borrow_mut()
                    .push(((0x00 & 0x00ff0000) >> 16) as u8)
                    .unwrap();

                self.registers.byte_count_lo = parameters[4];
                self.registers.byte_count_hi = parameters[3];

                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: super::super::HollyEventData::GdromEvent(
                        GdromEventData::AckCommandWithStat(
                            self.registers
                                .status
                                .get()
                                .clear_bit(7) // clear bsy
                                .set_bit(6) // set drdy
                                .set_bit(4) // dsc??
                                .eval_bit(3, len > 0), // set drq if len > 0
                        ),
                    ),
                });
            }
            _ => panic!("gdrom unimplemented spi command {:02x}", cmd),
        }
    }
}
