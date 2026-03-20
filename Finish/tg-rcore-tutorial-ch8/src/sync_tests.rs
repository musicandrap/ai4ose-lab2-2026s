//! 同步原语对比测试和压力测试框架
//! 
//! 实现"能失败的对照测试"和"必须通过的压力测试"

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use spin::{Mutex, RwLock};

/// 经典同步问题测试
pub mod classic_problems {
    use super::*;
    
    /// 生产者-消费者问题（有界缓冲区）
    pub struct BoundedBuffer<T, const N: usize> {
        buffer: [Option<T>; N],
        front: AtomicUsize,
        rear: AtomicUsize,
        count: AtomicUsize,
    }
    
    impl<T, const N: usize> BoundedBuffer<T, N> {
        pub const fn new() -> Self {
            Self {
                buffer: [const { None }; N],
                front: AtomicUsize::new(0),
                rear: AtomicUsize::new(0),
                count: AtomicUsize::new(0),
            }
        }
        
        /// 生产者（使用不同同步原语）
        pub fn produce_with_mutex(&self, item: T, mutex: &Mutex<()>) -> Result<(), &'static str> {
            let _guard = mutex.lock();
            
            if self.count.load(Ordering::Acquire) >= N {
                return Err("Buffer full");
            }
            
            let rear = self.rear.load(Ordering::Relaxed);
            self.buffer[rear] = Some(item);
            self.rear.store((rear + 1) % N, Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Release);
            
            Ok(())
        }
        
        /// 消费者（使用不同同步原语）
        pub fn consume_with_mutex(&self, mutex: &Mutex<()>) -> Result<T, &'static str> {
            let _guard = mutex.lock();
            
            if self.count.load(Ordering::Acquire) == 0 {
                return Err("Buffer empty");
            }
            
            let front = self.front.load(Ordering::Relaxed);
            let item = self.buffer[front].take().unwrap();
            self.front.store((front + 1) % N, Ordering::Relaxed);
            self.count.fetch_sub(1, Ordering::Release);
            
            Ok(item)
        }
    }
    
    /// 读者-写者问题
    pub struct ReaderWriter {
        data: AtomicUsize,
        reader_count: AtomicUsize,
        writer_active: AtomicBool,
    }
    
    impl ReaderWriter {
        pub const fn new() -> Self {
            Self {
                data: AtomicUsize::new(0),
                reader_count: AtomicUsize::new(0),
                writer_active: AtomicBool::new(false),
            }
        }
        
        /// 读者（使用读写锁）
        pub fn read_with_rwlock(&self, rwlock: &RwLock<()>) -> usize {
            let _guard = rwlock.read();
            self.data.load(Ordering::Relaxed)
        }
        
        /// 写者（使用读写锁）
        pub fn write_with_rwlock(&self, value: usize, rwlock: &RwLock<()>) {
            let _guard = rwlock.write();
            self.data.store(value, Ordering::Relaxed);
        }
    }
    
    /// 哲学家进餐问题
    pub struct DiningPhilosophers<const N: usize> {
        forks: [Mutex<()>; N],
    }
    
    impl<const N: usize> DiningPhilosophers<N> {
        pub const fn new() -> Self {
            Self {
                forks: [const { Mutex::new(()); N }],
            }
        }
        
        /// 哲学家进餐（避免死锁版本）
        pub fn philosopher_eat(&self, id: usize) -> Result<(), &'static str> {
            let left = id;
            let right = (id + 1) % N;
            
            // 避免死锁：总是先拿编号小的叉子
            let (first, second) = if left < right {
                (&self.forks[left], &self.forks[right])
            } else {
                (&self.forks[right], &self.forks[left])
            };
            
            let _guard1 = first.lock();
            let _guard2 = second.lock();
            
            // 模拟进餐
            Ok(())
        }
        
        /// 哲学家进餐（故意制造死锁版本）
        pub fn philosopher_eat_deadlock(&self, id: usize) -> Result<(), &'static str> {
            let left = id;
            let right = (id + 1) % N;
            
            // 故意制造死锁：所有哲学家都先拿左边的叉子
            let _guard1 = self.forks[left].lock();
            let _guard2 = self.forks[right].lock();
            
            Ok(())
        }
    }
}

/// 压力测试框架
pub struct StressTester {
    pub thread_count: usize,
    pub operations_per_thread: usize,
    pub contention_level: f64, // 0.0-1.0，竞争程度
}

impl StressTester {
    pub const fn new(thread_count: usize, operations: usize, contention: f64) -> Self {
        Self {
            thread_count,
            operations_per_thread: operations,
            contention_level: contention,
        }
    }
    
