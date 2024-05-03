use crate::{
    hw::{extensions::BitManipulation, holly::HollyEventData},
    scheduler::Scheduler,
};

pub struct Pvr {}

impl Pvr {
    pub fn new() -> Self {
        Self {}
    }

    pub fn receive_ta_fifo_dma_data(&mut self, scheduler: &mut Scheduler, data: &mut [u32]) {
       // println!("pvr: received TA fifo data from dma {}", data.len() % 8);
        self.handle_cmd(scheduler, data);
    }

    pub fn dump_ram_to_console(print_addr: u32, src: &[u8]) {
        // 16 bytes per line
        let mut bytes_printed = 0;
        for chunk in src.chunks(16) {
            println!("{:08X}   ", print_addr + bytes_printed);
            for &byte in chunk {
                print!("{:02X} ", byte);
                bytes_printed += 1;
            }
            println!();
        }
    }

    pub fn handle_cmd(&mut self, scheduler: &mut Scheduler, data: &mut [u32]) {
        assert!((data.len() % 8) == 0); // dmas should be at a multiple of 8 bytes

        let pcw = data[0];
        let parameter_type = (pcw & 0xE0000000) >> 29;
        let list_type = (pcw & 0x7000000) >> 24;
        let obj_control = (pcw & 0xffff) as u16;

        match parameter_type {
            0x04 => {
                println!("pvr: received polygon parameter");
            }
            0x07 => {
                println!("pvr: received vertex parameter");
            }
            0 => {
                println!("pvr: received end of list parameter");
                println!();
                println!();

                // fix me: set the right istnrm bit depending on the type of list
                scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
                    deadline: 200,
                    event_data: HollyEventData::RaiseInterruptNormal {
                        istnrm: 0.set_bit(7),
                    },
                });
            },
            _ => panic!("pvr: unhandled parameter type {:08x}!", parameter_type)
        }

        // the first thing we receive is the PCW or parameter control word
    }
}
