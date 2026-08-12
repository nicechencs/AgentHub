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
# Actions is the single release authority: it validates a clean release branch,
# claims an exact commit tag, and refuses existing releases/assets before any
# write. Keeping this guard before version/build work also makes accidental
# `-Publish` invocations side-effect free.
if ($Publish) {
    Fail "Local publishing is disabled. Push the release branch and let .github/workflows/release.yml publish the release."
}

function Read-PackageVersion {
    $pkgPath = Join-Path $Root "package.json"
    if (-not (Test-Path $pkgPath)) { Fail "package.json not found" }
    $pkg = Get-Content $pkgPath -Raw -Encoding UTF8 | ConvertFrom-Json
    return [string]$pkg.version
}

function Assert-ReleaseVersionsAligned {
    $metaScript = Join-Path $Root "scripts\release-metadata.mjs"
    if (-not (Test-Path $metaScript)) { Fail "scripts/release-metadata.mjs not found" }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        Fail "Node.js not found (needed to validate release versions)"
    }
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = & node $metaScript 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    if ($code -ne 0) {
        Fail ("Release version metadata invalid:`n" + ($output | Out-String).Trim())
    }
    return ($output | Out-String).Trim()
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

    Write-Info "Bumped version -> $ver (package.json, Cargo.toml, tauri.conf.json)"
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
    Write-Host "  git add package.json Cargo.toml Cargo.lock src-tauri/tauri.conf.json"
    Write-Host "  git commit -m `"chore(release): bump version to $Version`""
    Write-Host "  git push origin release"
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
Write-Host "Publishing is CI-only: push the release branch and let .github/workflows/release.yml publish the release." -ForegroundColor Yellow
Write-Host ""