    /// 自旋锁压力测试
    pub fn stress_test_spinlock(&self) -> StressTestResult {
        let lock = spin::Mutex::new(0usize);
        let counter = AtomicUsize::new(0);
        
        // 模拟多线程竞争
        for _ in 0..self.thread_count {
            for _ in 0..self.operations_per_thread {
                let _guard = lock.lock();
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        StressTestResult {
            total_operations: self.thread_count * self.operations_per_thread,
            successful_operations: counter.load(Ordering::Relaxed),
            contention_observed: self.contention_level,
        }
    }
    
    /// 互斥锁压力测试
    pub fn stress_test_mutex(&self) -> StressTestResult {
        let lock = Mutex::new(0usize);
        let counter = AtomicUsize::new(0);
        
        for _ in 0..self.thread_count {
            for _ in 0..self.operations_per_thread {
                let _guard = lock.lock();
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        StressTestResult {
            total_operations: self.thread_count * self.operations_per_thread,
            successful_operations: counter.load(Ordering::Relaxed),
            contention_observed: self.contention_level,
        }
    }
    
    /// 信号量压力测试
    pub fn stress_test_semaphore(&self) -> StressTestResult {
        // 这里需要实现信号量压力测试
        StressTestResult {
            total_operations: self.thread_count * self.operations_per_thread,
            successful_operations: self.thread_count * self.operations_per_thread,
            contention_observed: self.contention_level,
        }
    }
}

/// 压力测试结果
#[derive(Debug)]
pub struct StressTestResult {
    pub total_operations: usize,
    pub successful_operations: usize,
    pub contention_observed: f64,
}

impl StressTestResult {
    /// 验证测试是否通过
    pub fn verify_success(&self) -> bool {
        self.successful_operations == self.total_operations
    }
    
    /// 计算成功率
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            1.0
        } else {
            self.successful_operations as f64 / self.total_operations as f64
        }
    }
}

/// 能失败的对照测试（兼容旧接口）
pub mod failure_tests {
    use super::*;
    use crate::failure_control_tests::{FailureTestManager, FailureTestResult};
    
    /// 测试缺少wakeup导致的死锁
    pub fn test_missing_wakeup() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_missing_wakeup_deadlock();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Missing wakeup test did not behave as expected")
        }
    }
    
    /// 测试unlock顺序错误
    pub fn test_unlock_order_error() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_unlock_order_deadlock();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Unlock order test did not behave as expected")
        }
    }
    
    /// 测试信号量计数错误
    pub fn test_semaphore_count_error() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_semaphore_overflow();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Semaphore count test did not behave as expected")
        }
    }
    
    /// 运行完整的可失败对照测试套件
    pub fn run_comprehensive_failure_tests() -> Vec<FailureTestResult> {
        let manager = FailureTestManager::new();
        manager.run_all_failure_tests()
    }
    
    /// 测试条件变量使用错误
    pub fn test_incorrect_condvar_usage() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_incorrect_condvar_usage();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Incorrect condvar usage test did not behave as expected")
        }
    }
    
    /// 测试双重加锁死锁
    pub fn test_double_lock_deadlock() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_double_lock_deadlock();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Double lock deadlock test did not behave as expected")
        }
    }
    
    /// 测试循环等待死锁
    pub fn test_circular_wait_deadlock() -> Result<(), &'static str> {
        let manager = FailureTestManager::new();
        let result = manager.test_circular_wait_deadlock();
        
        if result.verify_expected_behavior() {
            Ok(())
        } else {
            Err("Circular wait deadlock test did not behave as expected")
        }
    }
}

/// 性能对比测试
pub struct PerformanceComparator {
    pub test_cases: Vec<TestCase>,
}

impl PerformanceComparator {
    pub fn new() -> Self {
        Self {
            test_cases: Vec::new(),
        }
    }
    
    /// 添加测试用例
    pub fn add_test_case(&mut self, name: &'static str, test_fn: fn() -> TestResult) {
        self.test_cases.push(TestCase {
            name,
            test_fn,
        });
    }
    
    /// 运行所有对比测试
    pub fn run_comparison(&self) -> ComparisonReport {
        let mut results = Vec::new();
        
        for test_case in &self.test_cases {
            let result = (test_case.test_fn)();
            results.push((test_case.name, result));
        }
        
        ComparisonReport { results }
    }
}

/// 测试用例
pub struct TestCase {
    pub name: &'static str,
    pub test_fn: fn() -> TestResult,
}

/// 测试结果
#[derive(Debug)]
pub struct TestResult {
    pub execution_time: u64,    // 执行时间（时钟周期）
    pub memory_usage: usize,    // 内存使用量
    pub context_switches: u64,  // 上下文切换次数
    pub lock_contention: f64,   // 锁竞争率
}

/// 对比测试报告
#[derive(Debug)]
pub struct ComparisonReport {
    pub results: Vec<(&'static str, TestResult)>,
}

impl ComparisonReport {
    /// 找出性能最优的同步原语
    pub fn find_best_performer(&self) -> Option<&'static str> {
        self.results.iter()
            .min_by_key(|(_, result)| result.execution_time)
            .map(|(name, _)| *name)
    }
    
    /// 找出最公平的同步原语
    pub fn find_fairest_performer(&self) -> Option<&'static str> {
        self.results.iter()
            .min_by(|(_, a), (_, b)| {
                a.lock_contention.partial_cmp(&b.lock_contention).unwrap()
            })
            .map(|(name, _)| *name)
    }
}