//! 同步原语性能监控和公平性验证框架
//! 
//! 从"能跑"升级到"公平/可证明不饿死"的实验框架

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use alloc::vec::Vec;
use spin::Mutex;

/// 锁性能统计
#[derive(Debug, Default)]
pub struct LockMetrics {
    // 竞争统计
    pub acquire_attempts: AtomicU64,
    pub acquire_success: AtomicU64,
    pub acquire_failures: AtomicU64,
    
    // 时间统计
    pub total_wait_time: AtomicU64,  // 总等待时间（时钟周期）
    pub max_wait_time: AtomicU64,    // 最大等待时间
    
    // 公平性统计
    pub thread_waits: Mutex<Vec<ThreadWaitStats>>,  // 各线程等待统计
}

/// 线程等待统计
#[derive(Debug, Clone)]
pub struct ThreadWaitStats {
    pub tid: usize,
    pub wait_count: u64,
    pub total_wait_time: u64,
    pub max_wait_time: u64,
}

impl LockMetrics {
    pub const fn new() -> Self {
        Self {
            acquire_attempts: AtomicU64::new(0),
            acquire_success: AtomicU64::new(0),
            acquire_failures: AtomicU64::new(0),
            total_wait_time: AtomicU64::new(0),
            max_wait_time: AtomicU64::new(0),
            thread_waits: Mutex::new(Vec::new()),
        }
    }
    
