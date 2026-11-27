#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The shared helpers of box/windows/tests/ (E0.12-T03-S07, #0260; D96; docs/operator.md, "Windows
# hosts", "The tests' seams"). Every test file dot-sources this one, and it dot-sources the
# conformance harness (conformance/lease-cli/harness.ps1) for its home rules, its process starter,
# its result lines and the shared mask.
#
# - A test MUST run each script as a separate pwsh 7 process, in a throwaway home made here under
#   the temp folder in the harness's own form (agentspace-conformance-<12 hex>), with USERPROFILE,
#   HOME, LOCALAPPDATA and APPDATA moved into it by the harness's child environment, and with the
#   script's seam variables the only agentspace ones set. The seams are operator.md's, and none is
#   named *_CMD.
# - A test MUST NOT touch the real registry, scheduled tasks, services, firewall, users, time zone,
#   shutdown, Windows Update or sshd. Each is reached only through a stand-in the seams name. As a
#   last net under a script that does not honour its seam, a shutdown.exe that does nothing is put
#   first on the child's PATH.
# - Every write and delete a test makes goes through Assert-ConformancePath at the call.
# - A test MUST NOT print the values of an environment. A reason that quotes captured text passes
#   through ConvertTo-ConformanceMasked before it is printed (Test-WtCase does it for every reason).
# - Every scratch file is inside the throwaway home, and a test removes the home in a finally.

$script:WtTestsDir = $PSScriptRoot
$script:WtRepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
. (Join-Path $script:WtRepoRoot 'conformance\lease-cli\harness.ps1')

$script:WtPwsh = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$script:WtUtf8 = [Text.UTF8Encoding]::new($false)

# Resolves the folder of the scripts under test: the -ScriptDir a test was given, or box/windows beside
# the tests. A plant proof gives a copy under target\s07test\planted-*.
function Resolve-WtScriptDir {
  param([string]$Given)
  if ([string]::IsNullOrWhiteSpace($Given)) { $Given = Join-Path $script:WtTestsDir '..' }
  return [IO.Path]::GetFullPath($Given)
}

# Makes a throwaway home as the harness does, with no CA: these tests start no operator.
function New-WtHome {
  $root = Join-Path $env:TEMP ('agentspace-conformance-' + [guid]::NewGuid().ToString('N').Substring(0, 12))
  $userProfile = Join-Path $root 'home'
  $h = [pscustomobject]@{
    Root = $root
    UserProfile = $userProfile
    StateDir = Join-Path $userProfile '.agentspace\state'
    SshDir = Join-Path $userProfile '.ssh'
    CaReady = $false
    CaReason = 'These tests need no CA'
    Counter = 0
    GuardBin = Join-Path $root 'guard-bin'
  }
  try {
    New-Item -ItemType Directory -Path (Assert-ConformanceHomeRoot -Root $root) | Out-Null
    foreach ($d in @($userProfile, $h.StateDir, $h.SshDir, (Join-Path $userProfile 'AppData\Local'), (Join-Path $userProfile 'AppData\Roaming'), $h.GuardBin, (Join-Path $root 'cases'))) {
      New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $d) | Out-Null
    }
    # An inert program under the name shutdown.exe (where.exe, which rejects these arguments and
    # changes nothing), first on the child's PATH: a script that ignored its seam would meet it.
    Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\where.exe') -Destination (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.GuardBin 'shutdown.exe'))
  } catch {
    try { if (Test-Path -LiteralPath (Assert-ConformanceHomeRoot -Root $root)) { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $root) -Recurse -Force -ErrorAction SilentlyContinue } } catch { }
    throw
  }
  return $h
}

# A fresh folder for one case, inside the home.
function New-WtCase {
  param([Parameter(Mandatory)]$TestHome)
  $TestHome.Counter = $TestHome.Counter + 1
  $dir = Assert-ConformancePath -ConformanceHome $TestHome -Path (Join-Path $TestHome.Root ('cases\c{0:D3}' -f $TestHome.Counter))
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  return $dir
}

function New-WtFolder {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path)
  $full = Assert-ConformancePath -ConformanceHome $TestHome -Path $Path
  New-Item -ItemType Directory -Force -Path $full | Out-Null
  return $full
}

