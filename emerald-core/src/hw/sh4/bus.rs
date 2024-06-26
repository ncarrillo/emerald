use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{bsc::Bsc, ccn::Ccn, cpg::Cpg, dmac::Dmac, intc::Intc, rtc::Rtc, tmu::Tmu};
use crate::emulator::Breakpoint;
use crate::{context::Context, hw::holly::Holly};
use chrono::TimeZone;
use chrono::{DateTime, Datelike, Duration, Local, Utc};
use std::io::{self, Write};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicalAddress(pub u32);

struct SerialBuffer {
    buffer: String,
}

impl SerialBuffer {
    fn new() -> SerialBuffer {
        SerialBuffer {
            buffer: String::new(),
        }
    }
}

impl Write for SerialBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let str_buf = match std::str::from_utf8(buf) {
            Ok(str_buf) => str_buf,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };

        for c in str_buf.chars() {
            if c == '\n' {
                #[cfg(feature = "log_serial")]
                println!("{}", self.buffer);

                self.buffer.clear();
            } else {
                self.buffer.push(c);
            }
        }

        Ok(buf.len())
    }

    // Flushes the remaining buffer if it's not empty.
    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            #[cfg(feature = "log_serial")]
            println!("{}", self.buffer);
            self.buffer.clear();
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MappedLocation {
    ExternalAddress(PhysicalAddress),
    InternalAddress(PhysicalAddress),
    OperandCache(PhysicalAddress),
    StoreQueue(PhysicalAddress),
    Nothing,
}

impl MappedLocation {
    pub fn phys(&self) -> PhysicalAddress {
        match self {
            Self::ExternalAddress(phys) => *phys,
            Self::InternalAddress(phys) => *phys,
            Self::OperandCache(phys) => *phys,
            Self::StoreQueue(phys) => *phys,
            Self::Nothing => PhysicalAddress(0),
        }
    }

    pub fn from_existing(&self, phys: PhysicalAddress) -> MappedLocation {
        match self {
            Self::ExternalAddress(_) => MappedLocation::ExternalAddress(phys),
            Self::InternalAddress(_) => MappedLocation::InternalAddress(phys),
            Self::OperandCache(_) => MappedLocation::OperandCache(phys),
            Self::StoreQueue(_) => MappedLocation::StoreQueue(phys),
            Self::Nothing => MappedLocation::Nothing,
        }
    }
}

pub struct CpuBus {
    mapper: MemoryMapper,
    pub ccn: Ccn,
    pub bsc: Bsc,
    pub tmu: Tmu,
    pub holly: Holly,
    pub rtc: Rtc,
    pub cpg: Cpg,
    pub dmac: Dmac,
    pub intc: Intc,
    pub system_ram: Vec<u8>,
    aica_ram: Vec<u8>,
    pub armsdt: u32,
    pub last_addr: Cell<u32>,
    pub last_complained: Cell<u32>,
    pub spun: Cell<bool>,
    pub store_queues: [[u32; 8]; 2],

    // fixme: ubc, move this to sh4
    pub basra: u8,
    pub basrb: u8,
    pub bara: u32,
    pub barb: u32,

    // fixme: move this to scif
    pub scfsr2: u16,
    pub unk_val: u32,
    pub unk_val1: u32,
    pub rtc_val: u32,

    pub flat_aspace: Vec<u8>,

    pub serial_buffer: SerialBuffer,
}

