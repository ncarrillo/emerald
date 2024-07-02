pub mod bsc;
pub mod bus;
pub mod ccn;
pub mod cpg;
pub mod cpu;
pub mod decoder;
pub mod dmac;
pub mod fpu;
pub mod intc;
pub mod rtc;
pub mod tmu;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SH4EventData {
    RaiseIRL { irl_number: usize },
}

// fixme: would be nice to have a module ot hang sh4 components off of so we can get them out of bus.rs
// analogous to the holly module but for sh4 components instead
