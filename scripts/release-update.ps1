#Requires -Version 5.1
<#
.SYNOPSIS
  AgentHub 桌面更新发版准备脚本：升版本 → 签名构建 → 生成 latest.json（发布统一由 CI 完成）。

.DESCRIPTION
  默认读取 package.json 当前版本；传入 -Version 可指定新版本。
  也可用 -BumpPatch / -BumpMinor / -BumpMajor 从当前版本自动 +1，并查询远端
  tag/release：若已占用则继续 patch +1，直到找到空闲版本（最多尝试 50 次）。
  -VersionOnly 只写回三处版本号，不构建、不生成 latest.json。
  签名私钥默认：%USERPROFILE%\.tauri\agenthub.key
  也可用环境变量 TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PATH。

.EXAMPLE
  # 用当前 package.json 版本构建 + 生成 latest.json（不改版本号、不上传）
  .\scripts\release-update.ps1

.EXAMPLE
  # 自动 patch 升版（0.2.1 → 0.2.2，若 v0.2.2 已占用则试 0.2.3…）并只改版本
  .\scripts\release-update.ps1 -BumpPatch -VersionOnly

.EXAMPLE
  # 升到 0.2.0、写回三处版本、构建、生成清单
  .\scripts\release-update.ps1 -Version 0.2.0 -Bump -Notes "修复更新与用量统计"

.EXAMPLE
  # 只根据已有构建产物生成 latest.json
  .\scripts\release-update.ps1 -SkipBuild -Version 0.2.0
#>
[CmdletBinding()]
param(
    [string]$Version = "",
    [switch]$Bump,
    [switch]$BumpPatch,
    [switch]$BumpMinor,
    [switch]$BumpMajor,
    [switch]$VersionOnly,
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

# Publishing from a developer worktree is intentionally disabled. GitHub
# Actions is the single release authority: it validates a v* tag on the
# release branch and refuses existing releases/assets before any write.
# Keeping this guard before version/build work also makes accidental
# `-Publish` invocations side-effect free.
if ($Publish) {
    Fail "Local publishing is disabled. Merge dev into release, tag on dev, push the v* tag after release:preflight passes, and let .github/workflows/release.yml publish the release."
}

function Read-PackageVersion {
    $pkgPath = Join-Path $Root "package.json"
    if (-not (Test-Path $pkgPath)) { Fail "package.json not found" }
    $pkg = Get-Content $pkgPath -Raw -Encoding UTF8 | ConvertFrom-Json
    return [string]$pkg.version
}

function Assert-ReleaseVersionsAligned([switch]$ThrowOnError) {
    $metaScript = Join-Path $Root "scripts\release-metadata.mjs"
    if (-not (Test-Path $metaScript)) {
        if ($ThrowOnError) { throw "scripts/release-metadata.mjs not found" }
        Fail "scripts/release-metadata.mjs not found"
    }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        if ($ThrowOnError) { throw "Node.js not found (needed to validate release versions)" }
        Fail "Node.js not found (needed to validate release versions)"
    }
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = & node $metaScript 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    if ($code -ne 0) {
        $message = "Release version metadata invalid:`n" + ($output | Out-String).Trim()
        if ($ThrowOnError) { throw $message }
        Fail $message
    }
    return ($output | Out-String).Trim()
}

function Get-Utf8NoBomText([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) {
        return [System.Text.Encoding]::UTF8.GetString($bytes, 3, $bytes.Length - 3)
    }
    return [System.Text.Encoding]::UTF8.GetString($bytes)
}

