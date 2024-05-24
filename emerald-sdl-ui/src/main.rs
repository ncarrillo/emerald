#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]

use emerald_core::{hw::sh4::cpu::Float32, json_tests::JsonTest, scheduler::Scheduler};

use std::{
    ffi::c_void,
    fs::File,
    io::BufReader,
    ops::DerefMut,
    sync::{mpsc, Arc, Mutex},
};

use emerald_core::{
    context::{CallStack, Context},
    emulator::{Breakpoint, Emulator, EmulatorState},
    hw::{
        extensions::BitManipulation,
        holly::g1::gdi::GdiParser,
        sh4::{bus::CpuBus, SH4EventData},
    },
    scheduler::ScheduledEvent,
};
use sdl2::{event::Event, keyboard::Keycode, libc::memcpy, pixels::PixelFormatEnum};

fn draw_from_vram(buffer: &mut [u8], vram: &mut [u8], start_offset: usize) {
    //println!("starting off is {:08x}", vram[0]);

    unsafe {
        // pointer to the vram buffer
        let vram_ptr = (vram.as_mut_ptr() as *const u8).add(start_offset as usize);
        // pointer to the sdl texture
        memcpy(
            buffer.as_mut_ptr() as *mut c_void,
            vram_ptr as *mut c_void,
            buffer.len(),
        );
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FramebufferFormat {
    Rgb555,
    Rgb565,
    Rgb888,  // (packed)
    ARgb888, // ??
}

pub fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let format = FramebufferFormat::Rgb555;

    let mut emulator = Arc::new(Mutex::new(Emulator::new()));
    let window = video_subsystem
        .window("", 640, 480)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;
    let tc = canvas.texture_creator();
    let mut atlas_texture = tc
        .create_texture_streaming(PixelFormatEnum::RGBA8888, 1024 as u32, 1024 as u32)
        .map_err(|e| e.to_string())?;
    let mut texture = tc
        .create_texture_streaming(PixelFormatEnum::RGB555, 640 as u32, 480 as u32)
        .map_err(|e| e.to_string())?;
    let mut current_depth = FramebufferFormat::Rgb555;
    let mut current_height = 640;
    let mut current_width = 480;

    #[cfg(feature = "json_tests")]
    {
        let mut emulator = Arc::new(Mutex::new(Emulator::new()));
        for entry in glob::glob("/Users/ncarrillo/Desktop/projects/sh4/*.json").unwrap() {
            match entry {
                Ok(path) => {
                    // Open the file in read-only mode with buffer.
                    let file = File::open(&path).expect("file not found");
                    let reader = BufReader::new(file);
                    println!("running {}..", path.to_str().unwrap());

                    let state: Vec<JsonTest> = serde_json::from_reader(reader).unwrap();

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
                        current_test: None,
                        breakpoints: vec![],
                    };

                    for test in state {
                        let mut emulator_guard = emulator.lock().unwrap();
                        let mut emulator = emulator_guard.deref_mut();

                        context.is_test_mode = true;
                        println!("\nrunning sub-test for opcode {:04x}", test.opcodes[1]);

                        emulator.cpu.registers.sr = test.initial.SR & 0x700083F3;
                        emulator.cpu.set_pr(test.initial.PR);
                        emulator.cpu.set_ssr(test.initial.SSR & 0x700083F3);
                        emulator.cpu.set_spc(test.initial.SPC);
                        emulator.cpu.set_gbr(test.initial.GBR);
                        emulator.cpu.set_vbr(test.initial.VBR);
                        emulator.cpu.set_dbr(test.initial.DBR);
                        emulator.cpu.set_fpscr(test.initial.FPSCR);
                        emulator.cpu.set_sgr(test.initial.SGR);
                        emulator.cpu.set_macl(test.initial.MACL);
                        emulator.cpu.set_mach(test.initial.MACH);
                        emulator.cpu.registers.current_pc = test.initial.PC;
                        emulator.cpu.set_fpul(Float32 {
                            u: test.initial.FPUL,
                        });

                        for i in 0..16 {
                            emulator.cpu.registers.r[i] = test.initial.R[i];
                        }

                        for i in 0..8 {
                            emulator.cpu.registers.r_bank[i] = test.initial.R_[i];
                        }

                        context.cyc = 0;
                        context.current_test = Some(test.clone());

                        let mut bus_lock = bus_arc.lock().unwrap();
                        while context.cyc <= 3 {
                            emulator.cpu.exec_in_test(&mut bus_lock, &mut context);
                        }

                        assert_eq!(
                            emulator.cpu.registers.current_pc, test.final_.PC,
                            "pc did not match emu had {:08x} but test had {:08x}",
                            emulator.cpu.registers.current_pc, test.final_.PC
                        );

                        for i in 0..16 {
                            let reg = emulator.cpu.registers.r[i];

                            if reg != test.final_.R[i] {
                                panic!(
                                    "r{} did not match emu had {:08x} but test had {:08x} vs our banked {:08x}",
                                    i, reg, test.final_.R[i], emulator.cpu.registers.r_bank[i & 0x7]
                                );
                            }
                        }

                        for i in 0..8 {
                            let reg_banked = emulator.cpu.registers.r_bank[i];
                            assert_eq!(
                                reg_banked, test.final_.R_[i],
                                "r_bank{} did not match {} {} vs {}",
                                i, reg_banked, test.final_.R_[i], emulator.cpu.registers.r[i]
                            );
                        }

                        if true {
                            if (emulator.cpu.registers.sr & 0x700083F3)
                                != (test.final_.SR & 0x700083F3)
                            {
                                panic!(
                                    "sr did not match emu had {:08x} but test had {:08x}",
                                    emulator.cpu.registers.sr, test.final_.SR
                                );
                            }
                        }

                        //println!("passed!")
                    }

                    println!("passed all tests for file {}!\n", path.to_str().unwrap());
                }
                Err(e) => println!("{:?}", e),
            }
        }

        return Ok(());
    }

    let (frame_ready_sender, frame_ready_receiver) = mpsc::channel();
    Emulator::run_loop(emulator.clone(), frame_ready_sender);

    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        if let Ok(mut frame) = frame_ready_receiver.try_recv() {
            canvas.clear();

            let width = 640;
            let height = 480;
            let depth = FramebufferFormat::Rgb565;

            if current_depth != depth || (width != current_width) || (height != current_height) {
                println!(
                    "emerald: WARNING framebuffer changed. {}x{} format: {:#?}",
                    width, height, depth
                );
                texture = tc
                    .create_texture_streaming(
                        match depth {
                            FramebufferFormat::Rgb555 => PixelFormatEnum::RGB555,
                            FramebufferFormat::Rgb565 => PixelFormatEnum::RGB565,
                            FramebufferFormat::Rgb888 => PixelFormatEnum::RGB888,
                            FramebufferFormat::ARgb888 => PixelFormatEnum::ABGR8888,
                        },
                        width as u32,
                        height as u32,
                    )
                    .map_err(|e| e.to_string())?;

                current_width = width;
                current_height = height;
                current_depth = depth;
            }

            texture.with_lock(None, |buffer: &mut [u8], _: usize| {
                let mut bus_lock = frame.bus.lock().unwrap();
                let bus = bus_lock.deref_mut();

                draw_from_vram(
                    buffer,
                    &mut bus.holly.pvr.vram[bus.holly.registers.fb_display_addr1 as usize..],
                    0 as usize,
                );
            })?;

            /*  atlas_texture.with_lock(None, |buffer: &mut [u8], _: usize| {
            unsafe {
                memcpy(
                    buffer.as_mut_ptr() as *mut c_void,
                    bus.holly.pvr.texture_atlas.data.as_ptr() as *const c_void,
                    1024 * 1024 * 4,
                )
            };
            })?;*/

            if false {
                //  let dest_rect = sdl2::rect::Rect::new(0, 0, 1024 * 1, 1024 * 1); // Scale by 14
                //    canvas.copy(&atlas_texture, None, Some(dest_rect));
            } else {
                //    let dest_rect = sdl2::rect::Rect::new(0, 0, 1024 * 1, 1024 * 1); // Scale by 14
                canvas.copy(&texture, None, None);
            }

            canvas.present();
        }
    }

    return Ok(());

    Ok(())
}
