#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]

use std::ffi::c_void;
use std::ops::DerefMut;
use std::sync::MutexGuard;
use std::sync::{mpsc, mpsc::Sender, Arc, Mutex};
use std::{ptr, thread};

use crate::hw::holly::spg::SpgEventData;
use crate::hw::holly::HollyEventData;
use crate::scheduler::Scheduler;
use crate::{
    context::{CallStack, Context},
    emulator::{Emulator, EmulatorState},
    hw::{
        extensions::BitManipulation,
        holly::g1::gdi::GdiParser,
        sh4::{bus::CpuBus, SH4EventData},
    },
    scheduler::ScheduledEvent,
};

pub mod context;
pub mod emulator;
pub mod fifo;
pub mod hw;
pub mod scheduler;

#[repr(C)]
pub struct MutexGuardHandle {
    guard: Option<MutexGuard<'static, CpuBus>>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FramebufferFormat {
    Rgb555,
    Rgb565,
    Rgb888,  // (packed)
    ARgb888, // ??
}

#[repr(C)]
pub struct EmulatorHandle {
    emulator: Arc<Mutex<Emulator>>,
    bus: Arc<Mutex<CpuBus>>,
    receiver: *mut mpsc::Receiver<RenderData>,
}

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

#[no_mangle]
pub extern "C" fn emulator_run_loop(handle: *mut EmulatorHandle) {
    let handle = unsafe { &mut *handle };
    let (sender, receiver) = mpsc::channel::<RenderData>();
    handle.receiver = Box::into_raw(Box::new(receiver));

    let emulator_arc = Arc::clone(&handle.emulator);

    Emulator::run_loop(emulator_arc, sender);
}

#[no_mangle]
pub extern "C" fn emulator_try_recv(handle: *mut EmulatorHandle) -> *mut RenderData {
    if handle.is_null() {
        return ptr::null_mut();
    }

    let handle = unsafe { &mut *handle };
    if handle.receiver.is_null() {
        return ptr::null_mut();
    }

    let receiver = unsafe { &*handle.receiver };

    match receiver.try_recv() {
        Ok(received_data) => {
            let data_ptr = Box::into_raw(Box::new(received_data));

            data_ptr
        }
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn bus_lock(handle: *mut RenderData) -> *mut MutexGuardHandle {
    if handle.is_null() {
        return ptr::null_mut();
    }

    let handle = unsafe { &*handle };
    let guard = handle.bus.lock().unwrap();

    let raw = Box::into_raw(Box::new(MutexGuardHandle {
        guard: Some(unsafe {
            std::mem::transmute::<MutexGuard<'_, CpuBus>, MutexGuard<'static, CpuBus>>(guard)
        }),
    }));

    raw
}

#[no_mangle]
pub extern "C" fn bus_unlock(guard_handle: *mut MutexGuardHandle) {
    if guard_handle.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(guard_handle));
    }
}

#[no_mangle]
pub extern "C" fn bus_get_vram(guard_handle: *mut MutexGuardHandle) -> *mut c_void {
    if guard_handle.is_null() {
        return ptr::null_mut();
    }

    let guard_handle = unsafe { &mut *guard_handle };
    if let Some(guard) = &mut guard_handle.guard {
        guard.holly.pvr.vram[guard.holly.registers.fb_display_addr1 as usize..].as_ptr()
            as *mut c_void
    } else {
        ptr::null_mut()
    }
}

#[repr(C)]
pub struct RenderData {
    pub bus: Arc<Mutex<CpuBus>>,
    pub sentinel: u32,
}

