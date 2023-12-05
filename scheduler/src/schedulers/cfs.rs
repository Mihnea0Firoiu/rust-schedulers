use crate::{Scheduler, Process};

use std::cmp::Ordering;
use std::num::NonZeroUsize;
use crate::Pid;
use crate::ProcessState;
use crate::Syscall;

use std::collections::VecDeque;

use std::ops::Add;


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

    /// Remaining slices
    pub remaining_slices: usize,

    /// How much a process must sleep
    pub sleep_time: usize,

    /// Virtual Runtime
    pub vruntime: usize
}

impl ProcessInfo {
    fn new(pid: Pid, priority: i8, timeslice: usize) -> Self {
        Self {
            pid,
            state: ProcessState::Ready,
            timings: (0, 0, 0),
            priority,
            extra: "vruntime=0".to_string(),
            remaining_slices: timeslice,
            sleep_time: 0,
            vruntime: 0,
        }
    }
}

/// Implemented 'Process' trait for ProcessInfo.
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

/// Implemented 'Clone' trait for ProcessInfo.
impl Clone for ProcessInfo {
    fn clone(&self) -> Self {
        Self {
            pid: self.pid,
            state: self.state,
            timings: self.timings,
            priority: self.priority,
            extra: self.extra.clone(),
            remaining_slices: self.remaining_slices,
            sleep_time: self.sleep_time,
            vruntime: self.vruntime
        }
    }
}

/// For sort.
/// Implemented 'PartialEq' trait for ProcessInfo.
impl PartialEq for ProcessInfo {
    fn eq(&self, other: &Self) -> bool {
        self.vruntime == other.vruntime
    }
}

/// Implemented 'PartialOrd' trait for ProcessInfo.
impl PartialOrd for ProcessInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let ord = self.vruntime.cmp(&other.vruntime);
        
        if let Ordering::Equal = ord {
            return Some(self.pid.cmp(&other.pid));
        }

        Some(self.vruntime.cmp(&other.vruntime))
    }
}

/// Implemented 'Add' trait for ProcessInfo.
impl Add<usize> for ProcessInfo {
    type Output = ProcessInfo;

    fn add(self, rhs: usize) -> ProcessInfo {
        ProcessInfo {
            // Copy the fields except for vruntime, which gets updated
            pid: self.pid,
            state: self.state,
            timings: self.timings,
            priority: self.priority,
            extra: self.extra,
            remaining_slices: self.remaining_slices,
            sleep_time: self.sleep_time,
            vruntime: self.vruntime + rhs,
        }
    }
}

pub struct Cfs {
    // The total amount of timeslices.
    pub cpu_time: usize,

    /// The maximum amout a process is allowed to run until it is preemted
    pub timeslice: usize,

    /// The process will be scheduled again if the
    /// remaining slices are greater or equal to this value.
    pub minimum_remaining_timeslice: usize,

    /// The last used pid.
    pub last_pid: usize,

    /// Field to hold the running process.
    pub running_process: Option<ProcessInfo>,

    /// Queue to hold the processes that are ready.
    pub ready_process_queue: VecDeque<ProcessInfo>,

    /// Queue to hold the processes that are waiting.
    pub waiting_process_queue: VecDeque<ProcessInfo>,

    /// Field that tells the scheduler how much to sleep.
    pub time_jump: usize,

    /// Field that tells if init was killed.
    pub killed_init: bool
}

impl Cfs {
    pub fn new(cpu_time: NonZeroUsize, minimum_remaining_timeslice: usize) -> Self {
        Self {
            cpu_time: cpu_time.get(),
            timeslice: cpu_time.get(),
            minimum_remaining_timeslice,
            last_pid: 0,
            running_process: None,
            ready_process_queue: VecDeque::new(),
            waiting_process_queue: VecDeque::new(),
            time_jump: 0,
            killed_init: false
        }
    }

    /// Checks if all processes from the waiting queue are waiting.
    pub fn all_waiting(&self) -> bool {
        for process in &self.waiting_process_queue {
            if let ProcessState::Waiting { event: None } = process.state {
                return false;
            }
        }
        true
    }

    /// Finds the minimum amount for the scheduler to sleep.
    pub fn minimum_sleeping_duration(&self) -> usize {
        let mut min = 0;
        for process in &self.waiting_process_queue {
            if process.sleep_time != 0 && (min == 0 || process.sleep_time < min) {
                min = process.sleep_time;
            }
        }
        min
    }

