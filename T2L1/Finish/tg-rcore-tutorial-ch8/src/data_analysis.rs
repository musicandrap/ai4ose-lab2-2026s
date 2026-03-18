//! 数据收集和对比分析模块
//! 
//! 用表格对比这些指标，分析它们在性能、上下文切换和公平性上的差异

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::stress_test::{StressTestResult, ComparisonReport};
use crate::instrumented_locks::LockStats;

/// 锁性能指标对比
#[derive(Debug, Clone)]
pub struct LockPerformanceMetrics {
    pub lock_type: String,
    pub throughput: f64,           // 吞吐量（操作/秒）
    pub contention_rate: f64,      // 竞争率
    pub avg_wait_time: f64,        // 平均等待时间（纳秒）
    pub avg_hold_time: f64,        // 平均持锁时间（纳秒）
    pub context_switches: u64,     // 上下文切换次数
    pub sleep_count: u64,          // sleep次数
    pub wakeup_count: u64,          // wakeup次数
    pub fairness_variance: f64,    // 公平性方差
    pub has_starvation: bool,      // 是否出现饥饿
    pub max_wait_time: u64,        // 最大等待时间
}

impl LockPerformanceMetrics {
    pub fn from_test_result(lock_type: &str, result: &StressTestResult) -> Self {
        let stats = &result.lock_stats;
        
        Self {
            lock_type: lock_type.to_string(),
            throughput: result.throughput,
            contention_rate: stats.contention_rate(),
            avg_wait_time: stats.avg_wait_time(),
            avg_hold_time: stats.avg_hold_time(),
            context_switches: stats.context_switches.load(core::sync::atomic::Ordering::Relaxed),
            sleep_count: stats.sleep_count.load(core::sync::atomic::Ordering::Relaxed),
            wakeup_count: stats.wakeup_count.load(core::sync::atomic::Ordering::Relaxed),
            fairness_variance: stats.fairness_variance(),
            has_starvation: stats.has_starvation(1_000_000), // 1ms阈值
            max_wait_time: stats.max_wait_time.load(core::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// 性能对比表格
pub struct PerformanceComparisonTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<LockPerformanceMetrics>,
}

impl PerformanceComparisonTable {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            headers: vec![
                "锁类型".to_string(),
                "吞吐量(ops/s)".to_string(),
                "竞争率".to_string(),
                "平均等待(ns)".to_string(),
                "平均持锁(ns)".to_string(),
                "上下文切换".to_string(),
                "sleep次数".to_string(),
                "wakeup次数".to_string(),
                "公平性方差".to_string(),
                "是否饥饿".to_string(),
                "最大等待(ns)".to_string(),
            ],
            rows: Vec::new(),
        }
    }
    
    pub fn add_metrics(&mut self, metrics: LockPerformanceMetrics) {
        self.rows.push(metrics);
    }
    
    pub fn generate_table(&self) -> String {
        let mut table = String::new();
        
        // 标题
        table.push_str(&format!("=== {} ===\n", self.title));
        
        // 表头
        table.push_str(&self.format_row(&self.headers));
        table.push_str(&"\n".repeat(2));
        
        // 数据行
        for row in &self.rows {
            let row_data = vec![
                row.lock_type.clone(),
                format!("{:.2}", row.throughput),
                format!("{:.3}", row.contention_rate),
                format!("{:.1}", row.avg_wait_time),
                format!("{:.1}", row.avg_hold_time),
                row.context_switches.to_string(),
                row.sleep_count.to_string(),
                row.wakeup_count.to_string(),
                format!("{:.6}", row.fairness_variance),
                if row.has_starvation { "是" } else { "否" }.to_string(),
                row.max_wait_time.to_string(),
            ];
            table.push_str(&self.format_row(&row_data));
        }
        
        table
    }
    
    fn format_row(&self, cells: &[String]) -> String {
        let widths = [10, 12, 8, 12, 12, 10, 10, 10, 12, 8, 12];
        
        let mut row = String::new();
        for (i, cell) in cells.iter().enumerate() {
            let width = widths[i];
            let formatted = if cell.len() > width {
                format!("{:.width$}", cell, width = width)
            } else {
                format!("{:width$}", cell, width = width)
            };
            row.push_str(&formatted);
            row.push(' ');
        }
        row.push('\n');
        row
    }
}

/// 数据分析器
pub struct DataAnalyzer;

impl DataAnalyzer {
    /// 分析性能差异
    pub fn analyze_performance_difference(report: &ComparisonReport) -> PerformanceAnalysis {
        let spinlock_metrics = LockPerformanceMetrics::from_test_result("spinlock", &report.spinlock_results);
        let mutex_metrics = LockPerformanceMetrics::from_test_result("mutex", &report.mutex_results);
        let rwlock_metrics = LockPerformanceMetrics::from_test_result("rwlock", &report.rwlock_results);
        
        let best_performer = report.find_best_performer();
        let fairest_performer = report.find_fairest_performer();
        
        // 计算性能提升百分比
        let spin_vs_mutex = ((spinlock_metrics.throughput - mutex_metrics.throughput) / mutex_metrics.throughput) * 100.0;
        let rw_vs_mutex = ((rwlock_metrics.throughput - mutex_metrics.throughput) / mutex_metrics.throughput) * 100.0;
        
        PerformanceAnalysis {
            best_performer: best_performer.to_string(),
            fairest_performer: fairest_performer.to_string(),
            spinlock_vs_mutex_percent: spin_vs_mutex,
            rwlock_vs_mutex_percent: rw_vs_mutex,
            context_switch_reduction: Self::calculate_context_switch_reduction(&spinlock_metrics, &mutex_metrics),
            starvation_analysis: Self::analyze_starvation(&spinlock_metrics, &mutex_metrics, &rwlock_metrics),
            fairness_improvement: Self::calculate_fairness_improvement(&spinlock_metrics, &mutex_metrics, &rwlock_metrics),
        }
    }
    
