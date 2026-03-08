use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut pi = process.inner_exclusive_access();
    if let Some(id) = pi.mutex_list.iter().enumerate()
        .find(|(_, item)| item.is_none()).map(|(id, _)| id)
    {
        pi.mutex_list[id] = mutex;
        id as isize
    } else {
        pi.mutex_list.push(mutex);
        pi.mutex_list.len() as isize - 1
    }
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let mutex = Arc::clone(pi.mutex_list[mutex_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    while pi.mutex_holders.len() <= mutex_id {
        pi.mutex_holders.push(usize::MAX);
    }
    if pi.deadlock_detect && pi.mutex_holders[mutex_id] == tid {
        return -0xDEAD_isize;
    }
    drop(pi);
    drop(process);
    mutex.lock();
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    while pi.mutex_holders.len() <= mutex_id { pi.mutex_holders.push(usize::MAX); }
    pi.mutex_holders[mutex_id] = tid;
    drop(pi);
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let mutex = Arc::clone(pi.mutex_list[mutex_id].as_ref().unwrap());
    if mutex_id < pi.mutex_holders.len() {
        pi.mutex_holders[mutex_id] = usize::MAX;
    }
    drop(pi);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let id = if let Some(id) = pi.semaphore_list.iter().enumerate()
        .find(|(_, item)| item.is_none()).map(|(id, _)| id)
    {
        pi.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        pi.semaphore_list.push(Some(Arc::new(Semaphore::new(res_count))));
        pi.semaphore_list.len() - 1
    };
    id as isize
}

fn really_blocked(sem_waiting: &[Option<usize>], avail: &[isize], t: usize) -> bool {
    if t >= sem_waiting.len() { return false; }
    if let Some(s) = sem_waiting[t] { s < avail.len() && avail[s] < 0 } else { false }
}

fn detect_sem_deadlock(
    alloc: &[Vec<usize>], waiting: &[Option<usize>],
    avail: &[isize], tid: usize, _sem_id: usize,
) -> bool {
    let nsem = avail.len();
    let mut vis = vec![false; waiting.len()];
    let mut stk = vec![false; waiting.len()];
    fn dfs(
        t: usize, alloc: &[Vec<usize>], waiting: &[Option<usize>],
        avail: &[isize], vis: &mut Vec<bool>, stk: &mut Vec<bool>,
        nsem: usize, target: usize,
    ) -> bool {
        if t >= vis.len() { return false; }
        vis[t] = true;
        stk[t] = true;
        let wanted = if t == target {
            Some(if target < waiting.len() { waiting[target].unwrap_or(usize::MAX) } else { usize::MAX })
        } else {
            waiting.get(t).copied().flatten()
        };
        if let Some(ws) = wanted {
            if ws < nsem && avail[ws] < 0 {
                for h in 0..alloc.len() {
                    if h == t { continue; }
                    if ws < alloc[h].len() && alloc[h][ws] > 0 {
                        if !really_blocked(waiting, avail, h) { continue; }
                        if stk.get(h) == Some(&true) { return true; }
                        if !vis.get(h).copied().unwrap_or(false)
                            && dfs(h, alloc, waiting, avail, vis, stk, nsem, target) { return true; }
                    }
                }
            }
        }
        stk[t] = false;
        false
    }
    dfs(tid, alloc, waiting, avail, &mut vis, &mut stk, nsem, tid)
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let sem = Arc::clone(pi.semaphore_list[sem_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    if tid < pi.sem_alloc.len() && sem_id < pi.sem_alloc[tid].len() {
        if pi.sem_alloc[tid][sem_id] > 0 { pi.sem_alloc[tid][sem_id] -= 1; }
    }
    drop(pi);
    sem.up();
    0
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let sem = Arc::clone(pi.semaphore_list[sem_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    if pi.deadlock_detect {
        let cnt = sem.inner.exclusive_access().count;
        if cnt <= 0 {
            let nsem = pi.semaphore_list.len();
            let mut avail: Vec<isize> = Vec::with_capacity(nsem);
            for i in 0..nsem {
                if let Some(ref s) = pi.semaphore_list[i] {
                    avail.push(s.inner.exclusive_access().count);
                } else { avail.push(0); }
            }
            avail[sem_id] -= 1;
            while pi.sem_waiting.len() <= tid { pi.sem_waiting.push(None); }
            pi.sem_waiting[tid] = Some(sem_id);
            if detect_sem_deadlock(&pi.sem_alloc, &pi.sem_waiting, &avail, tid, sem_id) {
                pi.sem_waiting[tid] = None;
                return -0xDEAD_isize;
            }
        }
    }
    drop(pi);
    sem.down();
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    while pi.sem_alloc.len() <= tid { pi.sem_alloc.push(Vec::new()); }
    let nsem = pi.semaphore_list.len();
    while pi.sem_alloc[tid].len() < nsem { pi.sem_alloc[tid].push(0); }
    pi.sem_alloc[tid][sem_id] += 1;
    if tid < pi.sem_waiting.len() { pi.sem_waiting[tid] = None; }
    drop(pi);
    0
}

pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    let id = if let Some(id) = pi.condvar_list.iter().enumerate()
        .find(|(_, item)| item.is_none()).map(|(id, _)| id)
    {
        pi.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        pi.condvar_list.push(Some(Arc::new(Condvar::new())));
        pi.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let pi = process.inner_exclusive_access();
    let condvar = Arc::clone(pi.condvar_list[condvar_id].as_ref().unwrap());
    drop(pi);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
    );
    let process = current_process();
    let pi = process.inner_exclusive_access();
    let condvar = Arc::clone(pi.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(pi.mutex_list[mutex_id].as_ref().unwrap());
    drop(pi);
    condvar.wait(mutex);
    0
}

pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    let process = current_process();
    let mut pi = process.inner_exclusive_access();
    pi.deadlock_detect = enabled != 0;
    0
}