function Get-WtUnixNow { return [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() }

# Writes a file, with its last write time set to a Unix time when one is given.
function Write-WtBytes {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Bytes, [Nullable[long]]$WrittenUnix = $null)
  $full = Assert-ConformancePath -ConformanceHome $TestHome -Path $Path
  $dir = Split-Path -Parent $full
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $TestHome -Path $dir) | Out-Null }
  [IO.File]::WriteAllBytes($full, $Bytes)
  if ($null -ne $WrittenUnix) { [IO.File]::SetLastWriteTimeUtc($full, [DateTimeOffset]::FromUnixTimeSeconds([long]$WrittenUnix).UtcDateTime) }
}

# Writes text as ASCII/UTF-8 bytes exactly (a test spells its own line endings).
function Write-WtText {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyString()][string]$Text, [Nullable[long]]$WrittenUnix = $null)
  Write-WtBytes -TestHome $TestHome -Path $Path -Bytes $script:WtUtf8.GetBytes($Text) -WrittenUnix $WrittenUnix
}

function Set-WtWritten {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][long]$WrittenUnix)
  $full = Assert-ConformancePath -ConformanceHome $TestHome -Path $Path
  [IO.File]::SetLastWriteTimeUtc($full, [DateTimeOffset]::FromUnixTimeSeconds($WrittenUnix).UtcDateTime)
}

function Remove-WtFile {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path)
  $full = Assert-ConformancePath -ConformanceHome $TestHome -Path $Path
  if (Test-Path -LiteralPath $full) { Remove-Item -LiteralPath $full -Force -Recurse }
}

function Read-WtBytes { param([Parameter(Mandatory)][string]$Path) return , [IO.File]::ReadAllBytes($Path) }

function Read-WtText { param([Parameter(Mandatory)][string]$Path) return $script:WtUtf8.GetString([IO.File]::ReadAllBytes($Path)) }

function Test-WtBytesEqual {
  param([byte[]]$Left, [byte[]]$Right)
  if ($null -eq $Left) { $Left = [byte[]]@() }
  if ($null -eq $Right) { $Right = [byte[]]@() }
  return [System.Linq.Enumerable]::SequenceEqual($Left, $Right)
}

# Bytes as printable text for a reason: control and non-ASCII bytes as \xNN, cut at 300 characters.
function Format-WtBytes {
  param([byte[]]$Bytes)
  if ($null -eq $Bytes) { return '(none)' }
  $sb = [Text.StringBuilder]::new()
  foreach ($b in $Bytes) {
    if ($b -eq 10) { [void]$sb.Append('\n') } elseif ($b -eq 13) { [void]$sb.Append('\r') }
    elseif ($b -ge 32 -and $b -lt 127) { [void]$sb.Append([char]$b) } else { [void]$sb.Append(('\x{0:X2}' -f $b)) }
  }
  $t = $sb.ToString()
  if ($t.Length -gt 300) { $t = $t.Substring(0, 300) + '...' }
  return $t
}

# The names in a folder, sorted, bare.
function Get-WtNames {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return @() }
  return @(Get-ChildItem -LiteralPath $Path -Force -Name | Sort-Object)
}

# A snapshot of a folder tree for before-and-after: relative path, size and a hash of each file.
function Get-WtSnapshot {
  param([Parameter(Mandatory)][string]$Path)
  $root = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  $rows = [Collections.Generic.List[string]]::new()
  if (Test-Path -LiteralPath $root) {
    foreach ($f in (Get-ChildItem -LiteralPath $root -Recurse -Force -File | Sort-Object FullName)) {
      $rel = $f.FullName.Substring($root.Length)
      $rows.Add($rel + '|' + (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash)
    }
    foreach ($d in (Get-ChildItem -LiteralPath $root -Recurse -Force -Directory | Sort-Object FullName)) {
      $rows.Add($d.FullName.Substring($root.Length) + '|dir')
    }
  }
  return ($rows -join "`n")
}

# Runs one script as a pwsh 7 process in the home. $Seam maps variable names to values, a null value
# leaving the variable unset. The harness's child environment removes every other AGENTSPACE_* name.
function Invoke-WtScript {
  param(
    [Parameter(Mandatory)]$TestHome,
    [Parameter(Mandatory)][string]$ScriptPath,
    [hashtable]$Seam = @{},
    [string[]]$Arguments = @(),
    [int]$TimeoutSeconds = 120
  )
  $override = @{}
  foreach ($k in $Seam.Keys) { $override[$k] = $Seam[$k] }
  $override['PATH'] = $TestHome.GuardBin + ';' + $env:PATH
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $TestHome -Override $override
  $argList = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $ScriptPath) + $Arguments
  return Start-ConformanceProcess -FilePath $script:WtPwsh -Arguments $argList -ChildEnvironment $childEnv -TimeoutSeconds $TimeoutSeconds -WorkingDirectory $TestHome.UserProfile
}

