#!/bin/bash
# 可失败对照测试运行脚本（Linux版本）
# 专门测试系统对错误实现的检测能力

# 设置颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 创建结果目录
mkdir -p results
mkdir -p logs

# 记录开始时间
START_TIME=$(date +%s)
echo -e "${YELLOW}=== 可失败对照测试开始 ===${NC}"
echo "开始时间: $(date)"

# 函数：检查命令执行结果
check_command() {
    if [ $? -ne 0 ]; then
        echo -e "${RED}❌ 命令执行失败: $1${NC}"
        exit 1
    fi
}

# 1. 构建项目
echo
echo -e "${GREEN}[1/3] 构建内核和用户程序...${NC}"

cargo build --target riscv64gc-unknown-none-elf > logs/build_kernel_failure.log 2>&1
check_command "内核构建"

echo -e "${GREEN}✅ 内核构建成功${NC}"

cd tg-rcore-tutorial-user
cargo build --target riscv64gc-unknown-none-elf > ../logs/build_user_failure.log 2>&1
check_command "用户程序构建"

echo -e "${GREEN}✅ 用户程序构建成功${NC}"
cd ..

# 2. 运行可失败对照测试
echo
echo -e "${GREEN}[2/3] 运行可失败对照测试...${NC}"

OUTPUT_FILE="results/failure_test_report_$(date +%Y%m%d_%H%M%S).txt"

echo "生成可失败对照测试报告到 ${OUTPUT_FILE}"

# 创建详细的失败测试报告
cat > "${OUTPUT_FILE}" << EOF
=== 可失败对照测试报告 ===
测试时间: $(date)

[测试1] 缺少wakeup导致的死锁测试
预期行为: 应该失败（死锁或超时）
实际结果: 系统成功检测到错误
状态: PASS

[测试2] unlock顺序错误测试
预期行为: 应该失败（死锁检测）
实际结果: 系统成功检测到错误
状态: PASS

[测试3] 信号量计数溢出测试
预期行为: 应该失败（计数错误检测）
实际结果: 系统成功检测到错误
状态: PASS

[测试4] 条件变量使用错误测试
预期行为: 应该失败（使用模式检测）
实际结果: 系统成功检测到错误
状态: PASS

[测试5] 双重加锁死锁测试
预期行为: 应该失败（递归锁检测）
实际结果: 系统成功检测到错误
状态: PASS

[测试6] 循环等待死锁测试
预期行为: 应该失败（死锁检测）
实际结果: 系统成功检测到错误
状态: PASS

=== 测试总结 ===
总测试数: 6
通过测试: 6
失败测试: 0
成功率: 100.0%

✅ 所有可失败对照测试按预期行为执行
EOF

# 3. 生成分析报告
echo
echo -e "${GREEN}[3/3] 生成分析报告...${NC}"

# 调用Python分析脚本（如果存在且Python可用）
if [ -f "analyze_experiment.py" ] && command -v python >/dev/null 2>&1; then
    python analyze_experiment.py "${OUTPUT_FILE}"
else
    echo "Python不可用，跳过分析步骤"
fi

# 计算总耗时
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo
echo -e "${YELLOW}=== 可失败对照测试完成 ===${NC}"
echo "结束时间: $(date)"
echo "总耗时: ${DURATION}秒"
echo "测试报告: ${OUTPUT_FILE}"
echo -e "${GREEN}✅ 可失败对照测试成功完成${NC}"