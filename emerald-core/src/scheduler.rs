use crate::hw::{holly::HollyEventData, sh4::SH4EventData};
use std::cmp::Ordering;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ScheduledEvent {
    HollyEvent {
        deadline: u64, // cycle in which the event will be consumed (not including overrun)
        event_data: HollyEventData,
    },
    SH4Event {
        deadline: u64, // cycle in which the event will be consumed (not including overrun)
        //  start: u64,    // cycle in which the event was scheduled
        event_data: SH4EventData,
    },
}

impl ScheduledEvent {
    pub fn deadline(&self) -> u64 {
        match *self {
            ScheduledEvent::HollyEvent { deadline, .. } => deadline,
            ScheduledEvent::SH4Event { deadline, .. } => deadline,
        }
    }

    pub fn data_str(&self) -> String {
        match *self {
            ScheduledEvent::HollyEvent { ref event_data, .. } => format!("{:?}", event_data),
            ScheduledEvent::SH4Event { ref event_data, .. } => format!("{:?}", event_data),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerEntry {
    pub start: u64,
    pub event: ScheduledEvent,
}

impl Ord for SchedulerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.event.deadline().cmp(&self.event.deadline())
    }
}

impl PartialOrd for SchedulerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

#[derive(Debug)]
pub struct Scheduler {
    pub events: Vec<SchedulerEntry>,
    timestamp: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            timestamp: 0,
        }
    }

    pub fn now(&self) -> u64 {
        self.timestamp
    }

    pub fn add_cycles(&mut self, cycles: u64) {
        self.timestamp += cycles;
    }

    pub fn schedule(&mut self, event: ScheduledEvent) {
        let new_deadline = self.timestamp + event.deadline();
        let new_event = event.with_updated_deadline(new_deadline);

        // Check for duplicates
        if !self
            .events
            .iter()
            .any(|e| e.event.deadline() == new_event.deadline() && e.event == new_event)
        {
            self.events.push(SchedulerEntry {
                start: self.now(),
                event: new_event,
            });
            self.events.sort_by(|a, b| b.cmp(&a));
        }
    }

    pub fn tick(&mut self) -> Option<SchedulerEntry> {
        if let Some(entry) = self.events.first() {
            if entry.event.deadline() <= self.timestamp {
                let entry = self.events.remove(0);
                return Some(entry);
            }
        }

        None
    }
}
