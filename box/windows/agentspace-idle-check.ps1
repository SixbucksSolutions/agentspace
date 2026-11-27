# agentspace-idle-check.ps1: the idle check of a Windows host (D11, D57, D78, D96), run as SYSTEM
# every 5 minutes by the scheduled task \agentspace\agentspace-idle, under PowerShell 7.
#
# Its rules, its lines, its settings and the order of its rules are the Linux check's
# (docs/operator.md, The idle check and the lease horizon); only what it measures differs
# (docs/operator.md, Windows hosts, The idle check on a Windows host). Each run decides once,
# writes one line to its stdout and to C:\ProgramData\agentspace\idle.log with the time in UTC,
# and either does nothing more or powers the host off with shutdown.exe /s /t 0, which stops the
# instance.
#
# The test seam (docs/operator.md, Windows hosts, The tests' seams): AGENTSPACE_IDLE_STATE,
# AGENTSPACE_IDLE_CONF and AGENTSPACE_IDLE_FACTS, set together or not at all. Set, they name the
# state folder, the settings file and a JSON file holding every probe; the check then starts no
# program, writes no log, and in place of shutdown.exe writes poweroff in the state folder. No
# task, script or file the install puts on a host sets any of them.

$ErrorActionPreference = 'Stop'
$scriptName = 'agentspace-idle-check'
$seamNames = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_IDLE_CONF', 'AGENTSPACE_IDLE_FACTS')

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

# The file's last write as Unix seconds, or $null when it is not a file.
function Get-AgentspaceWriteTime([string]$FilePath) {
    if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) { return $null }
    $fileItem = Get-Item -LiteralPath $FilePath -Force
    return [DateTimeOffset]::new($fileItem.LastWriteTimeUtc).ToUnixTimeSeconds()
}

# A state file counts only when it was written this boot (N7): one last written before this boot's
# start is read as missing, as /run is cleared at a Linux host's boot.
function Test-AgentspaceThisBoot([string]$FilePath, [long]$BootSeconds) {
    try {
        $written = Get-AgentspaceWriteTime $FilePath
    } catch {
        return $false
    }
    if ($null -eq $written) { return $false }
    return ($written -ge $BootSeconds)
}

# The first line of a state file as text, when the file was written this boot; $null otherwise.
function Read-AgentspaceStateLine([string]$FilePath, [long]$BootSeconds) {
    if (-not (Test-AgentspaceThisBoot $FilePath $BootSeconds)) { return $null }
    try {
        $text = [IO.File]::ReadAllText($FilePath)
    } catch {
        return $null
    }
    $lf = $text.IndexOf("`n")
    if ($lf -ge 0) { $text = $text.Substring(0, $lf) }
    return $text
}

function Test-AgentspaceDigits([string]$Text, [int]$MaxDigits) {
    if ($null -eq $Text) { return $false }
    return ($Text -cmatch ('^[0-9]{1,' + $MaxDigits + '}$'))
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

# The seam: every variable set, or none.
$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })
if ($seamSet.Count -gt 0 -and $seamSet.Count -lt $seamNames.Count) {
    Write-AgentspaceError ($scriptName + ': ' + ($seamNames -join ', ') + ' are set together or not at all; nothing was done')
    exit 1
}
$underSeam = ($seamSet.Count -eq $seamNames.Count)

