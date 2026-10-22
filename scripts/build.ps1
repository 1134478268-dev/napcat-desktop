# 发布构建：release 编译 + 把 exe 复制到项目根目录
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
if (Test-Path $toolchain) { $env:PATH = "$toolchain;$env:PATH" }

Push-Location (Join-Path $root 'src-tauri')
try {
    & cargo build --release @args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }

$built = Join-Path $root 'src-tauri\target\release\napcat-desktop.exe'
$target = Join-Path $root 'NapCatDesktop.exe'

if (-not (Test-Path $built)) {
    Write-Error "未找到构建产物: $built"
    exit 1
}

try {
    Copy-Item $built $target -Force
    Write-Host "已更新: $target"
}
catch {
    Write-Warning "复制失败（目标可能正在运行，请先退出 NapCat Desktop）: $_"
    exit 1
}
