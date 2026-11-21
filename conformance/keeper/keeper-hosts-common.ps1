#Requires -Version 7
# A conformance helper under D34, shared by the keeper's host cases: it starts the keeper only as a separate process, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The helpers that conformance/keeper/test-unmet-hosts-pass.ps1 and conformance/keeper/test-unmet-hosts-watch.ps1
# share (docs/keeper.md, "Conformance cases for a host per project", cases H2 to H18). A case file
# dot-sources conformance/keeper/keeper-common.ps1 first, then this file, both on Windows:
#   . (Join-Path $PSScriptRoot 'keeper-common.ps1')
#   . (Join-Path $PSScriptRoot 'keeper-hosts-common.ps1')
# then sets $keeperCmd to Resolve-KeeperProgram's answer and calls Initialize-KeeperRun and then
# Initialize-HostsRun, before the first case. The caller MUST set $ErrorActionPreference = 'Stop'.
# This file edits nothing of keeper-common.ps1's: it adds functions beside it and, in
# Initialize-HostsRun, points the shared $stub at its own stand-in. Nothing here loads prototype code,
# and nothing names the keeper's program (docs/conformance.md, "Naming the program under test").
#
# What it touches:
# - Every case's folders are keeper-common.ps1's: a state folder (AGENTSPACE_STATE_DIR) and a home
#   folder (USERPROFILE) under the run's folder, which is $env:TEMP\agentspace-conformance-keeper-<guid>.
#   The host registry is written where state.md places it, <USERPROFILE>\.agentspace\hosts.json, which
#   is never the state folder. Every write and delete in this file calls Assert-ConformancePath at the
#   call, against the run's home object ($runHome), as docs/conformance.md, "The program under test
#   is untrusted", asks.
# - The stand-in for ssh.exe is ssh-hosts-stub.cmd, written into the run's folder by
#   Initialize-HostsRun: it prints one line and waits about 20 minutes, like conformance\keeper\ssh-stub.cmd,
#   and it exits at once when a marker file named for its alias exists in the run's markers folder, so
#   that one stand-in makes one host's holder fail and another's live by the alias on its command line
#   (keeper.md, "Checking the keeper"). It connects nowhere and touches no file but its own output.
# - A sampler thread records the launch files' tokens as they appear, so that a case can show that
#   two starts never had their launch files in the folder at the same time.

# Watches the state folder for board-holder-launch.* files and records, in the order first seen, the
# token (32 lowercase hex digits) of every start, and the most tokens ever seen in one look. A look
# that sees two tokens at once is two starts whose launch files overlapped. Compiled before any
# environment variable is changed.
if (-not ('AgentspaceTestHosts.LaunchSampler' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;
using System.Threading;
namespace AgentspaceTestHosts
{
    public sealed class LaunchSampler
    {
        readonly string dir;
        Thread thread;
        volatile bool stop;
        readonly object gate = new object();
        readonly List<string> order = new List<string>();
        int maxConcurrent;
        string overlap = "";
        static readonly Regex Rx = new Regex(@"^board-holder-launch\.([0-9a-f]{32})\.", RegexOptions.Compiled);
        public LaunchSampler(string dir) { this.dir = dir; }
        public void Start() { thread = new Thread(Run); thread.IsBackground = true; thread.Start(); }
        public void Stop() { stop = true; thread.Join(); }
        public string[] Tokens { get { lock (gate) { return order.ToArray(); } } }
        public int MaxConcurrent { get { lock (gate) { return maxConcurrent; } } }
        public string Overlap { get { lock (gate) { return overlap; } } }
        void Run()
        {
            while (!stop)
            {
                try
                {
                    var names = new List<string>();
                    var now = new HashSet<string>();
                    foreach (string path in Directory.EnumerateFiles(dir, "board-holder-launch.*"))
                    {
                        string name = Path.GetFileName(path);
                        Match m = Rx.Match(name);
                        if (m.Success) { now.Add(m.Groups[1].Value); names.Add(name); }
                    }
                    lock (gate)
                    {
                        foreach (string t in now) { if (!order.Contains(t)) order.Add(t); }
                        if (now.Count > maxConcurrent) { maxConcurrent = now.Count; overlap = string.Join(", ", names); }
                    }
                }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
                Thread.Sleep(1);
            }
        }
    }
}
'@
}

# The projects the cases name: the box's, <p> (a host of its own), and <q>, a project that may have no
# record in the registry (keeper.md, "Conformance cases for a host per project").
$boxProject = 'swimcoach'
$pProject = 'agentspace'
$qProject = 'zeta'
$farFuture = '2099-01-01T00:00:00Z'

$script:caseId = ''
$script:caseSpec = 'keeper.md'

# One result, named for the spec section and the case it enforces: keeper.md#<section> (H3): <text>.
function Add-HostCheck([bool]$Ok, [string]$Text, [string]$Why = '') {
  $name = "$($script:caseSpec) ($($script:caseId)): $Text"
  if ($Ok) { Write-ConformanceResult -Name $name -Status PASS }
  else {
    if (-not $Why) {
      $Why = 'The check did not hold.'
      try { $l = Read-LogLines; if ($l.Count -gt 0) { $Why = 'The check did not hold. board-holder.log ends: ' + (($l | Select-Object -Last 4) -join ' | ') } } catch { }
    }
    Write-ConformanceResult -Name $name -Status FAIL -Reason $Why
  }
}

function Add-HostSkipped([string]$Text, [string]$Why) {
  Write-ConformanceResult -Name "$($script:caseSpec) ($($script:caseId)): $Text" -Status SKIPPED -Reason $Why
}

# Writes the stand-in for ssh into the run's folder and points keeper-common.ps1's $stub at it, so that
# every helper there (Invoke-Keeper, Start-Stub, Start-Watch) runs the keeper and its holders with it.
# The stand-in exits at once when <run>\markers\fail.<alias> exists. The alias is the last word of its
# arguments, found without starting another program: a batch splits its arguments at the = of
# ExitOnForwardFailure=yes too, and the for loop below gives the same last word whichever way.
function Initialize-HostsRun {
  $hostsStub = Join-Path $script:temp 'ssh-hosts-stub.cmd'
  $lines = @('@echo off',
    'REM A stub for ssh (a program outside agentspace) used by the keeper conformance checks of a host per project. See docs/conformance.md.',
    'for %%a in (%*) do set "last=%%a"',
    'if exist "%~dp0markers\fail.%last%" exit /b 1',
    'echo stub-started %*',
    'ping -n 1200 127.0.0.1 >nul')
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $script:temp 'markers')) | Out-Null
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $hostsStub), (($lines -join "`r`n") + "`r`n"))
  $script:stub = $hostsStub
}