# Without its seam it acts on the machine it runs on, so it runs only as SYSTEM, as its task runs
# it: a run anywhere else, a test that left its seam unset among them, powers nothing off.
if (-not $underSeam -and [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') {
    Write-AgentspaceError ($scriptName + ': without its test seam it runs only as SYSTEM, from its scheduled task; nothing was done')
    exit 1
}

if ($underSeam) {
    $stateDir = $env:AGENTSPACE_IDLE_STATE
    $confPath = $env:AGENTSPACE_IDLE_CONF
    $factsPath = $env:AGENTSPACE_IDLE_FACTS
    $logPath = $null
} else {
    $stateDir = 'C:\ProgramData\agentspace'
    $confPath = 'C:\ProgramData\agentspace\idle.conf'
    $factsPath = $null
    $logPath = 'C:\ProgramData\agentspace\idle.log'
}

# The facts: the seam's file, read before anything is written, or the host's own probes.
$facts = $null
if ($underSeam) {
    $factsBad = $false
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
        foreach ($need in @('boot', 'cpu_busy_ticks', 'ssh_connections', 'workers', 'patch_running')) {
            if ($members -notcontains $need) { $factsBad = $true }
        }
    }
    if (-not $factsBad) {
        $factBoot = ConvertTo-AgentspaceLong $facts.boot
        $factBusy = ConvertTo-AgentspaceLong $facts.cpu_busy_ticks
        $factSsh = ConvertTo-AgentspaceLong $facts.ssh_connections
        if ($null -eq $factBoot -or $null -eq $factBusy -or $null -eq $factSsh) { $factsBad = $true }
        if (-not ($facts.patch_running -is [bool])) { $factsBad = $true }
        $factWorkers = @()
        if ($null -ne $facts.workers) {
            foreach ($w in @($facts.workers)) {
                if ($null -eq $w -or -not ($w -is [System.Management.Automation.PSCustomObject])) { $factsBad = $true; continue }
                $toSshd = $w.sshd_connection
                if (-not ($toSshd -is [bool])) { $factsBad = $true; continue }
                $childNames = @()
                if ($null -ne $w.children) { $childNames = @($w.children | ForEach-Object { [string]$_ }) }
                $factWorkers += , @{ ToSshd = $toSshd; Children = $childNames }
            }
        }
    }
    if ($factsBad) {
        Write-AgentspaceError ($scriptName + ': ' + $factsPath + ' cannot be read as the test''s facts; nothing was done')
        exit 1
    }
    $bootSeconds = $factBoot
    $busyNow = $factBusy
    $sshCount = $factSsh
    $workers = $factWorkers
    $patchRunning = [bool]$facts.patch_running
}

# The host's own probes.
if (-not $underSeam) {
    $bootTime = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
    $bootSeconds = [DateTimeOffset]::new($bootTime.ToUniversalTime()).ToUnixTimeSeconds()

    # Busy time over every processor: kernel plus user less idle, in 100-nanosecond units. The
    # kernel time GetSystemTimes gives includes the idle time.
    if (-not ('Agentspace.SystemTimes' -as [type])) {
        Add-Type -Namespace Agentspace -Name SystemTimes -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError = true)]
public static extern bool GetSystemTimes(out long idleTime, out long kernelTime, out long userTime);
'@
    }
    $idleTicks = [long]0
    $kernelTicks = [long]0
    $userTicks = [long]0
    if (-not [Agentspace.SystemTimes]::GetSystemTimes([ref]$idleTicks, [ref]$kernelTicks, [ref]$userTicks)) {
        throw 'GetSystemTimes failed'
    }
    $busyNow = $kernelTicks + $userTicks - $idleTicks

    # An SSH session: an established TCP connection whose local port is 22, in either family.
    $sshCount = @(Get-NetTCPConnection -State Established -LocalPort 22 -ErrorAction SilentlyContinue).Count

    # Each Session Manager worker: whether it owns the client end of an established connection to
    # sshd at [::1]:22 or 127.0.0.1:22, and its children's names.
    $workers = @()
    foreach ($proc in @(Get-Process -Name 'ssm-session-worker' -ErrorAction SilentlyContinue)) {
        $workerPid = $proc.Id
        $toSshd = $false
        foreach ($conn in @(Get-NetTCPConnection -State Established -RemotePort 22 -OwningProcess $workerPid -ErrorAction SilentlyContinue)) {
            if ($conn.RemoteAddress -eq '::1' -or $conn.RemoteAddress -eq '127.0.0.1') { $toSshd = $true }
        }
        $childNames = @(Get-CimInstance -ClassName Win32_Process -Filter ('ParentProcessId=' + $workerPid) -ErrorAction SilentlyContinue | ForEach-Object { [string]$_.Name })
        $workers += , @{ ToSshd = $toSshd; Children = $childNames }
    }

    $patchRunning = $false
    try {
        $patchTask = Get-ScheduledTask -TaskPath '\agentspace\' -TaskName 'agentspace-patch' -ErrorAction Stop
        if ([string]$patchTask.State -eq 'Running') { $patchRunning = $true }
    } catch {
        $patchRunning = $false
    }
}

