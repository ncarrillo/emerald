use std::cell::Cell;

// system block
use crate::{context::Context, hw::{extensions::BitManipulation, sh4::bus::PhysicalAddress}};

#[derive(Clone, Debug)]
pub struct SbRegisters {
    pub istnrm: u32,  // interrupt status normal (rw)
    pub iml2nrm: u32, // interrupt mask level 2 normal
    pub iml4nrm: u32, // interrupt mask level 4 normal
    pub iml6nrm: u32, // interrupt mask level 6 normal
    pub istext: u32,  // interrupt status external (r)
    pub iml2ext: u32, // interrupt mask level 2 external
    pub iml4ext: u32, // interrupt mask level 4 external
    pub iml6ext: u32, // interrupt mask level 6 external
    pub isterr: u32,  // interrupt status error
    pub iml2err: u32, // interrupt mask leve 2 error
    pub iml4err: u32, // interrupt mask leve 4 error
    pub iml6err: u32, // interrupt mask leve 6 error
    
    // gd-dma
    pub gd_starting_addr: u32,
    pub gd_len: u32,
    pub gd_dir: u32,
    pub gd_en: u32,
    pub gd_start: u32,

    pub pdtnrm: u32,
    pub pdtext: u32,
    
    pub g2dnrm: u32,
    pub g2dext: u32,

    pub g2_dsto: u32,
    pub g2_trto: u32,
    pub g2_mdmto: u32,
    pub g2_mdmw: u32,
    pub g2_apro: u32,

    pub c2dstat: u32,
    pub c2dlen: u32,
    pub c2dst: u32,
    pub sdstaw: u32,
    pub sdbaaw: u32,
    pub sdwlt: u32,
    pub sdlas: u32,
    pub sdst: u32,
    pub dbreqm: u32,
    pub bavlwc: u32,
    pub c2dpryc: u32,
    pub c2dmaxl: u32,
    pub lmmode0: u32,
    pub lmmode1: u32,
    pub g1_rrc: u32,
    pub g1_rwc: u32,
    pub g1_frc: u32,
    pub g1_fwc: u32,
    pub g1_crc: u32,
    pub g1_cwc: u32,
    pub g1_drc: u32,
    pub g1_dwc: u32,
    pub g1_crdyc: u32,
    pub gd_apro: u32,
    pub ad_stag: u32,
    pub ad_star: u32,
    pub ad_len: u32,
    pub ad_dir: u32,
    pub ad_tsel: u32,
    pub ad_en: u32,
    pub ad_st: u32,
    pub ad_usp: u32,


    pub e1_stag: u32,
    pub e1_star: u32,
    pub e1_len: u32,
    pub e1_dir: u32,
    pub e1_tsel: u32,
    pub e1_en: u32,
    pub e1_st: u32,
    pub e1_usp: u32,

    pub e2_stag: u32,
    pub e2_star: u32,
    pub e2_len: u32,
    pub e2_dir: u32,
    pub e2_tsel: u32,
    pub e2_en: u32,
    pub e2_st: u32,
    pub e2_usp: u32,

    pub dd_stag: u32,
    pub dd_star: u32,
    pub dd_len: u32,
    pub dd_dir: u32,
    pub dd_tsel: u32,
    pub dd_en: u32,
    pub dd_st: u32,
    pub dd_usp: u32,

    pub ffst_cnt: Cell<u32>,
    pub ffst: Cell<u32>,

    // maple dma
    pub mden: u32,
    pub mdst: u32,
    pub mdsel: u32,
    pub mdstar: u32,

    // palette dma
    pub pden: u32,
    pub pdstap: u32,
    pub pdstar: u32,
    pub pdlen: u32,
    pub pdsel: u32,
    pub pdst: u32,
    pub pddir: u32,

}

impl Default for SbRegisters {
    fn default() -> SbRegisters {
        unsafe { std::mem::zeroed() }
    }
}

pub struct SystemBlock {
    pub registers: SbRegisters,
    pub last_addr: u32,

    // fixme: these redundant flags to trigger up to main that we need something to happen are tiring
    // we should properly pipe the scheduler into the bus accesses (and remove bus from context)
    pub pending_recalc: bool
}

impl SystemBlock {
    pub fn new() -> Self {
        Self {
            pending_recalc: false,
            registers: Default::default(),
            last_addr: 0
        }
    }

    pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
        match addr.0 {
            _ => panic!("sb: unimplemented read (8-bit) @ 0x{:08x}", addr.0),
        }
    }

    pub fn read_32(&self, addr: PhysicalAddress) -> u32 {
        let ret = match addr.0 {
            0x005f6808 => self.registers.c2dst,
            0x005f7818 => self.registers.ad_st,
            0x005f7838 => self.registers.e1_st,
            0x005f7858 => self.registers.e2_st,
            0x005f7878 => self.registers.dd_st,
            0x005f7418 => self.registers.gd_start,
            0x005f6c18 => self.registers.mdst,
            0x005F689C => 0x0b,
            0x005f688c => {
                // fifo status - match reicast to get identical traces
                // fwiw, redream returns 0 here unconditionally.
                self.registers.ffst_cnt.set(self.registers.ffst_cnt.get() + 1);

                if (self.registers.ffst_cnt.get() & 0x8) != 0 {
                    self.registers.ffst.set(self.registers.ffst.get() ^ 31);
                }

                self.registers.ffst.get()
            }

            0x005f6900 => self.registers.istnrm.eval_bit(31, self.registers.istext != 0), // fixme: implement
            0x005f6910 => self.registers.iml2nrm,
            0x005f6914 => self.registers.iml2ext,
            0x005f6918 => self.registers.iml2err,
            0x005f6920 => self.registers.iml4nrm,
            0x005f6924 => self.registers.iml4ext,
            0x005f6928 => self.registers.iml4err,
            0x005f6930 => self.registers.iml6nrm,
            0x005f6934 => self.registers.iml6ext,
            0x005f6938 => self.registers.iml6err,
            0x005f6904 => self.registers.istext, 
            0x005f6908 => self.registers.isterr,
            _ => { println!("sb: unimplemented read (32-bit) @ 0x{:08x}", addr.0); 0 },
        };

        #[cfg(feature = "log_io")]
        println!("sb: reading {:08x} from addr {:08x}", ret, addr.0);

        ret
    }

    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x005f7480 => self.registers.g1_rrc = value as u32,
            _ => {
                panic!("sb: unimplemented write (16-bit) @ 0x{:08x}", addr.0);
            }
        }
    }

    pub fn write_32(&mut self, addr: PhysicalAddress, value: u32, context: &mut Context) {
        #[cfg(feature = "log_io")]
        println!("sb: writing to {:08x} with {:08x}", addr.0, value);

        match addr.0 {
            0x005f7484 => self.registers.g1_rwc = value,
            0x005f7488 => self.registers.g1_frc = value,
            0x005f748c => self.registers.g1_fwc = value,
            0x005f7490 => self.registers.g1_frc = value,
            0x005f7494 => self.registers.g1_fwc = value,
            0x005f74a0 => self.registers.g1_drc = value,
            0x005f74a4 => self.registers.g1_dwc = value,
            0x005f74b4 => self.registers.g1_crdyc = value,
            0x005f74b8 => self.registers.gd_apro = value,
            0x005f7890 => self.registers.g2_dsto = value,
            0x005f7c08 => self.registers.pdstar = value,
            0x005f7c08 => self.registers.pdstap = value,
            0x005f7c08 => self.registers.pdlen = value,
            0x005f7c0c => self.registers.pddir = value,
            0x005f7c10 => self.registers.pdsel = value,
            0x005f7c14 => self.registers.pden = value,
            0x005f7c18 => self.registers.pdst = value,

            0x005f7800 => self.registers.ad_stag = value,
            0x005f7804 => self.registers.ad_star = value,
            0x005f7808 => self.registers.ad_len = value,
            0x005f780c => self.registers.ad_dir = value,
            0x005f7810 => self.registers.ad_tsel = value,
            0x005f7814 => self.registers.ad_en = value,
            0x005f7818 => self.registers.ad_st = value,
            0x005f781c => self.registers.ad_usp = value,

            0x005f7820 => self.registers.e1_stag = value,
            0x005f7824 => self.registers.e1_star = value,
            0x005f7828 => self.registers.e1_len = value,
            0x005f782c => self.registers.e1_dir = value,
            0x005f7830 => self.registers.e1_tsel = value,
            0x005f7834 => self.registers.e1_en = value,
            // test registers, bios uses them but they do nothing
            0x005F68AC | 0x005F78A4 | 0x005F78A0 | 0x005F78A8 | 0x005F78AC | 0x005F78B0 | 0x005F78B4 | 0x005F78B8 => {}
            0x005f7838 => self.registers.e1_st = value,
            0x005f783c => self.registers.e1_usp = value,
            0x005f7840 => self.registers.e2_stag = value,
            0x005f7844 => self.registers.e2_star = value,
            0x005f7848 => self.registers.e2_len = value,
            0x005f784c => self.registers.e2_dir = value,
            0x005f7850 => self.registers.e2_tsel = value,
            0x005f7854 => self.registers.e2_en = value,
            0x005f7858 => self.registers.e2_st = value,
            0x005f785c => self.registers.e2_usp = value,
            0x005f7860 => self.registers.dd_stag = value,
            0x005f7864 => self.registers.dd_star = value,
            0x005f7868 => self.registers.dd_len = value,
            0x005f786c => self.registers.dd_dir = value,
            0x005f7870 => self.registers.dd_tsel = value,
            0x005f7874 => self.registers.dd_en = value,
            0x005f7878 => self.registers.dd_st = value,
            0x005f787c => self.registers.dd_usp = value,
            0x005f6884 => self.registers.lmmode0 = value & 1,
            0x005f6888 => self.registers.lmmode1 = value & 1,
            0x005f6900 => {
                self.registers.istnrm &= !value;
                self.pending_recalc = true;
            },
            0x005f6904 => {
                self.registers.istext &= !value;
                self.pending_recalc = true;
            },
            0x005f6908 => {
                self.registers.isterr &= !value;
                self.pending_recalc = true;
            },
            0x005f6910 => {
                self.registers.iml2nrm = value;
                self.pending_recalc = true;
            }
            0x005f6914 => {
                self.registers.iml2ext = value & 0x0000000F;
                self.pending_recalc = true;
            }
            0x005f6918 => {
                self.registers.iml2err = value & 0x9FFFFFFF;
                self.pending_recalc = true;
            },
            0x005f6920 => {
                self.registers.iml4nrm = value;
                self.pending_recalc = true;
            }
            0x005f6924 => {
                self.registers.iml4ext = value & 0x0000000F;
                self.pending_recalc = true;
            },
            0x005f6928 => {
                self.registers.iml4err = value & 0x9FFFFFFF;
                self.pending_recalc = true;
            },
            0x005f6930 => {
                self.registers.iml6nrm = value;
                self.pending_recalc = true;
            },
            0x005f6934 => {
                self.pending_recalc = true;
                self.registers.iml6ext = value & 0x0000000F;
            },
            0x005f6938 => {
                self.registers.iml6err = value & 0x9FFFFFFF;
                self.pending_recalc = true;
            },
            0x005f6940 => self.registers.pdtnrm = value,
            0x005f6944 => self.registers.pdtext = value,
            0x005f6950 => self.registers.g2dnrm = value,
            0x005f6954 => self.registers.g2dext = value,
            0x005f7894 => self.registers.g2_trto = value,
            0x005f7898 => self.registers.g2_mdmto = value,
            0x005f789c => self.registers.g2_mdmw = value,
            0x005f78bc => self.registers.g2_apro = value,
            0x005f7404 => self.registers.gd_starting_addr = value,
            0x005f7408 => self.registers.gd_len = value,
            0x005f740c => self.registers.gd_dir = value,
            0x005f7414 => {
                if value == 1 {
                    panic!("gd-dma: start");
                }

                self.registers.gd_en = value
            },
            0x005f7418 => self.registers.gd_start = value,
            0x005f6800 => {
                self.registers.c2dstat = value & 0x03FFFFE0;
                if self.registers.c2dstat == 0 {
                    // from spec: If 0x0000 0000 is specified for an address, 0x1000 0000 is accessed.
                    self.registers.c2dstat = 0x10000000;
                }
            },
            0x005f6804 => self.registers.c2dlen = value & 0x00FFFFE0,
            0x005f6808 => {
                self.registers.c2dst = value & 1;
                if self.registers.c2dst == 0x1 {
                    context.scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent { deadline: 0, event_data: super::HollyEventData::Ch2DMA })
                }
            },
            0x005f6810 => self.registers.sdstaw = value & 0x07FFFFE0,
            0x005f6814 => self.registers.sdbaaw = value & 0x07FFFFE0,
            0x005f6818 => self.registers.sdwlt = value & 1,
            0x005f681c => self.registers.sdlas = value & 1,
            0x005f6820 => self.registers.sdst = value & 1,
            0x005f6840 => self.registers.dbreqm = value & 1,
            0x005f6844 => self.registers.bavlwc = value,
            0x005f6848 => self.registers.c2dpryc = value,
            0x005f684c => self.registers.c2dmaxl = value,
            0x005f68a0 => {} // rbsplt
            0x005f68a4 => {} // ???
            0x005f68ac => {} // ???
            0x005f6c04 => self.registers.mdstar = value & 0x1FFFFFE0,
            0x005f6c10 => {
                self.registers.mdsel = value & 1;
            },
            0x005f6c14 => self.registers.mden = value & 1,
            0x005f6c18 => {
                if self.registers.mden == 1 {
                    self.registers.mdst = value & 1;
                }
                
                if self.registers.mdst == 1 {
                    // fire an event to kick off Maple DMA. this will cause the host to start the transfer in the maple module
                    context.scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent { deadline: 0, event_data: super::HollyEventData::MapleDMA });
                }
            },
            0x005f6c80 => {}, // maple
            0x005f6c8c => {}, // maple
            0x005f6ce8 => {}, // maple
            0x005f74e4 => {} // some gdrom bios enable checksum thing, ignore
            _ => {
                #[cfg(feature = "log_io")]
                println!(
                    "sb: unimplemented write (32-bit) @ 0x{:08x} with 0x{:08x}",
                    addr.0, value
                );
            }
        }
    }
}