    /// 计算上下文切换减少量
    fn calculate_context_switch_reduction(spinlock: &LockPerformanceMetrics, mutex: &LockPerformanceMetrics) -> f64 {
        if mutex.context_switches > 0 {
            (mutex.context_switches as f64 - spinlock.context_switches as f64) / mutex.context_switches as f64 * 100.0
        } else {
            0.0
        }
    }
    
    /// 分析饥饿现象
    fn analyze_starvation(spinlock: &LockPerformanceMetrics, mutex: &LockPerformanceMetrics, rwlock: &LockPerformanceMetrics) -> String {
        let mut analysis = String::new();
        
        if spinlock.has_starvation {
            analysis.push_str("自旋锁出现饥饿现象（高竞争场景下低优先级线程无法获取锁）\n");
        }
        
        if mutex.has_starvation {
            analysis.push_str("互斥锁出现饥饿现象（可能由于唤醒机制不公平）\n");
        }
        
        if rwlock.has_starvation {
            analysis.push_str("读写锁出现饥饿现象（写者可能饿死读者，或反之）\n");
        }
        
        if analysis.is_empty() {
            analysis.push_str("所有锁类型在当前测试场景下均未出现明显饥饿现象");
        }
        
        analysis
    }
    
    /// 计算公平性改进
    fn calculate_fairness_improvement(spinlock: &LockPerformanceMetrics, mutex: &LockPerformanceMetrics, rwlock: &LockPerformanceMetrics) -> f64 {
        let min_variance = spinlock.fairness_variance.min(mutex.fairness_variance).min(rwlock.fairness_variance);
        let max_variance = spinlock.fairness_variance.max(mutex.fairness_variance).max(rwlock.fairness_variance);
        
        if max_variance > 0.0 {
            ((max_variance - min_variance) / max_variance) * 100.0
        } else {
            0.0
        }
    }
    
    /// 生成对比表格
    pub fn generate_comparison_table(report: &ComparisonReport, scenario_name: &str) -> String {
        let mut table = PerformanceComparisonTable::new(&format!("{}场景锁性能对比", scenario_name));
        
        table.add_metrics(LockPerformanceMetrics::from_test_result("自旋锁", &report.spinlock_results));
        table.add_metrics(LockPerformanceMetrics::from_test_result("互斥锁", &report.mutex_results));
        table.add_metrics(LockPerformanceMetrics::from_test_result("读写锁", &report.rwlock_results));
        
        table.generate_table()
    }
    