# The settings: read as text, never run. A missing file or setting takes the default.
$idleMinutes = '60'
$busyCpuSeconds = '6'
$graceText = '15'
$confLines = $null
try {
    if (Test-Path -LiteralPath $confPath -PathType Leaf) {
        $confLines = [IO.File]::ReadAllText($confPath) -split "`n"
    }
} catch {
    $confLines = $null
}
if ($null -ne $confLines) {
    foreach ($confLine in $confLines) {
        $line = $confLine
        if ($line.EndsWith("`r")) { $line = $line.Substring(0, $line.Length - 1) }
        if ($line.StartsWith('#')) { continue }
        $eq = $line.IndexOf('=')
        if ($eq -lt 1) { continue }
        $name = $line.Substring(0, $eq)
        $value = $line.Substring($eq + 1)
        if ($name -ceq 'IDLE_MINUTES') { $idleMinutes = $value }
        elseif ($name -ceq 'BUSY_CPU_SECONDS') { $busyCpuSeconds = $value }
        elseif ($name -ceq 'HORIZON_GRACE_MINUTES') { $graceText = $value }
    }
}
if (-not (Test-AgentspaceDigits $idleMinutes 9)) { $idleMinutes = '60' }
if (-not (Test-AgentspaceDigits $busyCpuSeconds 9)) { $busyCpuSeconds = '6' }
$idleLimit = [long]::Parse($idleMinutes, [Globalization.CultureInfo]::InvariantCulture)
$busyLimit = [long]::Parse($busyCpuSeconds, [Globalization.CultureInfo]::InvariantCulture)

if (-not (Test-Path -LiteralPath $stateDir -PathType Container)) {
    New-Item -ItemType Directory -Path $stateDir -Force | Out-Null
}
$lastActivePath = Join-Path $stateDir 'last-active'
$busyPath = Join-Path $stateDir 'busy'
$horizonPath = Join-Path $stateDir 'lease-horizon'
$now = Get-AgentspaceUnixNow

# The last activity starts as the first run's time each boot.
$lastActiveText = Read-AgentspaceStateLine $lastActivePath $bootSeconds
if (-not (Test-AgentspaceDigits $lastActiveText 15)) {
    [IO.File]::WriteAllText($lastActivePath, ([string]$now + "`n"))
    $lastActive = $now
} else {
    $lastActive = [long]::Parse($lastActiveText, [Globalization.CultureInfo]::InvariantCulture)
}

# 1. The CPU work since the last check, from the busy counter the last run recorded.
$prevText = Read-AgentspaceStateLine $busyPath $bootSeconds
$busyPrev = $busyNow
if ($null -ne $prevText -and $prevText -cmatch '^-?[0-9]{1,19}$') {
    try { $busyPrev = [long]::Parse($prevText, [Globalization.CultureInfo]::InvariantCulture) } catch { $busyPrev = $busyNow }
}
[IO.File]::WriteAllText($busyPath, ([string]$busyNow + "`n"))
$cpuSeconds = [long][Math]::Floor(([decimal]$busyNow - [decimal]$busyPrev) / [decimal]10000000)

# 2. A session: SSH first, then a Session Manager worker that holds sshd or runs a child other than
# conhost.exe (D78); with none, a patch run under way.
$session = ''
if ($sshCount -gt 0) {
    $session = 'SSH session open'
} else {
    foreach ($worker in $workers) {
        $hasChild = $false
        foreach ($childName in $worker.Children) {
            if (-not [string]::Equals($childName, 'conhost.exe', [StringComparison]::OrdinalIgnoreCase)) { $hasChild = $true }
        }
        if ($worker.ToSshd -or $hasChild) {
            $session = 'Session Manager session open'
            break
        }
    }
}
$patch = ''
if ($session -eq '' -and $patchRunning) {
    $patch = 'Patching'
}

