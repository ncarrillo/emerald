#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]
#![feature(f128)]
#![feature(extract_if)]
#![feature(hash_extract_if)]
#![feature(assert_matches)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{metadata, File};
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, mpsc::Sender, Arc, Mutex};
use std::sync::{MutexGuard, RwLock};
use std::{ptr, thread};

use hw::holly::g1::cdi::CdiParser;
use hw::holly::g2::aica::arm_bus::ArmBus;
use hw::holly::pvr::display_list::{DisplayList, DisplayListBuilder, VertexDefinition};
use hw::holly::pvr::ta::PvrListType;
use hw::holly::pvr::texture_cache::TextureAtlas;
use hw::holly::spg::SpgEventData;
use hw::holly::Holly;
use serde::{Deserialize, Serialize};

use crate::hw::holly::HollyEventData;
use crate::scheduler::Scheduler;
use crate::{
    context::{Context},
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

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub enum EmulatorFrontendRequest {
    ButtonPressed(ControllerButton),
    ButtonReleased(ControllerButton),
    Pause,
    Resume,
    SampleState,
    ToggleWireframe,
    RenderingDone,
}

#[repr(C)]
pub enum EmulatorFrontendResponse {
    RenderHwRast(
        u32,
        Arc<RwLock<TextureAtlas>>,
        Arc<RwLock<Vec<u8>>>, // vram
        Arc<RwLock<Vec<u8>>>, // pram
        [DisplayListBuilder; 5],
        [VertexDefinition; 4],
    ),
    BlitFramebuffer(Vec<u8>, u32, u32),
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub enum ControllerButton {
    A,
    B,
    X,
    Y,
    Start,
    Left,
    Right,
    Up,
    Down,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub enum FramebufferFormat {
    Rgb555,
    Rgb565,
    Rgb888,  // (packed)
    ARgb888, // ??
}

impl Emulator {
    pub fn run_loop(
        mut emulator: Self,
        frame_ready_sender: Sender<EmulatorFrontendResponse>,
        frontend_request_receiver: Receiver<EmulatorFrontendRequest>,
    ) {
        thread::spawn(move || {
            let mut bus = CpuBus::new();
            //let cdi_image =
            //  CdiParser::load_from_file("/Users/ncarrillo/Downloads/arm7wrestler.cdi");

            let gdi_image = GdiParser::load_from_file(
                "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/gf/gf.gdi",
            );

            let mut scheduler = Scheduler::new();

            {
                // initialize peripherals so they can schedule their initial events
                bus.holly.init(&mut scheduler);
                bus.holly.g1_bus.gd_rom.set_gdi(gdi_image);
            }

            let mut context = Context {
                scheduler: &mut scheduler,
                cyc: 0,
                tracing: false,
            };

            if false {
                let syms = Emulator::load_elf(
                    "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/pvr/example.elf",
                    &mut emulator.cpu,
                    &mut context,
                    &mut bus,
                )
                .unwrap();
                emulator.cpu.symbols_map = syms;
            }

            let mut total_cycles = 0_u64;
            const TIMESLICE: u64 = 448;
            const CPU_RATIO: u64 = 8;
            let mut time_slice = TIMESLICE;
            let mut send_frame = false;
            let mut saw_sr = false;
            let mut dl_id = 0;
            let mut blit_frame = false;

            loop {
                {
                    let running = emulator.state == EmulatorState::Running;
                    while time_slice > 0 && running {
                        emulator.cpu.step(&mut bus, &mut context, total_cycles);

                        let mut arm7bus = ArmBus {
                            aica: &mut bus.holly.aica,
                        };

                        bus.holly.arm7tdmi.step(&mut arm7bus);

                        bus.tmu.tick(&mut context);
                        time_slice -= CPU_RATIO;
                        total_cycles += CPU_RATIO;

                        if let Ok(frontend_request) = frontend_request_receiver.try_recv() {
                            match frontend_request {
                                EmulatorFrontendRequest::ButtonPressed(controller_button) => {
                                    match controller_button {
                                        ControllerButton::A => bus.holly.maple.is_a_pressed = true,
                                        ControllerButton::X => bus.holly.maple.is_x_pressed = true,
                                        ControllerButton::Start => {
                                            bus.holly.maple.is_start_pressed = true
                                        }
                                        ControllerButton::Right => {
                                            bus.holly.maple.is_right_pressed = true
                                        }
                                        ControllerButton::Up => {
                                            bus.holly.maple.is_up_pressed = true
                                        }
                                        ControllerButton::Down => {
                                            bus.holly.maple.is_down_pressed = true
                                        }
                                        _ => {}
                                    }
                                }
                                EmulatorFrontendRequest::ButtonReleased(controller_button) => {
                                    match controller_button {
                                        ControllerButton::A => bus.holly.maple.is_a_pressed = false,
                                        ControllerButton::X => bus.holly.maple.is_x_pressed = false,
                                        ControllerButton::Right => {
                                            bus.holly.maple.is_right_pressed = false
                                        }
                                        ControllerButton::Start => {
                                            bus.holly.maple.is_start_pressed = false
                                        }
                                        ControllerButton::Up => {
                                            bus.holly.maple.is_up_pressed = false
                                        }
                                        ControllerButton::Down => {
                                            bus.holly.maple.is_down_pressed = false
                                        }
                                        _ => {}
                                    }
                                }
                                EmulatorFrontendRequest::ToggleWireframe => {
                                    bus.holly.pvr.wireframe = !bus.holly.pvr.wireframe;
                                }
                                EmulatorFrontendRequest::RenderingDone => {
                                    //   panic!("");
                                    context.scheduler.schedule(
                                        crate::scheduler::ScheduledEvent::HollyEvent {
                                            deadline: 0,
                                            event_data: HollyEventData::RaiseInterruptNormal {
                                                istnrm: 0.set_bit(2),
                                            },
                                        },
                                    );

                                    context.scheduler.schedule(
                                        crate::scheduler::ScheduledEvent::HollyEvent {
                                            deadline: 0,
                                            event_data: HollyEventData::RaiseInterruptNormal {
                                                istnrm: 0.set_bit(1),
                                            },
                                        },
                                    );

                                    context.scheduler.schedule(
                                        crate::scheduler::ScheduledEvent::HollyEvent {
                                            deadline: 0,
                                            event_data: HollyEventData::RaiseInterruptNormal {
                                                istnrm: 0.set_bit(0),
                                            },
                                        },
                                    );
                                }
                                _ => {}
                            }
                        }

                        // fixme: see if we can move this out
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

                    // at this point, Reicast call UpdateSystem
                    if emulator.state == EmulatorState::Running {
                        emulator
                            .cpu
                            .process_interrupts(&mut bus, &mut context, total_cycles);

                        context.scheduler.add_cycles(TIMESLICE);
                        bus.holly.cyc += TIMESLICE;

                        for _ in 0..TIMESLICE {}

                        let now = context.scheduler.now();
                        while let Some(entry) = context.scheduler.tick() {
                            let evt = entry.event;
                            match evt {
                                ScheduledEvent::SH4Event { event_data, .. } => {
                                    // fixme: this processing should live somewhere? in cpu.rs? in mod.rs?
                                    match event_data {
                                        SH4EventData::RaiseIRL { irl_number } => {
                                            bus.intc.raise_irl(irl_number);
                                        }
                                    }
                                }
                                ScheduledEvent::HollyEvent {
                                    event_data,
                                    deadline,
                                } => {
                                    if let HollyEventData::FrameReady(dl_id2) = event_data {
                                        send_frame = true;
                                        saw_sr = true;
                                        dl_id = dl_id2;
                                    }

                                    if let HollyEventData::VBlank = event_data {
                                        blit_frame = true;
                                        bus.holly.framebuffer.invalidate_watches();
                                    }

                                    let mut dmac = bus.dmac;
                                    let mut system_ram = &mut bus.system_ram;

                                    let target = deadline - entry.start;
                                    let overrun = (now - entry.start) - target;

                                    bus.holly.on_scheduled_event(
                                        context.scheduler,
                                        &mut dmac,
                                        &mut system_ram,
                                        target,
                                        overrun,
                                        event_data.clone(),
                                    );
                                }
                            }
                        }
                    }
                }

                if !send_frame
                    && bus.holly.framebuffer.dirty
                    && bus.holly.framebuffer.registers.read_ctrl.fb_enable
                    && blit_frame
                {
                    bus.holly.framebuffer.dirty = false;
                    blit_frame = false;
                    let (vram, width, height) = bus
                        .holly
                        .framebuffer
                        .render_framebuffer(&bus.holly.pvr.vram.read().unwrap());

                    frame_ready_sender
                        .send(EmulatorFrontendResponse::BlitFramebuffer(
                            vram, width, height,
                        ))
                        .unwrap();
                } else if send_frame {
                    frame_ready_sender
                        .send(EmulatorFrontendResponse::RenderHwRast(
                            dl_id,
                            bus.holly.pvr.texture_atlas.clone(),
                            bus.holly.pvr.vram.clone(),
                            bus.holly.pvr.pram.clone(),
                            bus.holly.pvr.dlb.clone(),
                            bus.holly.pvr.build_bg_verts(),
                        ))
                        .unwrap();
                    send_frame = false;
                    saw_sr = false;
                }
            }
        });
    }
}
