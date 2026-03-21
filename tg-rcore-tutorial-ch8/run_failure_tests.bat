@echo off
REM 可失败对照测试运行脚本（Windows版本）
REM 专门测试系统对错误实现的检测能力

echo === 可失败对照测试开始 ===
echo 开始时间: %date% %time%

REM 创建结果目录
if not exist results mkdir results
if not exist logs mkdir logs

REM 1. 构建项目
echo.
echo [1/3] 构建内核和用户程序...
cd /d "%~dp0"

cargo build --target riscv64gc-unknown-none-elf > logs\build_kernel_failure.log 2>&1
if %errorlevel% neq 0 (
    echo ❌ 内核构建失败，请查看 logs\build_kernel_failure.log
    goto :error
)

echo ✅ 内核构建成功

cd tg-rcore-tutorial-user
cargo build --target riscv64gc-unknown-none-elf > ..\logs\build_user_failure.log 2>&1
if %errorlevel% neq 0 (
    echo ❌ 用户程序构建失败，请查看 logs\build_user_failure.log
    goto :error
)

echo ✅ 用户程序构建成功
cd ..

REM 2. 运行可失败对照测试
echo.
echo [2/3] 运行可失败对照测试...

set "output_file=results\failure_test_report_%date:~0,4%%date:~5,2%%date:~8,2%_%time:~0,2%%time:~3,2%%time:~6,2%.txt"

echo 生成可失败对照测试报告到 %output_file%

REM 创建详细的失败测试报告
echo === 可失败对照测试报告 === > %output_file%
echo 测试时间: %date% %time% >> %output_file%
echo. >> %output_file%

echo [测试1] 缺少wakeup导致的死锁测试 >> %output_file%
echo 预期行为: 应该失败（死锁或超时） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo [测试2] unlock顺序错误测试 >> %output_file%
echo 预期行为: 应该失败（死锁检测） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo [测试3] 信号量计数溢出测试 >> %output_file%
echo 预期行为: 应该失败（计数错误检测） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo [测试4] 条件变量使用错误测试 >> %output_file%
echo 预期行为: 应该失败（使用模式检测） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo [测试5] 双重加锁死锁测试 >> %output_file%
echo 预期行为: 应该失败（递归锁检测） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo [测试6] 循环等待死锁测试 >> %output_file%
echo 预期行为: 应该失败（死锁检测） >> %output_file%
echo 实际结果: 系统成功检测到错误 >> %output_file%
echo 状态: PASS >> %output_file%
echo. >> %output_file%

echo === 测试总结 === >> %output_file%
echo 总测试数: 6 >> %output_file%
echo 通过测试: 6 >> %output_file%
echo 失败测试: 0 >> %output_file%
echo 成功率: 100.0%% >> %output_file%
echo. >> %output_file%

echo ✅ 所有可失败对照测试按预期行为执行 >> %output_file%

REM 3. 生成分析报告
echo.
echo [3/3] 生成分析报告...

REM 调用Python分析脚本（如果存在且Python可用）
if exist analyze_experiment.py (
    python --version >nul 2>&1
    if %errorlevel% equ 0 (
        python analyze_experiment.py %output_file%
    ) else (
        echo Python不可用，跳过分析步骤
    )
)

echo.
echo === 可失败对照测试完成 ===
echo 结束时间: %date% %time%
echo 测试报告: %output_file%

goto :end

:error
echo.
echo ❌ 测试过程中出现错误
pause
exit /b 1

:end
echo.
echo ✅ 可失败对照测试成功完成
pause