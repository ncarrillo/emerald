use std::mem;

use crate::scheduler::Scheduler;
use crate::hw::extensions::BitManipulation;

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
#[repr(C, packed)]
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
    data: [u32; 251],
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

    fn write_device_info(buffer: &mut [u8], offset: usize, device_info: &MapleDeviceInfo) {
        let device_info_size = mem::size_of::<MapleDeviceInfo>();
        unsafe {
            let src = device_info as *const MapleDeviceInfo as *const u8;
            let dst = buffer.as_mut_ptr().add(offset);
            std::ptr::copy_nonoverlapping(src, dst, device_info_size);
        }
    }

    pub fn process_maple_frame(
        &mut self,
        tx_frame: &MapleFrame,
        system_ram: &mut [u8],
    ) {
        //let cmd_id = data[0] & 0xff;
        //let port = ((data[0] & 0x30000) >> 16) as usize;
        //let unit = ((data[0] & 0xC0) >> 6) as usize;

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
        let license = "Produced By or Under License From SEGA ENTERPRISES,LTD.";

        let mut name_buffer = [0x20u8; 30];
        let mut license_buffer = [0x20u8; 60];

        // Copy the bytes from input1 to name, ensuring not to exceed the array's size.
        let len_name = name.as_bytes().len().min(name_buffer.len());
        name_buffer[..len_name].copy_from_slice(&name.as_bytes()[..len_name]);

        // Copy the bytes from input2 to license, truncating if longer than 30 bytes.
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

        match tx_frame.cmd {
            0x01 => Self::write_device_info(system_ram, 0, &device_info),
            _ => panic!("maple: received unknown packet {:02x}", 0x00),
        };
    }

    pub fn perform_maple_transfer(&mut self, scheduler: &mut Scheduler, system_ram: &mut [u8]) {
        let mut send_offset = 0;
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
                0x00 => { // normal pattern
                    let receive_address = u32::from_le_bytes([
                        system_ram[send_offset],
                        system_ram[send_offset + 1],
                        system_ram[send_offset + 2],
                        system_ram[send_offset + 3],
                    ]);
                    
                    let recv_offset = (receive_address - 0x0c000000) as usize;
                    send_offset += 4;

                    // read out the send frame
                    let send_frame = {
                        let command_data = Self::as_u32_slice(&system_ram[send_offset..send_offset + transfer_len_in_bytes]);
                        unsafe { &*(command_data.as_ptr() as *const MapleFrame) }
                    };

                    // process cmd + write out the response
                    self.process_maple_frame(send_frame, &mut system_ram[recv_offset..]);
                    send_offset += transfer_len_in_bytes;
                }
                0x07 => {},
                _ => panic!("maple: got an unrecognized pattern {:08x}", pattern),
            };

            if command_header.check_bit(31) {
                break;
            }
        }

        // fixme: I hope this works
        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent { deadline: 20, event_data: super::HollyEventData::RaiseInterruptNormal { istnrm: 0.set_bit(12) } })
    }
}
