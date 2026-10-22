# 开发运行（debug 构建，带开发者工具）
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
if (Test-Path $toolchain) { $env:PATH = "$toolchain;$env:PATH" }

Push-Location (Join-Path $root 'src-tauri')
try {
    & cargo run @args
    exit $LASTEXITCODE
}
finally { Pop-Location }