# Non-empty lines of a text, a trailing CR of each removed.
function Get-WtLines {
  param([AllowEmptyString()][string]$Text)
  if ([string]::IsNullOrEmpty($Text)) { return @() }
  return @($Text -split "`n" | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ -ne '' })
}

# A marker or a poweroff-form file: one line, a Unix time as decimal digits, and one LF. Returns the
# time, or $null when the file is missing, and throws when it is not in that form.
function Read-WtUnixLine {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  $t = Read-WtText -Path $Path
  if ($t -notmatch '^\d{9,12}\n$') { throw ("The file {0} is not one Unix time and one LF: {1}" -f (Split-Path -Leaf $Path), (Format-WtBytes ([IO.File]::ReadAllBytes($Path)))) }
  return [long]($t.Trim())
}

# The laptop's h:mm and am or pm for a Unix time, in the zone read at the moment.
function Format-WtClock {
  param([Parameter(Mandatory)][long]$Unix)
  $t = [DateTimeOffset]::FromUnixTimeSeconds($Unix).ToLocalTime()
  return $t.ToString('h:mmtt', [Globalization.CultureInfo]::InvariantCulture).ToLowerInvariant()
}

function Write-WtJson {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Object)
  $json = $Object | ConvertTo-Json -Depth 8 -Compress
  Write-WtText -TestHome $TestHome -Path $Path -Text $json
}

function Assert-Wt {
  param([Parameter(Mandatory)][bool]$Condition, [Parameter(Mandatory)][string]$Message)
  if (-not $Condition) { throw $Message }
}

function Assert-WtEqual {
  param($Actual, $Expected, [Parameter(Mandatory)][string]$What)
  if ($Actual -cne $Expected) { throw ("{0}: expected '{1}', got '{2}'" -f $What, $Expected, $Actual) }
}

function Assert-WtBytes {
  param([byte[]]$Actual, [byte[]]$Expected, [Parameter(Mandatory)][string]$What)
  if (-not (Test-WtBytesEqual -Left $Actual -Right $Expected)) {
    throw ("{0}: expected {1}, got {2}" -f $What, (Format-WtBytes $Expected), (Format-WtBytes $Actual))
  }
}

function Assert-WtScriptExists {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw ("The script under test is missing: {0}" -f (Split-Path -Leaf $Path)) }
}

# Runs one case: a PASS when the body returns without throwing, else a FAIL with the thrown message,
# masked and then cut.
function Test-WtCase {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body)
  # A test file's -Only (a regular expression on the case's name) runs the matching cases alone, so
  # that a plant proof runs the cases that show its plant, not the whole file.
  if (-not [string]::IsNullOrEmpty($global:WtOnly) -and $Name -notmatch $global:WtOnly) { return }
  try {
    & $Body | Out-Null
    Write-ConformanceResult -Name $Name -Status PASS
  } catch {
    $reason = ConvertTo-ConformanceMasked ([string]$_.Exception.Message)
    if ($reason.Length -gt 700) { $reason = $reason.Substring(0, 700) + '...' }
    Write-ConformanceResult -Name $Name -Status FAIL -Reason $reason
  }
}

