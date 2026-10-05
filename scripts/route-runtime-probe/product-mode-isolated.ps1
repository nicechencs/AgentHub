# Dormant Product-mode supervisor real-process probe. Windows only.
#
# This script intentionally performs two separate disposable runs. The first
# covers the normal Product lifecycle. The second leaves the ready Rust probe
# alive only long enough to force-terminate it, proving the Go child belongs to
# its kill-on-close Job Object rather than merely observing orderly shutdown.

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

if (-not $IsWindows) {
    throw 'FAIL: this probe requires Windows'
}

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$scratchRoots = [System.Collections.Generic.List[string]]::new()
$mockProcess = $null
$heldProbeProcess = $null
$succeeded = $false

function New-ProbeScratch {
    $path = Join-Path ([IO.Path]::GetTempPath()) ("agenthub-product-go-route.{0}" -f [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($path) | Out-Null
    $scratchRoots.Add($path)
    return $path
}

function Test-LoopbackPortAvailable {
    param([Parameter(Mandatory = $true)][int]$Port)

    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $Port)
    try {
        $listener.Start()
        return $true
    } catch {
        return $false
    } finally {
        $listener.Stop()
    }
}

function Assert-LoopbackPortAvailable {
    param([Parameter(Mandatory = $true)][int]$Port, [Parameter(Mandatory = $true)][string]$Step)

    if (-not (Test-LoopbackPortAvailable -Port $Port)) {
        throw "FAIL: loopback port $Port is unavailable at $Step"
    }
}

function Get-FreeLoopbackPort {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

function Wait-ForCondition {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Condition,
        [Parameter(Mandatory = $true)][string]$Description,
        [int]$TimeoutSeconds = 30
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 100
    }
    throw "FAIL: timed out waiting for $Description"
}

function Test-ProcessRunning {
    param([Parameter(Mandatory = $true)][int]$Id)

    try {
        $process = Get-Process -Id $Id -ErrorAction Stop
        return -not $process.HasExited
    } catch {
        return $false
    }
}

function Read-LastJsonEvidence {
    param([Parameter(Mandatory = $true)][string]$Path)

    $row = Get-Content -LiteralPath $Path | Where-Object { $_.TrimStart().StartsWith('{') } | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($row)) {
        throw "FAIL: probe emitted no JSON evidence: $Path"
    }
    return ($row | ConvertFrom-Json)
}

function Assert-TrueEvidenceFields {
    param(
        [Parameter(Mandatory = $true)]$Evidence,
        [Parameter(Mandatory = $true)][string[]]$Fields,
        [Parameter(Mandatory = $true)][string]$Description
    )

    foreach ($field in $Fields) {
        if ($Evidence.$field -ne $true) {
            throw "FAIL: $Description omitted successful $field evidence"
        }
    }
}

function Assert-PrivateDacl {
    param([Parameter(Mandatory = $true)][string[]]$Paths)

    $ownerRightsSid = 'S-1-3-4'
    $systemSid = 'S-1-5-18'
    $expectedInheritance = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit
    $fullControl = [Security.AccessControl.FileSystemRights]::FullControl
    $allow = [Security.AccessControl.AccessControlType]::Allow
    foreach ($path in $Paths) {
        $acl = Get-Acl -LiteralPath $path
        if (-not $acl.AreAccessRulesProtected) {
            throw "FAIL: Product directory DACL is inheriting: $path"
        }
        $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
        if ($rules.Count -ne 2) {
            throw "FAIL: Product directory DACL does not use the minimal two-ACE allow list: $path"
        }
        $seenSids = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($rule in $rules) {
            $sid = $rule.IdentityReference.Value
            if ($rule.AccessControlType -ne $allow -or
                ($sid -ne $ownerRightsSid -and $sid -ne $systemSid) -or
                $rule.FileSystemRights -ne $fullControl -or
                $rule.InheritanceFlags -ne $expectedInheritance -or
                $rule.PropagationFlags -ne [Security.AccessControl.PropagationFlags]::None -or
                $rule.IsInherited) {
                throw "FAIL: Product directory DACL has an unexpected allow ACE: $path"
            }
            if (-not $seenSids.Add($sid)) {
                throw "FAIL: Product directory DACL repeats an allow ACE: $path"
            }
        }
        if (-not $seenSids.Contains($ownerRightsSid) -or -not $seenSids.Contains($systemSid)) {
            throw "FAIL: Product directory DACL is missing owner or SYSTEM full control: $path"
        }
    }
    return [ordered]@{
        protected = $true
        checked_paths = $Paths.Count
        owner_rights_full_control = $true
        system_full_control = $true
        only_owner_and_system_allow_aces = $true
    }
}

