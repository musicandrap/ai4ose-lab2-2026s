//! 同步原语可量化对比实验
//! 
//! 实现多线程压力测试，收集锁性能数据，输出CSV格式结果

#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{exit, thread_create, waittid, sleep};
use core::sync::atomic::{AtomicUsize, Ordering};

/// 实验配置
struct ExperimentConfig {
    thread_count: usize,
    operations_per_thread: usize,
    critical_section_time: u64, // 毫秒
}

/// 共享计数器（用于测试锁保护）
static SHARED_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 简单的自旋锁实现（用于对比）
struct SimpleSpinlock {
    locked: AtomicUsize,
}

impl SimpleSpinlock {
    const fn new() -> Self {
        Self {
            locked: AtomicUsize::new(0),
        }
    }
    
    fn lock(&self) {
        while self.locked.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // 自旋等待
        }
    }
    
    fn unlock(&self) {
        self.locked.store(0, Ordering::Release);
    }
}

/// 简单的互斥锁实现（睡眠锁）
struct SimpleMutex {
    locked: AtomicUsize,
}

impl SimpleMutex {
    const fn new() -> Self {
        Self {
            locked: AtomicUsize::new(0),
        }
    }
    
    fn lock(&self) {
        while self.locked.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // 睡眠等待（模拟）
            sleep(1); // 睡眠1毫秒
        }
    }
    
    fn unlock(&self) {
        self.locked.store(0, Ordering::Release);
    }
}

/// 测试自旋锁性能
fn test_spinlock(config: &ExperimentConfig) -> (u64, u64) {
    let lock = SimpleSpinlock::new();
    let start_time = user_lib::get_time();
    
    // 创建测试线程
    let mut threads = [0; 16]; // 最多16个线程
    
    for i in 0..config.thread_count {
        threads[i] = thread_create(spinlock_worker as usize, &lock as *const _ as usize);
    }
    
    // 等待所有线程完成
    for i in 0..config.thread_count {
        waittid(threads[i] as usize);
    }
    
    let end_time = user_lib::get_time();
    let total_time = end_time - start_time;
    let final_count = SHARED_COUNTER.load(Ordering::Relaxed);
    
    (total_time as u64, final_count as u64)
}

/// 自旋锁工作线程
fn spinlock_worker(lock_ptr: usize) -> isize {
    let lock = unsafe { &*(lock_ptr as *const SimpleSpinlock) };
    
    for _ in 0..1000 { // 每个线程1000次操作
        lock.lock();
        
        // 临界区操作
        SHARED_COUNTER.fetch_add(1, Ordering::Relaxed);
        
        lock.unlock();
    }
    
    exit(0)
}

/// 测试互斥锁性能
fn test_mutex(config: &ExperimentConfig) -> (u64, u64) {
    let lock = SimpleMutex::new();
    let start_time = user_lib::get_time();
    
    let mut threads = [0; 16];
    
    for i in 0..config.thread_count {
        threads[i] = thread_create(mutex_worker as usize, &lock as *const _ as usize);
    }
    
    for i in 0..config.thread_count {
        waittid(threads[i] as usize);
    }
    
    let end_time = user_lib::get_time();
    let total_time = end_time - start_time;
    let final_count = SHARED_COUNTER.load(Ordering::Relaxed);
    
    (total_time as u64, final_count as u64)
}

/// 互斥锁工作线程
fn mutex_worker(lock_ptr: usize) -> isize {
    let lock = unsafe { &*(lock_ptr as *const SimpleMutex) };
    
    for _ in 0..1000 {
        lock.lock();
        
        SHARED_COUNTER.fetch_add(1, Ordering::Relaxed);
        
        lock.unlock();
    }
    
    exit(0)
}

/// 输出CSV格式的实验结果
fn output_csv_results(lock_type: &str, threads: usize, total_time: u64, operations: u64) {
    let throughput = if total_time > 0 {
        (operations as f64 * 1000.0) / total_time as f64 // 操作/秒
    } else {
        0.0
    };
    
    // CSV格式输出：锁类型,线程数,总时间(ms),总操作数,吞吐量(ops/s)
    println!("EXPERIMENT_CSV,{},{},{},{},{:.2}", 
             lock_type, threads, total_time, operations, throughput);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main() -> i32 {
    println!("=== 同步原语可量化对比实验开始 ===");
    
    // 输出CSV表头
    println!("EXPERIMENT_CSV_HEADER,lock_type,thread_count,total_time_ms,total_operations,throughput_ops_per_sec");
    
    // 测试不同线程数下的性能
    for thread_count in [2, 4, 8].iter() {
        let config = ExperimentConfig {
            thread_count: *thread_count,
            operations_per_thread: 1000,
            critical_section_time: 1,
        };
        
        println!("\n测试 {} 线程场景:", thread_count);
        
        // 重置计数器
        SHARED_COUNTER.store(0, Ordering::Relaxed);
        
        // 测试自旋锁
        let (spin_time, spin_ops) = test_spinlock(&config);
        println!("自旋锁: 时间={}ms, 操作数={}", spin_time, spin_ops);
        output_csv_results("spinlock", *thread_count, spin_time, spin_ops);
        
        // 重置计数器
        SHARED_COUNTER.store(0, Ordering::Relaxed);
        
        // 测试互斥锁
        let (mutex_time, mutex_ops) = test_mutex(&config);
        println!("互斥锁: 时间={}ms, 操作数={}", mutex_time, mutex_ops);
        output_csv_results("mutex", *thread_count, mutex_time, mutex_ops);
        
        // 计算性能提升百分比
        if mutex_time > 0 && spin_time > 0 {
            let improvement = ((mutex_time as f64 - spin_time as f64) / mutex_time as f64) * 100.0;
            println!("自旋锁相比互斥锁性能提升: {:.1}%", improvement);
        }
    }
    
    println!("\n=== 实验完成 ===");
    println!("数据格式说明:");
    println!("- EXPERIMENT_CSV_HEADER: CSV表头");
    println!("- EXPERIMENT_CSV: 实验数据行");
    println!("- 可以通过重定向输出到文件进行数据分析");
    
    exit(0) as i32
}