#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The harness for the lease CLI's conformance checks (D34; docs/conformance.md). Every check in this
# folder dot-sources it, and it is the only code that starts the program under test.
#
# - The program under test is named by an environment variable (AGENTSPACE_CMD for the lease CLI,
#   AGENTSPACE_OPERATOR_CMD for the operator). A variable that is unset or empty takes its default
#   from conformance/defaults.ps1, and nowhere else: the install folder's program, found from the
#   real profile folder and never from the USERPROFILE variable, which the harness points into a
#   throwaway home. The harness and the checks MUST NOT name a program any other way.
# - Every program the harness starts runs as a separate process, with USERPROFILE, HOME,
#   LOCALAPPDATA, APPDATA, AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR pointed into a throwaway home
#   under $env:TEMP, and with the AWS variables arranged so that a mistaken run cannot reach AWS or
#   the real token. A
#   check MUST NOT read or write the real %USERPROFILE%\.agentspace, the real ~/.ssh or the real CA.
# - Nothing here loads a program's code.
#
# Interface, pinned for every story that builds on it (docs/conformance.md, "Running the suite"):
#   New-ConformanceHome, Invoke-AgentspaceCli, Test-ConformanceSchema, Write-ConformanceResult,
#   Remove-ConformanceHome and Exit-ConformanceRun, plus Invoke-ConformanceProgram, which runs any
#   program a conformance variable names, Assert-ConformancePath, which every write and delete a check
#   makes MUST call as the path of that very call, and Assert-ConformanceHomeRoot, which guards the
#   home's own creation and removal (E0.06-T04-S01, #0110). E0.07-T04-S01 (#0155) adds
#   Add-ConformanceReadyHost, which seeds a home's host registry, and Get-ConformanceTripwireStarts,
#   and gives every lease CLI run a default AGENTSPACE_PROVISION_CMD (below).
#
# The provisioning-run tripwire (E0.07-T04-S01, D48, D54; docs/driver.md, "Conformance cases for
# v0.07", pin P12). Under D48 a lease CLI that places a lease for a project whose host is not ready can
# start a provisioning run, and a run's default program is agentspace-operator.exe beside the CLI, which
# in target\dist is the real operator, against the real AWS. So every lease CLI run the harness starts
# (AGENTSPACE_CMD, through Invoke-AgentspaceCli or Invoke-ConformanceProgram) gets
# AGENTSPACE_PROVISION_CMD set to a tripwire in the home, a .cmd that records its start beside itself
# and exits 1 at once, unless the check names another value through -Environment. A check that
# reaches a run on purpose MUST name its own stand-in. The tripwire is not given to the operator, the
# ProxyCommand or the keeper, which never start a run, so that no other suite's view of its
# environment changes. Remove-ConformanceHome reports FAIL when the tripwire was ever started, because
# a check then reached a run it had not named a stand-in for.

$script:ConformanceRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$script:ConformanceSchemas = Join-Path $script:ConformanceRepo 'schemas'
$script:ConformanceDefaultsFile = Join-Path $script:ConformanceRepo 'conformance\defaults.ps1'
$global:ConformanceCounts = @{ PASS = 0; FAIL = 0; SKIPPED = 0 }

# Variables that make a child process reach AWS or the real token: removed from every child.
$script:ConformanceAwsRemoved = 'AWS_PROFILE', 'AWS_DEFAULT_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN',
  'AWS_CONTAINER_CREDENTIALS_FULL_URI', 'AWS_CONTAINER_CREDENTIALS_RELATIVE_URI'

# Returns the full path of the program a conformance variable names, or $null when neither the
# variable nor its default names one. A set variable always wins.
function Get-ConformanceProgram {
  param([Parameter(Mandatory)][string]$Variable)
  $set = [Environment]::GetEnvironmentVariable($Variable)
  if (-not [string]::IsNullOrEmpty($set)) { return $set }
  if (-not (Test-Path -LiteralPath $script:ConformanceDefaultsFile -PathType Leaf)) { return $null }
  $defaults = & $script:ConformanceDefaultsFile
  if ($defaults -isnot [hashtable] -or -not $defaults.ContainsKey($Variable)) { return $null }
  return [IO.Path]::GetFullPath([string]$defaults[$Variable])
}

