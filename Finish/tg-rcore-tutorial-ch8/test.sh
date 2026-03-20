#!/bin/bash
# ch8 测试脚本（精简版）
#
# 只测试保留的10个用户应用：
# - ch8_usertest
# - ch8b_usertest
# - ch8_deadlock_mutex1
# - ch8_deadlock_sem1
# - ch8_deadlock_sem2
# - threads
# - threads_arg
# - sync_sem
# - mpsc_sem
# - test_condvar

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

# 测试函数
run_test() {
    local test_name="$1"
    local expected_pattern="$2"
    
    if echo "$output" | grep -q "$expected_pattern"; then
        echo -e "${GREEN}[PASS]${NC} $test_name"
        return 0
    else
        echo -e "${RED}[FAIL]${NC} $test_name - 未找到预期输出: $expected_pattern"
        return 1
    fi
}

run_base() {
    echo "运行 ch8 基础测试..."
    cargo clean
    export CHAPTER=-8
    echo -e "${YELLOW}────────── cargo run 输出 ──────────${NC}"
    
    # 捕获cargo run的输出
    output=$(cargo run 2>&1 | tee /dev/stderr)
    
    echo ""
    echo -e "${YELLOW}────────── 测试结果 ──────────${NC}"
    
    local failed=0
    
    # 测试保留的10个应用
    run_test "threads test" "threads test passed" || ((failed++))
    run_test "threads with arg test" "threads with arg test passed" || ((failed++))
    run_test "sync_sem" "sync_sem passed" || ((failed++))
    run_test "mpsc_sem" "mpsc_sem passed" || ((failed++))
    run_test "test_condvar" "test_condvar passed" || ((failed++))
    
    # 检查是否有FAIL: T.T（不应该出现）
    if echo "$output" | grep -q "FAIL: T.T"; then
        echo -e "${RED}[FAIL]${NC} 发现测试失败标记: FAIL: T.T"
        ((failed++))
    else
        echo -e "${GREEN}[PASS]${NC} 未发现测试失败标记"
    fi
    
    if [ $failed -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✓ ch8 基础测试通过${NC}"
        cargo clean
        return 0
    else
        echo ""
        echo -e "${RED}✗ ch8 基础测试失败 (${failed}/5)${NC}"
        cargo clean
        return 1
    fi
}

run_exercise() {
    echo "运行 ch8 练习测试..."
    cargo clean
    export CHAPTER=8
    echo -e "${YELLOW}────────── cargo run --features exercise 输出 ──────────${NC}"
    
    # 捕获cargo run的输出
    output=$(cargo run --features exercise 2>&1 | tee /dev/stderr)
    
    echo ""
    echo -e "${YELLOW}────────── 测试结果 ──────────${NC}"
    
    local failed=0
    
    # 测试保留的10个应用（包括死锁测试）
    run_test "threads test" "threads test passed" || ((failed++))
    run_test "threads with arg test" "threads with arg test passed" || ((failed++))
    run_test "sync_sem" "sync_sem passed" || ((failed++))
    run_test "mpsc_sem" "mpsc_sem passed" || ((failed++))
    run_test "test_condvar" "test_condvar passed" || ((failed++))
    run_test "ch8_deadlock_mutex1" "ch8_deadlock_mutex1" || ((failed++))
    run_test "ch8_deadlock_sem1" "ch8_deadlock_sem1" || ((failed++))
    run_test "ch8_deadlock_sem2" "ch8_deadlock_sem2" || ((failed++))
    run_test "ch8_usertest" "ch8_usertest" || ((failed++))
    run_test "ch8b_usertest" "ch8b_usertest" || ((failed++))
    
    # 检查是否有FAIL: T.T（不应该出现）
    if echo "$output" | grep -q "FAIL: T.T"; then
        echo -e "${RED}[FAIL]${NC} 发现测试失败标记: FAIL: T.T"
        ((failed++))
    else
        echo -e "${GREEN}[PASS]${NC} 未发现测试失败标记"
    fi
    
    if [ $failed -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✓ ch8 练习测试通过${NC}"
        cargo clean
        return 0
    else
        echo ""
        echo -e "${RED}✗ ch8 练习测试失败 (${failed}/10)${NC}"
        cargo clean
        return 1
    fi
}

case "${1:-all}" in
    base)
        run_base
        ;;
    exercise)
        run_exercise
        ;;
    all)
        run_base
        echo ""
        run_exercise
        ;;
    *)
        echo "用法: $0 [base|exercise|all]"
        exit 1
        ;;
esac
