# Q7 输入法开发版：TIP 注册 / 反注册（需要管理员权限——写入 HKLM 机器级）
#
# 用法（以管理员身份打开 PowerShell，在仓库根目录）：
#   powershell -ExecutionPolicy Bypass -File scripts\dev-register.ps1              # 注册（x64 Debug DLL）
#   powershell -ExecutionPolicy Bypass -File scripts\dev-register.ps1 -Unregister  # 反注册
#
# 注意：
#   - 必须管理员运行：语言配置写入 HKLM\SOFTWARE\Microsoft\CTF，未提权会以 E_FAIL 失败；
#   - 重新编译 DLL 前请先反注册（被宿主应用加载时文件被锁，会导致构建失败）；
#   - 注册后可在「设置 → 时间和语言 → 语言和区域 → 中文(简体) → 键盘」列表中
#     看到「Q7 拼音（开发版）」，用 Win+Space 切换到它。
param(
    [switch]$Unregister,
    [string]$DllPath = "$PSScriptRoot\..\target\debug\q7_tsf.dll"
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "需要管理员权限（注册写入 HKLM 机器级）。请以管理员身份打开 PowerShell 后重试。"
    exit 1
}

$dll = (Resolve-Path -LiteralPath $DllPath -ErrorAction Stop).Path
$regsvr = Join-Path $env:SystemRoot "System32\regsvr32.exe"

if ($Unregister) {
    & $regsvr /u /s $dll
    if ($LASTEXITCODE -ne 0) { Write-Error "regsvr32 反注册失败，退出码 $LASTEXITCODE"; exit $LASTEXITCODE }
    Write-Host "已反注册: $dll" -ForegroundColor Green
} else {
    & $regsvr /s $dll
    if ($LASTEXITCODE -ne 0) { Write-Error "regsvr32 注册失败，退出码 $LASTEXITCODE"; exit $LASTEXITCODE }
    Write-Host "已注册: $dll" -ForegroundColor Green
    Write-Host "切换方式: Win+Space 或任务栏输入法列表中选择「Q7 拼音（开发版）」。"
    Write-Host "使用前请先启动服务进程: cargo run -p q7-server"
}
