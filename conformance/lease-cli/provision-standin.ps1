#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Helpers for the lease CLI's v0.07 conformance checks of a pending lease and of no_capacity
# (E0.07-T04-S01, card #0155; docs/driver.md, "Conformance cases for v0.07"). A check dot-sources
# harness.ps1, lease-cli-common.ps1 and lease-check-support.ps1 first, and this file last.
#
# - It starts no program of agentspace's. The lease CLI is started only through the harness
#   (Invoke-AgentspaceCli). The one program it places is a stand-in for the provisioning run, which
#   AGENTSPACE_PROVISION_CMD names, so that no check ever starts a real run (docs/lease-api.md,
#   "Environment"; docs/driver.md, pin P12).
# - The stand-in is a .cmd (or .bat) file under the throwaway home's standins folder. It makes no host.
#   It records, beside itself, its raw argument line, its working folder, whether its standard input
#   held anything, the NAMES of the variables it was given, the VALUES of USERPROFILE,
#   AGENTSPACE_STATE_DIR, AGENTSPACE_SSH_DIR and AGENTSPACE_PROXY_COMMAND only, its own start time and
#   process, and its parent's command line. The detached start gives it the user's logon environment
#   (docs/driver.md, "Starting a provisioning run", the detached start), so nothing of the CLI's
#   reaches it except the four variables the spec names, and what it records is the evidence.
# - Every write and delete a check makes through this file calls Assert-ConformancePath at the call.
#   The stand-in's own writes beside itself are the one exception (docs/conformance.md, "The program
#   under test is untrusted").
# - A check MUST NOT print, log or format the values of an environment the stand-in captured. A
#   failure reason names a variable, or counts, and never quotes a value.
# - Every wait is bounded. A process is stopped only by the PID the stand-in recorded, and only when
#   it still has the name and the start time the stand-in recorded.

$script:PvCleanup = [System.Collections.Generic.List[object]]::new()

# --- The recorder the stand-in runs (Windows PowerShell 5.1, ASCII only) --------------------------------

$script:PvRecorderText = @'
# A recorder for a stand-in provisioning run of a conformance check (conformance/lease-cli/provision-standin.ps1). It makes no host.
$ErrorActionPreference = 'Continue'
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$stamp = [DateTime]::UtcNow
$names = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Sort-Object)
function Get-PvEnv([string]$n) { if ($names -contains $n) { return [Environment]::GetEnvironmentVariable($n) } else { return $null } }
$redirected = [Console]::IsInputRedirected
$bytes = -1
if ($redirected) {
  $bytes = 0
  try {
    $in = [Console]::OpenStandardInput()
    $buf = New-Object byte[] 4096
    $t = $in.ReadAsync($buf, 0, $buf.Length)
    if ($t.Wait(3000)) { $bytes = [int]$t.Result }
  } catch { $bytes = 0 }
}
$me = Get-Process -Id $PID
$self = Get-CimInstance Win32_Process -Filter "ProcessId=$PID"
$parentId = [int]$self.ParentProcessId
$parent = Get-CimInstance Win32_Process -Filter "ProcessId=$parentId"
$rec = [ordered]@{
  args = @($args | ForEach-Object { [string]$_ })
  cwd = [Environment]::CurrentDirectory
  stdin_redirected = [bool]$redirected
  stdin_bytes = $bytes
  var_names = $names
  userprofile = Get-PvEnv 'USERPROFILE'
  state_dir = Get-PvEnv 'AGENTSPACE_STATE_DIR'
  ssh_dir = Get-PvEnv 'AGENTSPACE_SSH_DIR'
  proxy_command = Get-PvEnv 'AGENTSPACE_PROXY_COMMAND'
  start_utc = $stamp.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
  pid = $PID
  start_ticks = $me.StartTime.ToUniversalTime().Ticks
  parent_pid = $parentId
  parent_name = $(if ($parent) { [string]$parent.Name } else { '' })
  parent_command_line = $(if ($parent) { [string]$parent.CommandLine } else { '' })
  parent_start_ticks = $(if ($parent) { $parent.CreationDate.ToUniversalTime().Ticks } else { 0 })
}
$id = '{0}-{1}' -f $stamp.Ticks, $PID
$utf8 = New-Object System.Text.UTF8Encoding($false)
$tmp = Join-Path $dir ('call-' + $id + '.tmp')
[IO.File]::WriteAllText($tmp, ($rec | ConvertTo-Json -Compress -Depth 4), $utf8)
Move-Item -LiteralPath $tmp -Destination (Join-Path $dir ('call-' + $id + '.json'))
$sleep = 0
$sleepFile = Join-Path $dir 'sleep-seconds.txt'
if (Test-Path -LiteralPath $sleepFile) { $sleep = [int]((Get-Content -LiteralPath $sleepFile -Raw).Trim()) }
if ($sleep -gt 0) { Start-Sleep -Seconds $sleep }
[IO.File]::WriteAllText((Join-Path $dir ('call-' + $id + '.end')), 'ended', $utf8)
'@

