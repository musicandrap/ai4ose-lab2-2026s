# 环境验证脚本
# 在PowerShell中运行: .\check_environment.ps1

Write-Host "=== 同步原语实验环境验证 ===" -ForegroundColor Green

# 1. 检查Rust
Write-Host "`n1. 检查Rust工具链..." -ForegroundColor Yellow
try {
    $rustc_version = rustc --version
    Write-Host "   Rust版本: $rustc_version" -ForegroundColor Green
} catch {
    Write-Host "   ❌ Rust未安装" -ForegroundColor Red
    Write-Host "   请安装: https://rustup.rs/" -ForegroundColor Red
    exit 1
}

# 2. 检查Cargo
try {
    $cargo_version = cargo --version
    Write-Host "   Cargo版本: $cargo_version" -ForegroundColor Green
} catch {
    Write-Host "   ❌ Cargo未安装" -ForegroundColor Red
    exit 1
}

# 3. 检查RISC-V目标
try {
    $riscv_target = rustup target list | Select-String "riscv64gc-unknown-none-elf"
    if ($riscv_target -like "*installed*") {
        Write-Host "   RISC-V目标: 已安装" -ForegroundColor Green
    } else {
        Write-Host "   RISC-V目标: 未安装" -ForegroundColor Yellow
        Write-Host "   运行: rustup target add riscv64gc-unknown-none-elf" -ForegroundColor Yellow
    }
} catch {
    Write-Host "   无法检查RISC-V目标" -ForegroundColor Yellow
}

# 4. 检查Python
Write-Host "`n2. 检查Python环境..." -ForegroundColor Yellow
try {
    $python_version = python --version 2>&1
    Write-Host "   Python版本: $python_version" -ForegroundColor Green
} catch {
    Write-Host "   ❌ Python未安装" -ForegroundColor Red
    Write-Host "   请安装Python 3.7+" -ForegroundColor Red
}

# 5. 检查Python包
try {
    $pandas_check = python -c "import pandas; print('pandas: 已安装')" 2>&1
    Write-Host "   $pandas_check" -ForegroundColor Green
} catch {
    Write-Host "   pandas: 未安装" -ForegroundColor Yellow
}

try {
    $matplotlib_check = python -c "import matplotlib; print('matplotlib: 已安装')" 2>&1
    Write-Host "   $matplotlib_check" -ForegroundColor Green
} catch {
    Write-Host "   matplotlib: 未安装" -ForegroundColor Yellow
}

# 6. 检查项目构建
Write-Host "`n3. 检查项目构建..." -ForegroundColor Yellow
Set-Location $PSScriptRoot

try {
    cargo check --target riscv64gc-unknown-none-elf 2>&1 | Out-Null
    Write-Host "   内核构建: 正常" -ForegroundColor Green
} catch {
    Write-Host "   内核构建: 失败" -ForegroundColor Red
}

# 7. 检查用户程序构建
Set-Location "tg-rcore-tutorial-user"

try {
    cargo check --target riscv64gc-unknown-none-elf 2>&1 | Out-Null
    Write-Host "   用户程序构建: 正常" -ForegroundColor Green
} catch {
    Write-Host "   用户程序构建: 失败" -ForegroundColor Red
}

Set-Location $PSScriptRoot

Write-Host "`n=== 环境验证完成 ===" -ForegroundColor Green
Write-Host "如果所有检查都通过，您可以运行实验了！" -ForegroundColor Green
Write-Host "运行命令: .\batch_test.sh" -ForegroundColor Cyan