function Invoke-AgentspacePowerOff {
    if ($underSeam) {
        [IO.File]::WriteAllText((Join-Path $stateDir 'poweroff'), ([string](Get-AgentspaceUnixNow) + "`n"))
    } else {
        # By name, found on PATH, so a test's tripwire first on PATH would catch a start of it.
        & shutdown /s /t 0
    }
}

function Write-AgentspaceDecision([string]$Text) {
    [Console]::Out.WriteLine($Text)
    Write-AgentspaceLog $logPath $Text
}

# 3. The horizon rule (D57): the first line of the horizon file, 1 to 15 decimal digits and
# nothing else, written this boot. Anything else is no horizon, ignored without a word.
if ($session -eq '' -and $patch -eq '') {
    $horizonText = $null
    try {
        if (Test-AgentspaceThisBoot $horizonPath $bootSeconds) {
            $horizonBytes = [IO.File]::ReadAllBytes($horizonPath)
            $firstLength = [Array]::IndexOf($horizonBytes, [byte]10)
            if ($firstLength -lt 0) { $firstLength = $horizonBytes.Length }
            $digitsOnly = ($firstLength -ge 1 -and $firstLength -le 15)
            for ($i = 0; $digitsOnly -and $i -lt $firstLength; $i++) {
                if ($horizonBytes[$i] -lt 48 -or $horizonBytes[$i] -gt 57) { $digitsOnly = $false }
            }
            if ($digitsOnly) { $horizonText = [Text.Encoding]::ASCII.GetString($horizonBytes, 0, $firstLength) }
        }
    } catch {
        $horizonText = $null
    }
    if ($null -ne $horizonText) {
        $grace = $graceText
        if (-not (Test-AgentspaceDigits $grace 6)) {
            Write-AgentspaceError ('Invalid HORIZON_GRACE_MINUTES in ' + $confPath + '; using 15')
            $grace = '15'
        }
        $graceMinutes = [long]::Parse($grace, [Globalization.CultureInfo]::InvariantCulture)
        $horizon = [long]::Parse($horizonText, [Globalization.CultureInfo]::InvariantCulture)
        if ([decimal]$now -ge ([decimal]$horizon + [decimal]$graceMinutes * 60)) {
            try {
                $local = [DateTimeOffset]::FromUnixTimeSeconds($horizon).ToLocalTime()
                $invariant = [Globalization.CultureInfo]::InvariantCulture
                $horizonTime = $local.ToString('h:mm', $invariant) + $local.ToString('tt', $invariant).ToLowerInvariant()
            } catch {
                $horizonTime = '@' + $horizon
            }
            Write-AgentspaceDecision ('Past lease horizon ' + $horizonTime + ' plus ' + $graceMinutes + ' min with no session; powering off')
            Invoke-AgentspacePowerOff
            exit 0
        }
    }
}

# 4. D11's rule, unchanged.
$reason = $session
if ($reason -eq '') {
    if ($patch -ne '') {
        $reason = $patch
    } elseif ($cpuSeconds -ge $busyLimit) {
        $reason = 'CPU work: ' + $cpuSeconds + ' s since last check'
    }
}

if ($reason -ne '') {
    [IO.File]::WriteAllText($lastActivePath, ([string]$now + "`n"))
    Write-AgentspaceDecision ('Active: ' + $reason)
    exit 0
}

$idleFor = [long][Math]::Truncate(([double]$now - [double]$lastActive) / 60)
if ($idleFor -ge $idleLimit) {
    Write-AgentspaceDecision ('Idle for ' + $idleFor + ' min (limit ' + $idleLimit + '); powering off')
    Invoke-AgentspacePowerOff
} else {
    Write-AgentspaceDecision ('Idle for ' + $idleFor + ' min of ' + $idleLimit + ' (CPU ' + $cpuSeconds + ' s since last check)')
}
exit 0