# Makes the stand-in exit at once for one alias, from now on.
function Set-FailMarker([string]$Alias) {
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $script:temp "markers\fail.$Alias")), '')
}

function Clear-FailMarkers {
  foreach ($f in @(Get-ChildItem -LiteralPath (Join-Path $script:temp 'markers') -Force -ErrorAction SilentlyContinue)) {
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $runHome -Path $f.FullName) -Force -ErrorAction SilentlyContinue
  }
}

# One case: a fresh state folder and home folder with the harness's child environment for them, the
# registry's folder in the home, the body, and a cleanup of every process the case started and every
# marker, in a finally.
function Invoke-HostCase([string]$Id, [string]$Spec, [scriptblock]$Body) {
  $null = New-CaseHome
  $script:caseId = $Id
  $script:caseSpec = $Spec
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $env:USERPROFILE '.agentspace')) | Out-Null
  try { & $Body } catch { Add-HostCheck $false 'No unexpected error' "An error stopped the case: $($_.Exception.Message)" } finally { Stop-RunProcesses; Clear-FailMarkers }
}

# ---- Leases, the registry and the records ----

# A lease's ID and alias are made from its project and a two-digit hex tag, so that the case's own
# files can name them: <tag>-0000-4000-8000-000000000000 and agentspace-<project>-000000<tag>.
function Get-HostLeaseId([string]$Tag) { "000000$Tag-0000-4000-8000-000000000000" }
function Get-HostAlias([string]$Project, [string]$Tag) { "agentspace-$Project-000000$Tag" }

function New-HostLease([string]$Project, [string]$Tag, [string]$Expires, [switch]$Released) {
  if ($Released) {
    return [ordered]@{
      id = Get-HostLeaseId $Tag; project = $Project; state = 'released'; platform = 'linux/arm64'; profile = 'agent-linux'
      size = 'm'; ttl_seconds = 28800; created_at = $created; expires_at = $Expires; released_at = '2026-11-17T09:00:00Z'
    }
  }
  [ordered]@{
    id = Get-HostLeaseId $Tag; project = $Project; state = 'ready'; platform = 'linux/arm64'; profile = 'agent-linux'
    size = 'm'; ttl_seconds = 28800; created_at = $created; expires_at = $Expires
    ssh = [ordered]@{ host_alias = Get-HostAlias $Project $Tag; user = 'ubuntu' }
  }
}

function Get-RegistryPath { Join-Path $env:USERPROFILE '.agentspace\hosts.json' }

# A record of the host registry (state.md, "The host registry"): the box's stack is agentbox, every
# other host's is agentspace-host-<project>.
function New-HostRecord([string]$Project, [string]$State = 'ready') {
  [ordered]@{ project = $Project; stack = $(if ($Project -ceq $boxProject) { 'agentbox' } else { "agentspace-host-$Project" }); state = $State }
}

