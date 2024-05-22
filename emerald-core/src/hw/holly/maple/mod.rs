use std::mem;

use crate::hw::extensions::BitManipulation;
use crate::scheduler::Scheduler;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct MapleRegisters {}

pub struct Maple {
    registers: MapleRegisters,
}

// maple capabilities
pub const MAPLE_CAP_C: u32 = 1 << 24;
pub const MAPLE_CAP_B: u32 = 1 << 25;
pub const MAPLE_CAP_A: u32 = 1 << 26;
pub const MAPLE_CAP_START: u32 = 1 << 27;
pub const MAPLE_CAP_DPAD_UP: u32 = 1 << 28;
pub const MAPLE_CAP_DPAD_DOWN: u32 = 1 << 29;
pub const MAPLE_CAP_DPAD_LEFT: u32 = 1 << 30;
pub const MAPLE_CAP_DPAD_RIGHT: u32 = 1 << 31;
pub const MAPLE_CAP_Z: u32 = 1 << 16;
pub const MAPLE_CAP_Y: u32 = 1 << 17;
pub const MAPLE_CAP_X: u32 = 1 << 18;
pub const MAPLE_CAP_D: u32 = 1 << 19;
pub const MAPLE_CAP_DPAD2_UP: u32 = 1 << 20;
pub const MAPLE_CAP_DPAD2_DOWN: u32 = 1 << 21;
pub const MAPLE_CAP_DPAD2_LEFT: u32 = 1 << 22;
pub const MAPLE_CAP_DPAD2_RIGHT: u32 = 1 << 23;
pub const MAPLE_CAP_RTRIG: u32 = 1 << 8;
pub const MAPLE_CAP_LTRIG: u32 = 1 << 9;
pub const MAPLE_CAP_ANALOG_X: u32 = 1 << 10;
pub const MAPLE_CAP_ANALOG_Y: u32 = 1 << 11;
pub const MAPLE_CAP_ANALOG2_X: u32 = 1 << 12;
pub const MAPLE_CAP_ANALOG2_Y: u32 = 1 << 13;

pub const MAPLE_CAP_STANDARD_BUTTONS: u32 =
    MAPLE_CAP_A | MAPLE_CAP_B | MAPLE_CAP_X | MAPLE_CAP_Y | MAPLE_CAP_START;
pub const MAPLE_CAP_DPAD: u32 =
    MAPLE_CAP_DPAD_UP | MAPLE_CAP_DPAD_DOWN | MAPLE_CAP_DPAD_LEFT | MAPLE_CAP_DPAD_RIGHT;
pub const MAPLE_CAP_ANALOG: u32 = MAPLE_CAP_ANALOG_X | MAPLE_CAP_ANALOG_Y;
pub const MAPLE_CAP_TRIGGERS: u32 = MAPLE_CAP_LTRIG | MAPLE_CAP_RTRIG;

