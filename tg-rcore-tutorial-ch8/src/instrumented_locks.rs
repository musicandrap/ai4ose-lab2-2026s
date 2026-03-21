//! 带埋点统计的同步原语实现
//! 
//! 在锁内部埋点记录：加锁尝试、等待时间、持锁时间、sleep/wakeup等

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use core::time::{Duration, Instant};
use alloc::vec::Vec;
use spin::{Mutex, RwLock};

/// 锁性能统计数据结构
#[derive(Debug, Default)]
pub struct LockStats {
    // 竞争统计
    pub lock_attempts: AtomicU64,           // 加锁尝试次数
    pub lock_success: AtomicU64,            // 成功获取次数
    pub lock_failures: AtomicU64,           // 获取失败次数
    
    // 时间统计（纳秒）
    pub total_wait_time: AtomicU64,         // 总等待时间
    pub total_hold_time: AtomicU64,         // 总持锁时间
    pub max_wait_time: AtomicU64,           // 最大等待时间
    pub max_hold_time: AtomicU64,           // 最大持锁时间
    
    // 上下文切换统计
    pub sleep_count: AtomicU64,             // sleep次数
    pub wakeup_count: AtomicU64,             // wakeup次数
    pub context_switches: AtomicU64,        // 上下文切换次数
    
    // 公平性统计
    pub thread_stats: Mutex<Vec<ThreadLockStats>>, // 各线程统计
}

/// 线程级别的锁统计
#[derive(Debug, Clone)]
pub struct ThreadLockStats {
    pub tid: usize,
    pub attempts: u64,
    pub successes: u64,
    pub total_wait_time: u64,
    pub total_hold_time: u64,
    pub max_wait_time: u64,
}

impl LockStats {
    pub const fn new() -> Self {
        Self {
            lock_attempts: AtomicU64::new(0),
            lock_success: AtomicU64::new(0),
            lock_failures: AtomicU64::new(0),
            total_wait_time: AtomicU64::new(0),
            total_hold_time: AtomicU64::new(0),
            max_wait_time: AtomicU64::new(0),
            max_hold_time: AtomicU64::new(0),
            sleep_count: AtomicU64::new(0),
            wakeup_count: AtomicU64::new(0),
            context_switches: AtomicU64::new(0),
            thread_stats: Mutex::new(Vec::new()),
        }
    }
    
    /// 记录加锁尝试
    pub fn record_attempt(&self, tid: usize) {
        self.lock_attempts.fetch_add(1, Ordering::Relaxed);
        self.update_thread_stats(tid, |stats| {
            stats.attempts += 1;
        });
    }
    
    /// 记录加锁成功
    pub fn record_success(&self, tid: usize, wait_time: u64, hold_time: u64) {
        self.lock_success.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time.fetch_add(wait_time, Ordering::Relaxed);
        self.total_hold_time.fetch_add(hold_time, Ordering::Relaxed);
        
        // 更新最大等待时间
        self.update_max_time(&self.max_wait_time, wait_time);
        self.update_max_time(&self.max_hold_time, hold_time);
        
        self.update_thread_stats(tid, |stats| {
            stats.successes += 1;
            stats.total_wait_time += wait_time;
            stats.total_hold_time += hold_time;
            if wait_time > stats.max_wait_time {
                stats.max_wait_time = wait_time;
            }
        });
    }
    