# What every seam-in-part case asserts: exit 1, the message on stderr, nothing written. $Variables MUST
# be the row's names in the order operator.md's table gives them: the message is one stderr line, the
# row's names joined with ', ' in that order (pin P13 of v0.12-T03).
function Assert-WtSeamRefusal {
  param([Parameter(Mandatory)]$Result, [Parameter(Mandatory)][string]$ScriptName, [Parameter(Mandatory)][string[]]$Variables)
  Assert-Wt ($Result.ExitCode -eq 1) ("A seam set in part MUST exit 1, got {0}. stderr: {1}" -f $Result.ExitCode, $Result.Stderr)
  $tail = ' are set together or not at all; nothing was done'
  Assert-Wt ($Result.Stderr.Contains($ScriptName + ': ')) ("stderr MUST start the message with '{0}: ': {1}" -f $ScriptName, $Result.Stderr)
  Assert-Wt ($Result.Stderr.Contains($tail)) ("stderr MUST end the message with '{0}': {1}" -f $tail.Trim(), $Result.Stderr)
  foreach ($v in $Variables) { Assert-Wt ($Result.Stderr.Contains($v)) ("The message MUST name {0}: {1}" -f $v, $Result.Stderr) }
  $want = $ScriptName + ': ' + ($Variables -join ', ') + $tail
  Assert-Wt (@(Get-WtLines -Text $Result.Stderr) -ccontains $want) ("stderr MUST hold the line '{0}', the row's names joined with ', ' in the table's order: {1}" -f $want, $Result.Stderr)
  Assert-Wt (-not $Result.Stdout.Contains('are set together')) 'The message MUST be on stderr, not stdout'
}

# --- The idle check's case builder (shared by the two idle test files) ------------------------------

$script:WtIdleConf = "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=15`n"
$script:WtBaseTicks = [long]50000000000

# A case folder with a state folder holding a `busy` of the base counter, a settings file, and a facts
# file for an idle host with no session, no patch run and no CPU work since the last check, booted 3
# hours ago. $Facts members replace the defaults.
function New-WtIdle {
  param([Parameter(Mandatory)]$TestHome, [AllowEmptyString()][string]$Conf = $script:WtIdleConf, [switch]$NoConf, [switch]$NoBusy)
  $dir = New-WtCase -TestHome $TestHome
  $now = Get-WtUnixNow
  $c = [pscustomobject]@{
    Dir = $dir
    State = New-WtFolder -TestHome $TestHome -Path (Join-Path $dir 'state')
    ConfPath = Join-Path $dir 'idle.conf'
    FactsPath = Join-Path $dir 'facts.json'
    Now = $now
    Boot = $now - 10800
    Facts = [ordered]@{ boot = $now - 10800; cpu_busy_ticks = $script:WtBaseTicks; ssh_connections = 0; workers = @(); patch_running = $false }
  }
  if (-not $NoConf) { Write-WtText -TestHome $TestHome -Path $c.ConfPath -Text $Conf }
  if (-not $NoBusy) { Write-WtText -TestHome $TestHome -Path (Join-Path $c.State 'busy') -Text ("{0}`n" -f $script:WtBaseTicks) }
  Write-WtJson -TestHome $TestHome -Path $c.FactsPath -Object $c.Facts
  return $c
}

function Set-WtIdleFacts {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)]$Case, [Parameter(Mandatory)][hashtable]$Change)
  foreach ($k in $Change.Keys) { $Case.Facts[$k] = $Change[$k] }
  Write-WtJson -TestHome $TestHome -Path $Case.FactsPath -Object $Case.Facts
}

# Ages `last-active` by its content and its last write time, so either reading holds.
function Set-WtLastActive {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)]$Case, [Parameter(Mandatory)][long]$SecondsAgo)
  $t = $Case.Now - $SecondsAgo
  Write-WtText -TestHome $TestHome -Path (Join-Path $Case.State 'last-active') -Text ("{0}`n" -f $t) -WrittenUnix $t
}

# Writes `lease-horizon` as raw bytes (or text), last written at $WrittenUnix (default: now, which is
# after this boot's start).
function Set-WtHorizon {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)]$Case, [Parameter(Mandatory)][AllowEmptyString()][string]$Text, [Nullable[long]]$WrittenUnix = $null)
  if ($null -eq $WrittenUnix) { $WrittenUnix = $Case.Now }
  Write-WtText -TestHome $TestHome -Path (Join-Path $Case.State 'lease-horizon') -Text $Text -WrittenUnix $WrittenUnix
}

function Invoke-WtIdle {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)]$Case, [Parameter(Mandatory)][string]$ScriptPath, [hashtable]$Seam = $null)
  if ($null -eq $Seam) { $Seam = @{ AGENTSPACE_IDLE_STATE = $Case.State; AGENTSPACE_IDLE_CONF = $Case.ConfPath; AGENTSPACE_IDLE_FACTS = $Case.FactsPath } }
  return Invoke-WtScript -TestHome $TestHome -ScriptPath $ScriptPath -Seam $Seam
}

