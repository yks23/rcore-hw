# rCore Tutorial 2025S 实验指南

> 本指南对应 [LearningOS/rCore-Tutorial-Code-2025S](https://github.com/LearningOS/rCore-Tutorial-Code-2025S) 仓库，共 5 个评分实验（ch3, ch4, ch5, ch6, ch8）。
>
> 参考文档：[rCore-Tutorial-Guide-2025S](https://LearningOS.github.io/rCore-Tutorial-Guide-2025S/) | [rCore-Tutorial-Book-v3](https://rcore-os.github.io/rCore-Tutorial-Book-v3/)

---

## 通用操作

```bash
# 切换到某个实验分支
cd code && git checkout ch$ID

# 把测试用例放到 user 目录（如尚未放置）
# git clone https://github.com/LearningOS/rCore-Tutorial-Test-2025S.git user

# 构建并运行
cd os && make run

# 仅构建（跳过 rustup 检查）
cd os && make build OFFLINE=1

# 格式化检查
cd os && cargo fmt -- --check
```

测试通过的标志：所有 `ch${ID}_xxx` 和 `ch${ID}b_xxx` 测试程序输出 `OK` / `passed`，无 panic。

---

## 实验一：ch3 — 系统调用追踪（sys_trace）

### 背景

ch3 是**多道程序**章节。所有用户程序和内核共享同一物理地址空间（无页表、无虚拟内存）。每个应用被加载到 `0x80400000 + app_id * 0x20000` 的固定地址。内核通过时钟中断实现抢占式调度。

### 已有代码

| 文件 | 内容 |
|------|------|
| `os/src/syscall/mod.rs` | 系统调用分发，已有 `WRITE/EXIT/YIELD/GET_TIME/TRACE` |
| `os/src/syscall/process.rs` | `sys_trace` **桩函数**（返回 -1） |
| `os/src/task/task.rs` | `TaskControlBlock`：只有 `task_status` 和 `task_cx` |
| `os/src/task/mod.rs` | `TaskManager`：Round-Robin 调度器 |

### 你要实现的

**唯一任务：实现 `sys_trace(trace_request, id, data) -> isize`**

`trace_request` 的三种模式：

#### 模式 0：Read — 读取用户内存

```
sys_trace(0, addr, _) -> 该地址的字节值(0~255)，非法地址返回 -1
```

- 内核把 `id` 当作用户态地址，读取 1 字节
- ch3 无虚拟内存，直接用裸指针即可：`unsafe { *(id as *const u8) as isize }`
- 需要验证地址合法性（非零、在用户地址范围内等）

#### 模式 1：Write — 写入用户内存

```
sys_trace(1, addr, data) -> 0 成功，-1 失败
```

- 内核向 `id` 地址写入 `data as u8`
- 直接用裸指针：`unsafe { *(id as *mut u8) = data as u8; }`

#### 模式 2：Syscall — 统计系统调用次数

```
sys_trace(2, syscall_id, _) -> 当前任务调用 syscall_id 的累计次数
```

这是最核心的部分，需要：

1. **给 `TaskControlBlock` 添加计数数组**

   在 `os/src/task/task.rs` 中给 TCB 增加一个字段，例如：
   ```rust
   pub syscall_counts: [u32; MAX_SYSCALL_NUM]
   ```
   你需要选一个合理的 `MAX_SYSCALL_NUM`（比如 500，覆盖所有已定义的系统调用号）。

2. **在系统调用入口处计数**

   在 `os/src/syscall/mod.rs` 的 `syscall()` 函数中，**在 `match` 分发之前**就把 `syscall_id` 对应的计数 +1。这样 `sys_trace(Syscall, TRACE, _)` 本身也会被正确计数。

3. **提供访问当前任务 TCB 的方法**

   给 `TaskManager` 添加方法，让 `sys_trace` 能获取当前任务的 `syscall_counts`。

### 关键陷阱

- **`println!` 产生 2 次 `WRITE`**：`println!("text\n")` 宏会先输出字符串再输出换行，各调一次 `sys_write`，所以测试断言 `count_syscall(SYSCALL_WRITE) == 2`
- **`sleep()` 是用户态忙等待**：看 `user/src/lib.rs:263`，`sleep` 只是循环调 `sys_yield()`，不需要内核实现 `SYSCALL_SLEEP`
- **计数必须在 match 之前**：否则 `count_syscall(SYSCALL_TRACE)` 的计数会少 1
- **`#![deny(missing_docs)]`**：所有新增的 `pub` 项都必须写 `///` 文档注释
- **`#![deny(warnings)]`**：未使用变量等 warning 会变成编译错误

### 通过标志

```
get_time OK! {...}
Test sleep OK!
Test sleep1 passed!
Test trace OK!
```

---

## 实验二：ch4 — 内存映射（mmap/munmap）

### 背景

ch4 引入了 **SV39 页表虚拟内存**。每个任务有独立的 `MemorySet`（地址空间），内核使用 `PageTable` 管理页表。用户态传给内核的指针不再能直接解引用——需要通过页表翻译。

### 已有代码

| 文件 | 内容 |
|------|------|
| `os/src/mm/` | 完整的内存管理模块（`PageTable`, `MemorySet`, `FrameAllocator` 等） |
| `os/src/syscall/process.rs` | `sys_mmap`、`sys_munmap`、`sys_get_time`、`sys_trace` 全是**桩函数** |
| `os/src/mm/memory_set.rs` | `MemorySet` 结构，有 `push`/`insert_framed_area` 等方法 |

### 你要实现的

#### 1. `sys_get_time` — 重新实现

```rust
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize
```

ch3 里直接写用户指针就行，但 ch4 有虚拟内存后，内核不能直接解引用用户态指针了。

**思路**：用 `translated_byte_buffer` 或 `translated_refmut` 把用户虚拟地址翻译成内核可访问的物理地址，再写入 `TimeVal`。

#### 2. `sys_trace` — 重新实现

同样需要用页表翻译用户地址。Read/Write 操作要通过当前任务的页表翻译后才能访问。

#### 3. `sys_mmap(start, len, prot) -> isize` ⭐ 核心

```
成功返回 0，失败返回 -1
```

**参数约束**：
- `start` 必须页对齐（`start % PAGE_SIZE == 0`）
- `len > 0`，向上取整到页大小
- `prot` 的含义：bit0=读, bit1=写, bit2=执行
- `prot` 不能为 0（无任何权限没意义），不能包含除低 3 位以外的 bit（`prot & !0x7 != 0` 则非法）
- 映射区间不能和已有映射重叠

**实现思路**：
1. 校验参数合法性
2. 获取当前任务的 `MemorySet`
3. 调用 `MemorySet` 的方法（如 `insert_framed_area`）在 `[start, start+len)` 插入一个 `MapArea`
4. 将 `prot` 转换为 `MapPermission`（注意加上 `U` 位，因为是用户态映射）

**错误处理示例**（来自 `ch4_mmap3.rs`）：
```rust
mmap(start - len, len + 1, prot)  // start 不对齐 → -1
mmap(start + len + 1, len, prot)  // start 不对齐 → -1
mmap(start + len, len, 0)         // prot=0 无权限 → -1
mmap(start + len, len, prot | 8)  // prot 非法位 → -1
```

#### 4. `sys_munmap(start, len) -> isize`

```
成功返回 0，失败返回 -1
```

- `start` 必须页对齐
- 需要在当前任务的 `MemorySet` 中找到并移除对应的映射区间
- 解除映射后释放物理页帧

**实现思路**：给 `MemorySet` 添加一个 `munmap` 方法，遍历 `areas` 找到包含 `[start, start+len)` 的区域并移除。

### 关键提示

- 仔细阅读 `os/src/mm/memory_set.rs`，理解 `MapArea`、`MapType`、`MapPermission` 的含义
- `MapPermission` 需要加 `U` (User) 位：`MapPermission::R | MapPermission::W | MapPermission::U`
- `translated_byte_buffer` 和 `translated_refmut` 是在 `os/src/mm/page_table.rs` 里提供的工具函数
- 要获取当前任务的 token（页表根地址），用 `current_user_token()`

### 通过标志

```
Test 04_1 OK!
(ch4_mmap1: 因写保护触发 PageFault，被内核杀掉 — 这是正确行为)
(ch4_mmap2: 因读保护触发 PageFault，被内核杀掉 — 这是正确行为)
Test 04_4 test OK!
Test 04_5 ummap OK!
Test trace OK!  (ch4_trace1)
```

---

## 实验三：ch5 — 进程管理与 Stride 调度

### 背景

ch5 引入了**进程**的概念。`TaskControlBlock` 变成了 `Arc<TaskControlBlock>`，支持 `fork`/`exec`/`waitpid`。调度器从数组变成了 `VecDeque<Arc<TaskControlBlock>>`（FIFO 就绪队列）。

### 已有代码

| 文件 | 内容 |
|------|------|
| `os/src/task/manager.rs` | `TaskManager`：FIFO 就绪队列，`fetch()` 用 `pop_front()` |
| `os/src/task/processor.rs` | `Processor`：当前运行任务管理 |
| `os/src/syscall/process.rs` | `sys_fork`/`sys_exec`/`sys_waitpid` 已实现；`sys_spawn`/`sys_set_priority`/`sys_get_time`/`sys_mmap`/`sys_munmap` 是**桩函数** |

### 你要实现的

#### 1. `sys_spawn(path) -> isize`

```
成功返回子进程 PID，失败返回 -1
```

- 创建一个新进程，加载 `path` 指定的程序
- **不同于 fork+exec**：spawn 不复制父进程地址空间，而是直接创建新地址空间并加载程序
- 新进程是当前进程的子进程（加到 `children` 列表中）

**实现思路**：
1. 用 `translated_str` 翻译用户态路径字符串
2. 用 `get_app_data_by_name` 获取程序数据
3. 创建新的 `TaskControlBlock`（参考 `fork` 和 `exec` 的实现）
4. 设置父子关系
5. 用 `add_task` 加入就绪队列
6. 返回子进程 PID

#### 2. `sys_set_priority(prio) -> isize`

```
成功返回 prio，prio < 2 返回 -1
```

- 设置当前进程的调度优先级
- 优先级必须 >= 2
- 将 prio 存到当前进程的 TCB 中

#### 3. Stride 调度算法 ⭐ 核心

**原理**：
- 每个进程有 `stride`（步长）和 `pass`（累计值）
- 每次调度选择 `pass` 最小的进程运行
- 运行后更新：`pass += stride`
- 步长计算：`stride = BIG_STRIDE / priority`
- `BIG_STRIDE` 是一个大常数（如 2^16 或更大）

**你要改什么**：

1. **`TaskControlBlock`**：添加 `priority`（默认 16）、`stride`、`pass` 字段

2. **`TaskManager::fetch()`**：把 `pop_front()`（FIFO）改为选择 `pass` 最小的任务
   ```rust
   // 原来：self.ready_queue.pop_front()
   // 改为：找到 pass 最小的任务，移除并返回
   ```

3. **`TaskManager::add()`**：任务加入就绪队列时，可能需要更新 stride

**注意事项**：
- `pass` 溢出问题：用 `wrapping_add` 或者选择足够大的类型
- 默认优先级应使得未设置优先级的进程也能正常调度
- `BIG_STRIDE` 的选取：太大可能溢出，太小精度不够。一般用 `2^16 = 65536` 或 `isize::MAX`

#### 4. `sys_get_time` 和 `sys_mmap`/`sys_munmap`

ch5 仍需要这些（从 ch4 延续），如果 ch4 已实现则直接移植即可。

### 通过标志

```
Test set_priority OK!
Test getpid OK!
priority = 5, exitcode = N1, ratio = R1
priority = 6, exitcode = N2, ratio = R2
...
(6 个进程的 ratio 应大致相等，说明 CPU 时间正比于 priority)
```

---

## 实验四：ch6 — 文件系统（fstat/linkat/unlinkat）

### 背景

ch6 引入了 **easy-fs** 文件系统。基本的文件操作（`open`/`read`/`write`/`close`）已经实现。你需要实现文件元数据查询和硬链接。

### 已有代码

| 文件 | 内容 |
|------|------|
| `os/src/fs/` | 文件系统模块，`open_file` 已实现 |
| `os/src/syscall/fs.rs` | `sys_open`/`sys_read`/`sys_write`/`sys_close` 已实现；`sys_fstat`/`sys_linkat`/`sys_unlinkat` 是**桩函数** |
| `easy-fs/` | easy-fs 库（独立 crate） |
| `easy-fs-fuse/` | 文件系统镜像构建工具 |

### 你要实现的

#### 1. `sys_fstat(fd, st) -> isize`

```
成功返回 0，失败返回 -1
```

- 根据文件描述符 `fd`，填充 `Stat` 结构体
- `Stat` 结构体：`dev`（设备号）、`ino`（inode 号）、`mode`（文件类型）、`nlink`（硬链接数）
- `mode` 对普通文件应为 `StatMode::FILE`

**实现思路**：
1. 从当前任务的 `fd_table` 获取文件对象
2. 从文件对象获取 inode 信息
3. 需要给文件系统的 Inode 添加方法来获取 `ino` 和 `nlink`
4. 用 `translated_refmut` 翻译用户态 `st` 指针，写入结果

#### 2. `sys_linkat(old_name, new_name) -> isize`

```
成功返回 0，失败返回 -1
```

- 为 `old_name` 指向的文件创建一个名为 `new_name` 的硬链接
- 链接创建后，`nlink` 应 +1

**实现思路**：
1. 翻译路径字符串
2. 在 easy-fs 的根目录中找到 `old_name` 的 inode
3. 在根目录中添加一个新的目录项 `new_name`，指向同一个 inode
4. 增加该 inode 的 `nlink` 计数

#### 3. `sys_unlinkat(name) -> isize`

```
成功返回 0，失败返回 -1
```

- 删除 `name` 对应的目录项
- `nlink` 减 1
- 当 `nlink` 降为 0 时，可以释放 inode 和数据块（也可以不释放，测试不强制要求）

### 需要修改 easy-fs

你很可能需要修改 `easy-fs` crate 来支持：
- 获取 inode 编号
- 获取/修改 `nlink` 计数
- 在目录中添加/删除条目

**关键文件**：
- `easy-fs/src/layout.rs` — `DiskInode` 结构体，可能需要添加 `nlink` 字段
- `easy-fs/src/vfs.rs` — `Inode` 虚拟文件系统接口
- `easy-fs/src/efs.rs` — `EasyFileSystem` 核心逻辑

### 通过标志

```
Test fstat OK!
Test link OK!
(ch6_file0: 基础读写测试通过)
(ch6_file3: 批量创建/删除测试通过)
```

---

## 实验五：ch8 — 死锁检测

### 背景

ch8 引入了**线程**和**同步原语**（互斥锁、信号量、条件变量）。线程创建、互斥锁、信号量等基础功能已经实现。你需要在此基础上添加**死锁检测**。

### 已有代码

| 文件 | 内容 |
|------|------|
| `os/src/syscall/sync.rs` | 所有同步原语 syscall 已实现；`sys_enable_deadlock_detect` 是**桩函数** |
| `os/src/sync/` | `Mutex`（spin/blocking）、`Semaphore`、`Condvar` 已实现 |
| `os/src/task/process.rs` | `ProcessControlBlock` 有 `mutex_list`、`semaphore_list` |

### 你要实现的

**核心任务：实现死锁检测算法**

#### 1. `sys_enable_deadlock_detect(enabled) -> isize`

```
enabled=1 开启死锁检测，enabled=0 关闭，成功返回 0
```

- 在进程级别存储一个 `deadlock_detect` 开关

#### 2. 在 `mutex_lock` 中检测死锁

当 `deadlock_detect` 开启时，在 `sys_mutex_lock` **执行 lock 之前**进行死锁检测：

- **测试场景**（`ch8_deadlock_mutex1.rs`）：
  ```rust
  mutex_lock(mid);     // 第一次加锁，成功，返回 0
  mutex_lock(mid);     // 第二次加锁同一个锁 → 检测到死锁，返回 -0xdead
  ```
- 即：如果当前线程已经持有该锁，再次 lock 就是死锁

#### 3. 在 `semaphore_down` 中检测死锁

当 `deadlock_detect` 开启时，在 `sys_semaphore_down` **执行 down 之前**进行死锁检测：

- **测试场景 1**（`ch8_deadlock_sem1.rs`）——应检测到死锁：
  - 3 个线程，3 种资源（信号量 1-3），各自持有一些资源后请求其他资源
  - 形成循环等待 → 至少一个线程的 `semaphore_down` 返回 `-0xdead`

- **测试场景 2**（`ch8_deadlock_sem2.rs`）——不应误报：
  - 4 个线程，2 种资源（各 2 个），资源充足不会死锁
  - 所有线程都应正常完成（`waittid` 返回 0）

### 死锁检测算法

推荐使用**银行家算法**（Banker's Algorithm）的安全性检查：

**数据结构**（进程级别维护）：
- `available[i]`：第 i 种资源的可用数量
- `allocation[t][i]`：线程 t 当前持有的第 i 种资源数量
- `need[t][i]`：线程 t 还需要的第 i 种资源数量（即本次请求）

**检测流程**（在 lock/down 之前）：
1. 假设满足当前线程的请求（试分配）
2. 用银行家算法检查系统是否仍处于安全状态
3. 如果不安全（存在死锁风险），返回 `-0xdead`
4. 如果安全，允许操作继续

**简化版思路**：
- 对于 mutex：只需检查当前线程是否已持有该锁
- 对于 semaphore：维护资源分配图，检测循环等待

### 需要修改的关键位置

1. **`ProcessControlBlockInner`**：添加 `deadlock_detect: bool` 标志，以及资源分配追踪数据结构
2. **`sys_mutex_lock`**：在 `mutex.lock()` 之前加检测逻辑
3. **`sys_semaphore_down`**：在 `sem.down()` 之前加检测逻辑
4. **`sys_mutex_unlock` / `sys_semaphore_up`**：更新资源分配状态

### 关键提示

- `-0xdead` 作为 `isize` 是一个负数（`-57005`），测试中用 `assert_eq!(mutex_lock(mid), -0xdead)` 检查
- 死锁检测只在 `enable_deadlock_detect(true)` 后生效
- 注意不要产生**误报**：`ch8_deadlock_sem2` 测试的就是无死锁场景必须正常通过
- 需要在 lock/unlock 和 down/up 时都更新分配状态，保持数据结构一致

### 通过标志

```
deadlock test mutex 1 OK!
deadlock test semaphore 1 OK!
deadlock test semaphore 2 OK!
```

---

## 总结：各实验难度与建议顺序

| 实验 | 难度 | 核心知识点 | 建议用时 |
|------|------|-----------|---------|
| ch3 | ⭐⭐ | 系统调用机制、任务管理 | 1-2 天 |
| ch4 | ⭐⭐⭐ | 虚拟内存、页表、地址翻译 | 2-3 天 |
| ch5 | ⭐⭐⭐ | 进程管理、调度算法 | 2-3 天 |
| ch6 | ⭐⭐⭐ | 文件系统内部结构 | 2-3 天 |
| ch8 | ⭐⭐⭐⭐ | 线程同步、死锁检测算法 | 3-4 天 |

建议按 ch3 → ch4 → ch5 → ch6 → ch8 的顺序完成。每个实验**先读测试用例**，再读内核代码，最后动手实现。