    /// 记录加锁失败
    pub fn record_failure(&self, tid: usize) {
        self.lock_failures.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录sleep事件
    pub fn record_sleep(&self) {
        self.sleep_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录wakeup事件
    pub fn record_wakeup(&self) {
        self.wakeup_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录上下文切换
    pub fn record_context_switch(&self) {
        self.context_switches.fetch_add(1, Ordering::Relaxed);
    }
    
    fn update_max_time(&self, atomic: &AtomicU64, new_time: u64) {
        let mut current = atomic.load(Ordering::Relaxed);
        while new_time > current {
            match atomic.compare_exchange(
                current, new_time, Ordering::Relaxed, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(updated) => current = updated,
            }
        }
    }
    
    fn update_thread_stats(&self, tid: usize, update_fn: impl FnOnce(&mut ThreadLockStats)) {
        let mut stats = self.thread_stats.lock();
        
        if let Some(stat) = stats.iter_mut().find(|s| s.tid == tid) {
            update_fn(stat);
        } else {
            let mut new_stat = ThreadLockStats {
                tid,
                attempts: 0,
                successes: 0,
                total_wait_time: 0,
                total_hold_time: 0,
                max_wait_time: 0,
            };
            update_fn(&mut new_stat);
            stats.push(new_stat);
        }
    }
    
    /// 计算竞争率
    pub fn contention_rate(&self) -> f64 {
        let attempts = self.lock_attempts.load(Ordering::Relaxed);
        let failures = self.lock_failures.load(Ordering::Relaxed);
        
        if attempts == 0 { 0.0 } else { failures as f64 / attempts as f64 }
    }
    
    /// 计算平均等待时间（纳秒）
    pub fn avg_wait_time(&self) -> f64 {
        let success = self.lock_success.load(Ordering::Relaxed);
        let total = self.total_wait_time.load(Ordering::Relaxed);
        
        if success == 0 { 0.0 } else { total as f64 / success as f64 }
    }
    
    /// 计算平均持锁时间（纳秒）
    pub fn avg_hold_time(&self) -> f64 {
        let success = self.lock_success.load(Ordering::Relaxed);
        let total = self.total_hold_time.load(Ordering::Relaxed);
        
        if success == 0 { 0.0 } else { total as f64 / success as f64 }
    }
    
    /// 检查是否出现饥饿（starvation）
    pub fn has_starvation(&self, threshold_ns: u64) -> bool {
        let stats = self.thread_stats.lock();
        
        // 如果有线程的最大等待时间超过阈值，认为出现饥饿
        stats.iter().any(|s| s.max_wait_time > threshold_ns)
    }
    
    /// 计算公平性指标（等待时间方差）
    pub fn fairness_variance(&self) -> f64 {
        let stats = self.thread_stats.lock();
        if stats.len() < 2 {
            return 0.0;
        }
        
        let avg_wait: f64 = stats.iter()
            .filter(|s| s.successes > 0)
            .map(|s| s.total_wait_time as f64 / s.successes as f64)
            .sum::<f64>() / stats.len() as f64;
        
        let variance: f64 = stats.iter()
            .filter(|s| s.successes > 0)
            .map(|s| {
                let thread_avg = s.total_wait_time as f64 / s.successes as f64;
                (thread_avg - avg_wait).powi(2)
            })
            .sum::<f64>() / stats.len() as f64;
        
        variance
    }
}

/// 带埋点的自旋锁
pub struct InstrumentedSpinlock<T> {
    inner: spin::Mutex<T>,
    stats: LockStats,
}

impl<T> InstrumentedSpinlock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: spin::Mutex::new(value),
            stats: LockStats::new(),
        }
    }
    
    pub fn lock(&self, tid: usize) -> InstrumentedSpinlockGuard<'_, T> {
        self.stats.record_attempt(tid);
        
        let start_time = Instant::now();
        let guard = self.inner.lock();
        let wait_time = start_time.elapsed().as_nanos() as u64;
        
        InstrumentedSpinlockGuard {
            inner: guard,
            stats: &self.stats,
            tid,
            acquire_time: Instant::now(),
        }
    }
    
    pub fn get_stats(&self) -> &LockStats {
        &self.stats
    }
}

pub struct InstrumentedSpinlockGuard<'a, T> {
    inner: spin::MutexGuard<'a, T>,
    stats: &'a LockStats,
    tid: usize,
    acquire_time: Instant,
}

