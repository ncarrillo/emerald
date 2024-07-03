use crate::emulator::Emulator;

/*
#[no_mangle]
pub extern "C" fn emulator_alloc() -> *mut EmulatorHandle {
    let emulator = Arc::new(Mutex::new(Emulator::new()));
    let bus = Arc::new(Mutex::new(CpuBus::new()));
    Box::into_raw(Box::new(EmulatorHandle {
        emulator,
        bus,
        receiver: ptr::null_mut(),
    }))
}
*/
