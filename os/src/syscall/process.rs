use crate::{
    config::MEMORY_END,
    task::{exit_current_and_run_next, get_syscall_count, suspend_current_and_run_next},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    match trace_request {
        0 => {
            if id == 0 || id >= MEMORY_END { return -1; }
            unsafe { *(id as *const u8) as isize }
        }
        1 => {
            if id == 0 || id >= MEMORY_END { return -1; }
            unsafe { *(id as *mut u8) = data as u8; }
            0
        }
        2 => get_syscall_count(id) as isize,
        _ => -1,
    }
}
