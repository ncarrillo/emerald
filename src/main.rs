#![feature(const_for)]
#![feature(const_trait_impl)]
#![feature(effects)]
#![feature(const_mut_refs)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(stmt_expr_attributes)]

use std::{ffi::c_void, io::stdout, mem};

use context::Context;
use emulator::Emulator;
use hw::{holly::g1::gdi::GdiParser, sh4::bus::CpuBus};
use sdl2::{
    event::Event, keyboard::Keycode, libc::memcpy, pixels::PixelFormatEnum, rect::Rect,
};

use crate::{
    hw::{extensions::BitManipulation, sh4::SH4EventData},
    scheduler::ScheduledEvent,
};

mod context;
mod emulator;
mod fifo;
mod hw;
mod scheduler;

fn draw_from_vram(bus: &mut CpuBus, context: &mut Context, buffer: &mut [u8]) {
    let width = ((bus.holly.registers.fb_display_size & 0x3FF) as usize) + 1;
    let height = (((bus.holly.registers.fb_display_size >> 10) & 0x3FF) as usize) + 1;
    let start_offset = bus.holly.registers.fb_display_addr1 as usize;
    let stride = (bus.holly.registers.fb_display_size & 0x3FF00000) >> 20;
    let depth = (bus.holly.registers.fb_r_ctrl & 0xc) >> 2;

    if depth != 0 {}

    unsafe {
        // pointer to the vram buffer
        let vram_ptr = (bus.holly.vram.as_mut_ptr() as *const u8).add(start_offset);
        // pointer to the sdl texture
        memcpy(
            buffer.as_mut_ptr() as *mut c_void,
            vram_ptr as *mut c_void,
            buffer.len(),
        );
        /*
        buffer.as_mut_ptr() as *mut u32;
        for y in 0..height {
            // pointer to the current line in the texture
            let mut line_ptr = texture_ptr.add(y * width);

            for _ in 0..width {
                let rgb = vram_ptr as *const u8;
                let r: u32 = *rgb.add(0) as u32;
                let g: u32 = *rgb.add(1) as u32;
                let b: u32 = *rgb.add(2) as u32;

                *line_ptr = (b << 16) | (g << 8) | (r) | 0xFF000000;

                vram_ptr = vram_ptr.add(1);
                line_ptr = line_ptr.add(1);
            }

            if stride > 0 {
             //   vram_ptr = vram_ptr.add((stride - 1) as usize);
            }
        }*/
    }
}

const NS_PER_SEC: i64 = 1_000_000_000;

fn nano_to_cycles(ns: i64, hz: i64) -> i64 {
    ((ns as f64 / NS_PER_SEC as f64) * hz as f64) as i64
}

fn cycles_to_nano(cycles: i64, hz: i64) -> i64 {
    ((cycles as f64 / hz as f64) * NS_PER_SEC as f64) as i64
}

fn hz_to_nano(hz: i64) -> i64 {
    (NS_PER_SEC as f64 / hz as f64) as i64
}

pub fn main() -> Result<(), String> {
    let gdi_image = GdiParser::load_from_file(
        "/Users/ncarrillo/Desktop/projects/emerald/roms/crazytaxi/ct.gdi",
    );
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let mut emulator = Emulator::new();
    let mut bus = CpuBus::new();

    {
        // initialize peripherals so they can schedule their initial events
        bus.holly.init(&mut emulator.scheduler);
        bus.holly.g1_bus.gd_rom.set_gdi(gdi_image);
    }

    let mut context = Context {
        scheduler: &mut emulator.scheduler
    };

    if true {
        //Emulator::load_ip(&mut emulator.cpu, &mut context, &mut bus);
         let syms = Emulator::load_elf(
            "/Users/ncarrillo/Desktop/projects/emerald/roms/pvr/example.elf",
            &mut emulator.cpu,
            &mut context,
            &mut bus,
        )
        .unwrap();
        emulator.cpu.symbols_map = syms;
    }

    //emulator.load_rom(&mut context);

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

        const CYCLES_PER_FRAME: u64 = 3333333;
        let mut cycles_so_far = 0_u64;

        let mut lock = stdout().lock();
        let mut time_slice = 448;
        while cycles_so_far < CYCLES_PER_FRAME {
            while time_slice > 0 {
                emulator
                    .cpu
                    .step(&mut bus, &mut context, total_cycles, &mut lock);
                bus.tmu.tick(total_cycles);

                if !emulator.cpu.is_delay_slot {
                    time_slice -= 8;
                }
            }

            // fixme: taken from reicast to try to align traces
            time_slice += 448;
            bus.holly.cyc = total_cycles;
            cycles_so_far += 448;
            total_cycles += 448;

            context.scheduler.add_cycles(448);

            // fixme: processing these pending commands should be moved into the corresponding peripheral
            if let Some(cmd) = bus.holly.g1_bus.gd_rom.pending_cmd {
                context.scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 60,
                    event_data: hw::holly::HollyEventData::GdromEvent(
                        hw::holly::g1::gdrom::GdromEventData::ProcessCommand(cmd),
                    ),
                });

                bus.holly.g1_bus.gd_rom.pending_cmd = None;
            }

            if bus.holly.g1_bus.gd_rom.pending_clear.get() {
                bus.holly.sb.registers.istnrm = bus.holly.sb.registers.istnrm.clear_bit(0);
                bus.holly.g1_bus.gd_rom.pending_clear.set(false);
            }

            if bus.holly.g1_bus.gd_rom.pending_data.len() == 12 {
                let command_data =
                    mem::replace(&mut bus.holly.g1_bus.gd_rom.pending_data, Vec::new());
                context.scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: hw::holly::HollyEventData::GdromEvent(
                        hw::holly::g1::gdrom::GdromEventData::ProcessSPICommand(command_data),
                    ),
                });
            }

            if let Some(istext) = bus.holly.g1_bus.gd_rom.pending_ack {
                bus.holly.g1_bus.gd_rom.pending_ack = None;
                context.scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: hw::holly::HollyEventData::LowerExternalInterrupt { istext },
                });
            }

            if bus.holly.sb.pending_recalc {
                bus.holly.sb.pending_recalc = false;
                context.scheduler.schedule(ScheduledEvent::HollyEvent {
                    deadline: 0,
                    event_data: hw::holly::HollyEventData::RecalculateInterrupts,
                });
            }

            if let Some(evt) = context.scheduler.tick() {
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
                            event_data,
                        );
                    }
                }
            }
        }

        canvas.clear();

        let width = ((bus.holly.registers.fb_display_size & 0x3FF) as usize) + 1;
        let height = (((bus.holly.registers.fb_display_size >> 10) & 0x3FF) as usize) + 1;

        let mut texture = tc
            .create_texture_streaming(PixelFormatEnum::ARGB1555, width as u32, height as u32)
            .map_err(|e| e.to_string())?;

        texture.with_lock(None, |buffer: &mut [u8], _: usize| {
            draw_from_vram(&mut bus, &mut context, buffer);
        })?;

        canvas.copy(
            &texture,
            None,
            Some(Rect::new(0, 0, width as u32, height as u32)),
        )?;
        canvas.present();
        canvas.window().gl_swap_window();
    }

    Ok(())
}