function Assert-NoSyntheticRequestDataInLogs {
    param(
        [Parameter(Mandatory = $true)][string[]]$Paths,
        [Parameter(Mandatory = $true)][string]$Upstream
    )

    foreach ($value in @(
        'sk-product-go-route-probe-do-not-use-000000',
        'ahb-product-probe-wrong-bearer',
        'product-usage-request-body',
        'product-usage-response-ok',
        $Upstream
    )) {
        $matches = Select-String -LiteralPath $Paths -Pattern $value -SimpleMatch -ErrorAction SilentlyContinue
        if ($null -ne $matches) {
            throw 'FAIL: synthetic request data leaked into probe logs'
        }
    }
}

function Invoke-ProductProbe {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$Scratch,
        [Parameter(Mandatory = $true)][string]$Upstream,
        [Parameter(Mandatory = $true)][string]$Log,
        [string[]]$ExtraArguments = @()
    )

    & $Executable $Scratch $Upstream @ExtraArguments *> $Log
    if ($LASTEXITCODE -ne 0) {
        Get-Content -LiteralPath $Log -Tail 100 -ErrorAction SilentlyContinue | Write-Error
        throw 'FAIL: Product-mode probe failed'
    }
}

try {
    Assert-LoopbackPortAvailable -Port 43121 -Step 'initial preflight'

    $buildScratch = New-ProbeScratch
    $adapterdBinary = Join-Path $buildScratch 'agenthub-adapterd.exe'
    $goBuildLog = Join-Path $buildScratch 'go-build.log'
    $cargoBuildLog = Join-Path $buildScratch 'cargo-build.log'

    Push-Location (Join-Path $repoRoot 'go\agenthub-adapterd')
    try {
        & go build -trimpath -buildvcs=false -o $adapterdBinary . *> $goBuildLog
        if ($LASTEXITCODE -ne 0) {
            throw 'FAIL: build Product Go sidecar'
        }
    } finally {
        Pop-Location
    }
    Push-Location $repoRoot
    try {
        & cargo build -p agenthub-gui --example go_route_product_e2e_probe --features go-route-product-probe --locked *> $cargoBuildLog
        if ($LASTEXITCODE -ne 0) {
            throw 'FAIL: build Product-mode probe'
        }
    } finally {
        Pop-Location
    }
    $probeBinary = Join-Path $repoRoot 'target\debug\examples\go_route_product_e2e_probe.exe'
    if (-not (Test-Path -LiteralPath $adapterdBinary -PathType Leaf) -or
        -not (Test-Path -LiteralPath $probeBinary -PathType Leaf)) {
        throw 'FAIL: probe binaries are missing'
    }

    $upstreamPort = Get-FreeLoopbackPort
    $mockScript = Join-Path $buildScratch 'loopback-mock.ps1'
    $mockLog = Join-Path $buildScratch 'mock.log'
    $mockErrorLog = Join-Path $buildScratch 'mock.err.log'
    @'
param([Parameter(Mandatory = $true)][int]$Port)
$ErrorActionPreference = 'Stop'
$listener = [Net.HttpListener]::new()
$listener.Prefixes.Add(("http://127.0.0.1:{0}/" -f $Port))
$listener.Start()
$sourceKey = 'sk-product-go-route-probe-do-not-use-000000'
$requestMarker = 'product-usage-request-body'
$responseMarker = 'product-usage-response-ok'
function Write-JsonResponse {
    param([Parameter(Mandatory = $true)][System.Net.HttpListenerResponse]$Response, [Parameter(Mandatory = $true)][int]$Status, [Parameter(Mandatory = $true)][string]$Json)

    $payload = [Text.Encoding]::UTF8.GetBytes($Json)
    $Response.StatusCode = $Status
    $Response.ContentType = 'application/json'
    $Response.ContentLength64 = $payload.Length
    $Response.OutputStream.Write($payload, 0, $payload.Length)
    $Response.Close()
}
try {
    while ($listener.IsListening) {
        $context = $listener.GetContext()
        if ($context.Request.HttpMethod -eq 'GET') {
            Write-JsonResponse -Response $context.Response -Status 200 -Json '{"object":"list","data":[{"id":"probe-model"}]}'
            continue
        }
        if ($context.Request.HttpMethod -ne 'POST' -or $context.Request.Url.AbsolutePath -ne '/v1/chat/completions') {
            Write-JsonResponse -Response $context.Response -Status 400 -Json '{"error":"mock_request_rejected"}'
            [Console]::Out.WriteLine('{"event":"rejected"}')
            continue
        }
        try {
            $reader = [IO.StreamReader]::new($context.Request.InputStream, [Text.Encoding]::UTF8, $false, 8192, $true)
            try {
                $raw = $reader.ReadToEnd()
            } finally {
                $reader.Dispose()
            }
            $body = $raw | ConvertFrom-Json -ErrorAction Stop
            $properties = @($body.PSObject.Properties.Name)
            $messages = @($body.messages)
            $valid = (
                $context.Request.Headers['Authorization'] -eq ('Bearer ' + $sourceKey) -and
                [string]::IsNullOrEmpty($context.Request.Headers['X-API-Key']) -and
                [string]::IsNullOrEmpty($context.Request.Headers['Anthropic-Version']) -and
                $properties.Count -eq 3 -and
                $properties -contains 'model' -and $properties -contains 'messages' -and $properties -contains 'stream' -and
                $body.model -is [string] -and -not [string]::IsNullOrWhiteSpace($body.model) -and
                $body.stream -is [bool] -and
                $body.stream -eq $false -and
                $messages.Count -eq 1 -and
                @($messages[0].PSObject.Properties.Name).Count -eq 2 -and
                $messages[0].role -eq 'user' -and $messages[0].content -eq $requestMarker
            )
        } catch {
            $valid = $false
        }
        if (-not $valid) {
            Write-JsonResponse -Response $context.Response -Status 400 -Json '{"error":"mock_request_rejected"}'
            [Console]::Out.WriteLine('{"event":"rejected"}')
            continue
        }
        $json = ('{{"id":"chatcmpl_product_usage","object":"chat.completion","created":1720000000,"model":{0},"choices":[{{"index":0,"message":{{"role":"assistant","content":{1}}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}}}' -f ($body.model | ConvertTo-Json -Compress), ($responseMarker | ConvertTo-Json -Compress))
        Write-JsonResponse -Response $context.Response -Status 200 -Json $json
        [Console]::Out.WriteLine('{"event":"responses"}')
    }
} finally {
    $listener.Close()
}
'@ | Set-Content -LiteralPath $mockScript -Encoding utf8NoBOM
    $mockArguments = '-NoLogo -NoProfile -NonInteractive -File "{0}" -Port {1}' -f $mockScript, $upstreamPort
    $mockProcess = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList $mockArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput $mockLog -RedirectStandardError $mockErrorLog
    Wait-ForCondition -Description 'loopback mock startup' -Condition {
        if ($mockProcess.HasExited) {
            throw 'FAIL: loopback mock exited before accepting connections'
        }
        try {
            $client = [Net.Sockets.TcpClient]::new()
            try {
                $client.Connect('127.0.0.1', $upstreamPort)
                $stream = $client.GetStream()
                $request = [Text.Encoding]::ASCII.GetBytes("GET /health HTTP/1.1`r`nHost: 127.0.0.1`r`nConnection: close`r`n`r`n")
                $stream.Write($request, 0, $request.Length)
                $buffer = [byte[]]::new(64)
                return $stream.Read($buffer, 0, $buffer.Length) -gt 0
            } finally {
                $client.Dispose()
            }
        } catch {
            return $false
        }
    }
    $upstream = "http://127.0.0.1:$upstreamPort/v1"

    $previousAdapterdBin = $env:AGENTHUB_ADAPTERD_BIN
    $env:AGENTHUB_ADAPTERD_BIN = $adapterdBinary
    try {
        $normalScratch = New-ProbeScratch
        $normalLog = Join-Path $normalScratch 'run.log'
        Invoke-ProductProbe -Executable $probeBinary -Scratch $normalScratch -Upstream $upstream -Log $normalLog
        $normalEvidence = Read-LastJsonEvidence -Path $normalLog
        if ($normalEvidence.schema -ne 'go-route-product-e2e-probe.v1' -or $normalEvidence.status -ne 'ok' -or $normalEvidence.port -ne 43121) {
            throw 'FAIL: normal Product lifecycle evidence is invalid'
        }
        Assert-TrueEvidenceFields -Evidence $normalEvidence -Description 'normal Product lifecycle' -Fields @(
            'saved_port_preserved', 'reload_committed', 'same_port_recovered',
            'product_home_preserved', 'staging_cleaned', 'port_released',
            'runtime_secret_scan', 'usage_jsonl_recorded', 'usage_spool_and_logs_secret_scan',
            'usage_request_database_and_wal_unchanged', 'data_dir_mode_unchanged'
        )
        if ($normalEvidence.restart_count -lt 1) {
            throw 'FAIL: normal Product lifecycle did not recover after a killed Go process'
        }
        Assert-LoopbackPortAvailable -Port 43121 -Step 'normal Product lifecycle'

        $reparseScratch = New-ProbeScratch
        $reparseData = Join-Path $reparseScratch 'data'
        $junctionTarget = Join-Path $reparseScratch 'junction-target'
        $runtimeJunction = Join-Path $reparseData 'runtime'
        [IO.Directory]::CreateDirectory($reparseData) | Out-Null
        [IO.Directory]::CreateDirectory($junctionTarget) | Out-Null
        New-Item -ItemType Junction -Path $runtimeJunction -Target $junctionTarget | Out-Null
        if (-not ((Get-Item -LiteralPath $runtimeJunction).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'FAIL: Windows runtime junction fixture was not a reparse point'
        }
        Assert-LoopbackPortAvailable -Port 43121 -Step 'reparse preflight setup'
        $reparseLog = Join-Path $reparseScratch 'run.log'
        Invoke-ProductProbe -Executable $probeBinary -Scratch $reparseScratch -Upstream $upstream -Log $reparseLog -ExtraArguments @('--windows-reparse-preflight')
        $reparseEvidence = Read-LastJsonEvidence -Path $reparseLog
        if ($reparseEvidence.schema -ne 'go-route-product-reparse-preflight.v1' -or
            $reparseEvidence.status -ne 'ok' -or $reparseEvidence.port -ne 43121) {
            throw 'FAIL: Windows reparse preflight evidence is invalid'
        }
        Assert-TrueEvidenceFields -Evidence $reparseEvidence -Description 'Windows reparse preflight' -Fields @(
            'runtime_junction_rejected', 'go_process_not_started', 'port_available'
        )
        Assert-LoopbackPortAvailable -Port 43121 -Step 'reparse preflight rejection'

        $holdScratch = New-ProbeScratch
        $holdEvidencePath = Join-Path $holdScratch 'ready.json'
        $holdStdout = Join-Path $holdScratch 'run.log'
        $holdStderr = Join-Path $holdScratch 'run.err.log'
        $holdArguments = '"{0}" "{1}" --hold-after-ready "{2}"' -f $holdScratch, $upstream, $holdEvidencePath
        $heldProbeProcess = Start-Process -FilePath $probeBinary -ArgumentList $holdArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput $holdStdout -RedirectStandardError $holdStderr
        Wait-ForCondition -Description 'ready hold evidence' -Condition {
            if ($heldProbeProcess.HasExited) {
                Get-Content -LiteralPath $holdStderr -Tail 100 -ErrorAction SilentlyContinue | Write-Error
                throw 'FAIL: held Product probe exited before ready evidence'
            }
            return Test-Path -LiteralPath $holdEvidencePath -PathType Leaf
        }
        $readyEvidence = Get-Content -LiteralPath $holdEvidencePath -Raw | ConvertFrom-Json
        if ($readyEvidence.schema -ne 'go-route-product-ready-hold.v1' -or
            $readyEvidence.status -ne 'ready' -or $readyEvidence.port -ne 43121 -or
            $readyEvidence.rust_pid -ne $heldProbeProcess.Id -or
            $readyEvidence.go_pid -le 0) {
            throw 'FAIL: held Product probe PID evidence is invalid'
        }
        $productHome = [string]$readyEvidence.product_home
        $daclEvidence = Assert-PrivateDacl -Paths @(
            (Split-Path -Parent $productHome), $productHome,
            (Join-Path $productHome 'run'), (Join-Path $productHome 'config'), (Join-Path $productHome 'logs')
        )
        $rustPid = [int]$readyEvidence.rust_pid
        $goPid = [int]$readyEvidence.go_pid
        if ($goPid -eq $rustPid -or -not (Test-ProcessRunning -Id $goPid)) {
            throw 'FAIL: Product Go child was not running before forced Rust parent termination'
        }
        if (Test-LoopbackPortAvailable -Port 43121) {
            throw 'FAIL: Product Go child did not hold saved port before forced Rust parent termination'
        }
        Stop-Process -Id $rustPid -Force
        Wait-ForCondition -Description 'forced Rust probe termination' -Condition {
            return -not (Test-ProcessRunning -Id $rustPid)
        }
        Wait-ForCondition -Description 'Job Object Go-child reaping' -Condition {
            return -not (Test-ProcessRunning -Id $goPid)
        }
        Assert-LoopbackPortAvailable -Port 43121 -Step 'forced Rust parent termination'
        $heldProbeProcess = $null

        $mockEvents = @(Get-Content -LiteralPath $mockLog | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        if ($mockEvents.Count -ne 2 -or @($mockEvents | Where-Object { $_ -eq '{"event":"responses"}' }).Count -ne 2 -or
            @($mockEvents | Where-Object { $_ -eq '{"event":"rejected"}' }).Count -ne 0) {
            throw 'FAIL: loopback mock did not observe exactly the normal and held Product usage requests'
        }
        Assert-NoSyntheticRequestDataInLogs -Upstream $upstream -Paths @(
            $goBuildLog, $cargoBuildLog, $mockLog, $mockErrorLog,
            $normalLog, $reparseLog, $holdStdout, $holdStderr
        )
        $summary = [ordered]@{
            schema = 'go-route-product-windows-e2e-probe.v1'
            status = 'ok'
            normal_product_lifecycle = $true
            windows_runtime_junction_rejected = $true
            reparse_rejection_started_no_go_process = $true
            reparse_rejection_released_port = $true
            go_child_observed_before_parent_exit = $true
            saved_product_port_bound_before_parent_exit = $true
            forced_rust_parent_exit = $true
            job_object_reaped_go_child = $true
            saved_product_port_released = $true
            dacl = $daclEvidence
            secret_free_logs = $true
        }
        $summary | ConvertTo-Json -Compress
        $succeeded = $true
    } finally {
        if ($null -eq $previousAdapterdBin) {
            Remove-Item Env:AGENTHUB_ADAPTERD_BIN -ErrorAction SilentlyContinue
        } else {
            $env:AGENTHUB_ADAPTERD_BIN = $previousAdapterdBin
        }
    }
} finally {
    if ($null -ne $heldProbeProcess -and -not $heldProbeProcess.HasExited) {
        Stop-Process -Id $heldProbeProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $mockProcess -and -not $mockProcess.HasExited) {
        Stop-Process -Id $mockProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($succeeded) {
        foreach ($scratch in $scratchRoots) {
            if (Test-Path -LiteralPath $scratch) {
                Remove-Item -LiteralPath $scratch -Recurse -Force
            }
        }
    } else {
        $scratchRoots | ForEach-Object { Write-Error "probe evidence retained at $_" }
    }
}