impl<'a, T> Drop for InstrumentedSpinlockGuard<'a, T> {
    fn drop(&mut self) {
        let hold_time = self.acquire_time.elapsed().as_nanos() as u64;
        self.stats.record_success(self.tid, 0, hold_time); // 自旋锁等待时间为0
    }
}

/// 带埋点的互斥锁（睡眠锁）
pub struct InstrumentedMutex<T> {
    inner: Mutex<T>,
    stats: LockStats,
}

impl<T> InstrumentedMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
            stats: LockStats::new(),
        }
    }
    
    pub fn lock(&self, tid: usize) -> InstrumentedMutexGuard<'_, T> {
        self.stats.record_attempt(tid);
        self.stats.record_sleep(); // 睡眠锁会sleep
        
        let start_time = Instant::now();
        let guard = self.inner.lock();
        let wait_time = start_time.elapsed().as_nanos() as u64;
        
        self.stats.record_wakeup(); // 被唤醒
        self.stats.record_context_switch(); // 涉及上下文切换
        
        InstrumentedMutexGuard {
            inner: guard,
            stats: &self.stats,
            tid,
            acquire_time: Instant::now(),
            wait_time,
        }
    }
    
    pub fn get_stats(&self) -> &LockStats {
        &self.stats
    }
}

pub struct InstrumentedMutexGuard<'a, T> {
    inner: spin::MutexGuard<'a, T>,
    stats: &'a LockStats,
    tid: usize,
    acquire_time: Instant,
    wait_time: u64,
}

impl<'a, T> Drop for InstrumentedMutexGuard<'a, T> {
    fn drop(&mut self) {
        let hold_time = self.acquire_time.elapsed().as_nanos() as u64;
        self.stats.record_success(self.tid, self.wait_time, hold_time);
    }
}

/// 带埋点的读写锁
pub struct InstrumentedRwLock<T> {
    inner: RwLock<T>,
    stats: LockStats,
}

impl<T> InstrumentedRwLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: RwLock::new(value),
            stats: LockStats::new(),
        }
    }
    
    pub fn read(&self, tid: usize) -> InstrumentedRwLockReadGuard<'_, T> {
        self.stats.record_attempt(tid);
        
        let start_time = Instant::now();
        let guard = self.inner.read();
        let wait_time = start_time.elapsed().as_nanos() as u64;
        
        InstrumentedRwLockReadGuard {
            inner: guard,
            stats: &self.stats,
            tid,
            acquire_time: Instant::now(),
            wait_time,
        }
    }
    
    pub fn write(&self, tid: usize) -> InstrumentedRwLockWriteGuard<'_, T> {
        self.stats.record_attempt(tid);
        
        let start_time = Instant::now();
        let guard = self.inner.write();
        let wait_time = start_time.elapsed().as_nanos() as u64;
        
        InstrumentedRwLockWriteGuard {
            inner: guard,
            stats: &self.stats,
            tid,
            acquire_time: Instant::now(),
            wait_time,
        }
    }
    
    pub fn get_stats(&self) -> &LockStats {
        &self.stats
    }
}

pub struct InstrumentedRwLockReadGuard<'a, T> {
    inner: spin::RwLockReadGuard<'a, T>,
    stats: &'a LockStats,
    tid: usize,
    acquire_time: Instant,
    wait_time: u64,
}

impl<'a, T> Drop for InstrumentedRwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        let hold_time = self.acquire_time.elapsed().as_nanos() as u64;
        self.stats.record_success(self.tid, self.wait_time, hold_time);
    }
}

pub struct InstrumentedRwLockWriteGuard<'a, T> {
    inner: spin::RwLockWriteGuard<'a, T>,
    stats: &'a LockStats,
    tid: usize,
    acquire_time: Instant,
    wait_time: u64,
}

impl<'a, T> Drop for InstrumentedRwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        let hold_time = self.acquire_time.elapsed().as_nanos() as u64;
        self.stats.record_success(self.tid, self.wait_time, hold_time);
    }
}