    /// 生成竞争程度影响分析
    pub fn generate_contention_analysis(report: &ComparisonReport) -> String {
        let mut analysis = String::new();
        analysis.push_str("=== 竞争程度对锁性能的影响分析 ===\n\n");
        
        // 分析自旋锁在不同竞争程度下的表现
        analysis.push_str("自旋锁：\n");
        for (i, result) in report.spinlock_contention.iter().enumerate() {
            let metrics = LockPerformanceMetrics::from_test_result("自旋锁", result);
            analysis.push_str(&format!("  竞争率{:.1}: 吞吐量{:.0} ops/s, 等待时间{:.1}ns\n", 
                result.config.contention_level, metrics.throughput, metrics.avg_wait_time));
        }
        
        analysis.push_str("\n互斥锁：\n");
        for (i, result) in report.mutex_contention.iter().enumerate() {
            let metrics = LockPerformanceMetrics::from_test_result("互斥锁", result);
            analysis.push_str(&format!("  竞争率{:.1}: 吞吐量{:.0} ops/s, 上下文切换{}次\n", 
                result.config.contention_level, metrics.throughput, metrics.context_switches));
        }
        
        analysis
    }
}

/// 性能分析结果
#[derive(Debug)]
pub struct PerformanceAnalysis {
    pub best_performer: String,                 // 性能最优的锁类型
    pub fairest_performer: String,              // 最公平的锁类型
    pub spinlock_vs_mutex_percent: f64,         // 自旋锁相比互斥锁的性能提升百分比
    pub rwlock_vs_mutex_percent: f64,           // 读写锁相比互斥锁的性能提升百分比
    pub context_switch_reduction: f64,          // 上下文切换减少百分比
    pub starvation_analysis: String,            // 饥饿现象分析
    pub fairness_improvement: f64,              // 公平性改进百分比
}

impl fmt::Display for PerformanceAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== 锁性能深度分析报告 ===")?;
        writeln!(f, "性能最优锁类型: {}", self.best_performer)?;
        writeln!(f, "最公平锁类型: {}", self.fairest_performer)?;
        writeln!(f, "自旋锁相比互斥锁性能: {:.1}%", self.spinlock_vs_mutex_percent)?;
        writeln!(f, "读写锁相比互斥锁性能: {:.1}%", self.rwlock_vs_mutex_percent)?;
        writeln!(f, "上下文切换减少: {:.1}%", self.context_switch_reduction)?;
        writeln!(f, "公平性改进: {:.1}%", self.fairness_improvement)?;
        writeln!(f, "饥饿现象分析:\n{}", self.starvation_analysis)?;
        
        Ok(())
    }
}

/// 可视化报告生成器
pub struct VisualizationReport;

impl VisualizationReport {
    /// 生成完整的可视化报告
    pub fn generate_full_report(report: &ComparisonReport) -> String {
        let mut full_report = String::new();
        
        // 1. 基础场景对比
        full_report.push_str(&DataAnalyzer::generate_comparison_table(report, "基础"));
        full_report.push_str("\n\n");
        
        // 2. 性能深度分析
        let analysis = DataAnalyzer::analyze_performance_difference(report);
        full_report.push_str(&analysis.to_string());
        full_report.push_str("\n\n");
        
        // 3. 竞争程度影响分析
        full_report.push_str(&DataAnalyzer::generate_contention_analysis(report));
        
        full_report
    }
    
    /// 生成性能对比图表数据（可用于外部可视化）
    pub fn generate_chart_data(report: &ComparisonReport) -> ChartData {
        ChartData {
            lock_types: vec!["自旋锁".to_string(), "互斥锁".to_string(), "读写锁".to_string()],
            throughput: vec![
                report.spinlock_results.throughput,
                report.mutex_results.throughput,
                report.rwlock_results.throughput,
            ],
            avg_wait_times: vec![
                report.spinlock_results.lock_stats.avg_wait_time(),
                report.mutex_results.lock_stats.avg_wait_time(),
                report.rwlock_results.lock_stats.avg_wait_time(),
            ],
            context_switches: vec![
                report.spinlock_results.lock_stats.context_switches.load(core::sync::atomic::Ordering::Relaxed) as f64,
                report.mutex_results.lock_stats.context_switches.load(core::sync::atomic::Ordering::Relaxed) as f64,
                report.rwlock_results.lock_stats.context_switches.load(core::sync::atomic::Ordering::Relaxed) as f64,
            ],
        }
    }
}

/// 图表数据（可用于外部可视化工具）
#[derive(Debug)]
pub struct ChartData {
    pub lock_types: Vec<String>,
    pub throughput: Vec<f64>,
    pub avg_wait_times: Vec<f64>,
    pub context_switches: Vec<f64>,
}