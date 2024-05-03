use crate::scheduler::Scheduler;

pub struct Context<'b> {
  pub scheduler: &'b mut Scheduler
}