# --- Cleanup, registered per home -----------------------------------------------------------------------

# Disposes every lock stream a check holds, and stops every stand-in still running by its recorded PID,
# so that no process keeps the throwaway home's folders open when the home is removed.
function Clear-PvCleanup {
  param($Lab)
  foreach ($item in @($script:PvCleanup)) {
    if ($item -is [IO.FileStream]) { try { $item.Dispose() } catch { } }
  }
  foreach ($item in @($script:PvCleanup)) {
    if ($item -is [pscustomobject] -and $item.PSObject.Properties.Name -contains 'Dir' -and $item.PSObject.Properties.Name -contains 'Path') { Stop-PvStandIn -StandIn $item }
  }
  $script:PvCleanup.Clear()
}

# Makes a throwaway home, clears what the harness might have seeded in it, runs $Body with the home, and
# reports the check from the problems $Body returns. A home with no CA reports SKIPPED unless -NoCa.
# The working folder, any held lock and any stand-in still running are put right before the home goes.
function Use-PvLab {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body, [switch]$NoCa)
  $lab = $null
  $script:PvCleanup.Clear()
  $savedCwd = [Environment]::CurrentDirectory
  try {
    $lab = New-ConformanceHome
    if (-not $NoCa -and -not $lab.CaReady) {
      Write-ConformanceResult -Name $Name -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
      return
    }
    Reset-PvState -Lab $lab
    $out = @(& $Body $lab)
    $many = $out | Where-Object { $_ -is [System.Collections.IDictionary] } | Select-Object -Last 1
    $problems = $out | Where-Object { $_ -is [System.Collections.Generic.List[string]] } | Select-Object -Last 1
    if ($null -ne $many) {
      # A body that makes several checks of one run returns a dictionary of suffix to list of problems.
      foreach ($key in $many.Keys) { Complete-ConformanceCheck -Name "$Name $key" -Problems $many[$key] }
    }
    elseif ($null -eq $problems) { Write-ConformanceResult -Name $Name -Status FAIL -Reason 'The check body returned no list of problems' }
    else { Complete-ConformanceCheck -Name $Name -Problems $problems }
  } catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "It threw: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    [Environment]::CurrentDirectory = $savedCwd
    if ($lab) { Clear-PvCleanup -Lab $lab }
    if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
  }
}

# --- Files a check writes in the home --------------------------------------------------------------------

function Get-PvAgentspaceDir($Lab) { Join-Path $Lab.UserProfile '.agentspace' }
function Get-PvHostsPath($Lab) { Join-Path $Lab.UserProfile '.agentspace\hosts.json' }
function Get-PvProjectsPath($Lab) { Join-Path $Lab.UserProfile '.agentspace\projects.json' }
function Get-PvHostsLockPath($Lab) { Join-Path $Lab.UserProfile '.agentspace\hosts.lock' }
function Get-PvHostFolder($Lab, [string]$Project) { Join-Path $Lab.UserProfile ".agentspace\hosts\$Project" }
function Get-PvProvisionLockPath($Lab, [string]$Project) { Join-Path $Lab.UserProfile ".agentspace\hosts\$Project\provision.lock" }