pub const CONT_TYPE_STANDARD_CONTROLLER: u32 =
    MAPLE_CAP_STANDARD_BUTTONS | MAPLE_CAP_TRIGGERS | MAPLE_CAP_DPAD | MAPLE_CAP_ANALOG;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct MapleDeviceInfo {
    func: u32,
    data: [u32; 3],
    region: u8,
    direction: u8,
    name: [u8; 30],
    license: [u8; 60],
    standby_power: u16,
    max_power: u16,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct MapleFrame {
    cmd: u8,
    dest_addr: u8,
    source_addr: u8,
    length: u8,
    data: [u32; 255],
}

impl Maple {
    pub fn new() -> Self {
        Self {
            registers: MapleRegisters {
                ..Default::default()
            },
        }
    }

    fn as_u32_slice(bytes: &[u8]) -> &[u32] {
        unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u32, bytes.len() / 4) }
    }

    pub fn process_maple_frame(&mut self, tx_frame: &MapleFrame, rx_frame: &mut MapleFrame) {
        let cmd_id = tx_frame.cmd;
        //println!("maple: got command {:08x}", cmd_id);

        match cmd_id {
            0x01 => {
                /*

                int32 func                   ; function codes supported by this peripheral (or:ed together) (big endian)
                int32[3] function_data       ; additional info for the supported function codes (3 max) (big endian)
                int8 area_code               ; regional code of peripheral
                int8 connector_direction (?) ; physical orientation of bus connection
                char[30] product_name        ; name of peripheral
                char[60] product_license     ; license statement
                int16 standby_power          ; standby power consumption (little endian)
                int16 max_power              ; maximum power consumption (little endian)

                 */
                let name = "Dreamcast Controller";
                let license = "Produced By or Under License F rom SEGA ENTERPRISES,LTD.";

                let mut name_buffer = [0x20u8; 30];
                let mut license_buffer = [0x20u8; 60];

                let len_name = name.as_bytes().len().min(name_buffer.len());
                name_buffer[..len_name].copy_from_slice(&name.as_bytes()[..len_name]);

                let len_license = license.as_bytes().len().min(license_buffer.len());
                license_buffer[..len_license].copy_from_slice(&license.as_bytes()[..len_license]);

                let device_info = MapleDeviceInfo {
                    func: (0x01000000_u32),
                    data: [
                        // this is already big endian
                        CONT_TYPE_STANDARD_CONTROLLER,
                        0,
                        0,
                    ],
                    standby_power: 0x01ae,
                    max_power: 0x01f4,
                    region: 0xff,
                    direction: 0, // ?? idk
                    name: name_buffer,
                    license: license_buffer,
                };

                let device_info_bytes = unsafe {
                    let ptr = &device_info as *const MapleDeviceInfo as *const u8;
                    std::slice::from_raw_parts(ptr, std::mem::size_of::<MapleDeviceInfo>())
                };

                unsafe {
                    std::ptr::copy_nonoverlapping(
                        device_info_bytes.as_ptr(),
                        rx_frame.data.as_mut_ptr() as *mut u8,
                        mem::size_of::<MapleDeviceInfo>(),
                    );
                }
            }
            0x09 => {
            }
            _ => {
                panic!("maple: got an unimplemented command {:08x}", cmd_id);
            }
        };
    }

    pub fn perform_maple_transfer(
        &mut self,
        start_offset: usize,
        scheduler: &mut Scheduler,
        system_ram: &mut [u8],
    ) {
        let mut send_offset = start_offset;
        loop {
            let command_header = u32::from_le_bytes([
                system_ram[send_offset],
                system_ram[send_offset + 1],
                system_ram[send_offset + 2],
                system_ram[send_offset + 3],
            ]);
            send_offset += 4;

            let pattern = (command_header & 0x700) >> 8;
            let transfer_len_in_bytes = match command_header & 0xffff {
                0 => 4,
                1 => 8,
                0xfe => 1020,
                0xff => 1024,
                _ => 1024, //panic!("maple: got an unknown dma transfer length"),
            };

            match pattern {
                0x00 => {
                    // normal pattern
                    let receive_address = u32::from_le_bytes([
                        system_ram[send_offset],
                        system_ram[send_offset + 1],
                        system_ram[send_offset + 2],
                        system_ram[send_offset + 3],
                    ]);

                    let recv_offset = (receive_address - 0x0c000000) as usize;
                    send_offset += 4;

                    // read out the send frame
                    let tx_frame = {
                        let command_data = Self::as_u32_slice(&system_ram[send_offset..]);
                        unsafe { &*(command_data.as_ptr() as *const MapleFrame) }
                    };

                    let mut i = 4;
                    let mut rx_frame: MapleFrame = MapleFrame {
                        cmd: if tx_frame.cmd == 0x01 { 0x05 } else { 0x08 },
                        dest_addr: tx_frame.source_addr,
                        source_addr: 0.set_bit(5).set_bit(0),
                        length: (mem::size_of::<MapleDeviceInfo>() >> 2) as u8,
                        data: [0; 255],
                    };

                    // process cmd + write out the response
                    self.process_maple_frame(tx_frame, &mut rx_frame);

                    system_ram[recv_offset] = 0x05;
                    system_ram[recv_offset + 1] = rx_frame.dest_addr;
                    system_ram[recv_offset + 2] = rx_frame.source_addr;
                    system_ram[recv_offset + 3] = (mem::size_of::<MapleDeviceInfo>() >> 2) as u8;

                    let mut i = 4;
                    for word in rx_frame.data {
                        for b in word.to_le_bytes() {
                            system_ram[recv_offset + i] = b;
                            i += 1;
                        }
                    }

                    send_offset += transfer_len_in_bytes;
                }
                0x07 => {}
                _ => panic!("maple: got an unrecognized pattern {:08x}", pattern),
            };

            if command_header.check_bit(31) {
                break;
            }
        }

        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: 200,
            event_data: super::HollyEventData::RaiseInterruptNormal {
                istnrm: 0.set_bit(12),
            },
        })
    }
}