function Write-RegistryText([string]$Text) {
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Get-RegistryPath)), $Text, [Text.UTF8Encoding]::new($false))
}

# Writes a registry the way state.md gives its bytes: compact JSON and one LF.
function Write-Registry($Records) {
  Write-RegistryText (([ordered]@{ version = 0; hosts = @($Records) } | ConvertTo-Json -Compress -Depth 10) + "`n")
}

# A holder's record for one host: board-holder.json for the box, board-holder.<project>.json for another
# host (state.md, "Another host's holder"). -FileName names another file, for a case that writes one
# that is not a host's record.
function Get-HostRecordPath([string]$Project, [string]$FileName = '') {
  if (-not $FileName) { $FileName = if ($Project -ceq $boxProject) { 'board-holder.json' } else { "board-holder.$Project.json" } }
  Join-Path $env:AGENTSPACE_STATE_DIR $FileName
}

function Write-HostRecord([string]$Project, [int]$ProcessId, [string]$StartedAt, [string]$LeaseId, [string]$Alias, [string]$FileName = '') {
  $text = [ordered]@{ pid = $ProcessId; process_started_at = $StartedAt; lease_id = $LeaseId; alias = $Alias } | ConvertTo-Json -Compress
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path (Get-HostRecordPath $Project $FileName)), $text + "`n")
}

function Read-HostRecord([string]$Project) {
  $p = Get-HostRecordPath $Project
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $null }
  try { Get-Content -LiteralPath $p -Raw | ConvertFrom-Json -AsHashtable -DateKind String } catch { $null }
}

# A running holder the keeper would trust on a host: a stand-in with the holder's arguments, and the
# host's record naming it.
function Start-HostHolder([string]$Project, [string]$Tag) {
  $h = Start-Stub (Get-HostAlias $Project $Tag)
  Write-HostRecord $Project $h.Pid $h.Started (Get-HostLeaseId $Tag) (Get-HostAlias $Project $Tag)
  $h
}