# Removes whatever the harness might have seeded of the host registry, the project list and the hosts
# folder, so that each check starts from the files it writes itself.
function Reset-PvState {
  param([Parameter(Mandatory)]$Lab)
  foreach ($file in @((Get-PvHostsPath $Lab), (Get-PvProjectsPath $Lab), (Get-PvHostsLockPath $Lab))) {
    if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $file) -Recurse -Force }
  }
  $folder = Join-Path $Lab.UserProfile '.agentspace\hosts'
  if (Test-Path -LiteralPath $folder) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $folder) -Recurse -Force }
}

# Removes agentspace's CA from the throwaway home, for a check of a create with no CA.
function Remove-PvCa {
  param([Parameter(Mandatory)]$Lab)
  $ca = Join-Path $Lab.UserProfile '.agentspace\ca'
  if (Test-Path -LiteralPath $ca) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $ca) -Recurse -Force }
}

# Writes text as UTF-8 without a byte order mark, making the folder it is in first.
function Write-PvText {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyString()][string]$Text)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), [Text.UTF8Encoding]::new($false).GetBytes($Text))
}

$script:PvBoxRecord = '{"project":"swimcoach","stack":"agentbox","state":"ready"}'

# One host record in the form state.md's table gives, members in its order. The stack is the project's own
# unless -Stack says otherwise.
function New-PvHostRecord {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][ValidateSet('creating', 'ready', 'failed')][string]$State, [string]$InstanceId, [string]$LastAttemptAt, [string]$Stack)
  if (-not $Stack) { $Stack = "agentspace-host-$Project" }
  $s = '{"project":"' + $Project + '","stack":"' + $Stack + '","state":"' + $State + '"'
  if ($InstanceId) { $s += ',"instance_id":"' + $InstanceId + '"' }
  if ($LastAttemptAt) { $s += ',"last_attempt_at":"' + $LastAttemptAt + '"' }
  return $s + '}'
}

# The host registry's text, compact with one line feed, the box's record first unless -NoBox.
function New-PvHostsText {
  param([string[]]$Records = @(), [switch]$NoBox)
  $all = @()
  if (-not $NoBox) { $all += $script:PvBoxRecord }
  $all += $Records
  return '{"version":0,"hosts":[' + ($all -join ',') + ']}' + "`n"
}

# Writes hosts.json: the box's record, then the records given.
function Set-PvHosts {
  param([Parameter(Mandatory)]$Lab, [string[]]$Records = @(), [switch]$NoBox)
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text (New-PvHostsText -Records $Records -NoBox:$NoBox)
}

# Writes projects.json, listing each name as approved on a date.
function Set-PvProjects {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Names)
  $entries = foreach ($n in $Names) { '{"name":"' + $n + '","approved":"2026-11-20"}' }
  Write-PvText -Lab $Lab -Path (Get-PvProjectsPath $Lab) -Text ('{"version":0,"projects":[' + (@($entries) -join ',') + ']}' + "`n")
}

# A time $Seconds before now, in the contract's form, from the check's own clock.
function Get-PvAgo { param([int]$Seconds) ConvertTo-CcTime ([DateTime]::UtcNow.AddSeconds(-$Seconds)) }

# Takes the lock file at $Path as the check: open with no sharing, creating it (and its folder) when missing,
# and holds it until it is disposed or the home is cleaned up.
function Open-PvLock {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $Path)) | Out-Null
  $fs = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  $script:PvCleanup.Add($fs)
  return $fs
}

# Lets a held lock go after $AfterSeconds, from a thread of this process. Returns the job, which the
# caller waits for with a bound.
function Start-PvLockRelease {
  param([Parameter(Mandatory)][IO.FileStream]$Stream, [Parameter(Mandatory)][int]$AfterSeconds)
  Start-ThreadJob -ScriptBlock { param($s, $n) Start-Sleep -Seconds $n; try { $s.Dispose() } catch { } } -ArgumentList $Stream, $AfterSeconds
}

