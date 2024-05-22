use core::fmt;

use crate::{emulator::Breakpoint, scheduler::Scheduler};

pub struct Context<'b> {
    pub scheduler: &'b mut Scheduler,
    pub cyc: u64,
    pub tracing: bool,
    pub inside_int: bool,
    pub entered_main: bool,
    pub tripped_breakpoint: Option<Breakpoint>,
    pub breakpoints: Vec<Breakpoint>,

    pub callstack: CallStack,
    pub is_test_mode: bool,
    pub test_base: Option<u32>,
    pub test_opcodes: Vec<u16>,
}

#[derive(Default)]
pub struct CallStack {
    stack: Vec<String>,
}

impl fmt::Debug for CallStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "");
        for (i, addr) in self.stack.iter().enumerate().rev() {
            writeln!(f, "    at {}", addr)?;
        }
        writeln!(f, "")
    }
}

impl CallStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, addr: String) {
        self.stack.push(addr);
    }

    pub fn pop(&mut self) -> Option<String> {
        self.stack.pop()
    }

    pub fn peek(&self) -> Option<&String> {
        self.stack.last()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }
}