function Write-Utf8NoBomTemp([string]$Path, [string]$Content) {
    $utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
    $stream = New-Object System.IO.FileStream -ArgumentList @(
        $Path,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::None
    )
    try {
        $bytes = $utf8NoBom.GetBytes($Content)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
}

function Get-CargoLockWorkspaceText([string]$ver) {
    $lockPath = Join-Path $Root "Cargo.lock"
    if (-not (Test-Path -LiteralPath $lockPath)) {
        Fail "Cargo.lock not found"
    }
    $lockText = Get-Utf8NoBomText $lockPath
    $updated = $lockText
    $workspacePackages = @('agenthub-cli', 'agenthub-core', 'agenthub-gui')
    foreach ($name in $workspacePackages) {
        $namePattern = [regex]::Escape($name)
        $pattern = '(?ms)(\[\[package\]\]\s*name\s*=\s*"' + $namePattern + '"\s*version\s*=\s*")[^"]+(")'
        $matches = [regex]::Matches($updated, $pattern)
        if ($matches.Count -ne 1) {
            Fail "Cargo.lock must contain exactly one workspace package entry for $name"
        }
        $updated = [regex]::Replace($updated, $pattern, ('${1}' + $ver + '$2'), 1)
    }
    return $updated
}

function Assert-ReleaseContentPlan($plan, [string]$ver) {
    if ($plan.Count -ne 4) {
        throw "Release version transaction must contain exactly four files"
    }

    $package = $plan | Where-Object { $_.Name -eq 'package.json' }
    $cargo = $plan | Where-Object { $_.Name -eq 'Cargo.toml' }
    $tauri = $plan | Where-Object { $_.Name -eq 'src-tauri/tauri.conf.json' }
    $lock = $plan | Where-Object { $_.Name -eq 'Cargo.lock' }
    if (-not $package -or -not $cargo -or -not $tauri -or -not $lock) {
        throw "Release version transaction is missing one of package.json, Cargo.toml, src-tauri/tauri.conf.json, Cargo.lock"
    }

    try {
        $packageVersion = [string]((ConvertFrom-Json -InputObject $package.Updated).version)
    } catch {
        $packageVersion = ''
    }
    if ($packageVersion -ne $ver) {
        throw "Generated package.json version '$packageVersion' does not equal '$ver'"
    }

    $cargoVersionMatch = [regex]::Match(
        $cargo.Updated,
        '(?ms)\[workspace\.package\]\s*?version\s*=\s*"([^"]+)"'
    )
    if (-not $cargoVersionMatch.Success -or $cargoVersionMatch.Groups[1].Value -ne $ver) {
        throw "Generated Cargo.toml workspace version does not equal '$ver'"
    }

    try {
        $tauriVersion = [string]((ConvertFrom-Json -InputObject $tauri.Updated).version)
    } catch {
        $tauriVersion = ''
    }
    if ($tauriVersion -ne $ver) {
        throw "Generated tauri.conf.json version '$tauriVersion' does not equal '$ver'"
    }

    foreach ($name in @('agenthub-cli', 'agenthub-core', 'agenthub-gui')) {
        $pattern = '(?ms)\[\[package\]\]\s*name\s*=\s*"' + [regex]::Escape($name) + '"\s*version\s*=\s*"([^"]+)"'
        $matches = [regex]::Matches($lock.Updated, $pattern)
        if ($matches.Count -ne 1 -or $matches[0].Groups[1].Value -ne $ver) {
            throw "Generated Cargo.lock workspace package '$name' does not equal '$ver'"
        }
    }
}

function Assert-ReleaseFilesOnDisk($plan, [string]$ver) {
    $diskPlan = @()
    foreach ($item in $plan) {
        $diskPlan += [pscustomobject]@{
            Name = $item.Name
            Path = $item.Path
            Updated = Get-Utf8NoBomText $item.Path
        }
    }
    Assert-ReleaseContentPlan $diskPlan $ver
    if ($env:AGENTHUB_RELEASE_FAIL_POSTCHECK -eq '1') {
        throw 'Injected release post-check failure'
    }
}

function Restore-ReleaseVersionTransaction($plan) {
    $restoreErrors = @()
    $restoreCount = 0
    for ($index = $plan.Count - 1; $index -ge 0; $index--) {
        $item = $plan[$index]
        if (-not (Test-Path -LiteralPath $item['Backup'])) {
            continue
        }
        $restoreCount++
        $scratch = "$($item['Backup']).rollback-new"
        try {
            if ($env:AGENTHUB_RELEASE_FAIL_ROLLBACK_AT -and [int]$env:AGENTHUB_RELEASE_FAIL_ROLLBACK_AT -eq $restoreCount) {
                throw "Injected release rollback failure at file $restoreCount"
            }
            if (Test-Path -LiteralPath $scratch) {
                throw "Rollback scratch already exists: $([System.IO.Path]::GetFullPath($scratch))"
            }
            $item['RollbackScratch'] = $scratch
            if (Test-Path -LiteralPath $item['Path']) {
                # .NET Framework rejects a null destination backup path. Keep
                # the displaced new file in a unique scratch path, then let
                # finally remove that non-authoritative copy.
                [System.IO.File]::Replace($item['Backup'], $item['Path'], $scratch, $true)
            } else {
                [System.IO.File]::Move($item['Backup'], $item['Path'])
            }
        } catch {
            $absoluteBackup = [System.IO.Path]::GetFullPath($item['Backup'])
            $restoreErrors += "$($item['Name']): $($_.Exception.Message); backup retained at $absoluteBackup"
        } finally {
            if ($item['RollbackScratch'] -and (Test-Path -LiteralPath $item['RollbackScratch'])) {
                Remove-Item -LiteralPath $item['RollbackScratch'] -Force -ErrorAction SilentlyContinue
            }
        }
    }
    if ($restoreErrors.Count -gt 0) {
        throw "Release version rollback failed: $($restoreErrors -join '; ')"
    }
}

function Set-ReleaseVersionTransaction([string]$ver, $plan) {
    $transactionId = "$PID-$([Guid]::NewGuid().ToString('N'))"
    foreach ($item in $plan) {
        $item['Temp'] = Join-Path (Split-Path -Parent $item['Path']) ".$(Split-Path -Leaf $item['Path']).agenthub-$transactionId.tmp"
        $item['Backup'] = Join-Path (Split-Path -Parent $item['Path']) ".$(Split-Path -Leaf $item['Path']).agenthub-$transactionId.bak"
    }

    $transactionSucceeded = $false
    try {
        # Prepare every temp file before replacing any destination. This is the
        # transaction's pre-commit phase: parse/shape/version validation must
        # succeed while all original files are still untouched.
        Assert-ReleaseContentPlan $plan $ver
        foreach ($item in $plan) {
            if (-not (Test-Path -LiteralPath $item['Path'])) {
                throw "Release file not found: $($item['Path'])"
            }
            Write-Utf8NoBomTemp $item['Temp'] $item['Updated']
        }

        $replaceCount = 0
        foreach ($item in $plan) {
            $replaceCount++
            if ($env:AGENTHUB_RELEASE_FAIL_REPLACE_AT -and [int]$env:AGENTHUB_RELEASE_FAIL_REPLACE_AT -eq $replaceCount) {
                throw "Injected release replace failure at file $replaceCount"
            }
            # Every destination exists in a valid checkout. File.Replace moves
            # the original into the transaction backup and atomically installs
            # the prepared no-BOM temp file in its place.
            [System.IO.File]::Replace($item['Temp'], $item['Path'], $item['Backup'], $true)
        }

        Assert-ReleaseFilesOnDisk $plan $ver
        # Run the same repository-level metadata validator used before and
        # after the bump while rollback backups are still available. The
        # ThrowOnError mode is essential here: Fail exits the process and
        # would bypass this transaction's recovery path.
        Assert-ReleaseVersionsAligned -ThrowOnError | Out-Null
        $transactionSucceeded = $true
    } catch {
        $failure = $_.Exception.Message
        try {
            Restore-ReleaseVersionTransaction $plan
        } catch {
            throw "$failure; $($_.Exception.Message)"
        }
        throw $failure
    } finally {
        foreach ($item in $plan) {
            if ($item['Temp'] -and (Test-Path -LiteralPath $item['Temp'])) {
                Remove-Item -LiteralPath $item['Temp'] -Force -ErrorAction SilentlyContinue
            }
            if ($item['RollbackScratch'] -and (Test-Path -LiteralPath $item['RollbackScratch'])) {
                Remove-Item -LiteralPath $item['RollbackScratch'] -Force -ErrorAction SilentlyContinue
            }
            # A backup is the only durable copy of the original after a
            # successful File.Replace. On failure, never remove one that still
            # exists: a rollback error intentionally leaves it for recovery.
            if ($transactionSucceeded -and $item['Backup'] -and (Test-Path -LiteralPath $item['Backup'])) {
                Remove-Item -LiteralPath $item['Backup'] -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

function Get-CoreSemVerParts([string]$ver) {
    # Strip build metadata and prerelease so auto-bump always produces X.Y.Z.
    $core = ($ver.Split('+', 2)[0]).Split('-', 2)[0]
    if ($core -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$') {
        Fail "Cannot auto-bump version '$ver' (need strict X.Y.Z core)"
    }
    return [pscustomobject]@{
        Major = [int]$Matches[1]
        Minor = [int]$Matches[2]
        Patch = [int]$Matches[3]
    }
}

function Format-SemVer([int]$Major, [int]$Minor, [int]$Patch) {
    return "$Major.$Minor.$Patch"
}

function Get-NextSemVer([string]$current, [string]$kind) {
    $p = Get-CoreSemVerParts $current
    switch ($kind) {
        'patch' { return Format-SemVer $p.Major $p.Minor ($p.Patch + 1) }
        'minor' { return Format-SemVer $p.Major ($p.Minor + 1) 0 }
        'major' { return Format-SemVer ($p.Major + 1) 0 0 }
        default { Fail "Unknown bump kind: $kind" }
    }
}

function Test-RemoteReleaseTaken([string]$tag, [string]$repository) {
    # Prefer git ls-remote for tags; fall back to gh for Release objects without a tag.
    $tagRefs = $null
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $tagRefs = & git ls-remote --refs origin "refs/tags/$tag" 2>$null
    $gitCode = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    if ($gitCode -ne 0) {
        Fail "Unable to query remote tag $tag via git ls-remote; refuse to auto-bump without a definitive result."
    }
    if ($tagRefs -and ("$tagRefs".Trim().Length -gt 0)) {
        return $true
    }

    if (Get-Command gh -ErrorAction SilentlyContinue) {
        $ErrorActionPreference = "Continue"
        $releaseId = & gh api graphql `
            -f query='query($owner: String!, $name: String!, $tag: String!) { repository(owner: $owner, name: $name) { release(tagName: $tag) { id } } }' `
            -F owner="$($repository.Split('/')[0])" `
            -F name="$($repository.Split('/')[1])" `
            -F tag="$tag" `
            --jq '.data.repository.release.id // empty' 2>$null
        $ghCode = $LASTEXITCODE
        $ErrorActionPreference = $prevEap
        if ($ghCode -eq 0 -and $releaseId -and ("$releaseId".Trim().Length -gt 0)) {
            return $true
        }
        # gh failure after a successful empty tag query: treat as not taken only when
        # gh is unavailable/auth-less; do not block local bumps on optional gh checks.
    }
    return $false
}

function Resolve-FreeReleaseVersion([string]$startVersion, [string]$repository, [int]$maxAttempts = 50) {
    $candidate = $startVersion.TrimStart('v')
    for ($i = 0; $i -lt $maxAttempts; $i++) {
        $tag = "v$candidate"
        if (-not (Test-RemoteReleaseTaken $tag $repository)) {
            if ($i -gt 0) {
                Write-Info "Skipped $i occupied version(s); free version is $candidate"
            }
            return $candidate
        }
        Write-Info "Remote already has $tag; trying next patch..."
        $candidate = Get-NextSemVer $candidate 'patch'
    }
    Fail "Could not find a free release version after $maxAttempts attempts (last tried v$candidate)."
}

function Set-ProjectVersion([string]$ver) {
    if ($ver -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
        Fail "Invalid semver: $ver (expect X.Y.Z)"
    }

    # Build all four new file contents in memory first. No destination is
    # opened for writing until every generated version and lock entry has been
    # validated by Set-ReleaseVersionTransaction.
    # package.json
    $pkgPath = Join-Path $Root "package.json"
    $pkgText = Get-Utf8NoBomText $pkgPath
    $pkgNew = [regex]::Replace($pkgText, '("version"\s*:\s*")[^"]+(")', "`${1}$ver`${2}", 1)
    if ($pkgNew -eq $pkgText) { Fail "Failed to patch package.json version" }


    # Cargo.toml workspace.package.version
    $cargoPath = Join-Path $Root "Cargo.toml"
    $cargoText = Get-Utf8NoBomText $cargoPath
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

    # tauri.conf.json
    $tauriPath = Join-Path $Root "src-tauri\tauri.conf.json"
    $tauriText = Get-Utf8NoBomText $tauriPath
    $tauriNew = [regex]::Replace($tauriText, '("version"\s*:\s*")[^"]+(")', "`${1}$ver`${2}", 1)
    if ($tauriNew -eq $tauriText) { Fail "Failed to patch tauri.conf.json version" }

    # Cargo.lock contains package records for all three local workspace
    # crates. Refresh only those version fields; dependency resolution and
    # registry checks must remain untouched by a release version bump.
    $lockPath = Join-Path $Root "Cargo.lock"
    $lockNew = Get-CargoLockWorkspaceText $ver

    $plan = @(
        [ordered]@{ Name = 'package.json'; Path = $pkgPath; Updated = $pkgNew; Temp = $null; Backup = $null; RollbackScratch = $null },
        [ordered]@{ Name = 'Cargo.toml'; Path = $cargoPath; Updated = $cargoNew; Temp = $null; Backup = $null; RollbackScratch = $null },
        [ordered]@{ Name = 'src-tauri/tauri.conf.json'; Path = $tauriPath; Updated = $tauriNew; Temp = $null; Backup = $null; RollbackScratch = $null },
        [ordered]@{ Name = 'Cargo.lock'; Path = $lockPath; Updated = $lockNew; Temp = $null; Backup = $null; RollbackScratch = $null }
    )
    try {
        Set-ReleaseVersionTransaction $ver $plan
    } catch {
        # Keep transaction failures as one deliberate line. Letting an
        # unhandled PowerShell ErrorRecord format the exception would add
        # invocation/stack text and can split the retained backup path.
        Fail $_.Exception.Message
    }
    Write-Info "Bumped version -> $ver (package.json, Cargo.toml, tauri.conf.json, Cargo.lock)"
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
    # Tauri on Windows often fails to load TAURI_SIGNING_PRIVATE_KEY_PATH
    # ("public key found, but no private key"). Prefer file contents in
    # TAURI_SIGNING_PRIVATE_KEY and clear PATH to avoid "cannot use both".
    $keyBody = (Get-Content -LiteralPath $resolved -Raw -Encoding UTF8).Trim()
    if (-not $keyBody) { Fail "Signing key file is empty: $resolved" }
    $env:TAURI_SIGNING_PRIVATE_KEY = $keyBody
    if ($env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
        Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PATH -ErrorAction SilentlyContinue
    }
    # Always set password env (empty string OK) so build never blocks on interactive prompt.
    if ($KeyPassword -ne "") {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $KeyPassword
    } elseif (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
    }
    Write-Info "Signing key loaded from: $resolved (password from env or empty)"
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

function Collect-Artifacts([string]$bundleDir, [string]$destDir, [string]$ver) {
    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    # Drop stale assets from a previous partial release into the same OutDir.
    Get-ChildItem -Path $destDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne 'latest.json' } |
        Remove-Item -Force -ErrorAction SilentlyContinue

    $patterns = @(
        (Join-Path $bundleDir "nsis\*"),
        (Join-Path $bundleDir "msi\*"),
        (Join-Path $bundleDir "appimage\*"),
        (Join-Path $bundleDir "macos\*")
    )
    $verToken = $ver.TrimStart('v')
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
                ) -and (
                    # Prefer versioned installer names (AgentHub_0.2.0_...); keep
                    # unversioned updater tarballs (common on macOS).
                    ($_.Name -like "*${verToken}*") -or
                    ($_.Name -notmatch '_\d+\.\d+\.\d+')
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

$autoBumpKinds = @()
if ($BumpPatch) { $autoBumpKinds += 'patch' }
if ($BumpMinor) { $autoBumpKinds += 'minor' }
if ($BumpMajor) { $autoBumpKinds += 'major' }
if ($autoBumpKinds.Count -gt 1) {
    Fail "Use only one of -BumpPatch / -BumpMinor / -BumpMajor"
}
if ($autoBumpKinds.Count -eq 1 -and $Version) {
    Fail "-Version cannot be combined with -BumpPatch / -BumpMinor / -BumpMajor"
}
if ($VersionOnly -and -not ($Bump -or $autoBumpKinds.Count -eq 1 -or $Version)) {
    Fail "-VersionOnly requires -BumpPatch / -BumpMinor / -BumpMajor, or -Version with -Bump"
}

# VersionOnly implies no local build/artifact work; publishing remains CI-only.
if ($VersionOnly) {
    $SkipBuild = $true
}

$current = Read-PackageVersion
Write-Step "Validate current release metadata"
$metaJson = Assert-ReleaseVersionsAligned
Write-Info $metaJson

$willBump = $false
$versionExplicit = $PSBoundParameters.ContainsKey('Version') -and [string]$PSBoundParameters['Version']

if ($autoBumpKinds.Count -eq 1) {
    $kind = $autoBumpKinds[0]
    $computed = Get-NextSemVer $current $kind
    Write-Step "Auto-bump $kind from $current -> candidate $computed"
    Write-Info "Checking remote tags/releases for a free version..."
    $Version = Resolve-FreeReleaseVersion $computed $Repo
    $willBump = $true
    Write-Info "Selected free version: $Version (from $current)"
} elseif ($versionExplicit) {
    $Version = $Version.TrimStart('v')
    Write-Info "Requested version: $Version (package.json now: $current)"
    if ($Bump -or $VersionOnly) {
        Write-Info "Checking that v$Version is free on origin..."
        if (Test-RemoteReleaseTaken "v$Version" $Repo) {
            Fail "Release tag v$Version already exists; choose another -Version or use -BumpPatch"
        }
        $willBump = $true
    } elseif ($Version -ne $current) {
        # Building a different version than package.json without -Bump is allowed
        # only for artifact packaging; files are not rewritten.
        Write-Info "Using -Version $Version without rewriting project files (no -Bump)"
    }
} else {
    $Version = $current
    Write-Info "Version not specified, using package.json: $Version"
}

# Bare -Bump (no -Version, no -BumpPatch/Minor/Major) → patch auto-bump.
if ($Bump -and -not $willBump -and $autoBumpKinds.Count -eq 0 -and -not $versionExplicit) {
    Write-Step "Auto-bump patch from $current (bare -Bump)"
    $computed = Get-NextSemVer $current 'patch'
    $Version = Resolve-FreeReleaseVersion $computed $Repo
    $willBump = $true
    Write-Info "Selected free version: $Version"
} elseif ($Bump -and -not $willBump -and $versionExplicit -and $Version -eq $current) {
    Fail "Nothing to bump: package.json is already $current. Pass a new -Version or use -BumpPatch."
}

$tag = "v$($Version.TrimStart('v'))"
$Version = $Version.TrimStart('v')
$baseUrl = "https://github.com/$Repo/releases/download/$tag"

if (-not $OutDir) {
    $OutDir = Join-Path $Root "release-out\$Version"
}
$latestPath = Join-Path $OutDir "latest.json"

if (-not $VersionOnly) {
    Ensure-Tools -NeedGh:$Publish
} elseif (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Fail "Node.js not found"
}

if ($willBump) {
    Write-Step "Bump project version to $Version"
    if ($DryRun) {
        Write-Info "(dry-run) would patch package.json / Cargo.toml / tauri.conf.json"
    } else {
        Set-ProjectVersion $Version
        $metaAfter = Assert-ReleaseVersionsAligned
        Write-Info "Post-bump metadata: $metaAfter"
    }
}

if ($VersionOnly) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  Version bumped (no build)" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Version : $Version"
    Write-Host "Tag     : $tag"
    Write-Host ""
    Write-Host "Next:" -ForegroundColor Yellow
    Write-Host "  git add package.json Cargo.toml Cargo.lock"
    Write-Host "  git commit -m `"chore(release): bump version to $Version`""
    Write-Host "  git push origin dev"
    Write-Host "  pnpm release:preflight"
    Write-Host "  git tag -a $tag -m `"AgentHub $tag`""
    Write-Host "  git push origin $tag"
    Write-Host "  # After GitHub Release succeeds, merge dev into release."
    Write-Host ""
    exit 0
}

if (-not $SkipBuild) {
    Write-Step "Configure signing + tauri build"
    if ($DryRun) {
        Write-Info "(dry-run) would set signing env and run: pnpm tauri:build -- --locked"
    } else {
        Resolve-SigningKey
        Write-Info "Running pnpm tauri:build -- --locked (first time may take long)..."
        pnpm tauri:build -- --locked
        if ($LASTEXITCODE -ne 0) { Fail "tauri:build failed with code $LASTEXITCODE" }
    }
} else {
    Write-Step "Skip build (-SkipBuild)"
}

Write-Step "Collect artifacts -> $OutDir"
$bundleDir = Join-Path $Root "target\release\bundle"
if ($DryRun) {
    Write-Info "(dry-run) would copy from $bundleDir"
} else {
    if (-not (Test-Path $bundleDir)) {
        Fail "Bundle dir missing: $bundleDir (build first or drop -SkipBuild)"
    }
    $copied = Collect-Artifacts $bundleDir $OutDir $Version
    if ($copied.Count -eq 0) {
        Fail "No installers/signatures for version $Version under $bundleDir"
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

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Done" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host "Version : $Version"
Write-Host "Tag     : $tag"
Write-Host "OutDir  : $OutDir"
Write-Host "Feed URL: https://github.com/$Repo/releases/latest/download/latest.json"
Write-Host ""
Write-Host "Publishing is CI-only: merge dev into release, tag on dev, push the v* tag after release:preflight passes, and let .github/workflows/release.yml publish the release." -ForegroundColor Yellow
Write-Host "After GitHub Release succeeds, merge dev into release." -ForegroundColor Yellow
Write-Host ""
