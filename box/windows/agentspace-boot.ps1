# agentspace-boot.ps1: the boot task of a Windows host (D57, D96), run as SYSTEM at every boot by
# the scheduled task \agentspace\agentspace-boot, under PowerShell 7 (docs/operator.md, Windows
# hosts, The boot task). It:
#   1. Removes this boot's state files left by the last boot: lease-horizon, last-active, busy,
#      patched and patch-failed, each one missing being no error, and logs the removal.
#      last-patch-reboot is kept across boots.
#   2. Grows the C: partition, and its filesystem with it, to the largest size it supports. A
#      partition with nothing to grow is no error, and a failure is logged and does not stop step 3.
#   3. Starts the patch task, and exits 0.
# Its lines go to C:\ProgramData\agentspace\boot.log, with the time in UTC.
#
# The test seam (docs/operator.md, Windows hosts, The tests' seams; pin P17): AGENTSPACE_IDLE_STATE
# and AGENTSPACE_PATCH_STATE, set together or not at all. Set, lease-horizon, last-active and busy
# are removed from the first, patched and patch-failed from the second, where boot.log is written;
# in place of growing C: and starting the patch task, it writes partition-grown and patch-started
# in the second, and it touches nothing outside the two folders. No task, script or file the
# install puts on a host sets either.

$ErrorActionPreference = 'Stop'
$scriptName = 'agentspace-boot'
$seamNames = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_PATCH_STATE')

function Write-AgentspaceError([string]$Text) {
    [Console]::Error.WriteLine($Text)
}

function Get-AgentspaceUnixNow {
    return [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
}

# Appends one line to a log with the time in UTC; a log past 1 MiB is renamed over <name>.1 first.
function Write-AgentspaceLog([string]$LogPath, [string]$Text) {
    if ([string]::IsNullOrEmpty($LogPath)) { return }
    try {
        $logItem = Get-Item -LiteralPath $LogPath -Force -ErrorAction SilentlyContinue
        if ($null -ne $logItem -and -not $logItem.PSIsContainer -and $logItem.Length -gt 1048576) {
            Move-Item -LiteralPath $LogPath -Destination ($LogPath + '.1') -Force
        }
        $stamp = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
        [IO.File]::AppendAllText($LogPath, ($stamp + ' ' + $Text + "`n"))
    } catch {
    }
}

$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })
if ($seamSet.Count -gt 0 -and $seamSet.Count -lt $seamNames.Count) {
    Write-AgentspaceError ($scriptName + ': ' + ($seamNames -join ', ') + ' are set together or not at all; nothing was done')
    exit 1
}
$underSeam = ($seamSet.Count -eq $seamNames.Count)

# Without its seam it acts on the machine it runs on, so it runs only as SYSTEM, as its task runs
# it: a run anywhere else, a test that left its seam unset among them, changes nothing.
if (-not $underSeam -and [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') {
    Write-AgentspaceError ($scriptName + ': without its test seam it runs only as SYSTEM, from its scheduled task; nothing was done')
    exit 1
}

if ($underSeam) {
    $idleDir = $env:AGENTSPACE_IDLE_STATE
    $patchDir = $env:AGENTSPACE_PATCH_STATE
} else {
    $idleDir = 'C:\ProgramData\agentspace'
    $patchDir = 'C:\ProgramData\agentspace'
}
if (-not (Test-Path -LiteralPath $patchDir -PathType Container)) {
    New-Item -ItemType Directory -Path $patchDir -Force | Out-Null
}
$logPath = Join-Path $patchDir 'boot.log'

function Write-AgentspaceMarker([string]$Name) {
    [IO.File]::WriteAllText((Join-Path $patchDir $Name), ([string](Get-AgentspaceUnixNow) + "`n"))
}

Write-AgentspaceLog $logPath 'Boot task started'

# 1. This boot's state files, left by the last boot.
$stateFiles = @(
    (Join-Path $idleDir 'lease-horizon'),
    (Join-Path $idleDir 'last-active'),
    (Join-Path $idleDir 'busy'),
    (Join-Path $patchDir 'patched'),
    (Join-Path $patchDir 'patch-failed')
)
$removed = @()
foreach ($stateFile in $stateFiles) {
    try {
        if (Test-Path -LiteralPath $stateFile -PathType Leaf) {
            Remove-Item -LiteralPath $stateFile -Force
            $removed += (Split-Path -Leaf $stateFile)
        }
    } catch {
        Write-AgentspaceLog $logPath ('Could not remove ' + $stateFile + ': ' + $_.Exception.Message)
    }
}
if ($removed.Count -gt 0) {
    Write-AgentspaceLog $logPath ('Removed the last boot''s state files: ' + ($removed -join ', '))
} else {
    Write-AgentspaceLog $logPath 'No state files from the last boot to remove'
}

# 2. Grow C: to the largest size its disk supports for it.
if ($underSeam) {
    Write-AgentspaceMarker 'partition-grown'
} else {
    try {
        try { Update-HostStorageCache -ErrorAction Stop } catch { }
        $partition = Get-Partition -DriveLetter C -ErrorAction Stop
        $supported = Get-PartitionSupportedSize -DriveLetter C -ErrorAction Stop
        if ($supported.SizeMax -gt $partition.Size) {
            Resize-Partition -DriveLetter C -Size $supported.SizeMax -ErrorAction Stop
            Write-AgentspaceLog $logPath ('Grew C: from ' + $partition.Size + ' to ' + $supported.SizeMax + ' bytes')
        } else {
            Write-AgentspaceLog $logPath 'C: already fills its volume'
        }
    } catch {
        Write-AgentspaceLog $logPath ('Could not grow C:: ' + $_.Exception.Message)
    }
}

# 3. Start the patch task.
if ($underSeam) {
    Write-AgentspaceMarker 'patch-started'
} else {
    try {
        Start-ScheduledTask -TaskPath '\agentspace\' -TaskName 'agentspace-patch' -ErrorAction Stop
        Write-AgentspaceLog $logPath 'Started the patch task'
    } catch {
        Write-AgentspaceLog $logPath ('Could not start the patch task: ' + $_.Exception.Message)
    }
}
Write-AgentspaceLog $logPath 'Boot task finished'
exit 0