# The bytes of a file as text, shared as a reader SHOULD share (state.md, "The keeper's logs").
function Read-SharedText([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  try {
    $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { [IO.StreamReader]::new($fs, [Text.UTF8Encoding]::new($false)).ReadToEnd() } finally { $fs.Dispose() }
  } catch { $null }
}

# The text of a host's holder log: board-holder-ssh.log for the box, board-holder-ssh.<project>.log for another host.
function Get-HostSshLogText([string]$Project) {
  $n = if ($Project -ceq $boxProject) { 'board-holder-ssh.log' } else { "board-holder-ssh.$Project.log" }
  Read-SharedText (Join-Path $env:AGENTSPACE_STATE_DIR $n)
}

function Get-StateNames {
  @(Get-ChildItem -LiteralPath $env:AGENTSPACE_STATE_DIR -Force -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
}

# The names of the records of every host but the box that the state folder holds.
function Get-OtherRecordNames { @(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder\.[a-z0-9-]+\.json\z' }) }

# ---- Lines ----

$logTime = '\A\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ '

# A line of a pass: <time> <action> lease=<lease> alias=<alias> pid=<pid>[ host=<project>]. -Project ''
# means no host field. The line may carry a reason after the host field.
function Test-HostLine([string]$Line, [string]$Action, [string]$LeaseId, [string]$Alias, [string]$ProcessId, [string]$Project) {
  $tail = if ($Project) { ' host=' + [regex]::Escape($Project) + '(\s|\z)' } else { '(?! host=)(\s|\z)' }
  $Line -cmatch ($logTime + $Action + ' lease=' + [regex]::Escape($LeaseId) + ' alias=' + [regex]::Escape($Alias) + ' pid=' + [regex]::Escape($ProcessId) + $tail)
}

# An idle line: the box's has no host field, another host's names its project.
function Test-IdleLine([string]$Line, [string]$Project = '') {
  $Line -cmatch ($logTime + 'idle lease=- alias=- pid=-' + $(if ($Project) { ' host=' + [regex]::Escape($Project) } else { '' }) + '\z')
}

# The lines of the log that hold an action word (a space either side).
function Select-ActionLines($Log, [string]$Action) { @($Log | Where-Object { $_ -cmatch ($logTime + $Action + ' ') }) }

# The n of every "backoff seconds=<n> host=<project>" line, in order; with no project, of every backoff line that has no host.
function Get-HostBackoffs($Log, [string]$Project = '') {
  $rx = if ($Project) { $logTime + 'backoff seconds=(\d+) host=' + [regex]::Escape($Project) + '\z' } else { $logTime + 'backoff seconds=(\d+)\z' }
  @($Log | ForEach-Object { if ($_ -cmatch $rx) { [int]$Matches[1] } })
}

function Get-HostGraces($Log, [string]$Project = '') {
  $rx = if ($Project) { $logTime + 'grace seconds=(\d+) host=' + [regex]::Escape($Project) + '\z' } else { $logTime + 'grace seconds=(\d+)\z' }
  @($Log | ForEach-Object { if ($_ -cmatch $rx) { [int]$Matches[1] } })
}

function Get-FirstLineIndex($Log, [string]$Pattern) {
  for ($i = 0; $i -lt $Log.Count; $i++) { if ($Log[$i] -cmatch $Pattern) { return $i } }
  -1
}

function Read-JsonBytes([byte[]]$Bytes) {
  try { [Text.UTF8Encoding]::new($false).GetString($Bytes) | ConvertFrom-Json -AsHashtable -DateKind String } catch { $null }
}

# ---- Resetting, locking, launch files ----

# Forgets what a pass did: every holder, and the keeper's records, logs and holder logs.
function Reset-HostsState {
  Stop-RunProcesses
  foreach ($f in @(Get-ChildItem -LiteralPath $env:AGENTSPACE_STATE_DIR -Force -ErrorAction SilentlyContinue)) {
    if ($f.Name -cmatch '\Aboard-holder(\.[a-z0-9-]+)?\.json\z' -or $f.Name -ceq 'board-holder.log' -or $f.Name -cmatch '\Aboard-holder-ssh(\.[a-z0-9-]+)?\.log(\.1)?\z') {
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $runHome -Path $f.FullName) -Force -ErrorAction SilentlyContinue
    }
  }
}

# Takes board-holder.lock the way the keeper does (a share mode that refuses a second taker), as soon
# as the keeper's pass lets it go. Returns the stream that holds it, or $null.
function Lock-PassLock($Process) {
  $path = Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.lock'
  $box = @{ Stream = $null }
  $got = Wait-Until {
    try { $box.Stream = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path $path), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $true } catch [IO.IOException] { $false }
  } 30 $Process
  if ($got) { $box.Stream } else { $null }
}

# Runs one pass of the keeper while a watcher keeps a copy of every launch file and a sampler records
# the tokens of the starts. A pass whose files the watcher did not all see is forgotten (Reset-HostsState)
# and run again, up to 4 times, since the files live only while a holder is started. Returns Code, Files
# (name to bytes), Tokens (in the order the starts began), MaxConcurrent, Attempts and Complete (every
# start's request was seen, and as many starts as expected).
function Invoke-CapturedPass([string]$At, [int]$Starts) {
  $out = $null
  for ($attempt = 1; $attempt -le 4; $attempt++) {
    $cap = [AgentspaceTest.FileCapture]::new($env:AGENTSPACE_STATE_DIR)
    $smp = [AgentspaceTestHosts.LaunchSampler]::new($env:AGENTSPACE_STATE_DIR)
    $cap.Start(); $smp.Start()
    try { $code = Invoke-Keeper $At } finally { $smp.Stop(); $files = $cap.Stop() }
    $tokens = @($smp.Tokens)
    $names = @($files.Keys)
    $seen = @($tokens | Where-Object { $names -ccontains "board-holder-launch.$_.json" -and $names -ccontains "board-holder-launch.$_.claim" -and $names -ccontains "board-holder-launch.$_.result.json" })
    $complete = $tokens.Count -eq $Starts -and $seen.Count -eq $Starts
    $out = [pscustomobject]@{ Code = $code; Files = $files; Tokens = $tokens; MaxConcurrent = $smp.MaxConcurrent; Overlap = $smp.Overlap; Attempts = $attempt; Complete = $complete }
    if ($complete -or $Starts -eq 0) { break }
    if ($attempt -lt 4) { Reset-HostsState }
  }
  $out
}

# The launch request of the n-th start of a captured pass (see Invoke-CapturedPass), as a hashtable, or $null.
function Get-CapturedRequest($Result, [int]$Index) {
  if ($Index -ge @($Result.Tokens).Count) { return $null }
  $name = "board-holder-launch.$(@($Result.Tokens)[$Index]).json"
  if (-not $Result.Files.ContainsKey($name)) { return $null }
  Read-JsonBytes $Result.Files[$name]
}

# True when a launch request's log is the named file of the state folder, as a full path.
function Test-RequestLog($Request, [string]$LogName) {
  [bool]$Request -and [string]::Equals([string]$Request['log'], (Join-Path $env:AGENTSPACE_STATE_DIR $LogName), [StringComparison]::OrdinalIgnoreCase)
}
