#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct JsonTest {
    pub initial: Registers,

    #[serde(alias = "final")]
    pub final_: Registers,
    pub cycles: Vec<Cycle>,
    pub opcodes: Vec<u16>,
}

#[derive(Deserialize, Debug)]
pub struct Registers {
    pub R: Vec<u32>,
    pub R_: Vec<u32>,
    pub FP0: Vec<u32>,
    pub FP1: Vec<u32>,
    pub PC: u32,
    pub GBR: u32,
    pub SR: u32,
    pub SSR: u32,
    pub SPC: u32,
    pub VBR: u32,
    pub SGR: u32,
    pub DBR: u32,
    pub MACL: u32,
    pub MACH: u32,
    pub PR: u32,
    pub FPSCR: u32,
}

#[derive(Deserialize, Debug)]
pub struct Cycle {
    pub actions: u8,

    #[serde(default)]
    pub fetch_addr: u64,

    #[serde(default)]
    pub fetch_val: u64,

    #[serde(default)]
    pub write_addr: u64,

    #[serde(default)]
    pub write_val: u64,

    #[serde(default)]
    pub read_addr: u64,

    #[serde(default)]
    pub read_val: u64,
}

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
    let (frame_ready_sender, frame_ready_receiver) = mpsc::channel();

    Emulator::run_loop(emulator, frame_ready_sender);

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

    /*
    let emulator = Emulator::new();
    let mut bus = CpuBus::new();
    let gdi_image = GdiParser::load_from_file(
        "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/crazytaxi/ct.gdi",
        );

    {
        // initialize peripherals so they can schedule their initial events
        bus.holly.init(&mut emulator.scheduler);
        bus.holly.g1_bus.gd_rom.set_gdi(gdi_image);
        }

    let mut context = Context {
        scheduler: &mut emulator.scheduler,
        cyc: 0,
        tracing: false,
        inside_int: false,
        entered_main: false,
        is_test_mode: false,
        tripped_breakpoint: None,
        callstack: CallStack::new(),
        test_base: None,
        test_opcodes: vec![],
        breakpoints: vec![Breakpoint::MemoryBreakpoint {
            addr: 0x8c184000,
            read: false,
            write: false,
            fetch: true,
        }],
        };
        */

    #[cfg(feature = "json_tests")]
    {
        context.is_test_mode = true;
        for entry in glob::glob("/Users/ncarrillo/Desktop/projects/sh4/*.json").unwrap() {
            match entry {
                Ok(path) => {
                    // Open the file in read-only mode with buffer.
                    let file = File::open(&path).expect("file not found");
                    let reader = BufReader::new(file);
                    println!("running {}..", path.to_str().unwrap());

                    let state: Vec<JsonTest> = serde_json::from_reader(reader).unwrap();

                    for test in state {
                        println!("\nrunning sub-test for opcode {:04x}", test.opcodes[1]);

                        emulator.cpu.set_sr(test.initial.SR); //.set_sr(test.initial.SR);
                        emulator.cpu.set_pr(test.initial.PR);
                        emulator.cpu.set_ssr(test.initial.SSR);
                        emulator.cpu.set_spc(test.initial.SPC);
                        emulator.cpu.set_gbr(test.initial.GBR);
                        emulator.cpu.set_vbr(test.initial.VBR);
                        emulator.cpu.set_dbr(test.initial.DBR);
                        emulator.cpu.set_fpscr(test.initial.FPSCR);
                        emulator.cpu.set_sgr(test.initial.SGR);
                        emulator.cpu.set_macl(test.initial.MACL);
                        emulator.cpu.set_mach(test.initial.MACH);
                        emulator.cpu.registers.current_pc = test.initial.PC;

                        for i in 0..16 {
                            emulator.cpu.set_register_by_index(i, test.initial.R[i]);
                        }

                        for i in 0..8 {
                            emulator
                                .cpu
                                .set_banked_register_by_index(i, test.initial.R_[i]);
                        }

                        context.cyc = 0;
                        context.test_base = Some(emulator.cpu.registers.current_pc);
                        context.test_opcodes = test.opcodes.clone();

                        while context.cyc <= 3 {
                            emulator.cpu.exec_in_test(&mut bus, &mut context);
                        }

                        assert_eq!(
                            emulator.cpu.registers.current_pc, test.final_.PC,
                            "pc did not match emu had {:08x} but test had {:08x}",
                            emulator.cpu.registers.current_pc, test.final_.PC
                        );

                        if false {
                            if emulator.cpu.get_sr() != test.final_.SR {
                                panic!(
                                    "sr did not match emu had {:08x} but test had {:08x}",
                                    emulator.cpu.get_sr(),
                                    test.final_.SR
                                );
                            }
                        }

                        for i in 0..16 {
                            let reg = emulator.cpu.get_register_by_index(i);

                            if reg != test.final_.R[i] {
                                println!(
                                    "r{} did not match emu had {} but test had {}",
                                    i, reg, test.final_.R[i]
                                );
                            }
                        }

                        for i in 0..8 {
                            let reg_banked = emulator.cpu.get_banked_register_by_index(i);
                            assert_eq!(
                                reg_banked, test.final_.R_[i],
                                "r_bank{} did not match {} {}",
                                i, reg_banked, test.final_.R_[i]
                            );
                        }

                        println!("passed!")
                    }

                    println!("passed all tests for file {}!\n", path.to_str().unwrap());
                }
                Err(e) => println!("{:?}", e),
            }
        }

        return Ok(());
    }

    /*
    if false {
        //Emulator::load_ip(&mut emulator.cpu, &mut context, &mut bus);
        let syms = Emulator::load_elf(
            "/Users/ncarrillo/Desktop/projects/emerald/emerald-core/roms/pvr/tetris.elf",
            &mut emulator.cpu,
            &mut context,
            &mut bus,
        )
        .unwrap();
        emulator.cpu.symbols_map = syms;
    }

    if false {
        Emulator::_load_rom(&mut emulator.cpu, &mut context, &mut bus);
    }

    let window = video_subsystem
        .window("", 640, 480)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let mut event_pump = sdl_context.event_pump()?;
    let tc = canvas.texture_creator();
    let mut total_cycles = 0_u64;

    let mut atlas_texture = tc
        .create_texture_streaming(PixelFormatEnum::RGBA8888, 1024 as u32, 1024 as u32)
        .map_err(|e| e.to_string())?;

    let mut texture = tc
        .create_texture_streaming(PixelFormatEnum::RGB555, 640 as u32, 480 as u32)
        .map_err(|e| e.to_string())?;

    let mut current_depth = FramebufferFormat::Rgb555;
    let mut current_height = 640;
    let mut current_width = 480;
    let mut showing_atlas = false;

    'running: loop {
        const CYCLES_PER_FRAME: u64 = 3333333;
        const TIMESLICE: u64 = 448;
        const CPU_RATIO: u64 = 8;

        let mut cycles_so_far = 0_u64;
        let mut time_slice = TIMESLICE;

        // fixme: ??
        while cycles_so_far <= CYCLES_PER_FRAME {
            while time_slice > 0 && emulator.state == EmulatorState::Running {
                emulator.cpu.step(&mut bus, &mut context, total_cycles);

                if context.tripped_breakpoint.is_some() {
                    println!("\nbreakpoint tripped!");
                    println!("");

                    unsafe {
                        println!("\t{:08x} {:04x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}",
                        emulator.cpu.registers.current_pc, emulator.cpu.current_opcode,
                        emulator.cpu.get_register_by_index(0),
                        emulator.cpu.get_register_by_index(1),
                        emulator.cpu.get_register_by_index(2), emulator.cpu.get_register_by_index(3),
                        emulator.cpu.get_register_by_index(4), emulator.cpu.get_register_by_index(5),
                        emulator.cpu.get_register_by_index(6), emulator.cpu.get_register_by_index(7),
                        emulator.cpu.get_register_by_index(8), emulator.cpu.get_register_by_index(9),
                        emulator.cpu.get_register_by_index(10), emulator.cpu.get_register_by_index(11),
                        emulator.cpu.get_register_by_index(12), emulator.cpu.get_register_by_index(13),
                        emulator.cpu.get_register_by_index(14), emulator.cpu.get_register_by_index(15),
                        emulator.cpu.get_sr(), emulator.cpu.get_fpscr())
                    };

                    println!("");
                    println!("callstack:");
                    println!("\t{:#?}", context.callstack);

                    println!("scheduler:");
                    for event in &context.scheduler.events {
                        println!(
                            "\t{} coming up in {} cycles",
                            event.data_str(),
                            event.deadline() - context.scheduler.now()
                        );
                    }

                    println!("");

                    println!("press C to continue.");

                    emulator.state = EmulatorState::BreakpointTripped;
                    break;
                }

                time_slice -= CPU_RATIO;

                if bus.holly.g1_bus.gd_rom.output_fifo.borrow().is_empty() {
                    let pending_state = bus.holly.g1_bus.gd_rom.pending_state;

                    // needed bc this transitions during a mutable read ....
                    if let Some(pending_state) = bus.holly.g1_bus.gd_rom.pending_state {
                        bus.holly
                            .g1_bus
                            .gd_rom
                            .transition(context.scheduler, pending_state);
                        bus.holly.g1_bus.gd_rom.pending_state = None;
                        bus.holly
                            .g1_bus
                            .gd_rom
                            .registers
                            .status
                            .set(bus.holly.g1_bus.gd_rom.registers.status.get().clear_bit(3));
                    }
                }
            }

            if emulator.state == EmulatorState::Running {
                emulator
                    .cpu
                    .process_interrupts(&mut bus, &mut context, total_cycles);

                time_slice += TIMESLICE;
                cycles_so_far += TIMESLICE;
                total_cycles += TIMESLICE;

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
                            bus.holly.on_scheduled_event(
                                context.scheduler,
                                &mut bus.dmac,
                                &mut bus.system_ram,
                                event_data.clone(),
                            );
                        }
                    }
                }
            } else {
                break;
            }
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {
                    keycode: Some(Keycode::X),
                    ..
                } => {
                    showing_atlas = !showing_atlas;
                    println!("showing texture cache: {}", showing_atlas);
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } => {
                    if emulator.state != EmulatorState::Paused {
                        println!("emerald: pausing execution");
                        emulator.state = EmulatorState::Paused;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::C),
                    ..
                } => {
                    let mut s = String::new();

                    println!("emerald: resuming execution");
                    context.tripped_breakpoint = None;
                    emulator.state = EmulatorState::Running;
                }
                _ => {}
            }
        }

        if emulator.state == EmulatorState::Running {
            canvas.clear();

            let width = ((bus.holly.registers.fb_r_size & 0x3FF) as usize) + 1;
            let height = (((bus.holly.registers.fb_r_size >> 10) & 0x3FF) as usize) + 1;
            let depth = match bus.holly.pvr.depth {
                0x00 => FramebufferFormat::Rgb555,
                0x01 => FramebufferFormat::Rgb565,
                0x02 => FramebufferFormat::Rgb888,
                0x03 => FramebufferFormat::ARgb888,
                _ => unreachable!(),
            };

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

            //println!("{}", width);

            texture.with_lock(None, |buffer: &mut [u8], _: usize| {
                draw_from_vram(&mut bus, &mut context, buffer);
            })?;

            atlas_texture.with_lock(None, |buffer: &mut [u8], _: usize| {
                unsafe {
                    memcpy(
                        buffer.as_mut_ptr() as *mut c_void,
                        bus.holly.pvr.texture_atlas.data.as_ptr() as *const c_void,
                        1024 * 1024 * 4,
                    )
                };
            })?;

            canvas.clear();

            if showing_atlas {
                let dest_rect = sdl2::rect::Rect::new(0, 0, 1024 * 1, 1024 * 1); // Scale by 14
                canvas.copy(&atlas_texture, None, Some(dest_rect));
            } else {
                let dest_rect = sdl2::rect::Rect::new(0, 0, 1024 * 1, 1024 * 1); // Scale by 14
                canvas.copy(&texture, None, None);
            }

            canvas.present();
            //panic!("");
        }
        }*/

    Ok(())
}
