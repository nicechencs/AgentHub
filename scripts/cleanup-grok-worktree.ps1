# Remove the independent Grok worktree clone after WorkBuddy was merged into this repo.
# Run AFTER closing Grok/Claude sessions that use that folder as workspace.
# Usage (from anywhere):
#   powershell -ExecutionPolicy Bypass -File D:\demo_chen\2026\AgentHub\scripts\cleanup-grok-worktree.ps1

$ErrorActionPreference = "Stop"
$wt = "C:\Users\chen\.grok\worktrees\2026-agenthub\2026-08-02-1f4d5eb2"
$parent = "C:\Users\chen\.grok\worktrees\2026-agenthub"
$project = "D:\demo_chen\2026\AgentHub"

if (-not (Test-Path $project)) {
    Write-Error "Project not found: $project"
}

# Safety: never delete project
if ((Test-Path $wt) -and ((Resolve-Path $wt).Path -eq (Resolve-Path $project).Path)) {
    Write-Error "Refusing to delete project directory"
}

if (-not (Test-Path $wt)) {
    Write-Host "Already cleaned: $wt"
    exit 0
}

Write-Host "Removing: $wt"
# Drop heavy dirs first (often unlocks sooner)
foreach ($sub in @("target", "node_modules")) {
    $p = Join-Path $wt $sub
    if (Test-Path $p) {
        Write-Host "  rmdir $sub"
        cmd /c "rmdir /s /q `"$p`"" | Out-Null
    }
}

cmd /c "rmdir /s /q `"$wt`""
if (Test-Path $wt) {
    Remove-Item -LiteralPath $wt -Recurse -Force
}

if (Test-Path $wt) {
    Write-Error "Still locked. Close Grok/IDE terminals using that path, then re-run."
}

Write-Host "Removed OK: $wt"

if ((Test-Path $parent) -and -not (Get-ChildItem $parent -Force | Select-Object -First 1)) {
    Remove-Item -LiteralPath $parent -Force
    Write-Host "Removed empty parent: $parent"
}

Write-Host "Done. Project WorkBuddy still at: $project"
