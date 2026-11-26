#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Helpers shared by the lease CLI's conformance checks. Dot-source conformance/lease-cli/harness.ps1
# first. Nothing here starts a program, except through the harness, and nothing here loads prototype
# code. Every expectation below is taken from docs/lease-api.md or docs/state.md, never from a build.
# The program under test is untrusted (docs/conformance.md, "The program under test is untrusted"), so
# every write below calls Assert-ConformancePath first.

$script:ExitCodeOf = @{ internal = 1; usage = 2; invalid_value = 2; out_of_range = 2; not_found = 3; invalid_state = 4 }

# Reports one check: PASS when it has no problems, FAIL with the problems joined otherwise, through the
# suite's mask, since a problem can quote what a program printed (conformance\mask-common.ps1).
function Complete-ConformanceCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($Problems -join '; ')) }
}

# True when the text holds no whitespace outside its JSON strings (lease-api.md#output: compact).
function Test-ConformanceJsonCompact {
  param([string]$Text)
  $inString = $false
  $escaped = $false
  foreach ($ch in $Text.ToCharArray()) {
    if ($inString) {
      if ($escaped) { $escaped = $false }
      elseif ($ch -eq '\') { $escaped = $true }
      elseif ($ch -eq '"') { $inString = $false }
    } else {
      if ($ch -eq '"') { $inString = $true }
      elseif ($ch -eq ' ' -or $ch -eq "`t" -or $ch -eq "`r" -or $ch -eq "`n") { return $false }
    }
  }
  return $true
}

# True when the text is one line that ends in exactly one line feed, with no carriage return.
function Test-ConformanceOneLine {
  param([string]$Text)
  return ($Text.EndsWith("`n") -and $Text.IndexOf("`n") -eq $Text.Length - 1 -and -not $Text.Contains("`r"))
}

function Get-ConformanceHasBom {
  param([byte[]]$Bytes)
  return ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF)
}

# The problems with a run that was meant to succeed (lease-api.md#output): exit 0, nothing on stderr,
# exactly one compact JSON document and one line feed on stdout, no byte order mark, and a document
# that validates against $Schema.
function Get-ConformanceSuccessProblems {
  param([Parameter(Mandatory)]$Run, [string]$Schema = 'lease.output.schema.json')
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not 0") }
  if ($Run.StderrBytes.Length -ne 0) { $p.Add("Stderr holds $($Run.StderrBytes.Length) bytes") }
  if (-not (Test-ConformanceOneLine $Run.Stdout)) { $p.Add('Stdout is not one line ending in one line feed') }
  elseif (-not (Test-ConformanceJsonCompact $Run.Stdout.TrimEnd("`n"))) { $p.Add('Stdout is not compact JSON') }
  if (Get-ConformanceHasBom $Run.StdoutBytes) { $p.Add('Stdout starts with a byte order mark') }
  if ($null -eq $Run.Json) { $p.Add('Stdout is not a JSON document') }
  elseif (-not (Test-ConformanceSchema -Json $Run.Stdout.TrimEnd("`n") -Schema $Schema)) { $p.Add("Stdout does not validate against $Schema") }
  return , $p
}

