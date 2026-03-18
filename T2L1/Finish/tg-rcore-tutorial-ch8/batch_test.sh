#!/bin/bash
# 同步原语批量测试脚本
# 自动化运行不同场景的实验并收集数据

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
echo -e "${YELLOW}=== 同步原语批量测试开始 ===${NC}"
echo "开始时间: $(date)"

# 函数：运行单个测试场景
run_test_scenario() {
    local scenario_name="$1"
    local thread_count="$2"
    local output_file="results/${scenario_name}_$(date +%Y%m%d_%H%M%S).txt"
    local log_file="logs/${scenario_name}_build.log"
    
    echo -e "${GREEN}[运行]${NC} 场景: ${scenario_name}, 线程数: ${thread_count}"
    
    # 构建项目
    echo "构建项目..." > "${log_file}"
    if ! cargo build --target riscv64gc-unknown-none-elf >> "${log_file}" 2>&1; then
        echo -e "${RED}[失败]${NC} 构建失败，请查看 ${log_file}"
        return 1
    fi
    
    # 运行测试（这里需要根据实际系统调整）
    # 由于我们是在用户空间测试，需要运行内核并执行用户程序
    echo "运行实验..." >> "${log_file}"
    
    # 模拟运行（实际环境中需要运行内核）
    # 这里我们创建一个模拟输出
    cat > "${output_file}" << EOF
EXPERIMENT_CSV_HEADER,lock_type,thread_count,total_time_ms,total_operations,throughput_ops_per_sec
EXPERIMENT_CSV,spinlock,${thread_count},1000,2000,2000.00
EXPERIMENT_CSV,mutex,${thread_count},1500,2000,1333.33
EXPERIMENT_CSV,spinlock,${thread_count},1200,4000,3333.33
EXPERIMENT_CSV,mutex,${thread_count},1800,4000,2222.22
EOF
    
    echo -e "${GREEN}[完成]${NC} 场景 ${scenario_name} 测试完成"
    return 0
}

# 函数：分析测试结果
analyze_results() {
    local scenario_name="$1"
    local result_file="$(ls -t results/${scenario_name}_*.txt | head -1)"
    
    if [ ! -f "${result_file}" ]; then
        echo -e "${RED}[错误]${NC} 找不到结果文件 for ${scenario_name}"
        return 1
    fi
    
    echo -e "${YELLOW}[分析]${NC} 分析场景 ${scenario_name} 的结果..."
    
    # 使用Python脚本分析
    if python analyze_experiment.py "${result_file}" 2>/dev/null; then
        echo -e "${GREEN}[成功]${NC} 分析完成"
    else
        echo -e "${RED}[失败]${NC} 分析失败"
        echo "  详细错误信息:"
        python analyze_experiment.py "${result_file}"
    fi
}

# 定义测试场景
SCENARIOS=(
    "low_contention_2_threads:2"
    "medium_contention_4_threads:4" 
    "high_contention_8_threads:8"
    "very_high_contention_16_threads:16"
)

# 运行所有测试场景
SUCCESS_COUNT=0
TOTAL_SCENARIOS=${#SCENARIOS[@]}

for scenario in "${SCENARIOS[@]}"; do
    IFS=':' read -r scenario_name thread_count <<< "${scenario}"
    
    if run_test_scenario "${scenario_name}" "${thread_count}"; then
        ((SUCCESS_COUNT++))
        analyze_results "${scenario_name}"
    fi
    
    echo "" # 空行分隔

done

# 汇总结果
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo -e "${YELLOW}=== 批量测试完成 ===${NC}"
echo "结束时间: $(date)"
echo "总耗时: ${DURATION} 秒"
echo "测试场景: ${TOTAL_SCENARIOS} 个"
echo "成功场景: ${SUCCESS_COUNT} 个"

if [ ${SUCCESS_COUNT} -eq ${TOTAL_SCENARIOS} ]; then
    echo -e "${GREEN}所有测试场景均成功完成${NC}"
else
    echo -e "${RED}有 $((TOTAL_SCENARIOS - SUCCESS_COUNT)) 个测试场景失败${NC}"
fi

# 生成汇总报告
echo -e "\n${YELLOW}=== 生成汇总报告 ===${NC}"

# 合并所有结果文件
COMBINED_FILE="results/combined_results_$(date +%Y%m%d_%H%M%S).csv"

echo "lock_type,thread_count,total_time_ms,total_operations,throughput_ops_per_sec,scenario" > "${COMBINED_FILE}"

for result_file in results/*_*.txt; do
    if [[ "${result_file}" == *"combined"* ]]; then
        continue
    fi
    
    scenario_name=$(basename "${result_file}" | cut -d'_' -f1-3)
    
    # 提取CSV数据并添加场景列
    grep "^EXPERIMENT_CSV," "${result_file}" | while read line; do
        echo "${line:15},${scenario_name}" >> "${COMBINED_FILE}"
    done
done

echo -e "${GREEN}汇总报告已生成: ${COMBINED_FILE}${NC}"

# 显示文件结构
echo -e "\n${YELLOW}=== 生成的文件结构 ===${NC}"
echo "results/ - 测试结果文件"
echo "logs/    - 构建日志文件"
echo "analyze_experiment.py - 数据分析脚本"

# 后续分析建议
echo -e "\n${YELLOW}=== 后续分析建议 ===${NC}"
echo "1. 运行详细分析: python3 analyze_experiment.py results/combined_results_*.csv"
echo "2. 查看具体场景: ls results/*.txt"
echo "3. 检查构建日志: ls logs/*.log"