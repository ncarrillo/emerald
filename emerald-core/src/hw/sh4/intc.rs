use super::bus::PhysicalAddress;

#[derive(Copy, Clone, Debug, Default)]
pub struct IntcRegisters {
    pub icr: u16, // interrupt control register (??)
    pub ipra: u16,
    pub iprb: u16,
    pub iprc: u16,
    pub interrupt_requests: u64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
    pub interrupt_levels: [u8; 41],
    pub prioritized_interrupts: [InterruptKind; 41],
    pub interrupt_map: [u8; 41],
}

impl Intc {
    pub fn new() -> Self {
        let mut intc = Intc {
            registers: Default::default(),
            prioritized_interrupts: [
                InterruptKind::NMI,
                InterruptKind::IRL0,
                InterruptKind::IRL1,
                InterruptKind::IRL2,
                InterruptKind::IRL3,
                InterruptKind::IRL4,
                InterruptKind::IRL5,
                InterruptKind::IRL6,
                InterruptKind::IRL7,
                InterruptKind::IRL8,
                InterruptKind::IRL9,
                InterruptKind::IRL10,
                InterruptKind::IRL11,
                InterruptKind::IRL12,
                InterruptKind::IRL13,
                InterruptKind::IRL14,
                InterruptKind::HitachiUDI,
                InterruptKind::GPIO,
                InterruptKind::DMTE0,
                InterruptKind::DMTE1,
                InterruptKind::DMTE2,
                InterruptKind::DMTE3,
                InterruptKind::DMAE,
                InterruptKind::TUNI0,
                InterruptKind::TUNI1,
                InterruptKind::TUNI2,
                InterruptKind::TICPI2,
                InterruptKind::ATI,
                InterruptKind::PRI,
                InterruptKind::CUI,
                InterruptKind::SCI1_ERI,
                InterruptKind::SCI1_RXI,
                InterruptKind::SCI1_TXI,
                InterruptKind::SCI1_TEI,
                InterruptKind::SCIF_ERI,
                InterruptKind::SCIF_RXI,
                InterruptKind::SCIF_TXI,
                InterruptKind::SCIF_TEI,
                InterruptKind::ITI,
                InterruptKind::RCMI,
                InterruptKind::ROVI,
            ],
            interrupt_map: [0; 41],
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
        };

        intc.recalc_prio();
        intc
    }

    pub fn raise_irl(&mut self, irl: usize) {
        self.registers.interrupt_requests |= 1_u64 << self.interrupt_map[irl];
    }

    pub fn recalc_prio(&mut self) {
        //  println!("pre-prio: {:#?}", self.prioritized_interrupts);

        let len = self.prioritized_interrupts.len();
        let mut saved_requests: u64 = 0;
        if self.registers.interrupt_requests != 0 {
            for i in 0..len {
                if (self.registers.interrupt_requests >> i) & 1 == 1 {
                    saved_requests |= 1_u64 << self.prioritized_interrupts[i] as isize;
                }
            }
        }

        let IPRA = self.registers.ipra;
        let IPRC = self.registers.iprc;

        self.interrupt_levels[InterruptKind::TUNI0 as isize as usize] =
            ((IPRA & 0xf000) >> 12) as u8;
        self.interrupt_levels[InterruptKind::TUNI1 as isize as usize] = ((IPRA & 0xf00) >> 8) as u8;
        self.interrupt_levels[InterruptKind::TUNI2 as isize as usize] = ((IPRA & 0xf0) >> 4) as u8;
        self.interrupt_levels[InterruptKind::TICPI2 as isize as usize] = ((IPRA & 0xf0) >> 4) as u8;
        self.interrupt_levels[InterruptKind::ATI as isize as usize] = (IPRA & 0xf) as u8;
        self.interrupt_levels[InterruptKind::PRI as isize as usize] = (IPRA & 0xf) as u8;
        self.interrupt_levels[InterruptKind::CUI as isize as usize] = (IPRA & 0xf) as u8;

        self.prioritized_interrupts.sort_by(|lhs, rhs| {
            if self.interrupt_levels[*lhs as usize] == self.interrupt_levels[*rhs as usize] {
                (*lhs as isize).cmp(&(*rhs as isize))
            } else {
                self.interrupt_levels[*lhs as usize]
                    .cmp(&self.interrupt_levels[*rhs as usize])
                    .reverse()
            }
        });

        for i in 0..len {
            self.interrupt_map[self.prioritized_interrupts[i] as usize] = i as u8;
        }

        if saved_requests != 0 {
            self.registers.interrupt_requests = 0;
            let interrupt_map_len = self.interrupt_map.len();

            for i in 0..interrupt_map_len {
                if (saved_requests >> i) & 1 == 1 {
                    self.registers.interrupt_requests |= 1_u64 << self.interrupt_map[i];
                }
            }
        }
    }

    // fixme: recalculate priorities
    pub fn write_16(&mut self, addr: PhysicalAddress, value: u16) {
        match addr.0 {
            // unknown, but probably not important
            0x1fd00002 | 0x1fd00006 | 0x1fd0000a | 0x1fd0000e => {}

            0x1fd00000 => {
                self.registers.icr = value;
            }
            0x1fd00004 => {
                if self.registers.ipra != value {
                    self.registers.ipra = value;
                    self.recalc_prio();
                }
            }
            0x1fd00008 => {
                if self.registers.iprb != value {
                    self.registers.iprb = value;
                    self.recalc_prio();
                }
            }
            0x1fd0000c => {
                if self.registers.iprc != value {
                    self.registers.iprc = value;
                    self.recalc_prio();
                }
            }
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
