# agentspace-patch-check.ps1: the patch check of a Windows host (N6, D96), run as SYSTEM under
# PowerShell 7 by Run Command for up's patch check and sync's patch wait (docs/operator.md, Windows
# hosts, The patch run and its restart rule; What up waits for). It prints one word, from the
# patch task and this boot's markers, in this order:
#   - patched, when patched exists and was written this boot;
#   - failed, when the patch task is neither running nor queued, and either patch-failed exists and
#     was written this boot, or the task's last run started this boot and its last result is not 0;
#   - pending, when the patch task is registered and enabled;
#   - unknown, otherwise.
# A marker last written before this boot's start counts as missing (N7). It changes nothing.
#
# The test seam (docs/operator.md, Windows hosts, The tests' seams; pins P17 and P18):
# AGENTSPACE_PATCH_STATE and AGENTSPACE_PATCH_FACTS, set together or not at all. Set, they name the
# state folder, which holds patched and patch-failed, and a JSON file holding boot, task_state
# (running, queued, ready, disabled or missing), task_last_run and task_last_result. No task, script
# or file the install puts on a host sets either.

$ErrorActionPreference = 'Stop'
$scriptName = 'agentspace-patch-check'
$seamNames = @('AGENTSPACE_PATCH_STATE', 'AGENTSPACE_PATCH_FACTS')

function Write-AgentspaceError([string]$Text) {
    [Console]::Error.WriteLine($Text)
}

# A JSON value that is a whole number, as a long; $null otherwise.
function ConvertTo-AgentspaceLong($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [bool] -or $Value -is [string]) { return $null }
    try {
        if ($Value -is [int] -or $Value -is [long] -or $Value -is [int16] -or $Value -is [byte]) { return [long]$Value }
        if ($Value -is [double] -or $Value -is [decimal] -or $Value -is [single]) {
            if ([Math]::Floor([double]$Value) -eq [double]$Value) { return [long]$Value }
            return $null
        }
        if ($Value.GetType().FullName -eq 'System.Numerics.BigInteger') { return [long]$Value }
    } catch {
        return $null
    }
    return $null
}

# Whether the file exists and its last write is at or after this boot's start.
function Test-AgentspaceThisBoot([string]$FilePath, [long]$BootSeconds) {
    try {
        if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) { return $false }
        $fileItem = Get-Item -LiteralPath $FilePath -Force
        return ([DateTimeOffset]::new($fileItem.LastWriteTimeUtc).ToUnixTimeSeconds() -ge $BootSeconds)
    } catch {
        return $false
    }
}

$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })
if ($seamSet.Count -gt 0 -and $seamSet.Count -lt $seamNames.Count) {
    Write-AgentspaceError ($scriptName + ': ' + ($seamNames -join ', ') + ' are set together or not at all; nothing was done')
    exit 1
}
$underSeam = ($seamSet.Count -eq $seamNames.Count)

if ($underSeam) {
    $stateDir = $env:AGENTSPACE_PATCH_STATE
    $factsPath = $env:AGENTSPACE_PATCH_FACTS
    $factsBad = $false
    $facts = $null
    try {
        $factsText = Get-Content -LiteralPath $factsPath -Raw -ErrorAction Stop
        if ([string]::IsNullOrWhiteSpace($factsText)) { $factsBad = $true } else { $facts = $factsText | ConvertFrom-Json -ErrorAction Stop }
    } catch {
        $factsBad = $true
    }
    if (-not $factsBad) {
        if ($null -eq $facts -or -not ($facts -is [System.Management.Automation.PSCustomObject])) { $factsBad = $true }
    }
    if (-not $factsBad) {
        $members = @($facts.PSObject.Properties.Name)
        foreach ($need in @('boot', 'task_state', 'task_last_run', 'task_last_result')) {
            if ($members -notcontains $need) { $factsBad = $true }
        }
    }
    if (-not $factsBad) {
        $bootSeconds = ConvertTo-AgentspaceLong $facts.boot
        $taskState = $facts.task_state
        $lastRun = ConvertTo-AgentspaceLong $facts.task_last_run
        $lastResult = ConvertTo-AgentspaceLong $facts.task_last_result
        if ($null -eq $bootSeconds) { $factsBad = $true }
        if (-not ($taskState -is [string]) -or @('running', 'queued', 'ready', 'disabled', 'missing') -cnotcontains $taskState) { $factsBad = $true }
        if ($null -ne $facts.task_last_run -and $null -eq $lastRun) { $factsBad = $true }
        if ($null -ne $facts.task_last_result -and $null -eq $lastResult) { $factsBad = $true }
    }
    if ($factsBad) {
        Write-AgentspaceError ($scriptName + ': ' + $factsPath + ' cannot be read as the test''s facts; nothing was done')
        exit 1
    }
} else {
    $stateDir = 'C:\ProgramData\agentspace'
    $bootTime = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
    $bootSeconds = [DateTimeOffset]::new($bootTime.ToUniversalTime()).ToUnixTimeSeconds()
    $taskState = 'missing'
    $lastRun = $null
    $lastResult = $null
    try {
        $patchTask = Get-ScheduledTask -TaskPath '\agentspace\' -TaskName 'agentspace-patch' -ErrorAction Stop
        $taskState = ([string]$patchTask.State).ToLowerInvariant()
        $taskInfo = Get-ScheduledTaskInfo -InputObject $patchTask -ErrorAction Stop
        if ($null -ne $taskInfo.LastRunTime) {
            $lastRun = [DateTimeOffset]::new($taskInfo.LastRunTime.ToUniversalTime()).ToUnixTimeSeconds()
        }
        if ($null -ne $taskInfo.LastTaskResult) { $lastResult = [long]$taskInfo.LastTaskResult }
    } catch {
        # A task that is not registered stays missing; one whose last run cannot be read keeps the
        # state it was read with.
    }
}

$patchedPath = Join-Path $stateDir 'patched'
$failedPath = Join-Path $stateDir 'patch-failed'
$underWay = ($taskState -ceq 'running' -or $taskState -ceq 'queued')

if (Test-AgentspaceThisBoot $patchedPath $bootSeconds) {
    $answer = 'patched'
} elseif (-not $underWay -and ((Test-AgentspaceThisBoot $failedPath $bootSeconds) -or ($null -ne $lastRun -and $lastRun -ge $bootSeconds -and $null -ne $lastResult -and $lastResult -ne 0))) {
    $answer = 'failed'
} elseif ($taskState -ceq 'ready' -or $underWay) {
    $answer = 'pending'
} else {
    $answer = 'unknown'
}
[Console]::Out.WriteLine($answer)
exit 0
