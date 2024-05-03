use std::collections::HashMap;
use super::bus::PhysicalAddress;

#[derive(Copy, Clone, Debug, Default)]
pub struct IntcRegisters {
    pub icr: u16, // interrupt control register (??)
    pub ipra: u16,
    pub iprb: u16,
    pub iprc: u16,

    pub irl: usize
}

pub enum InterruptKind {
    // nmi + external interrupt lines
    NMI,
    IRL0,
    IRL1,
    IRL2,
    IRL3,
    IRL4,
    IRL5,
    IRL6,
    IRL7,
    IRL8,
    IRL9,
    IRL10,
    IRL11,
    IRL12,
    IRL13,
    IRL14,
    
    // on-chip interrupts (configurable)
    HitachiUDI,
    GPIO,
    DMTE0,
    DMTE1,
    DMTE2,
    DMTE3,
    DMAE,
    TUNI0,
    TUNI1,
    TUNI2,
    TICPI2,
    ATI,
    PRI,
    CUI,
    SCI1_ERI,
    SCI1_RXI,
    SCI1_TXI,
    SCI1_TEI,
    SCIF_ERI,
    SCIF_RXI,
    SCIF_TXI,
    SCIF_TEI,
    ITI,
    RCMI,
    ROVI,
}

#[derive(Clone, Debug)]
pub struct Intc {
    pub registers: IntcRegisters,
    pub interrupt_levels: [usize; 41],
    pub prio_int_map: HashMap<usize, usize>
}

impl Intc {
    pub fn new() -> Self {
        Self {
            registers: Default::default(),
            prio_int_map: HashMap::new(),
            interrupt_levels: [
                16, // NMI
                15, // IRL0
                14, // IRL1
                13, // IRL2
                12, // IRL3
                11, // IRL4
                10, // IRL5
                9,  // IRL6
                8,  // IRL7
                7,  // IRL8
                6,  // IRL9
                5,  // IRL10
                4,  // IRL11
                3,  // IRL12
                2,  // IRL13
                1,  // IRL14

                // Configurable priorities

                0, // Hitachi
                0, // GPIO
                0, // DMTE0
                0, // DMTE1
                0, // DMTE2
                0, // DMTE3
                0, // DMAE
                0, // TUNI0
                0, // TUNI1
                0, // TUNI2
                0, // TICPI2
                0, // ATI
                0, // PRI
                0, // CUI
                0, // SCI1_ERI
                0, // SCI1_RXI
                0, // SCI1_TXI
                0, // SCI1_TEI
                0, // SCIF_ERI
                0, // SCIF_RXI
                0, // SCIF_TXI
                0, // SCIF_TEI
                0, // ITI
                0, // RCMI
                0, // ROVI
            ],
        }
    }

    pub fn raise_irl(&mut self, irl: usize) {
        self.registers.irl = irl;
    }

    // fixme: recalculate priorities
    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            0x1fd00000 => self.registers.icr = value,
            0x1fd00004 => self.registers.ipra = value,
            0x1fd00008 => self.registers.iprb = value,
            0x1fd0000c => self.registers.iprc = value,
            _ => println!(
                "intc: unknown mmio write (16-bit) @ 0x{:08x} with value 0x{:08x}",
                addr.0, value
            ),
        }
    }

    pub fn read_16(&self, addr: PhysicalAddress) -> u16 {
        match addr.0 {
            0x1fd00000 => self.registers.icr,
            0x1fd00004 => self.registers.ipra,
            0x1fd00008 => self.registers.iprb,
            0x1fd0000c => self.registers.iprc,
            _ => panic!("intc: unknown mmio read (16-bit) @ 0x{:08x}", addr.0),
        }
    }
}