# The problems with a run that was meant to fail (lease-api.md#exit-codes-and-errors): the exit code of
# its error code, nothing on stdout, one compact error document on stderr that validates against the
# error schema, and the code, field, lease_id and state expected. An empty $Field means the error
# MUST NOT carry one.
function Get-ConformanceErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string]$LeaseId = '', [string]$State = '')
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -ne $script:ExitCodeOf[$Code]) { $p.Add("Exit $($Run.ExitCode), not $($script:ExitCodeOf[$Code])") }
  if ($Run.StdoutBytes.Length -ne 0) { $p.Add("Stdout holds $($Run.StdoutBytes.Length) bytes") }
  if (-not (Test-ConformanceOneLine $Run.Stderr)) { $p.Add('Stderr is not one line ending in one line feed') }
  elseif (-not (Test-ConformanceJsonCompact $Run.Stderr.TrimEnd("`n"))) { $p.Add('The error document is not compact JSON') }
  if (Get-ConformanceHasBom $Run.StderrBytes) { $p.Add('Stderr starts with a byte order mark') }
  $doc = $Run.ErrorJson
  if ($null -eq $doc) { $p.Add("Stderr is not a JSON document: $(ConvertTo-ConformanceMasked $Run.Stderr.Trim())"); return , $p }
  if (-not (Test-ConformanceSchema -Json $Run.Stderr.TrimEnd("`n") -Schema 'error.output.schema.json')) { $p.Add('The error document does not validate against error.output.schema.json') }
  if ($doc['code'] -cne $Code) { $p.Add("Code is '$(ConvertTo-ConformanceMasked $doc['code'])', not '$Code'") }
  if ($Field) {
    if (-not $doc.ContainsKey('field')) { $p.Add("No field, wanted '$Field'") }
    elseif ($doc['field'] -cne $Field) { $p.Add("Field is '$(ConvertTo-ConformanceMasked $doc['field'])', not '$Field'") }
  } elseif ($doc.ContainsKey('field')) { $p.Add("An unwanted field '$(ConvertTo-ConformanceMasked $doc['field'])'") }
  if ($LeaseId -and $doc['lease_id'] -cne $LeaseId) { $p.Add("lease_id is '$(ConvertTo-ConformanceMasked $doc['lease_id'])', not '$LeaseId'") }
  if ($State -and $doc['state'] -cne $State) { $p.Add("state is '$(ConvertTo-ConformanceMasked $doc['state'])', not '$State'") }
  return , $p
}

# Runs the lease CLI and checks that it fails as wanted. $Name names the check; $Arguments is the line.
function Test-ConformanceErrorRun {
  param(
    [Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string]$LeaseId = '', [string]$State = '', [hashtable]$Environment = @{}
  )
  $r = Invoke-AgentspaceCli -ConformanceHome $ConformanceHome -Arguments $Arguments -Environment $Environment
  $p = Get-ConformanceErrorProblems -Run $r -Code $Code -Field $Field -LeaseId $LeaseId -State $State
  Complete-ConformanceCheck -Name $Name -Problems $p
}

# A line as a person would type it, for a check's name.
function Format-ConformanceLine {
  param([string[]]$Arguments)
  $shown = foreach ($a in $Arguments) {
    $t = $a -replace "`r", '\r' -replace "`n", '\n' -replace "`t", '\t'
    if ($t -eq '' -or $t -match '\s') { "'$t'" } else { $t }
  }
  return ('agentspace ' + ($shown -join ' ')).TrimEnd()
}

# The help blocks of docs/lease-api.md, by level ('agentspace', 'agentspace lease', ...), each as the
# text --help MUST print: the lines of the block, each ending in a line feed.
function Get-ConformanceHelpBlocks {
  $doc = Join-Path $script:ConformanceRepo 'docs\lease-api.md'
  $lines = ([IO.File]::ReadAllText($doc) -replace "`r`n", "`n") -split "`n"
  $blocks = @{}
  $level = $null
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -cmatch '^### (agentspace(?: lease(?: [a-z]+)?)?)$') { $level = $Matches[1]; continue }
    if ($lines[$i] -ceq '**Help:**' -and $level) {
      $j = $i + 1
      while ($j -lt $lines.Count -and $lines[$j] -cne '```') { $j++ }
      $sb = [Text.StringBuilder]::new()
      for ($k = $j + 1; $k -lt $lines.Count -and $lines[$k] -cne '```'; $k++) { [void]$sb.Append($lines[$k]).Append("`n") }
      $blocks[$level] = $sb.ToString()
      $level = $null
    }
  }
  return $blocks
}

