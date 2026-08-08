# AgentHub desktop launcher (PowerShell)
$ErrorActionPreference = "Stop"
Set-Location -Path $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  AgentHub Desktop Launcher" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Working dir: $PWD"
Write-Host ""

function Fail([string]$msg) {
    Write-Host "[ERROR] $msg" -ForegroundColor Red
    Write-Host ""
    Write-Host "Press any key to close..."
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    exit 1
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Fail "Node.js not found. Install: https://nodejs.org/"
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "Rust/Cargo not found. Install: https://rustup.rs/ and ensure %USERPROFILE%\.cargo\bin is in PATH"
}
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    Write-Host "[INFO] pnpm not found, installing via npm..." -ForegroundColor Yellow
    npm install -g pnpm
    if ($LASTEXITCODE -ne 0) { Fail "pnpm install failed" }
}
if (-not (Test-Path "node_modules")) {
    Write-Host "[INFO] Installing deps: pnpm install..." -ForegroundColor Yellow
    pnpm install
    if ($LASTEXITCODE -ne 0) { Fail "pnpm install failed" }
    Write-Host ""
}

Write-Host "[START] pnpm tauri:dev" -ForegroundColor Green
Write-Host "[INFO] Vite + Tauri desktop (real backend, not browser mock)" -ForegroundColor DarkGray
Write-Host "[INFO] First build may take a while. Ctrl+C to stop." -ForegroundColor DarkGray
Write-Host ""

pnpm tauri:dev
if ($LASTEXITCODE -ne 0) {
    Fail "tauri:dev exited with code $LASTEXITCODE"
}