    /// Function that calculates time.
    pub fn time_master(&mut self, beginning: usize, end: usize) {
        if let Some(running_process) = &mut self.running_process {
            running_process.timings.0 = running_process.timings.0 + beginning - end;
            
            // vruntime
            let vruntime_update = beginning - end;
            *running_process = running_process.clone() + vruntime_update;
        }

        for ready_process in &mut self.ready_process_queue {
            ready_process.timings.0 = ready_process.timings.0 + beginning - end;
        }

        let mut index_vec = VecDeque::<usize>::new();

        for (index, waiting_process) in self.waiting_process_queue.iter_mut().enumerate() {
            waiting_process.timings.0 = waiting_process.timings.0 + beginning - end;
            if waiting_process.sleep_time > 0 {
                if waiting_process.sleep_time > (beginning - end) {
                    waiting_process.sleep_time -= beginning - end;
                } else {
                    waiting_process.sleep_time = 0;
                    waiting_process.state = ProcessState::Ready;
                    self.ready_process_queue.push_back(waiting_process.clone());
                    index_vec.push_front(index);
                }
            }
        }

        for index in index_vec {
            self.waiting_process_queue.remove(index);
        }
    }

    /// Returns 'true' if init was killed too soon.
    pub fn killed_init(&self) -> bool {
        if (!self.waiting_process_queue.is_empty() || !self.ready_process_queue.is_empty()) && self.killed_init {
            return true;
        }
        false
    }

    /// Sorts 'ready_process_queue' according to 'vruntime'
    pub fn sort(&mut self) {
        let mut sorted_vec: Vec<ProcessInfo> = self.ready_process_queue.drain(..).collect();
        // The compare never fails
        sorted_vec.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.ready_process_queue.extend(sorted_vec);
    }

    /// Update the timeslice and return the old one.
    pub fn update_timeslice(&mut self) -> usize {
        let old_timeslice = self.timeslice;
        match &self.running_process {
            Some(_) => {
                self.timeslice = self.cpu_time / (self.ready_process_queue.len() + 1);
            },
            None => {
                if !self.ready_process_queue.is_empty() {
                    self.timeslice = self.cpu_time / (self.ready_process_queue.len());
                }
            }
        }
        old_timeslice
    }

    /// Update vruntime.
    pub fn update_vruntime(&mut self) -> usize {
        let mut min = 0;

        if let Some(process) = &self.running_process {
            if process.vruntime < min || min == 0 {
                min = process.vruntime;
            }
        }

        for process in &self.ready_process_queue {
            if process.vruntime < min || min == 0 {
                min = process.vruntime;
            }
        }

        for process in &self.waiting_process_queue {
            if process.vruntime < min || min == 0 {
                min = process.vruntime;
            }
        }

        min
    }

    /// Update extra.
    pub fn update_extra(&mut self) {
        let vruntime = "vruntime=".to_string();

        if let Some(process) = &mut self.running_process {
            let pr_vruntime = process.vruntime.to_string();
            let mut vruntime_clone = vruntime.clone();
            vruntime_clone.push_str(&pr_vruntime);
            process.extra = vruntime_clone;
        }

        for process in &mut self.ready_process_queue {
            let pr_vruntime = process.vruntime.to_string();
            let mut vruntime_clone = vruntime.clone();
            vruntime_clone.push_str(&pr_vruntime);
            process.extra = vruntime_clone;
        }

        for process in &mut self.waiting_process_queue {
            let pr_vruntime = process.vruntime.to_string();
            let mut vruntime_clone = vruntime.clone();
            vruntime_clone.push_str(&pr_vruntime);
            process.extra = vruntime_clone;
        }

    }

}

impl Scheduler for Cfs {
    /// Makes a decision about scheduling.
    fn next(&mut self) -> crate::SchedulingDecision {
        if self.killed_init() {
            return crate::SchedulingDecision::Panic;
        }

        if self.time_jump != 0 {
            self.time_master(self.time_jump, 0);
            self.time_jump = 0;
        }

        let old_timeslice = self.update_timeslice();

        if let Some(process) = &mut self.running_process {
            let mut timeslice = self.timeslice;

            // The process goes in ready.
            if process.remaining_slices < self.minimum_remaining_timeslice {
                process.remaining_slices = timeslice;

                process.state = ProcessState::Ready;
                let cloned_process = process.clone();

                self.ready_process_queue.push_back(cloned_process);
                self.running_process = None;

            } else {
                // The timeslice resets depending on the situation.
                if self.timeslice < process.remaining_slices || process.remaining_slices == old_timeslice {
                    process.remaining_slices = self.timeslice;
                }

                timeslice = process.remaining_slices;
                // 'timeslice' will never be 0 or smaller than 0, so 'unwrap' was used.
                return crate::SchedulingDecision::Run { pid: process.pid(), timeslice: NonZeroUsize::new(timeslice).unwrap() };
            }
        }

        // Sort the 'ready_process_queue' before poping an element.
        self.sort();

        let first_element = self.ready_process_queue.pop_front();
        self.running_process = first_element;
        match &mut self.running_process {
            Some(first_element) => {
                // If there is a least an elemet in 'ready_process_queue'
                first_element.state = ProcessState::Running;

                first_element.remaining_slices = self.timeslice;
                
                // 'timeslice' will never be 0 or smaller than 0, so 'unwrap' was used.
                crate::SchedulingDecision::Run { pid: first_element.pid(), timeslice: NonZeroUsize::new(first_element.remaining_slices).unwrap() }
            },
            None => {
                self.time_jump = self.minimum_sleeping_duration();
                if self.time_jump != 0 {
                    return crate::SchedulingDecision::Sleep(NonZeroUsize::new(self.time_jump).unwrap());
                }
                
                if self.ready_process_queue.is_empty() && self.waiting_process_queue.is_empty() {
                    return crate::SchedulingDecision::Done;
                }

                if self.all_waiting() {
                    return crate::SchedulingDecision::Deadlock;
                }
                
                crate::SchedulingDecision::Panic
            }
        }
    }

