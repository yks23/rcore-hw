//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

const BIG_STRIDE: usize = 0x10000;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self { ready_queue: VecDeque::new() }
    }
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    // stride调度: 找stride最小的
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        if self.ready_queue.is_empty() { return None; }
        let mut min_i = 0;
        let mut min_s = usize::MAX;
        for (i, t) in self.ready_queue.iter().enumerate() {
            let inner = t.inner_exclusive_access();
            let s = inner.stride;
            drop(inner);
            if s < min_s { min_s = s; min_i = i; }
        }
        let task = self.ready_queue.remove(min_i).unwrap();
        {
            let mut inner = task.inner_exclusive_access();
            let p = inner.priority.max(2) as usize;
            inner.stride += BIG_STRIDE / p;
        }
        Some(task)
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}
