use std::{
    cell::Cell,
    cmp::Ordering,
    collections::BinaryHeap,
};

use crate::hw::{holly::HollyEventData, sh4::SH4EventData};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ScheduledEvent {
    HollyEvent {
        deadline: u64,
        event_data: HollyEventData
    },
    SH4Event {
        deadline: u64,
        event_data: SH4EventData
    }
}

impl ScheduledEvent {
    fn deadline(&self) -> u64 {
        match *self {
            ScheduledEvent::HollyEvent { deadline, .. } => deadline,
            ScheduledEvent::SH4Event { deadline, .. } => deadline,
        }
    }

    fn with_updated_deadline(&self, new_deadline: u64) -> ScheduledEvent {
        match *self {
            ScheduledEvent::SH4Event {
                event_data: ref data,
                ..
            } => ScheduledEvent::SH4Event {
                deadline: new_deadline,
                event_data: data.clone(),
            },
            ScheduledEvent::HollyEvent {
                event_data: ref data,
                ..
            } => ScheduledEvent::HollyEvent {
                deadline: new_deadline,
                event_data: data.clone(),
            },
        }
    }
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other.deadline().cmp(&self.deadline())
    }
}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub struct Scheduler {
    events: BinaryHeap<ScheduledEvent>,
    timestamp: Cell<u64>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            events: BinaryHeap::new(),
            timestamp: Cell::new(0),
        }
    }

    pub fn now(&self) -> u64 {
        self.timestamp.get()
    }

    pub fn add_cycles(&self, cycles: u64) {
        self.timestamp.set(self.timestamp.get() + cycles)
    }

    pub fn schedule(&mut self, event: ScheduledEvent) {
        self.events
            .push(event.with_updated_deadline(self.timestamp.get() + event.deadline()))
    }

    pub fn tick(&mut self) -> Option<ScheduledEvent> {
        if let Some(event) = self.events.peek() {
            if event.deadline() <= self.timestamp.get() {
                let event = self.events.pop().unwrap();
                return Some(event);
            }
        }

        return None;
    }
}
