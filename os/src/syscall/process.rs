//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::PAGE_SIZE,
    loader::get_app_data_by_name,
    mm::{translated_byte_buffer, translated_refmut, translated_str, MapPermission, VirtAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let cur = current_task().unwrap();
    let child = cur.fork();
    let pid = child.pid.0;
    let trap_cx = child.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0;
    add_task(child);
    pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        let exit_code = child.inner_exclusive_access().exit_code;
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
}

pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let token = current_user_token();
    let bufs = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());
    let tv = TimeVal { sec: us / 1_000_000, usec: us % 1_000_000 };
    let src = unsafe {
        core::slice::from_raw_parts(&tv as *const TimeVal as *const u8, core::mem::size_of::<TimeVal>())
    };
    let mut off = 0;
    for buf in bufs {
        let n = buf.len();
        buf.copy_from_slice(&src[off..off + n]);
        off += n;
    }
    0
}

pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    if start % PAGE_SIZE != 0 || len == 0 { return -1; }
    if port & !0x7 != 0 || port == 0 { return -1; }
    let mut perm = MapPermission::U;
    if port & 1 != 0 { perm |= MapPermission::R; }
    if port & 2 != 0 { perm |= MapPermission::W; }
    if port & 4 != 0 { perm |= MapPermission::X; }
    let alen = (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.memory_set.mmap(VirtAddr::from(start), VirtAddr::from(start + alen), perm)
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    if start % PAGE_SIZE != 0 || len == 0 { return -1; }
    let alen = (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.memory_set.munmap(VirtAddr::from(start), VirtAddr::from(start + alen))
}

pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let cur = current_task().unwrap();
        let child = cur.spawn(data);
        let pid = child.pid.0;
        add_task(child);
        pid as isize
    } else {
        -1
    }
}

pub fn sys_set_priority(prio: isize) -> isize {
    if prio < 2 { return -1; }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.priority = prio;
    prio
}