# Builds the environment every child process gets, as a dictionary of name to value (a $null value
# removes the variable). $Override MAY replace or remove any entry, for a check that needs to.
function Get-ConformanceChildEnvironment {
  param([Parameter(Mandatory)]$ConformanceHome, [hashtable]$Override = @{})
  $env2 = [ordered]@{}
  # A variable of agentspace's own that the caller's shell happens to hold MUST NOT reach a child.
  foreach ($name in [Environment]::GetEnvironmentVariables().Keys) {
    if ($name -like 'AGENTSPACE_*') { $env2[$name] = $null }
  }
  # A PowerShell 7 session's module path breaks a Windows PowerShell 5.1 child, which the prototype's
  # operator ran under; it is still cleared, so no program under test inherits the suite's own.
  $env2['PSModulePath'] = $null
  foreach ($name in $script:ConformanceAwsRemoved) { $env2[$name] = $null }
  $env2['USERPROFILE'] = $ConformanceHome.UserProfile
  $env2['HOME'] = $ConformanceHome.UserProfile
  # The operator reads the localswim receipt, and so the token, under LOCALAPPDATA (docs/state.md), so
  # both of these MUST point into the home too.
  $env2['LOCALAPPDATA'] = Join-Path $ConformanceHome.UserProfile 'AppData\Local'
  $env2['APPDATA'] = Join-Path $ConformanceHome.UserProfile 'AppData\Roaming'
  $env2['AGENTSPACE_STATE_DIR'] = $ConformanceHome.StateDir
  $env2['AGENTSPACE_SSH_DIR'] = $ConformanceHome.SshDir
  # Files that do not exist, so the AWS CLI finds no profile even if something starts it.
  $env2['AWS_CONFIG_FILE'] = Join-Path $ConformanceHome.UserProfile 'no-aws\config'
  $env2['AWS_SHARED_CREDENTIALS_FILE'] = Join-Path $ConformanceHome.UserProfile 'no-aws\credentials'
  # No instance-metadata credentials either, so no credential source is left.
  $env2['AWS_EC2_METADATA_DISABLED'] = 'true'
  # A harmless way to reach the lease's host, so that a ready lease needs nothing outside the home.
  $env2['AGENTSPACE_PROXY_COMMAND'] = "$env:ComSpec /d /c exit 1"
  # An override MUST NOT lead a child out of the home: it cannot replace the variables that name the
  # profile, and a state or ssh folder it names MUST be under the home's root. A refusal throws, and
  # the check that asked reports FAIL.
  $profileNames = 'USERPROFILE', 'HOME', 'LOCALAPPDATA', 'APPDATA'
  $root = [IO.Path]::GetFullPath([string]$ConformanceHome.Root).TrimEnd('\') + '\'
  foreach ($key in $Override.Keys) {
    if ($profileNames -contains $key) { throw "The override of $key is refused: it would lead a child out of the throwaway home." }
    if (($key -in 'AGENTSPACE_STATE_DIR', 'AGENTSPACE_SSH_DIR') -and -not [string]::IsNullOrEmpty([string]$Override[$key])) {
      $full = [IO.Path]::GetFullPath([string]$Override[$key])
      if (-not ($full + '\').StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw "The override of $key is refused: the path is not under the throwaway home." }
    }
    $env2[$key] = $Override[$key]
  }
  return $env2
}

# The provisioning-run tripwire's place: a folder in the home's root, outside the profile, so that no
# file map of .agentspace or .ssh sees it. Both paths are derived from the root, so a home object that
# was copied into another runspace needs no extra member.
function Get-ConformanceTripwirePath {
  param([Parameter(Mandatory)]$ConformanceHome)
  Join-Path ([string]$ConformanceHome.Root) 'tripwire\provision-tripwire.cmd'
}

# Makes the tripwire if it is not there: a .cmd that appends one line to tripwire-started.txt beside
# itself and exits 1 at once. It prints nothing, reads nothing and records no environment. Called by
# New-ConformanceHome while it is the only code running, so that a check's parallel runspaces only ever
# find it. The write is checked at the call.
function Initialize-ConformanceTripwire {
  param([Parameter(Mandatory)]$ConformanceHome)
  $path = Get-ConformanceTripwirePath -ConformanceHome $ConformanceHome
  if (Test-Path -LiteralPath $path -PathType Leaf) { return $path }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $ConformanceHome -Path (Split-Path -Parent $path)) | Out-Null
  $text = "@echo off`r`n" +
    "rem A conformance check reached a provisioning run without naming its own stand-in.`r`n" +
    "echo started>>`"%~dp0tripwire-started.txt`"`r`n" +
    "exit /b 1`r`n"
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $ConformanceHome -Path $path), [Text.Encoding]::ASCII.GetBytes($text))
  return $path
}

# How many times the tripwire has been started in this home. A check that means to show that a run was
# not started MAY use it, and Remove-ConformanceHome reports any start as a FAIL.
function Get-ConformanceTripwireStarts {
  param([Parameter(Mandatory)]$ConformanceHome)
  $marker = Join-Path (Split-Path -Parent (Get-ConformanceTripwirePath -ConformanceHome $ConformanceHome)) 'tripwire-started.txt'
  if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { return 0 }
  return @([IO.File]::ReadAllLines($marker) | Where-Object { $_ -ne '' }).Count
}

# Starts a program as a separate process, captures its output as bytes, and returns the result.
# stdin is always redirected: it carries $InputText when given, and is closed at once otherwise, so
# a program that reads it never waits for a terminal.
function Start-ConformanceProcess {
  param(
    [Parameter(Mandatory)][string]$FilePath,
    [string[]]$Arguments = @(),
    [Parameter(Mandatory)][System.Collections.IDictionary]$ChildEnvironment,
    [string]$InputText,
    [int]$TimeoutSeconds = 180,
    [string]$WorkingDirectory
  )
  $utf8 = [Text.UTF8Encoding]::new($false)
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $FilePath
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  if ($WorkingDirectory) { $psi.WorkingDirectory = $WorkingDirectory }
  foreach ($name in $ChildEnvironment.Keys) {
    if ($null -eq $ChildEnvironment[$name]) { [void]$psi.Environment.Remove($name) } else { $psi.Environment[$name] = [string]$ChildEnvironment[$name] }
  }
  try {
    $p = [Diagnostics.Process]::Start($psi)
  } catch {
    return [pscustomobject]@{
      ExitCode = -1; Stdout = ''; Stderr = "Could not start the program: $($_.Exception.Message)"
      StdoutBytes = [byte[]]@(); StderrBytes = [byte[]]@(); TimedOut = $false
    }
  }
  $out = [IO.MemoryStream]::new()
  $err = [IO.MemoryStream]::new()
  $t1 = $p.StandardOutput.BaseStream.CopyToAsync($out)
  $t2 = $p.StandardError.BaseStream.CopyToAsync($err)
  try {
    if ($PSBoundParameters.ContainsKey('InputText') -and $InputText) {
      $bytes = $utf8.GetBytes($InputText)
      $p.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
    }
    $p.StandardInput.Close()
  } catch { }
  $timedOut = $false
  if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
    # Only the process this call started, and its children.
    $timedOut = $true
    try { $p.Kill($true) } catch { }
    [void]$p.WaitForExit(10000)
  }
  [void]$t1.Wait(10000)
  [void]$t2.Wait(10000)
  $outBytes = $out.ToArray()
  $errBytes = $err.ToArray()
  [pscustomobject]@{
    ExitCode = if ($timedOut) { -1 } else { $p.ExitCode }
    Stdout = $utf8.GetString($outBytes)
    Stderr = $utf8.GetString($errBytes)
    StdoutBytes = $outBytes
    StderrBytes = $errBytes
    TimedOut = $timedOut
  }
}

# Parses a JSON document into hashtables and arrays, keeping every time as a string, or returns $null.
function ConvertFrom-ConformanceJson {
  param([string]$Text)
  if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
  try { return ($Text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 50 -ErrorAction Stop) } catch { return $null }
}

# Runs the program a conformance variable names, as a separate process inside a throwaway home.
# Returns ExitCode, Stdout, Stderr, Json (stdout parsed, or $null), ErrorJson (stderr parsed, or
# $null), StdoutBytes, StderrBytes and TimedOut.
function Invoke-ConformanceProgram {
  param(
    [Parameter(Mandatory)][string]$Variable,
    [Parameter(Mandatory)]$ConformanceHome,
    [string[]]$Arguments = @(),
    [string]$InputJson,
    [hashtable]$Environment = @{},
    [int]$TimeoutSeconds = 180
  )
  $program = Get-ConformanceProgram -Variable $Variable
  if (-not $program) {
    return [pscustomobject]@{
      ExitCode = -1; Stdout = ''; Stderr = "Neither $Variable nor its default names a program."
      StdoutBytes = [byte[]]@(); StderrBytes = [byte[]]@(); TimedOut = $false; Json = $null; ErrorJson = $null
    }
  }
  # The lease CLI, and only it, gets the provisioning-run tripwire unless the check names another
  # value (a stand-in of its own, an empty value, or $null to remove the variable).
  $runEnvironment = @{}
  if ($Variable -ceq 'AGENTSPACE_CMD') { $runEnvironment['AGENTSPACE_PROVISION_CMD'] = Initialize-ConformanceTripwire -ConformanceHome $ConformanceHome }
  foreach ($key in $Environment.Keys) { $runEnvironment[$key] = $Environment[$key] }
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $ConformanceHome -Override $runEnvironment
  $splat = @{ FilePath = $program; Arguments = $Arguments; ChildEnvironment = $childEnv; TimeoutSeconds = $TimeoutSeconds }
  if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputText'] = $InputJson }
  $r = Start-ConformanceProcess @splat
  $r | Add-Member -NotePropertyName Json -NotePropertyValue (ConvertFrom-ConformanceJson $r.Stdout)
  $r | Add-Member -NotePropertyName ErrorJson -NotePropertyValue (ConvertFrom-ConformanceJson $r.Stderr)
  return $r
}

# Checks a home's root before the home exists, and again when it is removed: returns the root's full
# path, or throws unless it is a folder named agentspace-conformance-<something> directly under
# $env:TEMP and, when it already exists, not a reparse point (a junction or a symbolic link), because
# a create or a delete through one could reach outside the temp folder. This is the one check that
# does not go through Assert-ConformancePath, because Assert-ConformancePath measures a path against
# a home's root, and this check is what makes that root safe to measure against. New-ConformanceHome
# creates the root, and Remove-ConformanceHome deletes it, only through this check at the call.
function Assert-ConformanceHomeRoot {
  param([Parameter(Mandatory)][AllowEmptyString()][string]$Root)
  if ([string]::IsNullOrWhiteSpace($Root)) { throw 'Refusing an empty path: it is not a conformance home under the temp folder.' }
  $full = [IO.Path]::GetFullPath($Root).TrimEnd('\')
  $temp = [IO.Path]::GetFullPath([string]$env:TEMP).TrimEnd('\')
  if ((Split-Path -Parent $full) -ne $temp -or (Split-Path -Leaf $full) -notlike 'agentspace-conformance-*') {
    throw "Refusing a folder that is not a conformance home under the temp folder: $full"
  }
  $item = Get-Item -LiteralPath $full -Force -ErrorAction SilentlyContinue
  if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Refusing a conformance home that is a junction or symbolic link: $full"
  }
  return $full
}

# Makes a throwaway home: a folder under $env:TEMP that holds a fresh USERPROFILE (with .agentspace
# and .ssh) and a throwaway CA made by the operator's ca under it. Returns Root, UserProfile,
# StateDir, SshDir, CaReady and CaReason. When no CA can be made, CaReady is false and CaReason says
# why; the home is still usable by a check that needs no CA, and the real CA is never used instead.
# Every create and the one delete here is checked at the call: the root's by Assert-ConformanceHomeRoot,
# and every folder under it by Assert-ConformancePath, once the root exists.
function New-ConformanceHome {
  $root = Join-Path $env:TEMP ('agentspace-conformance-' + [guid]::NewGuid().ToString('N').Substring(0, 12))
  $userProfile = Join-Path $root 'home'
  $h = [pscustomobject]@{
    Root = $root
    UserProfile = $userProfile
    StateDir = Join-Path $userProfile '.agentspace\state'
    SshDir = Join-Path $userProfile '.ssh'
    CaReady = $false
    CaReason = ''
  }
  try {
    New-Item -ItemType Directory -Path (Assert-ConformanceHomeRoot -Root $root) | Out-Null
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $userProfile '.agentspace')) | Out-Null
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $h.SshDir) | Out-Null
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $userProfile 'AppData\Local')) | Out-Null
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $userProfile 'AppData\Roaming')) | Out-Null
    # The provisioning-run tripwire, made here while nothing else runs (see the header).
    [void](Initialize-ConformanceTripwire -ConformanceHome $h)
    $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('ca', '-Quiet') -TimeoutSeconds 120
  } catch {
    # The caller never gets the object, so it could not remove the folder: this MUST. The check that
    # guards the create guards this delete too, so a root it refused is never deleted.
    try {
      if (Test-Path -LiteralPath (Assert-ConformanceHomeRoot -Root $root)) { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $root) -Recurse -Force -ErrorAction SilentlyContinue }
    } catch { }
    throw
  }
  $caPrivate = Join-Path $userProfile '.agentspace\ca\agentspace_ca'
  $caPublic = Join-Path $userProfile '.agentspace\ca\agentspace_ca.pub'
  if ($r.ExitCode -ne 0) {
    # The operator's own words can name the user and the computer, so only the exit code is kept.
    $h.CaReason = "The operator's ca exited with code $($r.ExitCode)$(if ($r.TimedOut) { ' (timed out)' })"
  } elseif (-not ((Test-Path -LiteralPath $caPrivate -PathType Leaf) -and (Test-Path -LiteralPath $caPublic -PathType Leaf))) {
    $h.CaReason = "The operator's ca exited 0 but made no CA under the home"
  } else {
    $h.CaReady = $true
  }
  return $h
}

# Deletes the home's root, and only it. Refuses a path that is not a folder this harness makes
# directly under $env:TEMP, or that is a junction or a symbolic link (Assert-ConformanceHomeRoot),
# and checks it again at each delete.
function Remove-ConformanceHome {
  param([Parameter(Mandatory)]$ConformanceHome)
  $root = Assert-ConformanceHomeRoot -Root ([string]$ConformanceHome.Root)
  # A start of the provisioning-run tripwire means a check reached a run it had named no stand-in for.
  # It is counted and reported by number alone: the tripwire records no argument or variable.
  $starts = Get-ConformanceTripwireStarts -ConformanceHome $ConformanceHome
  if ($starts -gt 0) {
    Write-ConformanceResult -Name 'Remove-ConformanceHome: No provisioning run was started without a stand-in' -Status FAIL `
      -Reason "A program started the provisioning-run tripwire $starts time(s): a check reached a run without naming its own AGENTSPACE_PROVISION_CMD stand-in"
  }
  for ($try = 0; $try -lt 5 -and (Test-Path -LiteralPath $root); $try++) {
    try { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root ([string]$ConformanceHome.Root)) -Recurse -Force -ErrorAction Stop } catch { Start-Sleep -Milliseconds 300 }
  }
  if (Test-Path -LiteralPath $root) { Write-Host "FAIL Remove-ConformanceHome: The throwaway home could not be removed: $root"; $global:ConformanceCounts['FAIL']++ }
}