# --- The stand-in -----------------------------------------------------------------------------------------

# Places a stand-in under the home's standins folder, and returns Dir and Path (the full path to give
# AGENTSPACE_PROVISION_CMD). $Extension is cmd or bat, in any case. $SleepSeconds makes it keep running
# after it has recorded. It writes no log and makes no host.
function New-PvStandIn {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Name, [string]$Extension = 'cmd', [int]$SleepSeconds = 0)
  $dir = Join-Path $Lab.Root "standins\$Name"
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $dir) | Out-Null
  $program = Join-Path $dir "provision.$Extension"
  $lines = @(
    '@echo off'
    'REM A stand-in for a provisioning run, for a conformance check of the lease CLI. It makes no host.'
    '>>"%~dp0started.log" echo ARGS=%*'
    'echo STANDIN-STDOUT-MARKER'
    'echo STANDIN-STDERR-MARKER 1>&2'
    '"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0recorder.ps1" %*'
    'exit /b 0'
  )
  Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $program) -Encoding ascii -Value $lines
  Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $dir 'recorder.ps1')) -Encoding ascii -Value $script:PvRecorderText
  if ($SleepSeconds -gt 0) { Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $dir 'sleep-seconds.txt')) -Encoding ascii -Value ([string]$SleepSeconds) }
  $standIn = [pscustomobject]@{
    Dir = (Assert-ConformancePath -ConformanceHome $Lab -Path $dir)
    Path = (Assert-ConformancePath -ConformanceHome $Lab -Path $program)
    Name = $Name
  }
  $script:PvCleanup.Add($standIn)
  return $standIn
}

# The raw argument lines the stand-in's .cmd logged, one for each start.
function Get-PvStartedLines {
  param([Parameter(Mandatory)]$StandIn)
  $f = Join-Path $StandIn.Dir 'started.log'
  if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { return @() }
  try { return @([IO.File]::ReadAllLines($f) | Where-Object { $_ -ne '' }) } catch { return @() }
}

# The records the stand-in wrote, in the order of their names, each a hashtable with Id and Ended added.
function Get-PvCalls {
  param([Parameter(Mandatory)]$StandIn)
  $calls = @()
  if (-not (Test-Path -LiteralPath $StandIn.Dir -PathType Container)) { return $calls }
  foreach ($f in @(Get-ChildItem -LiteralPath $StandIn.Dir -Filter 'call-*.json' -File | Sort-Object Name)) {
    try {
      $c = [IO.File]::ReadAllText($f.FullName) | ConvertFrom-Json -AsHashtable -DateKind String -ErrorAction Stop
      $c['Id'] = $f.BaseName
      $c['Ended'] = Test-Path -LiteralPath (Join-Path $StandIn.Dir ($f.BaseName + '.end')) -PathType Leaf
      $calls += $c
    } catch { }
  }
  return $calls
}

# Waits, with a bound, until the stand-in's .cmd has logged $Count starts. Returns the count it saw.
function Wait-PvStarted {
  param([Parameter(Mandatory)]$StandIn, [int]$Count = 1, [int]$TimeoutSeconds = 10)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  do {
    $n = @(Get-PvStartedLines $StandIn).Count
    if ($n -ge $Count) { return $n }
    Start-Sleep -Milliseconds 200
  } while ($sw.Elapsed.TotalSeconds -lt $TimeoutSeconds)
  return @(Get-PvStartedLines $StandIn).Count
}

# Waits, with a bound, until the stand-in has written $Count records. Returns the records it has.
function Wait-PvCalls {
  param([Parameter(Mandatory)]$StandIn, [int]$Count = 1, [int]$TimeoutSeconds = 40)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  do {
    $c = @(Get-PvCalls $StandIn)
    if ($c.Count -ge $Count) { return $c }
    Start-Sleep -Milliseconds 250
  } while ($sw.Elapsed.TotalSeconds -lt $TimeoutSeconds)
  return @(Get-PvCalls $StandIn)
}