# The poweroff stand-in's mark in the case's state folder: the time, or $null when none.
function Get-WtPoweroff {
  param([Parameter(Mandatory)]$Case)
  return Read-WtUnixLine -Path (Join-Path $Case.State 'poweroff')
}

function Assert-WtPoweredOff {
  param([Parameter(Mandatory)]$Case, [Parameter(Mandatory)]$Result)
  $t = Get-WtPoweroff -Case $Case
  Assert-Wt ($null -ne $t) ("The host MUST be powered off (poweroff written). stdout: {0} stderr: {1}" -f $Result.Stdout, $Result.Stderr)
  Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'poweroff MUST hold the time of the power-off as Unix seconds'
}

function Assert-WtNotPoweredOff {
  param([Parameter(Mandatory)]$Case, [Parameter(Mandatory)]$Result)
  Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $Case.State 'poweroff'))) ("The host MUST NOT be powered off. stdout: {0} stderr: {1}" -f $Result.Stdout, $Result.Stderr)
}

# One stdout line the check printed, matched whole.
function Assert-WtStdoutLine {
  param([Parameter(Mandatory)]$Result, [Parameter(Mandatory)][string]$Pattern)
  $lines = Get-WtLines -Text $Result.Stdout
  foreach ($l in $lines) { if ($l -match $Pattern) { return } }
  throw ("No stdout line matches {0}. stdout: {1} stderr: {2}" -f $Pattern, $Result.Stdout, $Result.Stderr)
}

function Assert-WtNoStdoutLine {
  param([Parameter(Mandatory)]$Result, [Parameter(Mandatory)][string]$Pattern)
  foreach ($l in (Get-WtLines -Text $Result.Stdout)) { if ($l -match $Pattern) { throw ("A stdout line matches {0} and MUST NOT: {1}" -f $Pattern, $l) } }
}

# A machine-readable form of what the idle check decided, for a reason only.
function Format-WtRun {
  param([Parameter(Mandatory)]$Result)
  return ("exit {0}; stdout: {1}; stderr: {2}" -f $Result.ExitCode, $Result.Stdout.Trim(), $Result.Stderr.Trim())
}

# The stand-in the install test names as the sshd program.
# Records each call's first and second argument in calls.txt beside itself; with -T prints
# probe-output.txt; with a file copies it to test-copy.txt and exits with exit-code.txt's number.
function New-WtSshdStandIn {
  param([Parameter(Mandatory)]$TestHome, [Parameter(Mandatory)][string]$Folder, [int]$TestExit = 0, [string]$ProbeOutput = '')
  $dir = New-WtFolder -TestHome $TestHome -Path $Folder
  $cmd = "@echo off`r`n" +
    "setlocal`r`n" +
    ">>`"%~dp0calls.txt`" echo [%~1] [%~2]`r`n" +
    "if /i `"%~1`"==`"-T`" goto probe`r`n" +
    "copy /y `"%~1`" `"%~dp0test-copy.txt`" >nul`r`n" +
    "set /p CODE=<`"%~dp0exit-code.txt`"`r`n" +
    "exit /b %CODE%`r`n" +
    ":probe`r`n" +
    "type `"%~dp0probe-output.txt`"`r`n" +
    "exit /b 0`r`n"
  Write-WtText -TestHome $TestHome -Path (Join-Path $dir 'sshd-standin.cmd') -Text $cmd
  Write-WtText -TestHome $TestHome -Path (Join-Path $dir 'exit-code.txt') -Text ("{0}`r`n" -f $TestExit)
  Write-WtText -TestHome $TestHome -Path (Join-Path $dir 'probe-output.txt') -Text $ProbeOutput
  return (Join-Path $dir 'sshd-standin.cmd')
}

# A canary for a script that ignores its seam: whether the real machine folder a host installs into
# exists and when it last changed. It names no file and reads none.
function Get-WtRealMark {
  $p = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'agentspace'
  if (-not (Test-Path -LiteralPath $p)) { return 'absent' }
  $i = Get-Item -LiteralPath $p -Force
  return ('present|{0}|{1}' -f $i.LastWriteTimeUtc.Ticks, @(Get-ChildItem -LiteralPath $p -Force -Name).Count)
}

function Assert-WtRealMarkUnchanged {
  param([Parameter(Mandatory)][string]$Before)
  Assert-WtEqual (Get-WtRealMark) $Before 'The machine''s agentspace folder (a script ignored its seam)'
}
