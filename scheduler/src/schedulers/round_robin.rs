use crate::{Scheduler, Process};

use std::num::NonZeroUsize;
use crate::Pid;
use crate::ProcessState;
use crate::Syscall;

use std::collections::VecDeque;


pub struct ProcessInfo {
    /// The PID of the process.
    pub pid: Pid,

    /// The process state.
    pub state: ProcessState,

    /// The process timings (total time, system call time, running time).
    pub timings: (usize, usize, usize),

    /// The process priority
    pub priority: i8,

    /// Extra details about the process
    pub extra: String,
    pub remaining_slices: usize,
    pub sleep_time: usize,
}

impl ProcessInfo {
    fn new(pid: Pid, priority: i8, timeslice: usize) -> Self {
        Self {
            pid,
            state: ProcessState::Ready,
            timings: (0, 0, 0),
            priority,
            extra: String::new(),
            remaining_slices: timeslice,
            sleep_time: 0,
        }
    }
}

impl Process for ProcessInfo {
    fn pid(&self) -> Pid {
        self.pid
    }

    fn state(&self) -> ProcessState {
        self.state
    }

    fn timings(&self) -> (usize, usize, usize) {
        self.timings
    }

    fn priority(&self) -> i8 {
        self.priority
    }

    fn extra(&self) -> String {
        self.extra.clone()
    }
}

impl Clone for ProcessInfo {
    fn clone(&self) -> Self {
        Self {
            pid: self.pid.clone(),
            state: self.state.clone(),
            timings: self.timings.clone(),
            priority: self.priority,
            extra: self.extra.clone(),
            remaining_slices: self.remaining_slices,
            sleep_time: self.sleep_time,
        }
    }
}

pub struct RoundRobin {
    pub timeslice: usize,
    pub minimum_remaining_timeslice: usize,
    pub last_pid: usize,
    pub running_process: Option<ProcessInfo>,
    pub ready_process_queue: VecDeque<ProcessInfo>,
    pub waiting_process_queue: VecDeque<ProcessInfo>,
    pub time_jump: usize
}

impl RoundRobin {
    pub fn new(timeslice: NonZeroUsize, minimum_remaining_timeslice: usize) -> Self {
        Self {
            timeslice: timeslice.get(),
            minimum_remaining_timeslice,
            last_pid: 0,
            running_process: None,
            ready_process_queue: VecDeque::new(),
            waiting_process_queue: VecDeque::new(),
            time_jump: 0
        }
    }