# Resolves a path and returns it in full, or throws unless it is the home's root or under it. It also
# throws when the root itself, or any folder or file that exists between the root and the path, is a
# reparse point (a junction or a symbolic link), because a write or a delete through one could reach
# outside the home. The program under test is untrusted (docs/conformance.md), so every delete and
# every write a check makes MUST go through this first, and a path built from the program's output (a
# lease ID, a host alias) MUST be checked here after its document has passed its schema.
function Assert-ConformancePath {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][AllowEmptyString()][string]$Path)
  if ([string]::IsNullOrWhiteSpace($Path)) { throw 'Refusing an empty path: it is not under the throwaway home.' }
  $root = [IO.Path]::GetFullPath([string]$ConformanceHome.Root).TrimEnd('\')
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  if (-not ($full -eq $root -or $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase))) {
    throw "Refusing a path that is not under the throwaway home: $full"
  }
  $reparse = [IO.FileAttributes]::ReparsePoint
  $step = $root
  $rest = if ($full.Length -gt $root.Length) { $full.Substring($root.Length + 1) } else { '' }
  $parts = @('') + @($rest -split '\\' | Where-Object { $_ -ne '' })
  foreach ($part in $parts) {
    if ($part -ne '') { $step = $step + '\' + $part }
    $item = Get-Item -LiteralPath $step -Force -ErrorAction SilentlyContinue
    if ($null -eq $item) { break }
    if (($item.Attributes -band $reparse) -ne 0) { throw "Refusing a path through a junction or symbolic link: $step" }
  }
  return $full
}

