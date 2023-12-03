use crate::Scheduler;

pub struct PriorityQueue;

impl Scheduler for PriorityQueue {
    fn next(&mut self) -> crate::SchedulingDecision {
        unimplemented!()
    }

    fn stop(&mut self, _reason: crate::StopReason) -> crate::SyscallResult {
        unimplemented!()
    }

    fn list(&mut self) -> Vec<&dyn crate::Process> {
        unimplemented!()
    }
}