    /// Does something when a process expires or make a syscall.
    fn stop(&mut self, _reason: crate::StopReason) -> crate::SyscallResult {
        match _reason {

            // The behaviour for 'Expired'
            crate::StopReason::Expired => {

                if let Some(process) = &mut self.running_process {
                    let beginning = process.remaining_slices;
                    self.time_master(beginning, 0);
                }
                
                if let Some(process) = &mut self.running_process {
                    process.timings.2 += process.remaining_slices;
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
                    // The behaviour for 'Fork'
                    Syscall::Fork(priority) => {
                        if self.running_process.is_none() && self.last_pid != 0 {
                            return crate::SyscallResult::NoRunningProcess;
                        }
                        
                        self.last_pid += 1;
                        let pid = Pid::new(self.last_pid);

                        let mut new_process = ProcessInfo::new(pid, priority, self.timeslice);
                        new_process.vruntime = self.update_vruntime();
                        self.ready_process_queue.push_back(new_process);

                        crate::SyscallResult::Pid(pid)
                    }
                    // The behaviour for 'Sleep'
                    Syscall::Sleep(time) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }
                        
                        if let Some(process) = &mut self.running_process {
                            process.sleep_time = time;
                            process.state = ProcessState::Waiting { event: None };
                            process.remaining_slices = self.timeslice;
                            self.waiting_process_queue.push_back(process.clone());
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                    // The behaviour for 'Wait'
                    Syscall::Wait(event_number) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }

                        if let Some(process) = &mut self.running_process {
                            process.state = ProcessState::Waiting { event: Some(event_number) };
                            process.remaining_slices = self.timeslice;
                            self.waiting_process_queue.push_back(process.clone());
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                    // The behaviour for 'Signal'
                    Syscall::Signal(event_number) => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }    

                        let mut index_vec = VecDeque::new();

                        for (index, process) in self.waiting_process_queue.iter_mut().enumerate() {
                            if let ProcessState::Waiting { event: Some(event) } = process.state {
                                if event == event_number {
                                    process.state = ProcessState::Ready;
                                    self.ready_process_queue.push_back(process.clone());
                                    index_vec.push_front(index);
                                }
                            }
                        }

                        for index in index_vec {
                            self.waiting_process_queue.remove(index);
                        }

                        crate::SyscallResult::Success
                    }
                    // The behaviour for 'Exit'
                    Syscall::Exit => {
                        if self.running_process.is_none() {
                            return crate::SyscallResult::NoRunningProcess;
                        }

                        if let Some(process) = &self.running_process {
                            if process.pid == Pid::new(1) {
                                self.killed_init = true;
                            }
                        }

                        self.running_process = None;

                        crate::SyscallResult::Success
                    }
                }
            }
        }
    }

    /// Takes all the processes and puts them in a common 'Vec<&dyn crate::Process>'.
    fn list(&mut self) -> Vec<&dyn crate::Process> {
        let mut all_processes: VecDeque<&dyn crate::Process> = VecDeque::new();
        self.update_extra();

        if let Some(running_process) = &mut self.running_process {
            all_processes.push_back(running_process);
        }

        for process in &mut self.ready_process_queue {
            all_processes.push_back(process);
        }

        for process in &mut self.waiting_process_queue {
            all_processes.push_back(process);
        }

        // Convert the VecDeque to Vec<&dyn Process>
        let vec_of_trait_objects: Vec<&dyn crate::Process> = all_processes.iter().cloned().collect();
        vec_of_trait_objects
    }
}
