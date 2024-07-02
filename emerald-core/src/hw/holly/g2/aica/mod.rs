use rtc::Rtc;

use crate::hw::{extensions::BitManipulation, sh4::bus::PhysicalAddress};

pub mod arm;
pub mod arm_bus;
pub mod rtc;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct ChannelRegisters {
    pub r: [u32; 18],
    pub inactive: [u32; 14], // inactive registers
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct TimerControlRegister(pub u32);

impl TimerControlRegister {
    pub fn value(&self) -> u32 {
        self.0 & 0xFF
    }

    pub fn set_value(&mut self, value: u32) {
        self.0 = (self.0 & !0xFF) | (value & 0xFF);
    }

    pub fn prescale(&self) -> u32 {
        match (self.0 >> 8) & 0b111 {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            4 => 16,
            5 => 32,
            6 => 64,
            7 => 128,
            _ => unreachable!(),
        }
    }
}

pub struct Aica {
    pub wave_ram: Vec<u8>,
    pub rtc: Rtc,
    pub channels: [ChannelRegisters; 64],
    pub timers: [TimerControlRegister; 3],
    pub sound_cpu_interrupts_enabled: bool,
}

impl Aica {
    pub fn new() -> Aica {
        Aica {
            wave_ram: vec![0; 0x7FFFFF],
            rtc: Rtc::new(),
            channels: [Default::default(); 64],
            timers: [Default::default(); 3],
            sound_cpu_interrupts_enabled: false,
        }
    }

    pub fn read_aica_wave_32(&self, physical_addr: PhysicalAddress) -> u32 {
        let addr = (physical_addr.0 as usize - 0x00800000) % self.wave_ram.len() as usize;
        let value = u32::from_le_bytes([
            self.wave_ram[addr],
            self.wave_ram[addr + 1],
            self.wave_ram[addr + 2],
            self.wave_ram[addr + 3],
        ]);

        if physical_addr.0 == 0x80005c {
            // hack -- this is for crazy taxi
            println!(
                "returning aica wave32 hack0, arm core @ {:08x} had {} but we needed 0x01",
                addr, value
            );
            return 0x01;
        }

        if physical_addr.0 == 0x00800104
            || physical_addr.0 == 0x008001C4
            || physical_addr.0 == 0x00800164
            || physical_addr.0 == 0x00800224
        {
            // hack -- this is for crazy taxi
            println!("returning aica wave32 hack1");
            return 0x0090;
        }

        if physical_addr.0 == 0x00800284 || physical_addr.0 == 0x00800288 {
            // hack -- this is for crazy taxi
            println!("returning aica wave32 hack2");
            return 0x0090;
        }

        if addr == 0x00000020 {
            println!(
                "aica: got a read (32-bit) from {:08x} with {:08x}",
                addr, value
            );
        }

        value
    }

    pub fn read_aica_wave_16(&self, physical_addr: PhysicalAddress) -> u16 {
        if physical_addr.0 == 0x80005c {
            // hack -- this is for crazy taxi
            return 0x01;
        }

        if physical_addr.0 == 0x00800104
            || physical_addr.0 == 0x008001C4
            || physical_addr.0 == 0x00800164
            || physical_addr.0 == 0x00800224
        {
            // hack -- this is for crazy taxi
            return 0x0090;
        }

        if physical_addr.0 == 0x00800284 || physical_addr.0 == 0x00800288 {
            // hack -- this is for crazy taxi
            return 0x0090;
        }

        let addr = (physical_addr.0 as usize - 0x00800000) % self.wave_ram.len() as usize;
        let mut value = 0;
        for i in 0..2 {
            value |= (self.wave_ram[addr + i] as u16) << (i * 8);
        }

        value
    }

    pub fn write_aica_wave_16(&mut self, physical_addr: PhysicalAddress, value: u16) {
        let addr = (physical_addr.0 as usize - 0x00800000) % self.wave_ram.len() as usize;
        let value_bytes = value.to_le_bytes();
        for (i, byte) in value_bytes.iter().enumerate() {
            self.wave_ram[addr + i] = *byte;
        }
    }

    pub fn write_aica_wave_32(&mut self, physical_addr: PhysicalAddress, value: u32) {
        let addr = (physical_addr.0 as usize - 0x00800000) % self.wave_ram.len() as usize;
        let value_bytes = value.to_le_bytes();

        for (i, byte) in value_bytes.iter().enumerate() {
            self.wave_ram[addr + i] = *byte;
        }
    }

    pub fn read_aica_register_32(&self, physical_addr: PhysicalAddress) -> u32 {
        let addr = physical_addr.0 & 0x0000FFFF;
        0
    }

    pub fn write_channel_register(&mut self, channel: usize, index: usize, value: u32) {
        match index {
            0 => {
                self.channels[channel].r[index] = value & 0xffff;

                if self.channels[channel].r[index].check_bit(14) {
                    println!(
                        "aica: WARNING: channel {} has been queued for key-on",
                        channel
                    );
                }
            }
            4 => {
                // channel sample addr lo
                self.channels[channel].r[index] = value & 0xffff;
            }
            18..=31 => {}
            _ => println!("aica: unimplemented channel {} register {}", channel, index),
        }
    }

    pub fn tick(&mut self) {}

    pub fn write_aica_register_32(&mut self, physical_addr: PhysicalAddress, value: u32) {
        let addr = physical_addr.0 & 0x0000FFFF;

        match addr {
            0x00000000..=0x00001fff => {
                self.write_channel_register((addr / 128) as usize, (addr % 128) as usize, value);
            }
            0x00002800 => { // master volume
            }
            0x000028B4 => {
                // main cpu interrupt enable
                println!("MCIEB?");
            }
            0x000028BC => {
                // main cpu interrupt reset
                println!("MCIRE?");
            }
            0x0000289C => {
                // sound cpu interrupt enable
                if value != 0 {
                    println!("aica: got a write to SCIEB (int enable) with {:08x}", value);
                }

                self.sound_cpu_interrupts_enabled = value == 1;
            }
            0x000028B8 => {
                if value != 0 {
                    println!(
                        "aica: got a write to MCIPD (int pending) with {:08x}",
                        value
                    );
                }
            }
            0x000028A4 => {
                println!("got a write to SCIRE?");
            }
            _ => println!("unkw reg write to {:08x} with {:08x}", addr, value),
        }
    }
}
