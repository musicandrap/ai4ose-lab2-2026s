//! 进程与线程管理模块
//!
//! ## 与第七章的区别
//!
//! 第七章中 `Process` 既是资源容器又是执行单元。
//! 第八章将两者分离：
//! - **Process**：资源容器，管理地址空间、文件描述符、**同步原语列表**、信号
//! - **Thread**：执行单元，管理 TID 和上下文
//!
//! 同一进程的所有线程共享 `Process` 中的资源。
//!
//! ## 新增字段
//!
//! | 字段 | 说明 |
//! |------|------|
//! | `semaphore_list` | 信号量列表（进程内所有线程共享） |
//! | `mutex_list` | 互斥锁列表 |
//! | `condvar_list` | 条件变量列表 |
//!
//! 教程阅读建议：
//!
//! - 先看 `Process` 与 `Thread` 的字段分工：明确“资源归进程、执行归线程”；
//! - 再看 `fork/exec/from_elf`：理解跨线程模型后，进程复制与替换语义如何变化；
//! - 最后结合 `processor.rs` 看线程生命周期与进程资源回收的关系。

use crate::{
    build_flags, fs::Fd, map_portal, parse_flags, processor::ProcessorInner, Sv39, Sv39Manager,
    PROCESSOR,
};
use alloc::{alloc::alloc_zeroed, boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use core::alloc::Layout;
use spin::Mutex;
use tg_kernel_context::{foreign::ForeignContext, LocalContext};
use tg_kernel_vm::{
    page_table::{MmuMeta, VAddr, PPN, VPN},
    AddressSpace,
};
use tg_signal::Signal;
use tg_signal_impl::SignalImpl;
use tg_sync::{Condvar, Mutex as MutexTrait, Semaphore};
use tg_task_manage::{ProcId, ThreadId};
use xmas_elf::{
    header::{self, HeaderPt2, Machine},
    program, ElfFile,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resource {
    Mutex(usize),
    Semaphore(usize),
}

/// 线程（执行单元）
///
/// 每个线程有独立的 TID 和上下文（寄存器状态、satp）。
/// 同一进程的多个线程共享地址空间。
pub struct Thread {
    /// 线程 ID（不可变）
    pub tid: ThreadId,
    /// 执行上下文（包含 LocalContext + satp）
    pub context: ForeignContext,
}

impl Thread {
    /// 创建新线程
    pub fn new(satp: usize, context: LocalContext) -> Self {
        Self {
            tid: ThreadId::new(),
            context: ForeignContext { context, satp },
        }
    }
}

/// 进程（资源容器）
///
/// 管理地址空间、文件描述符、同步原语、信号等共享资源。
/// 一个进程可以包含多个线程。
pub struct Process {
    /// 进程 ID
    pub pid: ProcId,
    /// 地址空间（所有线程共享）
    pub address_space: AddressSpace<Sv39, Sv39Manager>,
    /// 文件描述符表（所有线程共享）
    pub fd_table: Vec<Option<Mutex<Fd>>>,
    /// 信号处理器
    pub signal: Box<dyn Signal>,
    /// 信号量列表（**本章新增**，所有线程共享）
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// 互斥锁列表（**本章新增**，所有线程共享）
    pub mutex_list: Vec<Option<Arc<dyn MutexTrait>>>,
    /// 条件变量列表（**本章新增**，所有线程共享）
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    /// 死锁检测开关
    pub deadlock_detect_enabled: bool,
    /// 互斥锁持有者记录（对应银行家算法的 Allocation 矩阵的一部分）
    /// 记录每个互斥锁当前被哪个线程持有（None 表示未被持有）
    pub mutex_owners: Vec<Option<ThreadId>>,
    /// 信号量持有者记录（对应银行家算法的 Allocation 矩阵的一部分）
    /// 记录每个信号量被各线程持有的数量：Map<ThreadId, count>
    pub sem_owners: Vec<BTreeMap<ThreadId, usize>>,
    /// 信号量可用资源记录（对应银行家算法的 Available 向量）
    /// 记录每个信号量当前剩余的可用数量
    pub sem_avail: Vec<usize>,
    /// 线程等待资源记录（对应银行家算法的 Need 矩阵的一部分）
    /// 记录每个线程当前正在等待哪个资源（Mutex 或 Semaphore）
    /// 注意：在我们的模型中，线程一次只能阻塞在一个资源上，所以 Need 简化为“正在等待的那个资源”
    pub thread_waiting_for: BTreeMap<ThreadId, Resource>,
}

impl Process {
    /// 银行家算法/死锁检测核心逻辑
    ///
    /// # 参数
    /// - `current_tid`: 当前发起资源请求的线程 ID
    /// - `request`: 请求的资源（Mutex 或 Semaphore）
    ///
    /// # 返回值
    /// - `true`: 安全，允许分配（不会导致死锁）
    /// - `false`: 不安全，拒绝分配（可能会导致死锁）
    pub fn check_deadlock(&self, current_tid: ThreadId, request: Resource) -> bool {
        if !self.deadlock_detect_enabled {
            return true;
        }

        // 0. 收集所有相关的线程（包括当前线程、持有资源的线程、正在等待资源的线程）
        // 我们只关心通过资源依赖关系能联系到的线程集合
        let mut threads = BTreeMap::new(); // 使用 BTreeMap 作为 Set 去重
        threads.insert(current_tid, ());
        for owner in &self.mutex_owners {
            if let Some(tid) = owner {
                threads.insert(*tid, ());
            }
        }
        for map in &self.sem_owners {
            for tid in map.keys() {
                threads.insert(*tid, ());
            }
        }
        for tid in self.thread_waiting_for.keys() {
            threads.insert(*tid, ());
        }
        let threads: Vec<ThreadId> = threads.keys().cloned().collect();

        match request {
            Resource::Mutex(mutex_id) => {
                // === 针对互斥锁的银行家算法 ===
                
                // 1. 初始化 Work 向量（模拟当前系统可用的资源）
                // 对于 Mutex，如果 owners[i] 为 None，则 Available[i] = 1 (true)，否则为 0 (false)
                let mut work: Vec<bool> = self.mutex_owners.iter().map(|o| o.is_none()).collect();
                
                // 2. 初始化 Finish 向量（标记线程是否能完成执行）
                // 初始状态所有线程都未完成
                let mut finish: BTreeMap<ThreadId, bool> = threads.iter().map(|t| (*t, false)).collect();

                // 3. 循环寻找“安全序列”
                // 只要能找到一个线程，它的需求（Need）能被当前 Work 满足，就假设它能执行完并释放资源
                loop {
                    let mut found = false;
                    for &tid in &threads {
                        if !finish[&tid] {
                            // 检查条件：Need[tid] <= Work
                            // 我们需要判断线程 tid 需要的资源是否可用
                            
                            let mut need_met = true;
                            
                            if tid == current_tid {
                                // 特殊情况：当前线程正在请求 mutex_id
                                // 它的 Need 包含 mutex_id
                                if !work[mutex_id] {
                                    need_met = false;
                                }
                            } else if let Some(Resource::Mutex(req_id)) = self.thread_waiting_for.get(&tid) {
                                // 其他线程：如果它正在等待某个 Mutex，检查该 Mutex 是否在 Work 中可用
                                if !work[*req_id] {
                                    need_met = false;
                                }
                            }
                            // 如果线程正在等待 Semaphore，我们在互斥锁检测中暂时忽略（简化处理），
                            // 或者认为它被卡在信号量上，无法释放 Mutex。
                            // 这里我们采用简化的假设：如果它没在等 Mutex，就认为它在 Mutex 资源上没有未满足的需求。

                            if need_met {
                                // 模拟分配与回收：
                                // 假设该线程能获取资源 -> 执行 -> 结束 -> 释放它持有的所有资源
                                // Work += Allocation[tid]
                                for (mid, owner) in self.mutex_owners.iter().enumerate() {
                                    if let Some(owner_tid) = owner {
                                        if *owner_tid == tid {
                                            work[mid] = true; // 释放它持有的 Mutex
                                        }
                                    }
                                }
                                finish.insert(tid, true); // 标记该线程可完成
                                found = true;
                            }
                        }
                    }
                    // 如果一轮循环下来没有找到任何可执行的线程，说明陷入死局，跳出
                    if !found { break; }
                }

                // 4. 结论：如果当前请求资源的线程无法被标记为 Finish，说明它无法获得资源并完成，系统不安全
                *finish.get(&current_tid).unwrap_or(&false)
            }
            Resource::Semaphore(sem_id) => {
                // === 针对信号量的银行家算法 ===
                
                // 1. 初始化 Work = Available (当前可用的信号量计数)
                let mut work = self.sem_avail.clone();
                
                // 2. 初始化 Finish = false
                let mut finish: BTreeMap<ThreadId, bool> = threads.iter().map(|t| (*t, false)).collect();

                // 3. 寻找安全序列
                loop {
                    let mut found = false;
                    for &tid in &threads {
                        if !finish[&tid] {
                            // 检查 Need <= Work
                            let mut need_met = true;
                            
                            if tid == current_tid {
                                // 当前线程请求 sem_id，需要至少 1 个可用资源
                                if work[sem_id] < 1 { need_met = false; }
                            } else if let Some(Resource::Semaphore(req_id)) = self.thread_waiting_for.get(&tid) {
                                // 其他线程等待 req_id，需要至少 1 个可用资源
                                if work[*req_id] < 1 { need_met = false; }
                            }
                            
                            if need_met {
                                // 模拟回收：Work += Allocation[tid]
                                // 释放该线程持有的所有信号量资源
                                for (sid, map) in self.sem_owners.iter().enumerate() {
                                    if let Some(&count) = map.get(&tid) {
                                        work[sid] += count;
                                    }
                                }
                                finish.insert(tid, true);
                                found = true;
                            }
                        }
                    }
                    if !found { break; }
                }
                
                // 4. 结论
                *finish.get(&current_tid).unwrap_or(&false)
            }
        }
    }

    /// exec：替换当前进程的地址空间和主线程上下文
    ///
    /// 注意：只支持单线程进程执行 exec
    pub fn exec(&mut self, elf: ElfFile) {
        let (proc, thread) = Process::from_elf(elf).unwrap();
        self.address_space = proc.address_space;
        let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
        unsafe {
            let pthreads = (*processor).get_thread(self.pid).unwrap();
            (*processor).get_task(pthreads[0]).unwrap().context = thread.context;
        }
    }

    /// fork：创建子进程（复制地址空间和主线程上下文）
    ///
    /// 子进程继承父进程的地址空间（深拷贝）、文件描述符和信号配置。
    /// 同步原语列表不继承（子进程创建空的列表）。
    pub fn fork(&mut self) -> Option<(Self, Thread)> {
        let pid = ProcId::new();
        // 深拷贝地址空间
        let parent_addr_space = &self.address_space;
        let mut address_space: AddressSpace<Sv39, Sv39Manager> = AddressSpace::new();
        parent_addr_space.cloneself(&mut address_space);
        map_portal(&address_space);
        // 复制主线程上下文
        let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
        let pthreads = unsafe { (*processor).get_thread(self.pid).unwrap() };
        let context = unsafe {
            (*processor).get_task(pthreads[0]).unwrap().context.context.clone()
        };
        let satp = (8 << 60) | address_space.root_ppn().val();
        let thread = Thread::new(satp, context);
        // 复制文件描述符表
        let new_fd_table: Vec<Option<Mutex<Fd>>> = self.fd_table
            .iter()
            .map(|fd| fd.as_ref().map(|f| Mutex::new(f.lock().clone())))
            .collect();
        Some((
            Self {
                pid,
                address_space,
                fd_table: new_fd_table,
                signal: self.signal.from_fork(),
                // 子进程的同步原语列表初始为空
                semaphore_list: Vec::new(),
                mutex_list: Vec::new(),
                condvar_list: Vec::new(),
                deadlock_detect_enabled: self.deadlock_detect_enabled,
                mutex_owners: Vec::new(),
                sem_owners: Vec::new(),
                sem_avail: Vec::new(),
                thread_waiting_for: BTreeMap::new(),
            },
            thread,
        ))
    }

    /// 从 ELF 文件创建进程和主线程
    ///
    /// 解析 ELF 段，建立地址空间，分配用户栈，创建初始上下文。
    pub fn from_elf(elf: ElfFile) -> Option<(Self, Thread)> {
        let entry = match elf.header.pt2 {
            HeaderPt2::Header64(pt2)
                if pt2.type_.as_type() == header::Type::Executable
                    && pt2.machine.as_machine() == Machine::RISC_V =>
            { pt2.entry_point as usize }
            _ => None?,
        };

        const PAGE_SIZE: usize = 1 << Sv39::PAGE_BITS;
        const PAGE_MASK: usize = PAGE_SIZE - 1;

        let mut address_space = AddressSpace::new();
        for program in elf.program_iter() {
            if !matches!(program.get_type(), Ok(program::Type::Load)) { continue; }
            let off_file = program.offset() as usize;
            let len_file = program.file_size() as usize;
            let off_mem = program.virtual_addr() as usize;
            let end_mem = off_mem + program.mem_size() as usize;
            assert_eq!(off_file & PAGE_MASK, off_mem & PAGE_MASK);
            let mut flags: [u8; 5] = *b"U___V";
            if program.flags().is_execute() { flags[1] = b'X'; }
            if program.flags().is_write() { flags[2] = b'W'; }
            if program.flags().is_read() { flags[3] = b'R'; }
            address_space.map(
                VAddr::new(off_mem).floor()..VAddr::new(end_mem).ceil(),
                &elf.input[off_file..][..len_file],
                off_mem & PAGE_MASK,
                parse_flags(unsafe { core::str::from_utf8_unchecked(&flags) }).unwrap(),
            );
        }
        // 分配 2 页用户栈
        let stack = unsafe {
            alloc_zeroed(Layout::from_size_align_unchecked(
                2 << Sv39::PAGE_BITS, 1 << Sv39::PAGE_BITS,
            ))
        };
        address_space.map_extern(
            VPN::new((1 << 26) - 2)..VPN::new(1 << 26),
            PPN::new(stack as usize >> Sv39::PAGE_BITS),
            build_flags("U_WRV"),
        );
        map_portal(&address_space);
        let satp = (8 << 60) | address_space.root_ppn().val();
        let mut context = LocalContext::user(entry);
        *context.sp_mut() = 1 << 38;
        let thread = Thread::new(satp, context);

        Some((
            Self {
                pid: ProcId::new(),
                address_space,
                fd_table: vec![
                    // stdin
                    Some(Mutex::new(Fd::Empty { read: true, write: false })),
                    // stdout
                    Some(Mutex::new(Fd::Empty { read: false, write: true })),
                    // stderr
                    Some(Mutex::new(Fd::Empty { read: false, write: true })),
                ],
                signal: Box::new(SignalImpl::new()),
                semaphore_list: Vec::new(),
                mutex_list: Vec::new(),
                condvar_list: Vec::new(),
                deadlock_detect_enabled: false,
                mutex_owners: Vec::new(),
                sem_owners: Vec::new(),
                sem_avail: Vec::new(),
                thread_waiting_for: BTreeMap::new(),
            },
            thread,
        ))
    }
}
