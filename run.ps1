# AgentHub desktop launcher (PowerShell)
$ErrorActionPreference = "Stop"
Set-Location -Path $PSScriptRoot

# Double-click / Explorer cmd often inherits a stale PATH. Merge Machine + User
# and put cargo/pnpm first so `tauri:dev` finds the same tools as a terminal.
$machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$extra = @(
    (Join-Path $env:USERPROFILE '.cargo\bin'),
    (Join-Path $env:LOCALAPPDATA 'pnpm')
) | Where-Object { $_ -and (Test-Path $_) }
$env:Path = (@($extra + $machinePath + $userPath + $env:Path) | Where-Object { $_ }) -join ';'

$runtime = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'scripts\dev-runtime.json') -Raw | ConvertFrom-Json
$DevPort = [int]$runtime.port

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

function Get-ListenersOnPort([int]$Port) {
    # Prefer Get-NetTCPConnection; fall back to netstat parsing.
    $rows = @()
    try {
        $conns = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction Stop
        foreach ($c in $conns) {
            $p = Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue
            $rows += [pscustomobject]@{
                Pid  = $c.OwningProcess
                Name = if ($p) { $p.ProcessName } else { '?' }
                Path = if ($p) { $p.Path } else { $null }
            }
        }
    } catch {
        $lines = netstat -ano -p tcp 2>$null | Select-String ":$Port\s+.*LISTENING\s+(\d+)\s*$"
        foreach ($m in $lines) {
            if ($m.Line -match 'LISTENING\s+(\d+)\s*$') {
                $procId = [int]$Matches[1]
                $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
                $rows += [pscustomobject]@{
                    Pid  = $procId
                    Name = if ($p) { $p.ProcessName } else { '?' }
                    Path = if ($p) { $p.Path } else { $null }
                }
            }
        }
    }
    $rows | Sort-Object Pid -Unique
}

function Ensure-DevPortFree([int]$Port) {
    $holders = @(Get-ListenersOnPort $Port)
    if ($holders.Count -eq 0) { return }

    Write-Host "[WARN] Port $Port is already in use (Vite/tauri:dev needs it exclusive)." -ForegroundColor Yellow
    foreach ($h in $holders) {
        Write-Host ("  - PID {0}  {1}  {2}" -f $h.Pid, $h.Name, $h.Path) -ForegroundColor DarkYellow
    }
    Write-Host ""
    Write-Host "Common causes: previous tauri:dev still running, or pnpm dev / dev:mock." -ForegroundColor DarkGray
    Write-Host "Press Y to stop those process(es) and continue, or any other key to abort." -ForegroundColor Yellow
    $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    if ($key.Character -ne 'y' -and $key.Character -ne 'Y') {
        Fail "Aborted: free port $Port then retry (e.g. stop old node/vite / close other AgentHub dev)."
    }

    foreach ($h in $holders) {
        try {
            Stop-Process -Id $h.Pid -Force -ErrorAction Stop
            Write-Host "[INFO] Stopped PID $($h.Pid) ($($h.Name))" -ForegroundColor Green
        } catch {
            Fail "Could not stop PID $($h.Pid): $($_.Exception.Message)"
        }
    }
    Start-Sleep -Milliseconds 400
    $still = @(Get-ListenersOnPort $Port)
    if ($still.Count -gt 0) {
        Fail "Port $Port still busy after kill. Close the process manually and retry."
    }
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

Write-Host ("[INFO] node {0} | pnpm {1}" -f (node -v), (pnpm -v)) -ForegroundColor DarkGray
Ensure-DevPortFree -Port $DevPort

# Installed release app may already be open; single-instance plugin focuses it.
$runningGui = Get-Process -Name "agenthub-gui" -ErrorAction SilentlyContinue
if ($runningGui) {
    Write-Host "[WARN] agenthub-gui already running (PID(s): $($runningGui.Id -join ', '))." -ForegroundColor Yellow
    Write-Host "       Second instances may exit immediately (single-instance). Close it if dev fails to open." -ForegroundColor DarkGray
}

Write-Host "[START] pnpm tauri:dev" -ForegroundColor Green
Write-Host "[INFO] Vite + Tauri desktop (real backend, not browser mock)" -ForegroundColor DarkGray
Write-Host "[INFO] First build may take a while. Ctrl+C to stop." -ForegroundColor DarkGray
Write-Host ""

pnpm tauri:dev
if ($LASTEXITCODE -ne 0) {
    Fail @"
tauri:dev exited with code $LASTEXITCODE

If you saw "Port $DevPort is already in use":
  - Close other pnpm dev / tauri:dev / AgentHub windows, then retry
  - Or: Get-NetTCPConnection -LocalPort $DevPort | % { Stop-Process -Id `$_.OwningProcess -Force }

If cargo/node not found when double-clicking:
  - Reopen terminal after install, or add cargo/node to user PATH
"@
}