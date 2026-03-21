@echo off
REM 同步原语实验运行脚本（Windows版本）
REM 在PowerShell或CMD中运行

echo === 同步原语实验开始 ===
echo 开始时间: %date% %time%

REM 创建结果目录
if not exist results mkdir results
if not exist logs mkdir logs

REM 1. 构建项目
echo.
echo [1/3] 构建内核和用户程序...
cd /d "%~dp0"

cargo build --target riscv64gc-unknown-none-elf > logs\build_kernel.log 2>&1
if %errorlevel% neq 0 (
    echo ❌ 内核构建失败，请查看 logs\build_kernel.log
    goto :error
)

echo ✅ 内核构建成功

cd tg-rcore-tutorial-user
cargo build --target riscv64gc-unknown-none-elf > ..\logs\build_user.log 2>&1
if %errorlevel% neq 0 (
    echo ❌ 用户程序构建失败，请查看 logs\build_user.log
    goto :error
)

echo ✅ 用户程序构建成功
cd ..

REM 2. 运行实验（模拟运行，因为实际需要QEMU环境）
echo.
echo [2/3] 运行实验...
REM 注意：实际环境中需要运行内核，这里创建模拟数据

set "output_file=results\experiment_%date:~0,4%%date:~5,2%%date:~8,2%_%time:~0,2%%time:~3,2%%time:~6,2%.txt"

echo 创建模拟实验数据到 %output_file%

REM 创建模拟CSV数据
echo EXPERIMENT_CSV_HEADER,lock_type,thread_count,total_time_ms,total_operations,throughput_ops_per_sec > %output_file%
echo EXPERIMENT_CSV,spinlock,2,1000,2000,2000.00 >> %output_file%
echo EXPERIMENT_CSV,mutex,2,1500,2000,1333.33 >> %output_file%
echo EXPERIMENT_CSV,spinlock,4,1200,4000,3333.33 >> %output_file%
echo EXPERIMENT_CSV,mutex,4,1800,4000,2222.22 >> %output_file%
echo EXPERIMENT_CSV,spinlock,8,1500,8000,5333.33 >> %output_file%
echo EXPERIMENT_CSV,mutex,8,2500,8000,3200.00 >> %output_file%

echo ✅ 实验数据生成完成

REM 3. 分析结果
echo.
echo [3/3] 分析实验结果...
python analyze_experiment.py %output_file%

if %errorlevel% neq 0 (
    echo ❌ 数据分析失败
    goto :error
)

echo.
echo === 实验完成 ===
echo 结束时间: %date% %time%
echo.
echo 生成的文件:
echo - %output_file% (实验数据)
echo - results\throughput_comparison.png (吞吐量对比图)
echo - results\execution_time.png (执行时间对比图)
echo - results\detailed_analysis.png (详细分析图)
echo - experiment_results.csv (详细数据表格)
echo.
echo 要查看图表，请打开 results 目录中的PNG文件
echo.
goto :end

:error
echo.
echo ❌ 实验过程中出现错误
echo 请检查日志文件:
echo - logs\build_kernel.log
echo - logs\build_user.log
echo.

:end
pause