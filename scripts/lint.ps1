# 严格检查：clippy 警告视为错误 + 跑单元测试。提交/发布前跑这个。
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$toolchain = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
if (Test-Path $toolchain) { $env:PATH = "$toolchain;$env:PATH" }

Push-Location (Join-Path $root 'src-tauri')
try {
    Write-Host '== clippy (warnings = errors) ==' -ForegroundColor Cyan
    & cargo clippy --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host '== tests ==' -ForegroundColor Cyan
    & cargo test
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host '== frontend tests ==' -ForegroundColor Cyan
    & node (Join-Path $root 'scripts\test-frontend.js')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host 'OK: clippy clean, tests passed' -ForegroundColor Green
    exit 0
}
finally { Pop-Location }
