#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]
#![feature(hash_extract_if)]

use emerald_core::context::Context;
use emerald_core::hw::extensions::BitManipulation;
use emerald_core::hw::holly::g1::gdi::GdiParser;
use emerald_core::hw::sh4::bus::CpuBus;
use emerald_core::scheduler::Scheduler;
use emerald_core::EmulatorFrontendResponse;
use emerald_core::{ControllerButton, EmulatorFrontendRequest, FramebufferFormat};
use gpu::HardwareRasterizer;
use sdl2::sys::abs;
use sdl2::video::Window;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use wgpu::util::DeviceExt;

use std::ops::Sub;
use std::{
    ffi::c_void,
    ops::DerefMut,
    sync::{mpsc, Arc, Mutex},
};

use emerald_core::emulator::Emulator;
use sdl2::{event::Event, keyboard::Keycode, libc::memcpy, pixels::PixelFormatEnum};

mod gpu;

pub fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let mut window = video_subsystem
        .window("emerald", 1280, 960)
        .position_centered()
        .metal_view()
        .build()
        .map_err(|e| e.to_string())?;

    let mut hw_rasterizer = HardwareRasterizer::new(&window);
    let mut event_pump = sdl_context.event_pump()?;
    let emulator = Emulator::new();

    let (frame_ready_sender, frame_ready_receiver) = mpsc::channel();
    let (frontend_request_sender, frontend_request_receiver) = mpsc::channel();

    Emulator::run_loop(emulator, frame_ready_sender, frontend_request_receiver);

    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {
                    keycode: Some(Keycode::R),
                    ..
                } => {}
                Event::KeyDown {
                    keycode: Some(Keycode::X),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(ControllerButton::X))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::X),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(ControllerButton::X))
                        .unwrap();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::A),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(ControllerButton::A))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::A),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(ControllerButton::A))
                        .unwrap();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::C),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(
                            ControllerButton::Start,
                        ))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::C),
                    ..
                } => {
                    // we need a way to pipe to maple that something here is wrong.
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(
                            ControllerButton::Start,
                        ))
                        .unwrap();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Num1),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ToggleWireframe)
                        .unwrap();
                    hw_rasterizer.toggle_wireframe();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::D),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(
                            ControllerButton::Right,
                        ))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::D),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(
                            ControllerButton::Right,
                        ))
                        .unwrap();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::W),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(ControllerButton::Up))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::W),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(
                            ControllerButton::Up,
                        ))
                        .unwrap();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::S),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonPressed(
                            ControllerButton::Down,
                        ))
                        .unwrap();
                }
                Event::KeyUp {
                    keycode: Some(Keycode::S),
                    ..
                } => {
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::ButtonReleased(
                            ControllerButton::Down,
                        ))
                        .unwrap();
                }
                _ => {}
            }
        }

        if let Ok(resp) = frame_ready_receiver.try_recv() {
            match resp {
                EmulatorFrontendResponse::BlitFramebuffer(vram, width, height) => {
                    println!("fb rendering {}x{} vs 480", width, height);
                    hw_rasterizer.blit_fb(vram, width, 480);
                }
                EmulatorFrontendResponse::RenderHwRast(
                    dl_id,
                    texture_atlas,
                    vram,
                    pram,
                    lists,
                    bg_verts,
                ) => {
                    hw_rasterizer.render(dl_id, texture_atlas, vram, pram, bg_verts, lists);
                    frontend_request_sender
                        .send(EmulatorFrontendRequest::RenderingDone)
                        .unwrap();
                }
            }
        }
    }

    Ok(())
}
