# 编译检查（不产出 exe，最快）
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
if (Test-Path $toolchain) { $env:PATH = "$toolchain;$env:PATH" }

Push-Location (Join-Path $root 'src-tauri')
try {
    & cargo check @args
    exit $LASTEXITCODE
}
finally { Pop-Location }
