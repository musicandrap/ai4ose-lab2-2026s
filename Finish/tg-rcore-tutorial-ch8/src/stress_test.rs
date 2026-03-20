//! 统一的多线程压力测试程序
//! 
//! 循环加锁→执行临界区→解锁，控制线程数和临界区时间

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::{Duration, Instant};
use spin::Mutex;

use crate::instrumented_locks::{InstrumentedSpinlock, InstrumentedMutex, InstrumentedRwLock, LockStats};

/// 压力测试配置
#[derive(Debug, Clone)]
pub struct StressTestConfig {
    pub thread_count: usize,           // 线程数量
    pub operations_per_thread: usize, // 每个线程操作次数
    pub critical_section_time: u64,   // 临界区执行时间（纳秒）
    pub contention_level: f64,        // 竞争程度 0.0-1.0
}

impl StressTestConfig {
    pub const fn new(threads: usize, operations: usize, cs_time: u64, contention: f64) -> Self {
        Self {
            thread_count: threads,
            operations_per_thread: operations,
            critical_section_time: cs_time,
            contention_level: contention,
        }
    }
}

/// 压力测试结果
#[derive(Debug)]
pub struct StressTestResult {
    pub config: StressTestConfig,
    pub total_operations: usize,
    pub successful_operations: usize,
    pub total_time_ns: u64,
    pub throughput: f64,           // 操作/秒
    pub lock_stats: LockStats,
}

impl StressTestResult {
    pub fn new(config: StressTestConfig, lock_stats: LockStats, total_time_ns: u64) -> Self {
        let total_operations = config.thread_count * config.operations_per_thread;
        let successful_operations = lock_stats.lock_success.load(Ordering::Relaxed) as usize;
        let throughput = if total_time_ns > 0 {
            (successful_operations as f64 * 1_000_000_000.0) / total_time_ns as f64
        } else {
            0.0
        };
        
        Self {
            config,
            total_operations,
            successful_operations,
            total_time_ns,
            throughput,
            lock_stats,
        }
    }
    
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            1.0
        } else {
            self.successful_operations as f64 / self.total_operations as f64
        }
    }
}

/// 压力测试运行器
pub struct StressTestRunner {
    shared_counter: AtomicUsize,
}

impl StressTestRunner {
    pub const fn new() -> Self {
        Self {
            shared_counter: AtomicUsize::new(0),
        }
    }
    
    /// 运行自旋锁压力测试
    pub fn run_spinlock_test(&self, config: &StressTestConfig) -> StressTestResult {
        let lock = InstrumentedSpinlock::new(0usize);
        
        let start_time = Instant::now();
        
        // 模拟多线程竞争
        for tid in 0..config.thread_count {
            for _ in 0..config.operations_per_thread {
                let guard = lock.lock(tid);
                
                // 模拟临界区操作
                self.simulate_critical_section(config.critical_section_time);
                
                // 访问共享数据
                self.shared_counter.fetch_add(1, Ordering::Relaxed);
                
                drop(guard); // 自动记录统计信息
            }
        }
        
        let total_time = start_time.elapsed().as_nanos() as u64;
        StressTestResult::new(config.clone(), lock.get_stats().clone(), total_time)
    }
    
    /// 运行互斥锁压力测试
    pub fn run_mutex_test(&self, config: &StressTestConfig) -> StressTestResult {
        let lock = InstrumentedMutex::new(0usize);
        
        let start_time = Instant::now();
        
        for tid in 0..config.thread_count {
            for _ in 0..config.operations_per_thread {
                let guard = lock.lock(tid);
                
                self.simulate_critical_section(config.critical_section_time);
                self.shared_counter.fetch_add(1, Ordering::Relaxed);
                
                drop(guard);
            }
        }
        
        let total_time = start_time.elapsed().as_nanos() as u64;
        StressTestResult::new(config.clone(), lock.get_stats().clone(), total_time)
    }
    
    /// 运行读写锁压力测试（混合读写）
    pub fn run_rwlock_test(&self, config: &StressTestConfig) -> StressTestResult {
        let lock = InstrumentedRwLock::new(0usize);
        
        let start_time = Instant::now();
        
        for tid in 0..config.thread_count {
            for op_index in 0..config.operations_per_thread {
                // 80%读操作，20%写操作
                if op_index % 5 != 0 {
                    // 读操作
                    let guard = lock.read(tid);
                    self.simulate_critical_section(config.critical_section_time / 2); // 读操作更快
                    let _value = *guard.inner;
                    drop(guard);
                } else {
                    // 写操作
                    let mut guard = lock.write(tid);
                    self.simulate_critical_section(config.critical_section_time);
                    *guard.inner += 1;
                    drop(guard);
                }
            }
        }
        
        let total_time = start_time.elapsed().as_nanos() as u64;
        StressTestResult::new(config.clone(), lock.get_stats().clone(), total_time)
    }
    