# Checks that a run printed the level's help: exit 0, stdout byte for byte the doc's block, no stderr.
function Test-ConformanceHelpRun {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [Parameter(Mandatory)][string]$Level, [Parameter(Mandatory)]$Blocks)
  $r = Invoke-AgentspaceCli -ConformanceHome $ConformanceHome -Arguments $Arguments
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not $Blocks.ContainsKey($Level)) { $p.Add("docs/lease-api.md has no help block for '$Level'") }
  elseif ([string]::IsNullOrEmpty($Blocks[$Level])) { $p.Add("The help block for '$Level' in docs/lease-api.md is empty, so there is nothing to match") }
  else {
    $want = [Text.UTF8Encoding]::new($false).GetBytes($Blocks[$Level])
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    if ($r.StderrBytes.Length -ne 0) { $p.Add("Stderr holds $($r.StderrBytes.Length) bytes") }
    if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$r.StdoutBytes, [byte[]]$want)) { $p.Add("Stdout is $($r.StdoutBytes.Length) bytes and not the doc's $($want.Length)") }
  }
  Complete-ConformanceCheck -Name $Name -Problems $p
}

# --- The state file (docs/state.md, "The lease state file"), for checks that seed or read it ------

function Get-ConformanceStatePath {
  param([Parameter(Mandatory)]$ConformanceHome, [string]$StateDir)
  if (-not $StateDir) { $StateDir = $ConformanceHome.StateDir }
  return (Join-Path $StateDir 'leases.json')
}

# Reads the state file into objects, with every time kept as a string. $null when it does not exist.
function Read-ConformanceStateFile {
  param([Parameter(Mandatory)]$ConformanceHome, [string]$StateDir)
  $path = Get-ConformanceStatePath -ConformanceHome $ConformanceHome -StateDir $StateDir
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  return ([IO.File]::ReadAllText($path) | ConvertFrom-Json -DateKind String -Depth 50)
}

# Writes the document back as state.md says: compact JSON, UTF-8 without a byte order mark, one line feed.
function Write-ConformanceStateFile {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)]$Document, [string]$StateDir)
  $path = Get-ConformanceStatePath -ConformanceHome $ConformanceHome -StateDir $StateDir
  $json = $Document | ConvertTo-Json -Compress -Depth 50
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $ConformanceHome -Path $path), $json + "`n", [Text.UTF8Encoding]::new($false))
}

# Sets one member of one lease's record in the state file, to seed a time. Returns $false, and writes
# nothing, when there is no state file or no record with that ID.
function Set-ConformanceRecordMember {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$Member, [Parameter(Mandatory)][string]$Value)
  $doc = Read-ConformanceStateFile -ConformanceHome $ConformanceHome
  if ($null -eq $doc) { return $false }
  $rec = @($doc.leases | Where-Object { $_.id -ceq $Id })[0]
  if ($null -eq $rec) { return $false }
  $rec.$Member = $Value
  Write-ConformanceStateFile -ConformanceHome $ConformanceHome -Document $doc
  return $true
}

# The SHA-256 of the state file's bytes, or 'absent'.
function Get-ConformanceStateHash {
  param([Parameter(Mandatory)]$ConformanceHome)
  $path = Get-ConformanceStatePath -ConformanceHome $ConformanceHome
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return 'absent' }
  return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
}

# --- Times (lease-api.md#times) ------------------------------------------------------------------

# A contract time, YYYY-MM-DDTHH:MM:SSZ, as a UTC DateTime; $null when it is not one.
function ConvertFrom-ConformanceTime {
  param([string]$Text)
  $out = [DateTime]::MinValue
  if ([DateTime]::TryParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal, [ref]$out)) { return $out }
  return $null
}

# The suite's own clock, truncated to the whole second, in UTC.
function Get-ConformanceNow {
  $now = [DateTime]::UtcNow
  return [DateTime]::new($now.Year, $now.Month, $now.Day, $now.Hour, $now.Minute, $now.Second, [DateTimeKind]::Utc)
}
