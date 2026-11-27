# agentspace-patch.ps1: the patch run of a Windows host (D46, D96), run as SYSTEM under PowerShell 7
# by the scheduled task \agentspace\agentspace-patch, which the boot task starts at every boot and
# the install starts when this boot's run has not finished (docs/operator.md, Windows hosts, The
# patch run and its restart rule). Each run:
#   1. Removes patched and patch-failed, and logs its start.
#   2. Searches Windows Update through the Windows Update Agent's own API, the
#      Microsoft.Update.Session COM object, with no module fetched; accepts each update's licence,
#      downloads the updates and installs them, logging the count and each result code.
#   3. Decides on a restart, when an installed update asks for one or the system already has one
#      pending. The restart rule (D96): a patch run never restarts the host while a login session
#      is open, and one that needs a restart restarts it only before the host's first session after
#      its start. It restarts only when no login has reached sshd since this boot's start, no
#      ssm-session-worker is running, and last-patch-reboot is missing or 15 minutes or more ago;
#      then it writes the time to last-patch-reboot, logs the restart, runs shutdown.exe /r /t 0 and
#      exits 0 with no marker. Otherwise it logs that the restart waits for the next boot.
#   4. Starts agentspace-tools.ps1 and waits for it; its failure is logged and does not fail the run.
#   5. Writes patched, holding the time as Unix seconds, and exits 0.
# A search, download or install that fails writes patch-failed, logs the error, makes no restart and
# exits 1. Its lines go to C:\ProgramData\agentspace\patch.log, with the time in UTC.
#
# The test seam (docs/operator.md, Windows hosts, The tests' seams): AGENTSPACE_PATCH_STATE and
# AGENTSPACE_PATCH_FACTS, set together or not at all. Set, they name the state folder, which holds
# the markers, last-patch-reboot and patch.log, and a JSON file holding boot, result (ok or
# failed), restart_needed, logins_since_boot and workers. The run then calls no Windows Update API
# and starts no tools, and in place of shutdown.exe /r writes restart in the state folder. No task,
# script or file the install puts on a host sets either.

$ErrorActionPreference = 'Stop'
$scriptName = 'agentspace-patch'
$seamNames = @('AGENTSPACE_PATCH_STATE', 'AGENTSPACE_PATCH_FACTS')
$restartSpacingSeconds = 15 * 60

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

$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })
if ($seamSet.Count -gt 0 -and $seamSet.Count -lt $seamNames.Count) {
    Write-AgentspaceError ($scriptName + ': ' + ($seamNames -join ', ') + ' are set together or not at all; nothing was done')
    exit 1
}
$underSeam = ($seamSet.Count -eq $seamNames.Count)

# The account guard (pin P13): without its seam it acts on the machine it runs on, so it runs only
# as SYSTEM (S-1-5-18), as its task runs it, and a run as anyone else, a test that left its seam
# unset among them, restarts nothing. It MUST stay the first statement after the seam's detection,
# with nothing before it that writes, makes, starts or logs anything.
if (-not $underSeam -and [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') {
    Write-AgentspaceError ($scriptName + ': without its test seam it runs only as SYSTEM; nothing was done')
    exit 1
}

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
        foreach ($need in @('boot', 'result', 'restart_needed', 'logins_since_boot', 'workers')) {
            if ($members -notcontains $need) { $factsBad = $true }
        }
    }
    if (-not $factsBad) {
        $factResult = $facts.result
        $factLogins = ConvertTo-AgentspaceLong $facts.logins_since_boot
        $factWorkers = ConvertTo-AgentspaceLong $facts.workers
        if (-not ($factResult -is [string]) -or ($factResult -cne 'ok' -and $factResult -cne 'failed')) { $factsBad = $true }
        if (-not ($facts.restart_needed -is [bool])) { $factsBad = $true }
        if ($null -eq $factLogins -or $null -eq $factWorkers -or $null -eq (ConvertTo-AgentspaceLong $facts.boot)) { $factsBad = $true }
    }
    if ($factsBad) {
        Write-AgentspaceError ($scriptName + ': ' + $factsPath + ' cannot be read as the test''s facts; nothing was done')
        exit 1
    }
} else {
    $stateDir = 'C:\ProgramData\agentspace'
}

if (-not (Test-Path -LiteralPath $stateDir -PathType Container)) {
    New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
}
$logPath = Join-Path $stateDir 'patch.log'
$patchedPath = Join-Path $stateDir 'patched'
$failedPath = Join-Path $stateDir 'patch-failed'
$lastRebootPath = Join-Path $stateDir 'last-patch-reboot'

function Write-AgentspaceStamp([string]$FilePath) {
    [IO.File]::WriteAllText($FilePath, ([string](Get-AgentspaceUnixNow) + "`n"))
}

function Exit-AgentspaceFailed([string]$Text) {
    try { Write-AgentspaceStamp $failedPath } catch { }
    Write-AgentspaceLog $logPath ('Patch run failed: ' + $Text)
    Write-AgentspaceError ($scriptName + ': ' + $Text)
    exit 1
}

# 1. A run starts with neither marker.
foreach ($marker in @($patchedPath, $failedPath)) {
    if (Test-Path -LiteralPath $marker -PathType Leaf) { Remove-Item -LiteralPath $marker -Force }
}
Write-AgentspaceLog $logPath 'Patch run started'

