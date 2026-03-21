# 可失败对照测试框架使用指南

## 概述

本框架旨在证明系统不仅能正常运行，还能识别错误实现。通过设计一系列故意包含错误的测试用例，验证系统能够正确检测并处理这些错误情况。

## 测试用例设计

### 1. 缺少wakeup导致的死锁测试
- **预期行为**：应该失败（死锁或超时）
- **测试目的**：验证系统能检测到缺少wakeup调用导致的死锁
- **错误模式**：在条件等待循环中故意不调用wakeup

### 2. unlock顺序错误测试
- **预期行为**：应该失败（死锁检测）
- **测试目的**：验证系统能检测到unlock顺序错误
- **错误模式**：故意颠倒unlock顺序，制造潜在死锁

### 3. 信号量计数溢出测试
- **预期行为**：应该失败（计数错误检测）
- **测试目的**：验证系统能处理信号量计数超出范围的情况
- **错误模式**：故意让信号量计数超出合理范围

### 4. 条件变量使用错误测试
- **预期行为**：应该失败（使用模式检测）
- **测试目的**：验证系统能检测到条件变量的错误使用模式
- **错误模式**：在不持有锁的情况下调用wait

### 5. 双重加锁死锁测试
- **预期行为**：应该失败（递归锁检测）
- **测试目的**：验证系统能检测到非递归锁的双重加锁
- **错误模式**：在同一线程中重复加锁

### 6. 循环等待死锁测试
- **预期行为**：应该失败（死锁检测）
- **测试目的**：验证系统能检测到循环等待死锁
- **错误模式**：制造A->B->C->A的循环等待

## 使用方法

### 运行完整测试套件

```bash
# Windows
run_failure_tests.bat

# Linux/Mac
./run_failure_tests.sh
```

### 集成到批量测试

```bash
# 运行所有测试场景（包括可失败对照测试）
./batch_test.sh
```

### 在代码中使用

```rust
use crate::failure_control_tests::{FailureTestManager, FailureTestReporter};

// 运行所有可失败测试
let manager = FailureTestManager::new();
let results = manager.run_all_failure_tests();

// 生成报告
let reporter = FailureTestReporter::new(results);
let report = reporter.generate_report();

// 验证所有测试是否按预期行为
let all_passed = reporter.verify_all_tests();
```

## 测试结果解读

每个测试用例都会返回一个`FailureTestResult`结构体，包含：

- `test_name`: 测试名称
- `should_fail`: 预期是否应该失败
- `actual_failed`: 实际是否失败
- `timeout_occurred`: 是否发生超时
- `deadlock_detected`: 是否检测到死锁
- `error_message`: 错误信息
- `execution_time`: 执行时间

**测试通过标准**：`should_fail == actual_failed`

## 预期行为

所有可失败对照测试都应该**按预期失败**，即：
- 当测试包含故意错误时，系统应该检测到这些错误
- 测试应该失败（超时、死锁检测或错误处理）
- 如果测试意外成功，说明系统未能正确检测错误

## 集成到现有测试框架

新的可失败对照测试框架已经集成到现有的`sync_tests`模块中，可以通过以下方式调用：

```rust
use crate::sync_tests::failure_tests;

// 运行单个测试
let result = failure_tests::test_missing_wakeup();

// 运行完整套件
let results = failure_tests::run_comprehensive_failure_tests();
```

## 扩展测试用例

要添加新的可失败测试用例：

1. 在`failure_control_tests.rs`中的`FailureTestManager`添加新方法
2. 在`sync_tests.rs`中的`failure_tests`模块添加对应的包装函数
3. 更新`sync_experiment.rs`中的`run_failure_tests`方法
4. 更新测试脚本以包含新测试

## 注意事项

1. **测试环境**：这些测试需要在真实的并发环境中运行才能完全验证
2. **超时设置**：根据系统性能调整超时阈值
3. **错误检测**：确保系统有适当的错误检测机制
4. **日志记录**：测试过程中应该记录详细的调试信息

## 验证系统健壮性

通过可失败对照测试，可以验证系统在以下方面的健壮性：
- **错误检测能力**：系统是否能识别常见的同步错误
- **死锁预防**：系统是否能避免或检测死锁情况
- **超时处理**：系统是否能正确处理长时间阻塞
- **资源管理**：系统是否能正确处理资源溢出情况

这个框架不仅测试系统"能跑"，更重要的是测试系统"能识别错误"，这是高质量并发系统的重要特征。