# Seeds the home's host registry (docs/state.md, "The host registry") so that a lease CLI case that
# places a lease for a project gets `ready` as before D54. Under D54 a home with no projects.json
# answers `unsatisfiable` with `no_capacity` for a supported create of any project but swimcoach, whose
# host is the box (a missing hosts.json reads as the box's record alone), so a case that expects
# `ready` for project P needs a host record for P in state `ready`. Place reads a ready record's
# `project`, `stack` and `state` alone and makes no AWS call, so the record carries no instance_id.
# It never touches projects.json: a seeded record is not a project list entry.
#
# - Each name is normalized as the contract normalizes a project (trimmed, lower case) and MUST match
#   the contract's pattern, or this throws, since it is the check's mistake and not the program's.
# - The box, `swimcoach`, is skipped: its record is the seed, and a case about swimcoach's v0.06 bytes
#   (C3) needs hosts.json absent, which this keeps. A project that already has a record keeps it as
#   it is, whatever its state, so a case that seeded a `creating` record of its own is never undone.
# - When nothing is added the file is not written. When it is, the whole document is written as
#   state.md says (compact JSON, UTF-8 with no byte order mark, one LF), the box's record first, and
#   checked against schemas/state/hosts.state.schema.json before it is written. An existing file that
#   is not valid is never repaired: this throws.
# - The write is checked at the call (Assert-ConformancePath). The path is %USERPROFILE%\.agentspace\
#   hosts.json of the HOME, which AGENTSPACE_STATE_DIR does not move.
# Returns the names it added, as an array (empty when none).
function Add-ConformanceReadyHost {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][string[]]$Project)
  $dir = Join-Path $ConformanceHome.UserProfile '.agentspace'
  $path = Join-Path $dir 'hosts.json'
  $records = [System.Collections.Generic.List[object]]::new()
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    $text = [IO.File]::ReadAllText($path, [Text.UTF8Encoding]::new($false))
    $doc = ConvertFrom-ConformanceJson $text
    if ($null -eq $doc -or -not (Test-ConformanceSchema -Json $text -Schema 'state/hosts.state.schema.json')) { throw 'Refusing to seed a hosts.json that is not valid.' }
    # Each record is kept as it was read, its members written in the order of state.md's table.
    foreach ($r in $doc['hosts']) {
      $rec = [ordered]@{}
      foreach ($m in 'project', 'stack', 'state', 'instance_id', 'last_attempt_at') { if ($r.ContainsKey($m)) { $rec[$m] = $r[$m] } }
      $records.Add($rec)
    }
  } else {
    $records.Add([ordered]@{ project = 'swimcoach'; stack = 'agentbox'; state = 'ready' })
  }
  $added = [System.Collections.Generic.List[string]]::new()
  foreach ($name in $Project) {
    $p = $name.Trim().ToLowerInvariant()
    if ($p -cnotmatch '\A[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?\z') { throw "Refusing to seed a host for a project that is not a normalized project name: '$p'." }
    if (@($records | Where-Object { $_['project'] -ceq $p }).Count -gt 0) { continue }
    $records.Add([ordered]@{ project = $p; stack = "agentspace-host-$p"; state = 'ready' })
    $added.Add($p)
  }
  if ($added.Count -eq 0) { return , @() }
  $json = ConvertTo-Json -InputObject ([ordered]@{ version = 0; hosts = @($records) }) -Compress -Depth 10
  if (-not (Test-ConformanceSchema -Json $json -Schema 'state/hosts.state.schema.json')) { throw 'The seeded hosts.json does not validate against its schema.' }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $ConformanceHome -Path $dir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $ConformanceHome -Path $path), [Text.UTF8Encoding]::new($false).GetBytes($json + "`n"))
  return , @($added)
}