    /// 模拟不同竞争程度的测试
    pub fn run_contention_test(&self, lock_type: &str, base_config: &StressTestConfig) -> Vec<StressTestResult> {
        let contention_levels = [0.1, 0.3, 0.5, 0.7, 0.9];
        let mut results = Vec::new();
        
        for &contention in &contention_levels {
            let config = StressTestConfig {
                contention_level: contention,
                ..base_config.clone()
            };
            
            let result = match lock_type {
                "spinlock" => self.run_spinlock_test(&config),
                "mutex" => self.run_mutex_test(&config),
                "rwlock" => self.run_rwlock_test(&config),
                _ => panic!("Unknown lock type: {}", lock_type),
            };
            
            results.push(result);
        }
        
        results
    }
    
    /// 模拟临界区执行时间
    fn simulate_critical_section(&self, duration_ns: u64) {
        if duration_ns > 0 {
            let start = Instant::now();
            while start.elapsed().as_nanos() < duration_ns as u128 {
                // 忙等待，模拟计算密集型操作
                core::hint::spin_loop();
            }
        }
    }
}

/// 对比测试管理器
pub struct ComparisonTestManager {
    runner: StressTestRunner,
}

impl ComparisonTestManager {
    pub const fn new() -> Self {
        Self {
            runner: StressTestRunner::new(),
        }
    }
    
    /// 运行完整的对比测试
    pub fn run_comparison_test(&self, base_config: &StressTestConfig) -> ComparisonReport {
        let mut report = ComparisonReport::new();
        
        // 测试不同锁类型
        report.spinlock_results = self.runner.run_spinlock_test(base_config);
        report.mutex_results = self.runner.run_mutex_test(base_config);
        report.rwlock_results = self.runner.run_rwlock_test(base_config);
        
        // 测试不同竞争程度
        report.spinlock_contention = self.runner.run_contention_test("spinlock", base_config);
        report.mutex_contention = self.runner.run_contention_test("mutex", base_config);
        report.rwlock_contention = self.runner.run_contention_test("rwlock", base_config);
        
        report
    }
}

/// 对比测试报告
#[derive(Debug)]
pub struct ComparisonReport {
    pub spinlock_results: StressTestResult,
    pub mutex_results: StressTestResult,
    pub rwlock_results: StressTestResult,
    pub spinlock_contention: Vec<StressTestResult>,
    pub mutex_contention: Vec<StressTestResult>,
    pub rwlock_contention: Vec<StressTestResult>,
}

impl ComparisonReport {
    pub fn new() -> Self {
        Self {
            spinlock_results: StressTestResult::new(
                StressTestConfig::new(0, 0, 0, 0.0), 
                LockStats::new(), 
                0
            ),
            mutex_results: StressTestResult::new(
                StressTestConfig::new(0, 0, 0, 0.0), 
                LockStats::new(), 
                0
            ),
            rwlock_results: StressTestResult::new(
                StressTestConfig::new(0, 0, 0, 0.0), 
                LockStats::new(), 
                0
            ),
            spinlock_contention: Vec::new(),
            mutex_contention: Vec::new(),
            rwlock_contention: Vec::new(),
        }
    }
    
    /// 找出性能最优的锁类型
    pub fn find_best_performer(&self) -> &'static str {
        let results = [
            ("spinlock", self.spinlock_results.throughput),
            ("mutex", self.mutex_results.throughput),
            ("rwlock", self.rwlock_results.throughput),
        ];
        
        results.iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(name, _)| *name)
            .unwrap_or("unknown")
    }
    
    /// 找出最公平的锁类型
    pub fn find_fairest_performer(&self) -> &'static str {
        let results = [
            ("spinlock", self.spinlock_results.lock_stats.fairness_variance()),
            ("mutex", self.mutex_results.lock_stats.fairness_variance()),
            ("rwlock", self.rwlock_results.lock_stats.fairness_variance()),
        ];
        
        results.iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(name, _)| *name)
            .unwrap_or("unknown")
    }
}

/// 测试场景生成器
pub struct TestScenarioGenerator;

impl TestScenarioGenerator {
    /// 生成低竞争场景
    pub fn low_contention_scenario() -> StressTestConfig {
        StressTestConfig::new(4, 1000, 1000, 0.1) // 4线程，低竞争
    }
    
    /// 生成高竞争场景
    pub fn high_contention_scenario() -> StressTestConfig {
        StressTestConfig::new(16, 1000, 5000, 0.8) // 16线程，高竞争
    }
    
    /// 生成读写混合场景
    pub fn read_write_mix_scenario() -> StressTestConfig {
        StressTestConfig::new(8, 2000, 2000, 0.5) // 8线程，中等竞争
    }
    
    /// 生成长临界区场景
    pub fn long_critical_section_scenario() -> StressTestConfig {
        StressTestConfig::new(4, 500, 10000, 0.3) // 长临界区操作
    }
}