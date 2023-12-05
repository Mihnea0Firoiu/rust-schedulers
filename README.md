[![Review Assignment Due Date](https://classroom.github.com/assets/deadline-readme-button-24ddc0f5d75046c5622901739e7c5dd533143b0c8e959d652212380cedb1ea36.svg)](https://classroom.github.com/a/2eN9hsMw)
# Process Scheduler

## Implementation - Mihnea-Ioan Firoiu

* `General`:

   Multiple support functions were used, the most important being `time_master`. This function calculates timings for all processes.

   All support functions can be found in the `impl ProcessInfo` and `impl <Scheduler Name>` blocks.

* round_robin:

   This is the most straight forward out of the three. Each process has maximum `timeslice` slices. When the process gets bellow `minimum_remaining_timeslice` slices and makes a syscall, it gets preempted (the time `remaining_slices` variable resets to `timeslice` and the process is pushed to the end of `ready_process_queue`).

   For this scheduler `priority` is not relevant.

* priority_queue:

   The `Priority Based Round-Robin` (or `priority_queue` as it is named here) does exactly as the name says: a Round Robin where priorities are relevant.

   Before popping an element out of the `ready_process_queue` queue, it gets sorted according to priority. 

* cfs:

   The most complex out of the three, it introduces two new notions: `cpu_time` and `vruntime`. To get the previous `timeslice`, `cpu_time` must be divided (dynamically) by the total number of processes that are ready.
   
   `vruntime` is a field that corresponds to every process. It gets updated according to the execution time and works similarly to the `priority`. The queue gets updated before extracting an element.

* `Observations`

   - The processes that sleep could have their own queue, `sleep_process_queue`, but I considered that it is more sugestive to have them in `waiting_process_queue`.
   - `cfs` and `priority_queue` could be implemented using a heap. I used a queue because I used the already written code from round_robin.
   - I don't see the utility of the `Add` trait used with `cfs`.

## Getting started

Please run `cargo doc --open` to create and open the documentation.

Your job is:
1. Implement the schedulers in the `scheduler` crate in the folder `scheduler/src/schedulers`.
2. Export the scheduler in the `scheduler/src/lib.rs` file using the three functions
   - `round_robin(...)`
   - `priority_queue(...)`
   - `cfs(...)`
3. Test them using the `runner` crate by using them in `runner/src/main.rs`.