# Removes the home's host registry, so that it reads as the box's record alone again (docs/state.md,
# "The host registry": a missing hosts.json). For a check that needs a ready host only while it makes
# its ready leases, and then needs the registry as an unseeded home has it: a `get` of a `pending` lease
# whose project has a ready host starts a provisioning run (D48), which such a check is not about. It
# deletes hosts.json and nothing else, checked at the call, and never hosts.lock.
function Clear-ConformanceHosts {
  param([Parameter(Mandatory)]$ConformanceHome)
  $path = Join-Path $ConformanceHome.UserProfile '.agentspace\hosts.json'
  if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $ConformanceHome -Path $path) -Force }
}

# Runs the lease CLI, the program AGENTSPACE_CMD names, inside a throwaway home. $Arguments reach it
# unchanged. Returns ExitCode, Stdout, Stderr, Json and ErrorJson (see Invoke-ConformanceProgram).
function Invoke-AgentspaceCli {
  param(
    [Parameter(Mandatory)]$ConformanceHome,
    [string[]]$Arguments = @(),
    [string]$InputJson,
    [hashtable]$Environment = @{}
  )
  $splat = @{ Variable = 'AGENTSPACE_CMD'; ConformanceHome = $ConformanceHome; Arguments = $Arguments; Environment = $Environment }
  if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputJson'] = $InputJson }
  Invoke-ConformanceProgram @splat
}