impl Emulator {
    pub fn run_loop(emulator_arc: Arc<Mutex<Self>>, frame_ready_sender: Sender<RenderData>) {
        thread::spawn(move || {
            let bus_arc = Arc::new(Mutex::new(CpuBus::new()));
            let gdi_image = GdiParser::load_from_file(
                "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/crazytaxi/ct.gdi",
            );

            let mut scheduler = Scheduler::new();

            {
                // initialize peripherals so they can schedule their initial events
                bus_arc.lock().unwrap().holly.init(&mut scheduler);
                bus_arc
                    .lock()
                    .unwrap()
                    .holly
                    .g1_bus
                    .gd_rom
                    .set_gdi(gdi_image);
            }

            let mut context = Context {
                scheduler: &mut scheduler,
                cyc: 0,
                tracing: false,
                inside_int: false,
                entered_main: false,
                is_test_mode: false,
                tripped_breakpoint: None,
                callstack: CallStack::new(),
                test_base: None,
                test_opcodes: vec![],
                breakpoints: vec![],
            };

            if false {
                //Emulator::load_ip(&mut emulator.cpu, &mut context, &mut bus);
                let syms = Emulator::load_elf(
                    "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/pvr/example.elf",
                    &mut emulator_arc.lock().unwrap().cpu,
                    &mut context,
                    &mut bus_arc.lock().unwrap(),
                )
                .unwrap();
                emulator_arc.lock().unwrap().cpu.symbols_map = syms;
            }

            let mut total_cycles = 0_u64;
            const TIMESLICE: u64 = 448;
            const CPU_RATIO: u64 = 8;
            let mut time_slice = TIMESLICE;
            let mut send_frame = false;

            loop {
                {
                    let running = emulator_arc.lock().unwrap().state == EmulatorState::Running;
                    let mut bus_lock = bus_arc.lock().unwrap();
                    let mut bus = bus_lock.deref_mut();

                    while time_slice > 0 && running {
                        emulator_arc
                            .lock()
                            .unwrap()
                            .cpu
                            .step(&mut bus, &mut context, total_cycles);
                        time_slice -= CPU_RATIO;
                        total_cycles += CPU_RATIO;

                        if bus.holly.g1_bus.gd_rom.output_fifo.borrow().is_empty() {
                            // needed bc this transitions during a mutable read ....
                            if let Some(pending_state) = bus.holly.g1_bus.gd_rom.pending_state {
                                bus.holly
                                    .g1_bus
                                    .gd_rom
                                    .transition(context.scheduler, pending_state);
                                bus.holly.g1_bus.gd_rom.pending_state = None;
                                bus.holly.g1_bus.gd_rom.registers.status.set(
                                    bus.holly.g1_bus.gd_rom.registers.status.get().clear_bit(3),
                                );
                            }
                        }
                    }

                    time_slice += TIMESLICE;

                    if emulator_arc.lock().unwrap().state == EmulatorState::Running {
                        emulator_arc.lock().unwrap().cpu.process_interrupts(
                            &mut bus,
                            &mut context,
                            total_cycles,
                        );

                        context.scheduler.add_cycles(TIMESLICE);
                        bus.holly.cyc += TIMESLICE;

                        for _ in 0..TIMESLICE {
                            bus.tmu.tick(&mut context);
                        }

                        while let Some(evt) = context.scheduler.tick() {
                            match evt {
                                ScheduledEvent::SH4Event { event_data, .. } => {
                                    // fixme: this processing should live somewhere? in cpu.rs? in mod.rs?
                                    match event_data {
                                        SH4EventData::RaiseIRL { irl_number } => {
                                            bus.intc.raise_irl(irl_number);
                                        }
                                    }
                                }
                                ScheduledEvent::HollyEvent { event_data, .. } => {
                                    if let HollyEventData::FrameEnd = event_data {
                                        send_frame = true;
                                    }

                                    let mut dmac = bus.dmac;
                                    let mut system_ram = &mut bus.system_ram;

                                    bus.holly.on_scheduled_event(
                                        context.scheduler,
                                        &mut dmac,
                                        &mut system_ram,
                                        event_data.clone(),
                                    );
                                }
                            }
                        }

                        // Signal that a new frame is ready
                        //
                    }
                }

                if send_frame {
                    frame_ready_sender
                        .send(RenderData {
                            bus: bus_arc.clone(),
                            sentinel: 0821,
                        })
                        .unwrap();
                    send_frame = false;
                }
            }
        });
    }
}