# 2. Windows Update, or the seam's result.
$restartNeeded = $false
if ($underSeam) {
    if ($facts.result -ceq 'failed') {
        Exit-AgentspaceFailed 'the facts give a failed update run'
    }
    Write-AgentspaceLog $logPath 'Windows Update stood in by the test''s facts: result ok'
    $restartNeeded = [bool]$facts.restart_needed
} else {
    try {
        $updateSession = New-Object -ComObject 'Microsoft.Update.Session'
        $updateSession.ClientApplicationID = 'agentspace-patch'
        $searcher = $updateSession.CreateUpdateSearcher()
        $search = $searcher.Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
        Write-AgentspaceLog $logPath ('Search result code ' + [int]$search.ResultCode + ', ' + $search.Updates.Count + ' updates')
        if ([int]$search.ResultCode -ne 2 -and [int]$search.ResultCode -ne 3) {
            Exit-AgentspaceFailed ('the search ended with result code ' + [int]$search.ResultCode)
        }
        if ($search.Updates.Count -gt 0) {
            $toInstall = New-Object -ComObject 'Microsoft.Update.UpdateColl'
            foreach ($update in $search.Updates) {
                if (-not $update.EulaAccepted) { $update.AcceptEula() }
                [void]$toInstall.Add($update)
            }
            $downloader = $updateSession.CreateUpdateDownloader()
            $downloader.Updates = $toInstall
            $download = $downloader.Download()
            Write-AgentspaceLog $logPath ('Download result code ' + [int]$download.ResultCode)
            if ([int]$download.ResultCode -ge 4) {
                Exit-AgentspaceFailed ('the download ended with result code ' + [int]$download.ResultCode)
            }
            $installer = $updateSession.CreateUpdateInstaller()
            $installer.Updates = $toInstall
            $install = $installer.Install()
            for ($i = 0; $i -lt $toInstall.Count; $i++) {
                $one = $install.GetUpdateResult($i)
                Write-AgentspaceLog $logPath ('Update ' + $toInstall.Item($i).Title + ': result code ' + [int]$one.ResultCode)
            }
            Write-AgentspaceLog $logPath ('Install result code ' + [int]$install.ResultCode + ', restart required ' + [bool]$install.RebootRequired)
            if ([int]$install.ResultCode -ge 4) {
                Exit-AgentspaceFailed ('the install ended with result code ' + [int]$install.ResultCode)
            }
            if ($install.RebootRequired) { $restartNeeded = $true }
        }
        $systemInfo = New-Object -ComObject 'Microsoft.Update.SystemInfo'
        if ($systemInfo.RebootRequired) { $restartNeeded = $true }
    } catch {
        Exit-AgentspaceFailed ('Windows Update: ' + $_.Exception.Message)
    }
}

# 3. The restart rule.
if ($restartNeeded) {
    $now = Get-AgentspaceUnixNow
    $why = @()
    if ($underSeam) {
        $logins = ConvertTo-AgentspaceLong $facts.logins_since_boot
        $workerCount = ConvertTo-AgentspaceLong $facts.workers
    } else {
        # sshd's own log: an accepted login since this boot's start. A log that cannot be read
        # counts as a login, so the run never restarts on a guess.
        $bootTime = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
        $logins = 0
        try {
            $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'OpenSSH/Operational'; StartTime = $bootTime } -ErrorAction Stop)
            $logins = @($events | Where-Object { $_.Message -match '\bAccepted \S+' }).Count
        } catch {
            if ([string]$_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { $logins = 1 }
        }
        $workerCount = @(Get-Process -Name 'ssm-session-worker' -ErrorAction SilentlyContinue).Count
    }
    if ($logins -gt 0) { $why += 'a login has reached sshd since this boot''s start' }
    if ($workerCount -gt 0) { $why += 'a Session Manager session is open' }
    $lastText = $null
    try {
        if (Test-Path -LiteralPath $lastRebootPath -PathType Leaf) { $lastText = ([IO.File]::ReadAllText($lastRebootPath)).Trim() }
    } catch {
        $lastText = $null
    }
    if ($null -ne $lastText -and $lastText -cmatch '^[0-9]{1,15}$') {
        $lastReboot = [long]::Parse($lastText, [Globalization.CultureInfo]::InvariantCulture)
        if ($now - $lastReboot -lt $restartSpacingSeconds) { $why += 'the last patch restart was less than 15 minutes ago' }
    }
    if ($why.Count -eq 0) {
        Write-AgentspaceStamp $lastRebootPath
        Write-AgentspaceLog $logPath 'Restarting the host to finish its updates'
        if ($underSeam) {
            Write-AgentspaceStamp (Join-Path $stateDir 'restart')
        } else {
            # By its full path, never by name (pin P13): a SYSTEM task's PATH is the machine's.
            & 'C:\Windows\System32\shutdown.exe' /r /t 0
        }
        exit 0
    }
    Write-AgentspaceLog $logPath ('A restart is needed; it waits for the next boot, since ' + ($why -join ' and '))
}

# 4. The tools, best effort.
if (-not $underSeam) {
    $toolsScript = Join-Path $PSScriptRoot 'agentspace-tools.ps1'
    try {
        & 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $toolsScript
        if ($LASTEXITCODE -ne 0) { Write-AgentspaceLog $logPath ('agentspace-tools.ps1 exited ' + $LASTEXITCODE) }
    } catch {
        Write-AgentspaceLog $logPath ('agentspace-tools.ps1 could not run: ' + $_.Exception.Message)
    }
}

# 5. This boot's run has finished.
Write-AgentspaceStamp $patchedPath
Write-AgentspaceLog $logPath 'Patch run finished'
exit 0
