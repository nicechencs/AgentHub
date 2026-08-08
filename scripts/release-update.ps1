#Requires -Version 5.1
<#
.SYNOPSIS
  AgentHub 桌面更新发版一键脚本：升版本 → 签名构建 → 生成 latest.json →（可选）上传 GitHub Release。

.DESCRIPTION
  默认读取 package.json 当前版本；传入 -Version 可指定新版本。
  签名私钥默认：%USERPROFILE%\.tauri\agenthub.key
  也可用环境变量 TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PATH。

.EXAMPLE
  # 用当前 package.json 版本构建 + 生成 latest.json（不改版本号、不上传）
  .\scripts\release-update.ps1

.EXAMPLE
  # 升到 0.2.0、写回三处版本、构建、生成清单
  .\scripts\release-update.ps1 -Version 0.2.0 -Bump -Notes "修复更新与用量统计"

.EXAMPLE
  # 构建并上传到 GitHub Latest Release（需要已安装 gh 且已登录）
  .\scripts\release-update.ps1 -Version 0.2.0 -Bump -Notes "..." -Publish

.EXAMPLE
  # 只根据已有构建产物生成 latest.json
  .\scripts\release-update.ps1 -SkipBuild -Version 0.2.0
#>
[CmdletBinding()]
param(
    [string]$Version = "",
    [switch]$Bump,
    [string]$Notes = "",
    [string]$Repo = "nicechencs/AgentHub",
    [string]$KeyPath = "",
    [string]$KeyPassword = "",
    [switch]$SkipBuild,
    [switch]$Publish,
    [switch]$DryRun,
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
Set-Location -Path (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Root = (Get-Location).Path

function Write-Step([string]$msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

function Write-Info([string]$msg) {
    Write-Host "    $msg" -ForegroundColor DarkGray
}

function Fail([string]$msg) {
    Write-Host ""
    Write-Host "[ERROR] $msg" -ForegroundColor Red
    exit 1
}

function Read-PackageVersion {
    $pkgPath = Join-Path $Root "package.json"
    if (-not (Test-Path $pkgPath)) { Fail "package.json not found" }
    $pkg = Get-Content $pkgPath -Raw -Encoding UTF8 | ConvertFrom-Json
    return [string]$pkg.version
}

function Set-ProjectVersion([string]$ver) {
    if ($ver -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
        Fail "Invalid semver: $ver (expect X.Y.Z)"
    }

    # package.json
    $pkgPath = Join-Path $Root "package.json"
    $pkgText = Get-Content $pkgPath -Raw -Encoding UTF8
    $pkgNew = [regex]::Replace($pkgText, '("version"\s*:\s*")[^"]+(")', "`${1}$ver`${2}", 1)
    if ($pkgNew -eq $pkgText) { Fail "Failed to patch package.json version" }
    Set-Content -Path $pkgPath -Value $pkgNew -Encoding UTF8 -NoNewline

    # Cargo.toml workspace.package.version
    $cargoPath = Join-Path $Root "Cargo.toml"
    $cargoText = Get-Content $cargoPath -Raw -Encoding UTF8
    $cargoNew = [regex]::Replace(
        $cargoText,
        '(?ms)(\[workspace\.package\]\s*?version\s*=\s*")[^"]+(")',
        "`${1}$ver`${2}",
        1
    )
    if ($cargoNew -eq $cargoText) {
        # fallback: first package-level version near top
        $cargoNew = [regex]::Replace($cargoText, '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$ver`${2}", 1)
    }
    if ($cargoNew -eq $cargoText) { Fail "Failed to patch Cargo.toml version" }
    Set-Content -Path $cargoPath -Value $cargoNew -Encoding UTF8 -NoNewline

    # tauri.conf.json
    $tauriPath = Join-Path $Root "src-tauri\tauri.conf.json"
    $tauriText = Get-Content $tauriPath -Raw -Encoding UTF8
    $tauriNew = [regex]::Replace($tauriText, '("version"\s*:\s*")[^"]+(")', "`${1}$ver`${2}", 1)
    if ($tauriNew -eq $tauriText) { Fail "Failed to patch tauri.conf.json version" }
    Set-Content -Path $tauriPath -Value $tauriNew -Encoding UTF8 -NoNewline

    Write-Info "Bumped version → $ver (package.json, Cargo.toml, tauri.conf.json)"
}

function Resolve-SigningKey {
    if ($env:TAURI_SIGNING_PRIVATE_KEY -and $env:TAURI_SIGNING_PRIVATE_KEY.Trim()) {
        Write-Info "Using TAURI_SIGNING_PRIVATE_KEY from environment"
        return
    }
    $path = $KeyPath
    if (-not $path) {
        $path = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
    }
    if (-not $path) {
        $path = Join-Path $env:USERPROFILE ".tauri\agenthub.key"
    }
    if (-not (Test-Path $path)) {
        Fail @"
Signing private key not found: $path

Generate once:
  pnpm exec tauri signer generate -w `"$env:USERPROFILE\.tauri\agenthub.key`" --ci -p `"`"

Or set:
  `$env:TAURI_SIGNING_PRIVATE_KEY_PATH = 'C:\path\to\agenthub.key'
  `$env:TAURI_SIGNING_PRIVATE_KEY = '<key contents>'
"@
    }
    $resolved = (Resolve-Path $path).Path
    $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $resolved
    # Prefer PATH only (do not also set PRIVATE_KEY — tauri rejects both).
    # Always set password env (empty string OK) so build never blocks on interactive prompt.
    if ($KeyPassword -ne "") {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $KeyPassword
    } elseif (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
    }
    # Clear content env if we are using path mode to avoid "cannot use both" errors.
    if ($env:TAURI_SIGNING_PRIVATE_KEY) {
        Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    }
    Write-Info "Signing key path: $resolved (password from env or empty)"
}

function Ensure-Tools([switch]$NeedGh) {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        Fail "Node.js not found. Install: https://nodejs.org/"
    }
    if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
        Fail "pnpm not found. Install: npm i -g pnpm"
    }
    if (-not $SkipBuild) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            Fail "Rust/Cargo not found. Install: https://rustup.rs/"
        }
    }
    if ($NeedGh) {
        if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
            Fail "GitHub CLI (gh) not found. Install: https://cli.github.com/  then: gh auth login"
        }
    }
    if (-not (Test-Path (Join-Path $Root "node_modules"))) {
        Write-Step "pnpm install"
        if ($DryRun) { Write-Info "(dry-run) skip"; return }
        pnpm install
        if ($LASTEXITCODE -ne 0) { Fail "pnpm install failed" }
    }
}

function Collect-Artifacts([string]$bundleDir, [string]$destDir) {
    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    $patterns = @(
        (Join-Path $bundleDir "nsis\*"),
        (Join-Path $bundleDir "msi\*"),
        (Join-Path $bundleDir "appimage\*"),
        (Join-Path $bundleDir "macos\*")
    )
    $copied = @()
    foreach ($pat in $patterns) {
        $items = Get-Item -Path $pat -ErrorAction SilentlyContinue |
            Where-Object {
                -not $_.PSIsContainer -and (
                    $_.Name -match '\.(exe|msi|sig|AppImage|tar\.gz)$' -or
                    $_.Name -like '*.exe.sig' -or
                    $_.Name -like '*.msi.sig' -or
                    $_.Name -like '*.AppImage.sig' -or
                    $_.Name -like '*.tar.gz.sig'
                )
            }
        foreach ($f in $items) {
            $target = Join-Path $destDir $f.Name
            Copy-Item -LiteralPath $f.FullName -Destination $target -Force
            $copied += $f.Name
        }
    }
    return $copied
}

# ---------- main ----------
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  AgentHub Release / Updater" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Root: $Root"

$current = Read-PackageVersion
if (-not $Version) {
    $Version = $current
    Write-Info "Version not specified, using package.json: $Version"
} else {
    Write-Info "Requested version: $Version (package.json now: $current)"
}

$tag = "v$($Version.TrimStart('v'))"
$Version = $Version.TrimStart('v')
$baseUrl = "https://github.com/$Repo/releases/download/$tag"

if (-not $OutDir) {
    $OutDir = Join-Path $Root "release-out\$Version"
}
$latestPath = Join-Path $OutDir "latest.json"

Ensure-Tools -NeedGh:$Publish

if ($Bump) {
    Write-Step "Bump project version to $Version"
    if ($DryRun) {
        Write-Info "(dry-run) would patch package.json / Cargo.toml / tauri.conf.json"
    } else {
        Set-ProjectVersion $Version
    }
}

if (-not $SkipBuild) {
    Write-Step "Configure signing + tauri build"
    if ($DryRun) {
        Write-Info "(dry-run) would set signing env and run: pnpm tauri:build"
    } else {
        Resolve-SigningKey
        Write-Info "Running pnpm tauri:build (first time may take long)..."
        pnpm tauri:build
        if ($LASTEXITCODE -ne 0) { Fail "tauri:build failed with code $LASTEXITCODE" }
    }
} else {
    Write-Step "Skip build (-SkipBuild)"
}

Write-Step "Collect artifacts → $OutDir"
$bundleDir = Join-Path $Root "target\release\bundle"
if ($DryRun) {
    Write-Info "(dry-run) would copy from $bundleDir"
} else {
    if (-not (Test-Path $bundleDir)) {
        Fail "Bundle dir missing: $bundleDir (build first or drop -SkipBuild)"
    }
    $copied = Collect-Artifacts $bundleDir $OutDir
    if ($copied.Count -eq 0) {
        Fail "No installers/signatures found under $bundleDir"
    }
    Write-Info ("Copied: " + ($copied -join ", "))
}

Write-Step "Generate latest.json"
$notesArg = $Notes
if (-not $notesArg) { $notesArg = "AgentHub $tag" }
if ($DryRun) {
    Write-Info "(dry-run) node scripts/build-latest-json.mjs --version $Version --base-url $baseUrl --out $latestPath"
} else {
    node (Join-Path $Root "scripts\build-latest-json.mjs") `
        --version $Version `
        --base-url $baseUrl `
        --notes $notesArg `
        --out $latestPath `
        --target-dir $bundleDir
    if ($LASTEXITCODE -ne 0) { Fail "build-latest-json failed" }
    if (-not (Test-Path $latestPath)) { Fail "latest.json not written: $latestPath" }
    # also place a copy next to installers (already in OutDir)
    Write-Info "Wrote $latestPath"
}

if ($Publish) {
    Write-Step "Publish GitHub Release $tag → $Repo"
    if ($DryRun) {
        Write-Info "(dry-run) gh release create $tag ... --repo $Repo"
    } else {
        $assets = Get-ChildItem -Path $OutDir -File | ForEach-Object { $_.FullName }
        if ($assets.Count -eq 0) { Fail "No assets in $OutDir" }

        $existing = gh release view $tag --repo $Repo 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Info "Release $tag exists — uploading/clobbering assets"
            gh release upload $tag @assets --repo $Repo --clobber
            if ($LASTEXITCODE -ne 0) { Fail "gh release upload failed" }
        } else {
            Write-Info "Creating release $tag"
            gh release create $tag @assets `
                --repo $Repo `
                --title $tag `
                --notes $notesArg `
                --latest
            if ($LASTEXITCODE -ne 0) { Fail "gh release create failed" }
        }
        Write-Info "Release URL: https://github.com/$Repo/releases/tag/$tag"
        Write-Info "Updater feed: https://github.com/$Repo/releases/latest/download/latest.json"
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Done" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host "Version : $Version"
Write-Host "Tag     : $tag"
Write-Host "OutDir  : $OutDir"
Write-Host "Feed URL: https://github.com/$Repo/releases/latest/download/latest.json"
Write-Host ""
if (-not $Publish) {
    Write-Host "Next: upload everything in OutDir to GitHub Release $tag" -ForegroundColor Yellow
    Write-Host "  (or re-run with -Publish if gh is logged in)" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  gh release create $tag `"$OutDir\*`" --repo $Repo --title $tag --notes `"$notesArg`" --latest" -ForegroundColor DarkGray
}
Write-Host ""
