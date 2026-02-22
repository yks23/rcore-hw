//! Process management syscalls
use crate::{
    config::PAGE_SIZE,
    mm::{translated_byte_buffer, MapPermission},
    task::{
        change_program_brk, current_mmap, current_munmap, current_user_token,
        exit_current_and_run_next, get_syscall_count, suspend_current_and_run_next,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let tv = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let token = current_user_token();
    let dst = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());
    let src = unsafe {
        core::slice::from_raw_parts(&tv as *const TimeVal as *const u8, core::mem::size_of::<TimeVal>())
    };
    let mut off = 0;
    for chunk in dst {
        let n = chunk.len();
        chunk.copy_from_slice(&src[off..off + n]);
        off += n;
    }
    0
}

fn check_user_va(token: usize, va: usize) -> Option<usize> {
    let pt = crate::mm::PageTable::from_token(token);
    let vaddr = crate::mm::VirtAddr::from(va);
    let vpn = vaddr.floor();
    match pt.translate(vpn) {
        Some(pte) if pte.is_valid() && (pte.flags() & crate::mm::PTEFlags::U) != crate::mm::PTEFlags::empty() => {
            Some(crate::mm::PhysAddr::from(pte.ppn()).0 + vaddr.page_offset())
        }
        _ => None,
    }
}

pub fn sys_trace(req: usize, id: usize, data: usize) -> isize {
    match req {
        0 => {
            let token = current_user_token();
            match check_user_va(token, id) {
                Some(pa) => unsafe { *(pa as *const u8) as isize },
                None => -1,
            }
        }
        1 => {
            let token = current_user_token();
            let pt = crate::mm::PageTable::from_token(token);
            let vaddr = crate::mm::VirtAddr::from(id);
            match pt.translate(vaddr.floor()) {
                Some(pte) if pte.is_valid() && pte.writable()
                    && (pte.flags() & crate::mm::PTEFlags::U) != crate::mm::PTEFlags::empty() =>
                {
                    let pa = crate::mm::PhysAddr::from(pte.ppn()).0 + vaddr.page_offset();
                    unsafe { *(pa as *mut u8) = data as u8; }
                    0
                }
                _ => -1,
            }
        }
        2 => get_syscall_count(id) as isize,
        _ => -1,
    }
}

pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    if start % PAGE_SIZE != 0 { return -1; }
    if len == 0 { return -1; }
    if port & !0x7 != 0 || port == 0 { return -1; }
    let mut perm = MapPermission::U;
    if port & 1 != 0 { perm |= MapPermission::R; }
    if port & 2 != 0 { perm |= MapPermission::W; }
    if port & 4 != 0 { perm |= MapPermission::X; }
    let alen = (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
    current_mmap(start, alen, perm)
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if start % PAGE_SIZE != 0 || len == 0 { return -1; }
    let alen = (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
    current_munmap(start, alen)
}

pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