# The problems with a stand-in that MUST NOT have been started: after a settling wait, its .cmd logged
# no start. A start begins within a fraction of a second of the CLI's call returning, so 3 seconds is
# ample.
function Get-PvNoStartProblems {
  param([Parameter(Mandatory)]$StandIn, [int]$SettleSeconds = 3)
  $p = [System.Collections.Generic.List[string]]::new()
  Start-Sleep -Seconds $SettleSeconds
  $n = @(Get-PvStartedLines $StandIn).Count
  if ($n -ne 0) { $p.Add("The stand-in was started $n time(s), not 0") }
  return , $p
}

# The problems with a stand-in that MUST have been started exactly once, with exactly `provision -Host
# <project>` as its argument line, and, after it, nothing else. The record is read with Get-PvCalls.
function Get-PvStartedOnceProblems {
  param([Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][string]$Project, [int]$StartWaitSeconds = 10)
  $p = [System.Collections.Generic.List[string]]::new()
  $n = Wait-PvStarted -StandIn $StandIn -Count 1 -TimeoutSeconds $StartWaitSeconds
  if ($n -lt 1) { $p.Add("The stand-in was not started within $StartWaitSeconds seconds"); return , $p }
  $calls = @(Wait-PvCalls -StandIn $StandIn -Count 1 -TimeoutSeconds 40)
  if ($calls.Count -lt 1) { $p.Add('The stand-in was started but wrote no record within 40 seconds') }
  Start-Sleep -Seconds 2
  $lines = @(Get-PvStartedLines $StandIn)
  if ($lines.Count -ne 1) { $p.Add("The stand-in was started $($lines.Count) times, not once") }
  if ($lines.Count -ge 1 -and $lines[0] -cne "ARGS=provision -Host $Project") { $p.Add("The stand-in's raw argument line is not 'provision -Host <project>'") }
  if ($calls.Count -ge 1) {
    $argv = @($calls[0]['args'])
    if (($argv -join '|') -cne "provision|-Host|$Project") { $p.Add("The stand-in's arguments are not provision, -Host and the project: it got $($argv.Count) arguments") }
  }
  return , $p
}

# Normalizes a path for comparison: full, with a short 8.3 folder name expanded. $null stays $null.
function ConvertTo-PvFullPath {
  param([AllowNull()][string]$Path)
  if ([string]::IsNullOrEmpty($Path)) { return $null }
  try { return [IO.Path]::GetFullPath($Path).TrimEnd('\') } catch { return $Path }
}

# True when the process the stand-in recorded is still running as the same process.
function Test-PvRecordedAlive {
  param([Parameter(Mandatory)][int]$ProcessId, [Parameter(Mandatory)][long]$StartTicks, [string[]]$Names = @('cmd', 'powershell'))
  $proc = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
  if ($null -eq $proc) { return $false }
  if ($Names -notcontains $proc.ProcessName) { return $false }
  try { return ([Math]::Abs($proc.StartTime.ToUniversalTime().Ticks - $StartTicks) -lt 20000000) } catch { return $false }
}

# Stops a stand-in that is still running, by the PIDs it recorded, only where the process still has the
# recorded name and start time, and so is still the one the CLI started. It first waits, with a bound,
# for the stand-in to end on its own. Nothing is stopped by name.
function Stop-PvStandIn {
  param([Parameter(Mandatory)]$StandIn, [int]$WaitSeconds = 20)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  do {
    $started = @(Get-PvStartedLines $StandIn).Count
    $calls = @(Get-PvCalls $StandIn)
    if ($calls.Count -ge $started -and @($calls | Where-Object { -not $_['Ended'] }).Count -eq 0) { return }
    Start-Sleep -Milliseconds 300
  } while ($sw.Elapsed.TotalSeconds -lt $WaitSeconds)
  foreach ($c in @(Get-PvCalls $StandIn)) {
    if ($c['Ended']) { continue }
    if ($c['parent_name'] -ceq 'cmd.exe' -and (Test-PvRecordedAlive -ProcessId ([int]$c['parent_pid']) -StartTicks ([long]$c['parent_start_ticks']) -Names @('cmd'))) {
      try { (Get-Process -Id ([int]$c['parent_pid'])).Kill($true) } catch { }
    }
    if (Test-PvRecordedAlive -ProcessId ([int]$c['pid']) -StartTicks ([long]$c['start_ticks']) -Names @('powershell')) {
      try { (Get-Process -Id ([int]$c['pid'])).Kill($true) } catch { }
    }
  }
}

# --- Running the lease CLI ----------------------------------------------------------------------------------

# Runs `agentspace lease <Arguments>` with AGENTSPACE_PROVISION_CMD naming the stand-in (when one is given),
# and the check's ProxyCommand unless $EnvVars says otherwise. $EnvVars entries replace these, and a $null
# value removes one. -InputJson is written to the CLI's standard input. Returns the harness's result, and
# Seconds, the whole seconds the run took.
function Invoke-PvLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments, $StandIn, [hashtable]$EnvVars = @{}, [string]$InputJson)
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy }
  if ($null -ne $StandIn) { $e['AGENTSPACE_PROVISION_CMD'] = $StandIn.Path }
  foreach ($k in @($EnvVars.Keys)) { $e[$k] = $EnvVars[$k] }
  $splat = @{ ConformanceHome = $Lab; Arguments = (@('lease') + $Arguments); Environment = $e }
  if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputJson'] = $InputJson }
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $r = Invoke-AgentspaceCli @splat
  $r | Add-Member -NotePropertyName Seconds -NotePropertyValue $sw.Elapsed.TotalSeconds
  return $r
}