    /// 记录获取尝试
    pub fn record_acquire_attempt(&self) {
        self.acquire_attempts.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录获取成功
    pub fn record_acquire_success(&self, wait_time: u64) {
        self.acquire_success.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time.fetch_add(wait_time, Ordering::Relaxed);
        
        // 更新最大等待时间
        let mut max = self.max_wait_time.load(Ordering::Relaxed);
        while wait_time > max {
            match self.max_wait_time.compare_exchange(
                max, wait_time, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(current) => max = current,
            }
        }
    }
    
    /// 记录获取失败
    pub fn record_acquire_failure(&self) {
        self.acquire_failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录线程等待统计
    pub fn record_thread_wait(&self, tid: usize, wait_time: u64) {
        let mut stats = self.thread_waits.lock();
        
        // 查找或创建线程统计
        if let Some(stat) = stats.iter_mut().find(|s| s.tid == tid) {
            stat.wait_count += 1;
            stat.total_wait_time += wait_time;
            if wait_time > stat.max_wait_time {
                stat.max_wait_time = wait_time;
            }
        } else {
            stats.push(ThreadWaitStats {
                tid,
                wait_count: 1,
                total_wait_time: wait_time,
                max_wait_time: wait_time,
            });
        }
    }
    
    /// 获取竞争率
    pub fn contention_rate(&self) -> f64 {
        let attempts = self.acquire_attempts.load(Ordering::Relaxed);
        let failures = self.acquire_failures.load(Ordering::Relaxed);
        
        if attempts == 0 {
            0.0
        } else {
            failures as f64 / attempts as f64
        }
    }
    
    /// 获取平均等待时间
    pub fn avg_wait_time(&self) -> f64 {
        let success = self.acquire_success.load(Ordering::Relaxed);
        let total = self.total_wait_time.load(Ordering::Relaxed);
        
        if success == 0 {
            0.0
        } else {
            total as f64 / success as f64
        }
    }
    
    /// 检查公平性（最大等待时间是否可控）
    pub fn fairness_check(&self, max_allowed_wait: u64) -> bool {
        let max_wait = self.max_wait_time.load(Ordering::Relaxed);
        max_wait <= max_allowed_wait
    }
    
    /// 检查是否出现饥饿（starvation）
    pub fn starvation_check(&self, threshold: u64) -> bool {
        let stats = self.thread_waits.lock();
        
        // 如果有线程等待时间超过阈值，认为出现饥饿
        stats.iter().any(|s| s.max_wait_time > threshold)
    }
    
    /// 获取公平性指标（等待时间方差）
    pub fn fairness_variance(&self) -> f64 {
        let stats = self.thread_waits.lock();
        if stats.len() < 2 {
            return 0.0;
        }
        
        let avg_wait: f64 = stats.iter()
            .map(|s| s.total_wait_time as f64 / s.wait_count as f64)
            .sum::<f64>() / stats.len() as f64;
        
        let variance: f64 = stats.iter()
            .map(|s| {
                let thread_avg = s.total_wait_time as f64 / s.wait_count as f64;
                (thread_avg - avg_wait).powi(2)
            })
            .sum::<f64>() / stats.len() as f64;
        
        variance
    }
}

/// 上下文切换统计
#[derive(Debug, Default)]
pub struct ContextSwitchMetrics {
    pub spinlock_switches: AtomicU64,  // 自旋锁导致的切换
    pub sleeplock_switches: AtomicU64, // 睡眠锁导致的切换
    pub voluntary_switches: AtomicU64, // 自愿切换
}

impl ContextSwitchMetrics {
    pub const fn new() -> Self {
        Self {
            spinlock_switches: AtomicU64::new(0),
            sleeplock_switches: AtomicU64::new(0),
            voluntary_switches: AtomicU64::new(0),
        }
    }
    
    /// 比较自旋锁 vs 睡眠锁的切换开销
    pub fn compare_overhead(&self) -> (f64, f64) {
        let spin = self.spinlock_switches.load(Ordering::Relaxed) as f64;
        let sleep = self.sleeplock_switches.load(Ordering::Relaxed) as f64;
        
        let total = spin + sleep;
        if total == 0.0 {
            (0.0, 0.0)
        } else {
            (spin / total, sleep / total)
        }
    }
}

/// 全局性能监控器
pub struct SyncMonitor {
    pub spinlock_metrics: LockMetrics,
    pub mutex_metrics: LockMetrics,
    pub semaphore_metrics: LockMetrics,
    pub context_metrics: ContextSwitchMetrics,
}

impl SyncMonitor {
    pub const fn new() -> Self {
        Self {
            spinlock_metrics: LockMetrics::new(),
            mutex_metrics: LockMetrics::new(),
            semaphore_metrics: LockMetrics::new(),
            context_metrics: ContextSwitchMetrics::new(),
        }
    }
    
    /// 生成性能报告
    pub fn generate_report(&self) -> SyncPerformanceReport {
        SyncPerformanceReport {
            spinlock_contention: self.spinlock_metrics.contention_rate(),
            mutex_contention: self.mutex_metrics.contention_rate(),
            semaphore_contention: self.semaphore_metrics.contention_rate(),
            
            spinlock_avg_wait: self.spinlock_metrics.avg_wait_time(),
            mutex_avg_wait: self.mutex_metrics.avg_wait_time(),
            semaphore_avg_wait: self.semaphore_metrics.avg_wait_time(),
            
            spinlock_fairness: self.spinlock_metrics.fairness_variance(),
            mutex_fairness: self.mutex_metrics.fairness_variance(),
            semaphore_fairness: self.semaphore_metrics.fairness_variance(),
            
            context_switch_ratio: self.context_metrics.compare_overhead(),
        }
    }
}

/// 性能报告
#[derive(Debug)]
pub struct SyncPerformanceReport {
    pub spinlock_contention: f64,
    pub mutex_contention: f64,
    pub semaphore_contention: f64,
    
    pub spinlock_avg_wait: f64,
    pub mutex_avg_wait: f64,
    pub semaphore_avg_wait: f64,
    
    pub spinlock_fairness: f64,
    pub mutex_fairness: f64,
    pub semaphore_fairness: f64,
    
    pub context_switch_ratio: (f64, f64),
}

impl SyncPerformanceReport {
    /// 验证公平性（可证明不饿死）
    pub fn verify_fairness(&self, max_variance: f64) -> bool {
        self.spinlock_fairness <= max_variance &&
        self.mutex_fairness <= max_variance &&
        self.semaphore_fairness <= max_variance
    }
    
    /// 验证性能边界
    pub fn verify_performance_bounds(&self, max_contention: f64, max_wait: f64) -> bool {
        self.spinlock_contention <= max_contention &&
        self.mutex_contention <= max_contention &&
        self.semaphore_contention <= max_contention &&
        self.spinlock_avg_wait <= max_wait &&
        self.mutex_avg_wait <= max_wait &&
        self.semaphore_avg_wait <= max_wait
    }
}

// 全局监控器实例
pub static SYNC_MONITOR: SyncMonitor = SyncMonitor::new();