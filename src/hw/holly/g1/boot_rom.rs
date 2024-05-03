use crate::hw::sh4::bus::PhysicalAddress;

pub struct BootROM {}
pub const BIOS_DATA: &[u8] = include_bytes!("../../../../roms/dc_boot.bin");
pub const BIOS_FLASH: &[u8] = include_bytes!("../../../../roms/dc_flash.bin");

impl BootROM {
  pub fn new() -> Self {
    BootROM {}
  }
  
  pub fn read_8(&self, addr: PhysicalAddress) -> u8 {
    let raw = addr.0;
    match raw {
      0x00000000..=0x001fffff => {
        return BIOS_DATA[raw as usize];
      },
      0x00200000..=0x0021ffff => {
        return BIOS_FLASH[(raw-0x00200000) as usize];
      },
      _ => { panic!("out of bounds read in bios @ {:08x}", addr.0); 0 }
    }
  }
}