# --- Assertions shared by the checks --------------------------------------------------------------------------

# The problems with a run that printed a `pending` lease for $Project: the success test, state pending,
# and none of expires_at, released_at, ssh and reasons.
function Get-PvPendingDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    if ($doc['state'] -cne 'pending') { $p.Add("The lease's state is '$($doc['state'])', not pending") }
    if ($doc['project'] -cne $Project) { $p.Add('The lease is for another project') }
    foreach ($m in 'expires_at', 'released_at', 'ssh', 'reasons') { if ($doc.ContainsKey($m)) { $p.Add("A pending lease carries $m") } }
  }
  return , $p
}

# The problems with a lease printed as unsatisfiable with exactly the given reason codes, in that order.
function Get-PvUnsatisfiableDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string[]]$Codes)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    if ($doc['state'] -cne 'unsatisfiable') { $p.Add("The lease's state is '$($doc['state'])', not unsatisfiable") }
    if ($doc['project'] -cne $Project) { $p.Add('The lease is for another project') }
    $got = @($doc['reasons'] | ForEach-Object { [string]$_['code'] })
    if (($got -join ',') -cne ($Codes -join ',')) { $p.Add("The reason codes are [$($got -join ', ')], not [$($Codes -join ', ')]") }
    foreach ($r in @($doc['reasons'] | Where-Object { $null -ne $_ })) { if (-not (Test-CcNoBackendWords ([string]$r['message']))) { $p.Add("The $($r['code']) message names a backend, a location or a machine") } }
  }
  return , $p
}

# The problems with a run that was meant to fail with `internal`: the harness's error test (exit 1, nothing
# on stdout, one compact error document that validates), and a message with no absolute path.
function Get-PvInternalProblems {
  param([Parameter(Mandatory)]$Run)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceErrorProblems -Run $Run -Code 'internal')) { $p.Add($x) }
  if ($Run.ErrorJson -is [hashtable]) {
    $m = [string]$Run.ErrorJson['message']
    if ($m -match '(?i)[A-Za-z]:[\\/]' -or $m.Contains('\\')) { $p.Add('The message holds an absolute path') }
  }
  return , $p
}

# The names of the entries under the leases folder and of the alias files, which a pending or
# unsatisfiable answer and a failed create MUST leave empty.
function Get-PvLeaseFilesProblems {
  param([Parameter(Mandatory)]$Lab)
  $p = [System.Collections.Generic.List[string]]::new()
  $leases = @(Get-CcFileNames (Join-Path $Lab.UserProfile '.agentspace\leases'))
  if ($leases.Count -gt 0) { $p.Add("There are $($leases.Count) entries under the leases folder") }
  $aliases = @(Get-CcFileNames (Join-Path $Lab.SshDir 'agentspace.d'))
  if ($aliases.Count -gt 0) { $p.Add("There are $($aliases.Count) alias files") }
  return , $p
}

