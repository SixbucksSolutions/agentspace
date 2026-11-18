#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The harness for the lease CLI's conformance checks (D34; docs/conformance.md). Every check in this
# folder dot-sources it, and it is the only code that starts the program under test.
#
# - The program under test is named by an environment variable (AGENTSPACE_CMD for the lease CLI,
#   AGENTSPACE_OPERATOR_CMD for the operator). A variable that is unset or empty takes its default
#   from conformance/prototype/defaults.ps1, and nowhere else: the harness and the checks MUST NOT
#   name the prototype.
# - Every program the harness starts runs as a separate process, with USERPROFILE, HOME,
#   AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR pointed into a throwaway home under $env:TEMP, and
#   with the AWS variables arranged so that a mistaken run cannot reach AWS or the real token. A
#   check MUST NOT read or write the real %USERPROFILE%\.agentspace, the real ~/.ssh or the real CA.
# - Nothing here loads prototype code.
#
# Interface, pinned for every story that builds on it (docs/conformance.md, "Running the suite"):
#   New-ConformanceHome, Invoke-AgentspaceCli, Test-ConformanceSchema, Write-ConformanceResult,
#   Remove-ConformanceHome and Exit-ConformanceRun, plus Invoke-ConformanceProgram, which runs any
#   program a conformance variable names.

$script:ConformanceRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$script:ConformanceSchemas = Join-Path $script:ConformanceRepo 'schemas'
$script:ConformanceDefaultsFile = Join-Path $script:ConformanceRepo 'conformance\prototype\defaults.ps1'
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
  $path = Join-Path (Split-Path -Parent $script:ConformanceDefaultsFile) $defaults[$Variable]
  return [IO.Path]::GetFullPath($path)
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
  # A PowerShell 7 session's module path breaks Windows PowerShell 5.1, which the operator runs under.
  $env2['PSModulePath'] = $null
  foreach ($name in $script:ConformanceAwsRemoved) { $env2[$name] = $null }
  $env2['USERPROFILE'] = $ConformanceHome.UserProfile
  $env2['HOME'] = $ConformanceHome.UserProfile
  $env2['AGENTSPACE_STATE_DIR'] = $ConformanceHome.StateDir
  $env2['AGENTSPACE_SSH_DIR'] = $ConformanceHome.SshDir
  # Files that do not exist, so the AWS CLI finds no profile even if something starts it.
  $env2['AWS_CONFIG_FILE'] = Join-Path $ConformanceHome.UserProfile 'no-aws\config'
  $env2['AWS_SHARED_CREDENTIALS_FILE'] = Join-Path $ConformanceHome.UserProfile 'no-aws\credentials'
  # No instance-metadata credentials either, so no credential source is left.
  $env2['AWS_EC2_METADATA_DISABLED'] = 'true'
  # A harmless way to reach the lease's host, so that a ready lease needs nothing outside the home.
  $env2['AGENTSPACE_PROXY_COMMAND'] = 'C:\Windows\System32\cmd.exe /d /c exit 1'
  foreach ($key in $Override.Keys) { $env2[$key] = $Override[$key] }
  return $env2
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
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $ConformanceHome -Override $Environment
  $splat = @{ FilePath = $program; Arguments = $Arguments; ChildEnvironment = $childEnv; TimeoutSeconds = $TimeoutSeconds }
  if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputText'] = $InputJson }
  $r = Start-ConformanceProcess @splat
  $r | Add-Member -NotePropertyName Json -NotePropertyValue (ConvertFrom-ConformanceJson $r.Stdout)
  $r | Add-Member -NotePropertyName ErrorJson -NotePropertyValue (ConvertFrom-ConformanceJson $r.Stderr)
  return $r
}

# Makes a throwaway home: a folder under $env:TEMP that holds a fresh USERPROFILE (with .agentspace
# and .ssh) and a throwaway CA made by the operator's ca under it. Returns Root, UserProfile,
# StateDir, SshDir, CaReady and CaReason. When no CA can be made, CaReady is false and CaReason says
# why; the home is still usable by a check that needs no CA, and the real CA is never used instead.
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
    New-Item -ItemType Directory -Force -Path (Join-Path $userProfile '.agentspace') | Out-Null
    New-Item -ItemType Directory -Force -Path $h.SshDir | Out-Null
    $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('ca', '-Quiet') -TimeoutSeconds 120
  } catch {
    # The caller never gets the object, so it could not remove the folder: this MUST.
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
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
# directly under $env:TEMP.
function Remove-ConformanceHome {
  param([Parameter(Mandatory)]$ConformanceHome)
  $root = [IO.Path]::GetFullPath([string]$ConformanceHome.Root).TrimEnd('\')
  $temp = [IO.Path]::GetFullPath([string]$env:TEMP).TrimEnd('\')
  if ((Split-Path -Parent $root) -ne $temp -or (Split-Path -Leaf $root) -notlike 'agentspace-conformance-*') {
    throw "Refusing to delete a folder that is not a conformance home under the temp folder: $root"
  }
  for ($try = 0; $try -lt 5 -and (Test-Path -LiteralPath $root); $try++) {
    try { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop } catch { Start-Sleep -Milliseconds 300 }
  }
  if (Test-Path -LiteralPath $root) { Write-Host "FAIL Remove-ConformanceHome: The throwaway home could not be removed: $root"; $global:ConformanceCounts['FAIL']++ }
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
# failed but any was SKIPPED.
function Exit-ConformanceRun {
  $c = $global:ConformanceCounts
  Write-Host "$($c['PASS']) passed, $($c['FAIL']) failed, $($c['SKIPPED']) skipped"
  if ($c['FAIL'] -gt 0) { exit 1 }
  if ($c['SKIPPED'] -gt 0) { exit 2 }
  exit 0
}
