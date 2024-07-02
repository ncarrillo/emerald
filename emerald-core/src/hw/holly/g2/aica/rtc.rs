use std::time::{SystemTime, UNIX_EPOCH};

use crate::hw::holly::HollyEventData;

pub struct RtcRegisters {}
pub struct Rtc {
    pub timestamp: u32,
}

impl Rtc {
    pub fn new() -> Self {
        Self {
            timestamp: Self::get_rtc_now(),
        }
    }

    fn get_rtc_now() -> u32 {
        let dreamcast_epoch =
            UNIX_EPOCH - std::time::Duration::from_secs(20 * 365 * 24 * 60 * 60 + 5 * 24 * 60 * 60);
        let now = SystemTime::now();

        println!(
            "returning {:08x} for rtc",
            now.duration_since(dreamcast_epoch).unwrap().as_secs() as u32
        );
        now.duration_since(dreamcast_epoch).unwrap().as_secs() as u32
    }

    pub fn init(&mut self, scheduler: &mut crate::scheduler::Scheduler) {
        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: 200 * 1000 * 1000,
            event_data: HollyEventData::Rtc,
        });
    }

    pub fn on_scheduled_event(&mut self, scheduler: &mut crate::scheduler::Scheduler) {
        self.timestamp = self.timestamp.wrapping_add(1);

        scheduler.schedule(crate::scheduler::ScheduledEvent::HollyEvent {
            deadline: 200 * 1000 * 1000,
            event_data: HollyEventData::Rtc,
        })
    }
}