# True when the JSON document validates against the schema, a file name under schemas/ (a folder
# is allowed: state/leases.state.schema.json). False when it does not, or is not JSON at all.
function Test-ConformanceSchema {
  param([Parameter(Mandatory)][AllowEmptyString()][string]$Json, [Parameter(Mandatory)][string]$Schema)
  $file = Join-Path $script:ConformanceSchemas $Schema
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "The schema $Schema is not under schemas/." }
  if ([string]::IsNullOrWhiteSpace($Json)) { return $false }
  try { return [bool](Test-Json -Json $Json -SchemaFile $file -ErrorAction Stop) } catch { return $false }
}

# Prints one result line: PASS <name>, FAIL <name>: <reason> or SKIPPED <name>: <reason>, and counts it.
# A run with any SKIPPED is incomplete, never passed (Exit-ConformanceRun).
function Write-ConformanceResult {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][ValidateSet('PASS', 'FAIL', 'SKIPPED')][string]$Status,
    [string]$Reason = ''
  )
  $oneLine = { param($t) ($t -replace "`r", '\r' -replace "`n", '\n') }
  $global:ConformanceCounts[$Status]++
  if ($Status -ne 'PASS' -and -not $Reason) { $Reason = 'No reason given' }
  if ($Status -eq 'PASS') { Write-Host "PASS $(& $oneLine $Name)" }
  else { Write-Host "${Status} $(& $oneLine $Name): $(& $oneLine $Reason)" }
}

# Prints the summary line and exits: 0 when every result was PASS, 1 when any was FAIL, 2 when none
# failed but any was SKIPPED. A run that counted no result at all exits 1, with a FAIL line saying
# that no check ran, so that a script that never reached its checks cannot pass.
function Exit-ConformanceRun {
  $c = $global:ConformanceCounts
  if ($c['PASS'] + $c['FAIL'] + $c['SKIPPED'] -eq 0) { Write-ConformanceResult -Name 'The run counted a result' -Status FAIL -Reason 'No check ran' }
  Write-Host "$($c['PASS']) passed, $($c['FAIL']) failed, $($c['SKIPPED']) skipped"
  if ($c['FAIL'] -gt 0) { exit 1 }
  if ($c['SKIPPED'] -gt 0) { exit 2 }
  exit 0
}
