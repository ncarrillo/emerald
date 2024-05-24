use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct JsonTest {
    pub initial: CpuState,

    #[serde(alias = "final")]
    pub final_: CpuState,
    pub cycles: Vec<Cycle>,
    pub opcodes: Vec<u16>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CpuState {
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
    pub FPUL: u32,
}

#[derive(Deserialize, Copy, Clone, Debug)]
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
