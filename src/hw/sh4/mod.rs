pub mod cpu;
pub mod ccn;
pub mod bsc;
pub mod tmu;
pub mod rtc;
pub mod cpg;
pub mod dmac;
pub mod bus;
pub mod intc;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SH4EventData {
  RaiseIRL { irl_number: usize}
}

// fixme: would be nice to have a module ot hang sh4 components off of so we can get them out of bus.rs
// analogous to the holly module but for sh4 components instead