    pub fn all_waiting(&self) -> bool {
        for process in &self.waiting_process_queue {
            match process.state {
                ProcessState::Waiting { event } => {
                    if let None = event {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }

    pub fn minimum_sleeping_duration(&self) -> usize {
        let mut min = 0;
        for process in &self.waiting_process_queue {
            if min == 0 && process.sleep_time != 0 {
                min = process.sleep_time;
            } else if min != 0 && process.sleep_time < min {
                min = process.sleep_time;
            }
        }
        min
    }

    pub fn time_master(&mut self, beginning: usize, end: usize) {
        if let Some(running_process) = &mut self.running_process {
            running_process.timings.0 = running_process.timings.0 + beginning - end;
        }

        for ready_process in &mut self.ready_process_queue {
            ready_process.timings.0 = ready_process.timings.0 + beginning - end;
        }

        let mut index = 0;
        let mut index_vec = VecDeque::<usize>::new();

        for waiting_process in &mut self.waiting_process_queue {
            waiting_process.timings.0 = waiting_process.timings.0 + beginning - end;
            if waiting_process.sleep_time > 0 {
                if waiting_process.sleep_time - (beginning - end) > 0 {
                    waiting_process.sleep_time = waiting_process.sleep_time - (beginning - end);
                } else {
                    waiting_process.sleep_time = 0;
                    waiting_process.state = ProcessState::Ready;
                    self.ready_process_queue.push_back(waiting_process.clone());
                    index_vec.push_back(index);
                }
            }
            index += 1;
        }

        for index in index_vec {
            self.waiting_process_queue.remove(index);
        }
    }
}

impl Scheduler for RoundRobin {
    fn next(&mut self) -> crate::SchedulingDecision {
        if self.time_jump != 0 {
            self.time_master(self.time_jump, 0);
            self.time_jump = 0;
        }

        if let Some(process) = &mut self.running_process {
            let timeslice = self.timeslice;
            if process.remaining_slices < self.minimum_remaining_timeslice { //time?
                process.remaining_slices = timeslice;

                process.state = ProcessState::Ready;
                let cloned_process = process.clone();

                self.ready_process_queue.push_back(cloned_process);
                self.running_process = None;

            } else {
                return crate::SchedulingDecision::Run { pid: process.pid(), timeslice: NonZeroUsize::new(timeslice).unwrap() };
            }
        }

        let first_element = self.ready_process_queue.pop_front();
        self.running_process = first_element;
        match &mut self.running_process {
            Some(first_element) => {
                first_element.state = ProcessState::Running;
                crate::SchedulingDecision::Run { pid: first_element.pid(), timeslice: NonZeroUsize::new(first_element.remaining_slices).unwrap() }
            },
            None => {
                if self.all_waiting() {
                    return crate::SchedulingDecision::Deadlock;
                }

                self.time_jump = self.minimum_sleeping_duration();
                if self.time_jump != 0 {
                    return crate::SchedulingDecision::Sleep(NonZeroUsize::new(self.time_jump).unwrap());
                }
                
                if self.ready_process_queue.is_empty() && self.waiting_process_queue.is_empty() {
                    return crate::SchedulingDecision::Done;
                }
                
                crate::SchedulingDecision::Panic
            }
        }
    }

    fn stop(&mut self, _reason: crate::StopReason) -> crate::SyscallResult {
        match _reason {
            crate::StopReason::Expired => {

                if let Some(process) = &mut self.running_process {
                    let beginning = process.remaining_slices;
                    self.time_master(beginning, 0);
                }
                
                if let Some(process) = &mut self.running_process {
                    process.timings.2 = process.timings.2 + process.remaining_slices;
                    process.remaining_slices = self.timeslice;

                    process.state = ProcessState::Ready;
                    self.ready_process_queue.push_back(process.clone());
                    self.running_process = None;

                    return crate::SyscallResult::Success;
                }

                crate::SyscallResult::NoRunningProcess
            }
            crate::StopReason::Syscall { syscall, remaining } => {

                if let Some(process) = &mut self.running_process {
                    let beginning = process.remaining_slices;
                    self.time_master(beginning, remaining);
                    
                }

                if let Some(process) = &mut self.running_process {
                    process.timings.1 += 1;
                    process.timings.2 = process.timings.2 + process.remaining_slices - remaining - 1;
                    process.remaining_slices = remaining;
                }

                match syscall {
                    Syscall::Fork(priority) => {
                        if self.running_process.is_none() && self.last_pid != 0 {
                            return crate::SyscallResult::NoRunningProcess;
                        }
                        
                        self.last_pid += 1;
                        let pid = Pid::new(self.last_pid);

                        self.ready_process_queue.push_back(ProcessInfo::new(pid, priority, self.timeslice));

                        crate::SyscallResult::Pid(pid)
                    }
                    Syscall::Sleep(time) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }
                        
                        if let Some(process) = &mut self.running_process {
                            process.sleep_time = time;
                            process.state = ProcessState::Waiting { event: None };
                            self.waiting_process_queue.push_back(process.clone());
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                    Syscall::Wait(event_number) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }

                        if let Some(process) = &mut self.running_process {
                            process.state = ProcessState::Waiting { event: Some(event_number) };
                            self.waiting_process_queue.push_back(process.clone());
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                    Syscall::Signal(event_number) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }    

                        let mut index = 0;
                        let mut index_vec = VecDeque::<usize>::new();

                        for process in &mut self.waiting_process_queue {
                            match process.state {
                                ProcessState::Waiting { event} => {
                                    if let Some(event) = event {
                                        if event == event_number {
                                            process.state = ProcessState::Ready;
                                            self.ready_process_queue.push_back(process.clone());
                                            index_vec.push_back(index);
                                        }
                                    }
                                }
                                _ => {}
                            }
                            index += 1;
                        }

                        for index in index_vec {
                            self.waiting_process_queue.remove(index);
                        }

                        crate::SyscallResult::Success
                    }
                    Syscall::Exit => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                }
            }
        }
    }

    fn list(&mut self) -> Vec<&dyn crate::Process> {
        let mut all_processes: VecDeque<&dyn crate::Process> = VecDeque::new();

        if let Some(running_process) = &self.running_process {
            all_processes.push_back(running_process);
        }

        for process in &self.ready_process_queue {
            all_processes.push_back(process);
        }

        for process in &self.waiting_process_queue {
            all_processes.push_back(process);
        }

        // Convert the VecDeque to Vec<&dyn Process>
        let vec_of_trait_objects: Vec<&dyn crate::Process> = all_processes.iter().map(|p| *p).collect();
        vec_of_trait_objects
    }
}