# The number of lease records in the lease state file: 0 for no file, -1 for a file that is not a document.
function Get-PvRecordCount {
  param([Parameter(Mandatory)]$Lab)
  $path = Get-CcStatePath $Lab
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return 0 }
  try { return @(([IO.File]::ReadAllText($path) | ConvertFrom-Json -Depth 50 -ErrorAction Stop).leases).Count } catch { return -1 }
}

# The problems with a failed create: no lease record, no lease folder and no alias file.
function Get-PvNothingRecordedProblems {
  param([Parameter(Mandatory)]$Lab)
  $p = [System.Collections.Generic.List[string]]::new()
  $n = Get-PvRecordCount $Lab
  if ($n -ne 0) { $p.Add($(if ($n -lt 0) { 'The lease state file is not a JSON document' } else { "The lease state file holds $n records" })) }
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $Lab)) { $p.Add($x) }
  return , $p
}

# The hash of hosts.json's bytes, or 'absent'.
function Get-PvHostsHash { param([Parameter(Mandatory)]$Lab) Get-CcFileHash (Get-PvHostsPath $Lab) }

# The record for $Project in hosts.json, as a hashtable, or $null when there is none or no valid file.
function Get-PvHostRecord {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project)
  $path = Get-PvHostsPath $Lab
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  $doc = ConvertFrom-CcJson ([IO.File]::ReadAllText($path))
  if ($doc -isnot [hashtable]) { return $null }
  foreach ($r in @($doc['hosts'])) { if ($r['project'] -ceq $Project) { return $r } }
  return $null
}

# The problems with hosts.json as a file a writer wrote: it validates against its schema, is UTF-8 with no
# byte order mark, is compact JSON, and ends in exactly one line feed (state.md, "The host registry").
function Get-PvHostsFileProblems {
  param([Parameter(Mandatory)]$Lab)
  $p = [System.Collections.Generic.List[string]]::new()
  $path = Get-PvHostsPath $Lab
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { $p.Add('There is no hosts.json'); return , $p }
  $bytes = [IO.File]::ReadAllBytes($path)
  if (Get-ConformanceHasBom $bytes) { $p.Add('hosts.json starts with a byte order mark') }
  $text = [Text.UTF8Encoding]::new($false).GetString($bytes)
  if (-not (Test-ConformanceOneLine $text)) { $p.Add('hosts.json is not one line ending in one line feed') }
  elseif (-not (Test-ConformanceJsonCompact $text.TrimEnd("`n"))) { $p.Add('hosts.json is not compact JSON') }
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { $p.Add('hosts.json does not validate against hosts.state.schema.json') }
  return , $p
}

# The problems with hosts.json not being exactly the bytes $Expected (UTF-8, no byte order mark).
function Get-PvHostsTextProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Expected)
  $p = [System.Collections.Generic.List[string]]::new()
  $path = Get-PvHostsPath $Lab
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { $p.Add('There is no hosts.json'); return , $p }
  $text = [Text.UTF8Encoding]::new($false).GetString([IO.File]::ReadAllBytes($path))
  if ($text -cne $Expected) { $p.Add('hosts.json is not the bytes the spec gives for this change') }
  return , $p
}

# True when $Time is a contract time within [$Before - 1s, $After + 1s]. The second of slack each way allows
# for a writer that rounds the clock to the second where this check truncates it.
function Test-PvTimeBetween {
  param([string]$Time, [DateTime]$Before, [DateTime]$After)
  if ($Time -cnotmatch '\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\z') { return $false }
  $t = ConvertFrom-CcTime $Time
  return ($t -ge $Before.AddSeconds(-1) -and $t -le $After.AddSeconds(1))
}