impl CpuBus {
    pub fn new() -> Self {
        let mut mapper = MemoryMapper::new();

        // map p0 to the physical address space, mirror it 4x
        mapper.add_range(MappedRange {
            start: LogicalAddress(0),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        mapper.add_range(MappedRange {
            start: LogicalAddress(0x20000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        mapper.add_range(MappedRange {
            start: LogicalAddress(0x40000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        mapper.add_range(MappedRange {
            start: LogicalAddress(0x60000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        mapper.add_range(MappedRange {
            start: LogicalAddress(0x7c000000),
            size: 0x3ffffff,
            location: MappedLocation::OperandCache(PhysicalAddress(0)),
        });

        // map p1
        mapper.add_range(MappedRange {
            start: LogicalAddress(0x80000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        // map p2
        mapper.add_range(MappedRange {
            start: LogicalAddress(0xa0000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        // map p3
        mapper.add_range(MappedRange {
            start: LogicalAddress(0xc0000000),
            size: 0x1bffffff,
            location: MappedLocation::ExternalAddress(PhysicalAddress(0)),
        });

        // map p4
        mapper.add_range(MappedRange {
            start: LogicalAddress(0xfc000000),
            size: 0x3ffffff,
            location: MappedLocation::InternalAddress(PhysicalAddress(0x1c000000)),
        });

        // identity map the store queue range
        mapper.add_range(MappedRange {
            start: LogicalAddress(0xe0000000),
            size: 0x3ffffff,
            location: MappedLocation::StoreQueue(PhysicalAddress(0xe0000000)),
        });

        const SYSTEM_RAM_SIZE: usize = 16 * 1024 * 1024;

        CpuBus {
            last_addr: Cell::new(0),
            last_complained: Cell::new(0),
            spun: Cell::new(false),
            armsdt: 0,
            intc: Intc::new(),
            mapper: mapper,
            ccn: Ccn::new(),
            bsc: Bsc::new(),
            tmu: Tmu::new(),
            holly: Holly::new(),
            rtc: Rtc::new(),
            cpg: Cpg::new(),
            dmac: Dmac::new(),
            basrb: 0,
            basra: 0,
            bara: 0,
            barb: 0,
            serial_buffer: SerialBuffer::new(),
            scfsr2: 0x60,
            store_queues: [[0; 8]; 2],
            system_ram: vec![0; SYSTEM_RAM_SIZE],
            aica_ram: vec![0; 0x1FFFFF + 1],
            unk_val: 0,
            unk_val1: 0,
            rtc_val: Self::get_rtc_now() as u32,
            flat_aspace: vec![0; 0xffffffff],
        }
    }

    pub fn check_bp(context: &mut Context, addr: u32, read: bool, fetch: bool, write: bool) {
        context.tripped_breakpoint = context
            .breakpoints
            .iter()
            .find(|f| {
                if let Breakpoint::MemoryBreakpoint {
                    addr: bp_addr,
                    read: bp_read,
                    write: bp_write,
                    fetch: bp_fetch,
                } = f
                {
                    *bp_addr == addr && *bp_read == read && *bp_write == write && *bp_fetch == fetch
                } else {
                    false
                }
            })
            .cloned();
    }

    fn get_rtc_now() -> u32 {
        let dreamcast_epoch =
            UNIX_EPOCH - std::time::Duration::from_secs(20 * 365 * 24 * 60 * 60 + 5 * 24 * 60 * 60);
        let now = SystemTime::now();
        match now.duration_since(dreamcast_epoch) {
            Ok(duration) => duration.as_secs() as u32,
            Err(_) => 0,
        }
    }

    pub fn write_64(&mut self, addr: u32, value: u64, context: &mut Context) {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let mut i = 0;
            for b in value.to_le_bytes() {
                self.flat_aspace[(addr + i) as usize] = b;
                i += 1;
            }
            return;
        }

        let tracing = context.tracing;
        context.tracing = false;
        self.write_32(addr, ((value >> 32) & 0xffffffff) as u32, context);
        self.write_32(addr + 4, (value & 0xffffffff) as u32, context);
        context.tracing = tracing;

        if context.tracing {
            println!(" write64  ({:08x}) {:016x}", addr, value);
        }
    }

    pub fn write_32(&mut self, addr: u32, value: u32, context: &mut Context) {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let mut i = 0;
            for b in value.to_le_bytes() {
                self.flat_aspace[(addr + i) as usize] = b;
                i += 1;
            }
            return;
        }

        let mapped_location = self.mapper.translate(LogicalAddress(addr));

        if context.tracing {
            println!(" write32  ({:08x}) {:08x}", addr, value);
        }

        match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                0x00702c00 => self.armsdt = value,
                0x005f6800..=0x005f9fff => self.holly.write_32(physical_addr, value, context),
                0x05000000..=0x05800000 => {
                    for i in 0..4 {
                        self.holly.write_8(
                            PhysicalAddress((physical_addr.0 + i) as u32),
                            ((value >> (i * 8)) & 0xFF) as u8,
                            context,
                        )
                    }
                }
                0x04000000..=0x04800000 => {
                    for i in 0..4 {
                        self.holly.write_8(
                            PhysicalAddress((physical_addr.0 + i) as u32),
                            ((value >> (i * 8)) & 0xFF) as u8,
                            context,
                        )
                    }
                }
                0x0c000000..=0x0cffffff => {
                    let addr_base = (physical_addr.0 - 0x0c000000) as usize;

                    for i in 0..4 {
                        self.system_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                0x0d000000..=0x0dffffff => {
                    let addr_base = (physical_addr.0 - 0x0d000000) as usize;
                    for i in 0..4 {
                        self.system_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                0x00800000..=0x009fffff => {
                    let addr_base = (physical_addr.0 - 0x00800000) as usize;
                    for i in 0..4 {
                        self.aica_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                0x10000000..=0x13FFFFFF => {
                    self.holly
                        .pvr
                        .receive_ta_data(context.scheduler, physical_addr, value)
                }
                0x00700000..=0x0071000b => {} // fixme: aica
                0x14000000..=0x17FFFFFF => {}
                _ => {
                    panic!(
                        "bus: unexpected external 32-bit write to {:08x} with value {:08x}",
                        addr, value
                    )
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1f000000..=0x1f00003c => self.ccn.write_32(physical_addr, value),
                0x1f800000..=0x1f999999 => self.bsc.write_32(physical_addr, value),
                0x1fa00000..=0x1fa00040 => self.dmac.write_32(physical_addr, value),
                0x1fc80000..=0x1fc8003c => self.rtc.write_32(physical_addr, value),
                0x1fd80000..=0x1fd8002c => self.tmu.write_32(physical_addr, value),
                0x1ffffff8 => self.unk_val = value,
                0x1ffffff4 => self.unk_val1 = value,
                0x1f200000 => self.bara = value,
                0x1f20000c => self.barb = value,
                0x1fe80000..=0x1fe80024 => {}
                _ => panic!(
                    "bus: unexpected internal 32-bit write to {:08x} with value {:08x}",
                    addr, value
                ),
            },
            MappedLocation::OperandCache(physical_addr) => {
                self.ccn.write_oc_32(physical_addr, value);
            }
            MappedLocation::StoreQueue(physical_addr) => {
                let sq_addr = addr & 0x1FFFFFFF;
                let sq = ((sq_addr >> 5) & 1) as usize;
                let idx = ((sq_addr & 0x1c) >> 2) as usize;

                let is_pvr = sq_addr >= 0x10000000 && sq_addr <= 0x13FFFFFF;
                self.store_queues[sq][idx] = value;
            }
            MappedLocation::Nothing => {}
        }
    }

    pub fn write_16(&mut self, addr: u32, value: u16, context: &mut Context) {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let mut i = 0;
            for b in value.to_le_bytes() {
                self.flat_aspace[(addr + i) as usize] = b;
                i += 1;
            }
            return;
        }

        let mapped_location = self.mapper.translate(LogicalAddress(addr));

        if context.tracing {
            println!(" write16  ({:08x}) {:04x}", addr, value);
        }

        match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                0x005f6800..=0x005f9fff => self.holly.write_16(physical_addr, value, context),
                0x05000000..=0x05800000 => {
                    for i in 0..2 {
                        self.holly.write_8(
                            PhysicalAddress((physical_addr.0 + i) as u32),
                            ((value >> (i * 8)) & 0xFF) as u8,
                            context,
                        )
                    }
                }
                0x04000000..=0x04800000 => {
                    for i in 0..2 {
                        self.holly.write_8(
                            PhysicalAddress((physical_addr.0 + i) as u32),
                            ((value >> (i * 8)) & 0xFF) as u8,
                            context,
                        )
                    }
                }
                0x0c000000..=0x0cffffff => {
                    let addr_base = (physical_addr.0 - 0x0c000000) as usize;
                    for i in 0..2 {
                        self.system_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                0x0d000000..=0x0dffffff => {
                    let addr_base = (physical_addr.0 - 0x0d000000) as usize;
                    for i in 0..2 {
                        self.system_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                0x00800000..=0x009fffff => {
                    let addr_base = (physical_addr.0 - 0x00800000) as usize;
                    for i in 0..2 {
                        self.aica_ram[addr_base + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
                _ => {
                    panic!(
                        "bus: unexpected 16-bit write to {:08x} with value {:04x}",
                        addr, value
                    )
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1f800000..=0x1f999999 => self.bsc.write_16(physical_addr, value),
                0x1fc80000..=0x1fc8003c => self.rtc.write_16(physical_addr, value),
                0x1fd00000..=0x1fd0000c => self.intc.write_16(physical_addr, value),
                0x1fd80000..=0x1fd8002c => self.tmu.write_16(physical_addr, value),
                0x1fc00000..=0x1fc00010 => self.cpg.write_16(physical_addr, value), // clock pulse generator
                0x1fe80010 => {}
                0x1fe80000..=0x1fe80024 => {} // scif
                0x1f200000..=0x1f200021 => {} // break controller

                0x1f000084..=0x1f000088 => {}
                _ => panic!(
                    "bus: unexpected 16-bit write to {:08x} with value {:04x}",
                    addr, value
                ),
            },
            MappedLocation::OperandCache(physical_addr) => {
                self.ccn.write_oc_16(physical_addr, value)
            }
            _ => panic!(
                "bus: unexpected 16-bit write to {:08x} with value {:04x}",
                addr, value
            ),
        }
    }

    pub fn write_8(&mut self, addr: u32, value: u8, context: &mut Context) {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let mut i = 0;
            for b in value.to_le_bytes() {
                self.flat_aspace[(addr + i) as usize] = b;
                i += 1;
            }
            return;
        }

        let mapped_location = self.mapper.translate(LogicalAddress(addr));

        if context.tracing {
            println!(" write8   ({:08x}) {:02x}", addr, value);
        }

        match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                // holly
                0x04000000..=0x07ffffff => self.holly.write_8(physical_addr, value, context),
                0x005f6800..=0x005f9fff => self.holly.write_8(physical_addr, value, context),

                // sram + mirrors
                0x0c000000..=0x0cffffff => {
                    self.system_ram[(physical_addr.0 - 0x0c000000) as usize] = value;
                }
                0x0d000000..=0x0dffffff => {
                    self.system_ram[(physical_addr.0 - 0x0d000000) as usize] = value;
                }

                // aica wave ram
                0x00800000..=0x009fffff => {}
                _ => {
                    panic!(
                    "bus: got an unknown external write (8-bit) to 0x{:08x} with {:02x} {:#?} @ cyc {}",
                    physical_addr.0, value, mapped_location, context.cyc
                    )
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1f800000..=0x1f999999 => self.bsc.write_8(physical_addr, value), // bus state controller
                0x1fd80000..=0x1fd8002c => self.tmu.write_8(physical_addr, value), // timer
                0x1fc80000..=0x1fc8003c => self.rtc.write_8(physical_addr, value), // rtc
                0x1fc00000..=0x1fc00010 => self.cpg.write_8(physical_addr, value), // clock pulse generator

                // scif
                0x1fe8000c => {
                    write!(self.serial_buffer, "{}", value as char);
                }
                0x1fe80000..=0x1fe80024 => {} // more scif stuff, ignore for now
                0x1f200000..=0x1f200021 => {} // break controller
                0x1f000014 => self.basra = value,
                0x1f000018 => self.basrb = value,
                _ => {
                    panic!(
                        "bus: got an unknown internal write (8-bit) to 0x{:08x} with {:02x} {:#?}",
                        physical_addr.0, value, mapped_location
                    );
                }
            },
            MappedLocation::OperandCache(physical_addr) => {
                self.ccn.write_oc_8(physical_addr, value)
            }
            _ => {}
        }
    }

    pub fn read_64(&self, addr: u32, context: &mut Context) -> u64 {
        let tracing = context.tracing;
        context.tracing = false;
        let valuelo = self.read_32(addr, context) as u64;
        let valuehi = self.read_32(addr + 4, context) as u64;
        context.tracing = tracing;

        if context.tracing {
            println!(
                " read64   ({:08x}) {:016x}",
                addr,
                (valuehi << 32) | valuelo
            );
        }

        // Combine the two halves into a 64-bit value
        (valuehi << 32) | valuelo
    }

    pub fn read_32(&self, addr: u32, context: &mut Context) -> u32 {
        let mapped_location = self.mapper.translate(LogicalAddress(addr));
        let value = match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                // aica hacks to bypass trace comparison, I hope these dont matter :D
                // at least the 0071xxxx ones are RTC though..
                0x00702c00 => self.armsdt,
                0x00710000 => ((Self::get_rtc_now() as u32) >> 16) & 0x0000FFFFF,
                0x00710004 => (Self::get_rtc_now() as u32) & 0x0000FFFFF,
                0x00702040 => 0,
                0x00702044 => 0,
                0x00710008..=0x0071000B => 0,
                0x0c000000..=0x0cffffff => {
                    let addr_base = (physical_addr.0 - 0x0c000000) as usize;
                    let bytes = [
                        self.system_ram[addr_base],
                        self.system_ram[addr_base + 1],
                        self.system_ram[addr_base + 2],
                        self.system_ram[addr_base + 3],
                    ];

                    let mut value = u32::from_le_bytes(bytes);
                    value
                }
                0x005f6800..=0x005f9fff => self.holly.read_32(physical_addr), // holly
                _ => {
                    let lower = self.read_16(addr, true, context) as u32;
                    let upper = self.read_16(addr + 2, true, context) as u32;

                    (upper << 16) | lower
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1f000000..=0x1f00003c => self.ccn.read_32(physical_addr), // ccn
                0x1f800000..=0x1f999999 => self.bsc.read_32(physical_addr), // bus state controller
                0x1fd80000..=0x1fd8002c => self.tmu.read_32(physical_addr), // timer
                0x1fa00000..=0x1fa00040 => self.dmac.read_32(physical_addr), // dmac
                _ => {
                    let lower = self.read_16(addr, true, context) as u32;
                    let upper = self.read_16(addr + 2, true, context) as u32;

                    (upper << 16) | lower
                }
            },
            MappedLocation::OperandCache(physical_addr) => self.ccn.read_oc_32(physical_addr),
            _ => 0,
        };

        if context.tracing {
            println!(" read32   ({:08x}) {:08x}", addr, value);
        }

        value
    }

    pub fn read_16(&self, addr: u32, fetching: bool, context: &mut Context) -> u16 {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let test = context.current_test.clone().unwrap();
            if fetching {
                assert_eq!(addr, test.cycles[context.cyc as usize].fetch_addr as u32);
                return test.cycles[context.cyc as usize].fetch_val as u16;
            } else {
                assert_eq!(addr, test.cycles[context.cyc as usize].read_addr as u32);
                return test.cycles[context.cyc as usize].read_val as u16;
            }
        }

        let mapped_location = self.mapper.translate(LogicalAddress(addr));
        Self::check_bp(context, addr, !fetching, fetching, false);

        let value = match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                0x005f6800..=0x005f9fff => self.holly.read_16(physical_addr, context),
                0x0c000000..=0x0cffffff => {
                    let addr_base = (physical_addr.0 - 0x0c000000) as usize;
                    let bytes = [self.system_ram[addr_base], self.system_ram[addr_base + 1]];

                    u16::from_le_bytes(bytes)
                }
                0x00800000..=0x009fffff => {
                    let addr_base = (physical_addr.0 - 0x00800000) as usize;
                    let bytes = [self.aica_ram[addr_base], self.aica_ram[addr_base + 1]];

                    u16::from_le_bytes(bytes)
                } // aica wave ram
                _ => {
                    let lower = self.read_8(addr, true, context) as u16;
                    let upper = self.read_8(addr + 1, true, context) as u16;

                    (upper << 8) | lower
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1f800000..=0x1f999999 => self.bsc.read_16(physical_addr), // bus state controller
                0x1fd00000..=0x1fd0000c => self.intc.read_16(physical_addr), // interrupt controller
                0x1fd80000..=0x1fd8002c => self.tmu.read_16(physical_addr), // timer

                // fixme: more atrocities in the name of getting traces to match..
                0x1fe80010 => 0x60,
                0x1f000084 | 0x1f000088 => 0,
                0x1fe80014..=0x1fe80024 => 0,
                _ => {
                    let lower = self.read_8(addr, true, context) as u16;
                    let upper = self.read_8(addr + 1, true, context) as u16;

                    (upper << 8) | lower
                }
            },
            _ => {
                let lower = self.read_8(addr, true, context) as u16;
                let upper = self.read_8(addr + 1, true, context) as u16;

                (upper << 8) | lower
            }
        };

        if context.tracing && !fetching {
            println!(" read16   ({:08x}) {:04x}", addr, value);
        }

        value
    }

    pub fn read_8(&self, addr: u32, fetching: bool, context: &mut Context) -> u8 {
        #[cfg(feature = "json_tests")]
        if context.is_test_mode {
            let test = context.current_test.clone().unwrap();
            assert_eq!(addr, test.cycles[context.cyc as usize].read_addr as u32);
            return test.cycles[context.cyc as usize].read_val as u8;
        }

        let mapped_location = self.mapper.translate(LogicalAddress(addr));

        let value = match mapped_location {
            MappedLocation::ExternalAddress(physical_addr) => match physical_addr.0 {
                0..=0x0023ffff => self.holly.read_8(physical_addr, context), // boot rom
                0x04000000..=0x04800000 => self.holly.read_8(physical_addr, context), // vram
                0x05000000..=0x05800000 => self.holly.read_8(physical_addr, context), // vram
                0x07000000..=0x07800000 => self.holly.read_8(physical_addr, context), // vram
                0x005f6800..=0x005f9fff => self.holly.read_8(physical_addr, context), // holly
                0x00800000..=0x009fffff => self.aica_ram[(physical_addr.0 - 0x00800000) as usize], // aica wave ram
                0x00600000..=0x006fffff => 0, // fixme: async modem area?
                0x0c000000..=0x0cffffff => self.system_ram[(physical_addr.0 - 0x0c000000) as usize], // sram
                0x0d000000..=0x0dffffff => self.system_ram[(physical_addr.0 - 0x0d000000) as usize], // sram mirror
                _ => {
                    panic!(
                        "bus: got an unknown external read (8-bit) to 0x{:08x}",
                        physical_addr.0
                    );

                    0
                }
            },
            MappedLocation::InternalAddress(physical_addr) => match physical_addr.0 {
                0x1FE80004 => 0,
                0x1f800000..=0x1f999999 => self.bsc.read_8(physical_addr), // bus state controller
                0x1fd80000..=0x1fd8002c => self.tmu.read_8(physical_addr), // timer
                0x1fc0000c => 0,                                           // idk
                _ => {
                    panic!(
                        "bus: got an unknown internal read (8-bit) to 0x{:08x}",
                        physical_addr.0
                    );
                    0
                }
            },
            MappedLocation::OperandCache(physical_addr) => self.ccn.read_oc_8(physical_addr),
            MappedLocation::Nothing => 0,
            MappedLocation::StoreQueue(_) => unreachable!(),
        };

        if context.tracing && !fetching {
            println!(" read8    ({:08x}) {:02x}", addr, value);
        }

        value
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LogicalAddress(pub u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct MappedRange {
    start: LogicalAddress,
    size: usize,
    location: MappedLocation,
}

impl MappedRange {
    fn contains(&self, addr: LogicalAddress) -> bool {
        let LogicalAddress(logical) = addr;
        let LogicalAddress(start) = self.start;
        logical >= start && logical < (start + self.size as u32)
    }

    fn resolve(&self, addr: LogicalAddress) -> Option<MappedLocation> {
        if self.contains(addr) {
            let LogicalAddress(logical) = addr;
            let LogicalAddress(start) = self.start;
            Some(
                self.location
                    .from_existing(PhysicalAddress(self.location.phys().0 + (logical - start))),
            )
        } else {
            None
        }
    }
}

struct MemoryMapper {
    ranges: Vec<MappedRange>,
}

impl MemoryMapper {
    pub fn new() -> Self {
        MemoryMapper { ranges: Vec::new() }
    }

    pub fn add_range(&mut self, range: MappedRange) {
        self.ranges.push(range);
    }

    pub fn translate(&self, addr: LogicalAddress) -> MappedLocation {
        for range in &self.ranges {
            if let Some(phys_addr) = range.resolve(addr) {
                return phys_addr;
            }
        }

        MappedLocation::Nothing
    }
}
