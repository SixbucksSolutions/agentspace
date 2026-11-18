#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Helpers shared by the lease CLI conformance checks of renew, release, the credential, the alias,
# the driver as a caller sees it, and concurrency (E0.04-T03-S03). A check dot-sources harness.ps1
# first and this file second.
#
# - It uses the harness's pinned functions only, and the lease CLI only through
#   Invoke-AgentspaceCli. It names no prototype path and loads no prototype code (D34).
# - It reads the files the specs document, in the check's throwaway home only: the lease state file
#   (docs/state.md, "The lease state file"), a lease's folder, the alias file and the config
#   (state.md, "A lease's credential"), and the throwaway CA's public key (state.md, "The CA").
# - ssh-keygen and ssh are programs outside agentspace (D34), and are the oracle for a certificate
#   and for an alias file.
# - Nothing here may read, write or list the real %USERPROFILE%\.agentspace, the real ~/.ssh or the
#   real CA. Every path it builds starts at the throwaway home the caller passes in.
# - The program under test is untrusted (docs/conformance.md, "The program under test is untrusted").
#   A check MUST NOT build a path from its output until the document has passed Test-CcSuccess (the
#   schema, with its UUID and alias patterns), and every path built from it goes through
#   Get-CcLeaseDir or Get-CcAliasPath, which check the pattern and call Assert-ConformancePath. Every
#   delete and every write a check makes calls Assert-ConformancePath first, at the call.

$script:CcRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# The ProxyCommand every ready create in a check is given through AGENTSPACE_PROXY_COMMAND
# (docs/lease-api.md, "Environment"). Any text without a line break is a valid override; ssh never
# runs it, because no check connects anywhere. The harness removes every AGENTSPACE_* variable the
# caller holds, so the setting travels with the run, through -Environment, and never through this
# process's environment.
$script:CcProxy = 'agentspace-conformance-proxy --not-run'

# --- Results -----------------------------------------------------------------------------------

function Add-CcResult {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][bool]$Ok, [string]$Reason = 'The condition did not hold.')
  if ($Ok) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason $Reason }
}

# Runs a check body. The body returns $true to pass, or a string that says why it failed; a throw
# is a failure too. It MUST end by returning one of the two.
function Test-Cc {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body)
  try {
    $r = @(& $Body)
    $last = if ($r.Count -gt 0) { $r[$r.Count - 1] } else { $null }
    if ($last -is [bool] -and $last) { Write-ConformanceResult -Name $Name -Status PASS }
    elseif ($last -is [string] -and $last) { Write-ConformanceResult -Name $Name -Status FAIL -Reason $last }
    else { Write-ConformanceResult -Name $Name -Status FAIL -Reason 'The condition did not hold.' }
  } catch {
    $m = ($_.Exception.Message -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "It threw: $m"
  }
}

# A short account of a run, for a failure reason. A stdout or stderr can hold no key.
function Format-CcRun($Run) {
  if ($null -eq $Run) { return 'There was no run.' }
  $o = ([string]$Run.Stdout).Trim(); if ($o.Length -gt 160) { $o = $o.Substring(0, 160) + '...' }
  $e = ([string]$Run.Stderr).Trim(); if ($e.Length -gt 160) { $e = $e.Substring(0, 160) + '...' }
  "exit $($Run.ExitCode), stdout '$o', stderr '$e'"
}

# --- Running the lease CLI ----------------------------------------------------------------------

function ConvertFrom-CcJson {
  param([string]$Text)
  try { $Text | ConvertFrom-Json -AsHashtable -DateKind String -ErrorAction Stop } catch { $null }
}

# Runs `agentspace lease <Arguments>` through the harness. Every run gets AGENTSPACE_PROXY_COMMAND set
# to the check's own ProxyCommand, unless EnvVars names that variable: a value there replaces it, and
# a $null value removes it, so the program forms its default transport (lease-api.md, "Environment").
# Any other EnvVars entry is passed to the harness's -Environment as it is.
function Invoke-CcLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments, [hashtable]$EnvVars = @{})
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy }
  foreach ($k in @($EnvVars.Keys)) { $e[$k] = $EnvVars[$k] }
  Invoke-AgentspaceCli -ConformanceHome $Lab -Arguments (@('lease') + $Arguments) -Environment $e
}

# A ready lease made by `create`. Returns Run, Doc (the parsed stdout, only when Ok) and Ok, which is
# true only when the run passed Test-CcSuccess and printed a ready lease.
function New-CcReadyLease {
  param([Parameter(Mandatory)]$Lab, [string]$Project = 'conf', [string]$Ttl, [hashtable]$EnvVars = @{})
  $a = @('create', '--project', $Project)
  if ($Ttl) { $a += @('--ttl', $Ttl) }
  $run = Invoke-CcLease -Lab $Lab -Arguments $a -EnvVars $EnvVars
  $doc = ConvertFrom-CcJson $run.Stdout
  # Ok needs the whole success test, the schema and its UUID and alias patterns included, so that a
  # check never builds a path from an ID or an alias the program made up. Doc is $null otherwise.
  $ok = [bool]((Test-CcSuccess $run) -and $doc -and $doc['state'] -ceq 'ready')
  [pscustomobject]@{ Run = $run; Doc = $(if ($ok) { $doc } else { $null }); Ok = $ok }
}

# True when a run succeeded as the contract says: exit 0, one document and one line feed on
# stdout, nothing on stderr, and a document that validates against the schema (lease-api.md,
# "Output").
function Test-CcSuccess {
  param($Run, [string]$Schema = 'lease.output.schema.json')
  if ($null -eq $Run -or $Run.ExitCode -ne 0) { return $false }
  if ($Run.Stderr -ne '') { return $false }
  if (-not ($Run.Stdout.EndsWith("`n") -and -not $Run.Stdout.EndsWith("`n`n"))) { return $false }
  Test-ConformanceSchema -Json $Run.Stdout.TrimEnd("`n") -Schema $Schema
}

# True when a run failed as the contract says: the exit code, no stdout, one error document on
# stderr that validates, and the given code (and, when given, member values).
function Test-CcFailure {
  param($Run, [int]$Exit, [string]$Code, [hashtable]$Members = @{})
  if ($null -eq $Run -or $Run.ExitCode -ne $Exit -or $Run.Stdout -ne '') { return $false }
  $e = [string]$Run.Stderr
  if (-not ($e.EndsWith("`n") -and -not $e.EndsWith("`n`n"))) { return $false }
  if (-not (Test-ConformanceSchema -Json $e.TrimEnd("`n") -Schema 'error.output.schema.json')) { return $false }
  $doc = ConvertFrom-CcJson $e
  if (-not $doc -or $doc['code'] -cne $Code) { return $false }
  foreach ($k in $Members.Keys) { if (-not $doc.ContainsKey($k) -or $doc[$k] -cne $Members[$k]) { return $false } }
  $true
}

# --- The lease state file (docs/state.md, "The lease state file") -----------------------------------

function Get-CcStatePath($Lab) { Join-Path $Lab.StateDir 'leases.json' }

function Read-CcState($Lab) {
  $p = Get-CcStatePath $Lab
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $null }
  ConvertFrom-CcJson ([IO.File]::ReadAllText($p, [Text.UTF8Encoding]::new($false)))
}

# Writes a state document as state.md says: compact JSON, UTF-8 with no byte order mark, one LF.
function Write-CcState($Lab, $Doc) {
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.StateDir) | Out-Null
  $text = (ConvertTo-Json -InputObject $Doc -Compress -Depth 20) + "`n"
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path (Get-CcStatePath $Lab)), [Text.UTF8Encoding]::new($false).GetBytes($text))
}

function Get-CcRecord($Lab, [string]$Id) {
  $s = Read-CcState $Lab
  if (-not $s) { return $null }
  foreach ($r in $s['leases']) { if ($r['id'] -ceq $Id) { return $r } }
  $null
}

# Changes one record in the state file, between commands, so a check can reach a state that no
# command makes on its own (an expired lease, a pending one).
function Edit-CcRecord($Lab, [string]$Id, [scriptblock]$Change) {
  $s = Read-CcState $Lab
  foreach ($r in $s['leases']) { if ($r['id'] -ceq $Id) { & $Change $r } }
  Write-CcState $Lab $s
}

function Add-CcRecord($Lab, $Record) {
  $s = Read-CcState $Lab
  if (-not $s) { $s = @{ version = 0; leases = @() } }
  $s['leases'] = @($s['leases']) + @($Record)
  Write-CcState $Lab $s
}

# A canonical text of a parsed JSON value: sorted keys, so two documents compare by content and
# never by member order (lease-api.md, "Output": a caller MUST NOT depend on order).
function ConvertTo-CcCanonical($Value) {
  if ($null -eq $Value) { return 'null' }
  if ($Value -is [System.Collections.IDictionary]) {
    $keys = @($Value.Keys | ForEach-Object { [string]$_ })
    [array]::Sort($keys, [StringComparer]::Ordinal)
    $parts = foreach ($k in $keys) { (ConvertTo-Json -InputObject $k -Compress) + ':' + (ConvertTo-CcCanonical $Value[$k]) }
    return '{' + ($parts -join ',') + '}'
  }
  if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
    $items = foreach ($i in $Value) { ConvertTo-CcCanonical $i }
    return '[' + (@($items) -join ',') + ']'
  }
  ConvertTo-Json -InputObject $Value -Compress
}

function Get-CcStateCanonical($Lab) {
  $s = Read-CcState $Lab
  if ($null -eq $s) { 'absent' } else { ConvertTo-CcCanonical $s }
}

# True when the state file validates against its schema (state.md, "The lease state file").
function Test-CcStateFile($Lab) {
  $p = Get-CcStatePath $Lab
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $false }
  try {
    [bool](Test-Json -Json ([IO.File]::ReadAllText($p, [Text.UTF8Encoding]::new($false))) `
        -SchemaFile (Join-Path $script:CcRepo 'schemas\state\leases.state.schema.json') -ErrorAction Stop)
  } catch { $false }
}

# --- A lease's files (docs/state.md, "A lease's credential") ----------------------------------------

# The two paths a lease's files are at. Each takes an ID or an alias that may have come from the
# program under test, so each refuses one that does not match the schema's pattern, and then checks
# the path with Assert-ConformancePath. Each returns the path as built, because the alias file holds
# the path the program wrote and Assert-ConformancePath returns a full, long-form path that can differ
# from it in spelling (a short 8.3 folder name in %TEMP%). An empty alias names a file that never
# exists, for a check that has no lease to look at.
function Get-CcLeaseDir($Lab, [string]$Id) {
  if ($Id -cnotmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') { throw 'Refusing a lease ID that is not a lowercase UUIDv4, so no path is built from it.' }
  $path = Join-Path $Lab.UserProfile ".agentspace\leases\$Id"
  [void](Assert-ConformancePath -ConformanceHome $Lab -Path $path)
  $path
}
function Get-CcAliasPath($Lab, [string]$Alias) {
  if ($Alias -eq '') { $Alias = 'agentspace-no-alias-00000000' }
  if ($Alias -cnotmatch '\Aagentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}\z') { throw 'Refusing a host alias that does not match the schema, so no path is built from it.' }
  $path = Join-Path $Lab.SshDir "agentspace.d\$Alias"
  [void](Assert-ConformancePath -ConformanceHome $Lab -Path $path)
  $path
}

function Get-CcFileHash([string]$Path) {
  if (Test-Path -LiteralPath $Path -PathType Leaf) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash } else { 'absent' }
}

function Get-CcFileNames([string]$Dir) {
  if (Test-Path -LiteralPath $Dir -PathType Container) { @(Get-ChildItem -LiteralPath $Dir -Force | ForEach-Object Name | Sort-Object) } else { @() }
}

# Empties the throwaway ssh folder, only ever the one inside the check's own throwaway root.
function Reset-CcSshDir($Lab) {
  $full = Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.SshDir
  if ($full -ieq (Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.Root)) { throw 'The ssh folder is the throwaway root itself.' }
  if (Test-Path -LiteralPath $full) { Get-ChildItem -LiteralPath $full -Force | ForEach-Object { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $_.FullName) -Recurse -Force } }
  else { New-Item -ItemType Directory -Force -Path $full | Out-Null }
}

# --- Times ---------------------------------------------------------------------------------------

function ConvertFrom-CcTime([string]$Text) {
  [DateTime]::ParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal)
}

function ConvertTo-CcTime([DateTime]$T) {
  $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
}

function Get-CcNowSecond { $n = [DateTime]::UtcNow; [DateTime]::new($n.Ticks - ($n.Ticks % [TimeSpan]::TicksPerSecond), [DateTimeKind]::Utc) }

# --- Programs outside agentspace -------------------------------------------------------------------

function Get-CcTool([string]$Name) {
  if ($env:SystemRoot) {
    $p = Join-Path $env:SystemRoot "System32\OpenSSH\$Name.exe"
    if (Test-Path -LiteralPath $p) { return $p }
  }
  (Get-Command $Name -ErrorAction Stop).Source
}

# Reads a certificate with `ssh-keygen -L`, which prints its window in the machine's local time
# with no zone (state.md, "The certificate"), so the window is converted to UTC here.
function Get-CcCertificate([string]$Path) {
  $text = (& (Get-CcTool 'ssh-keygen') -L -f $Path 2>&1 | Out-String)
  $c = [ordered]@{ Type = $null; Kind = $null; KeyId = $null; From = $null; To = $null; Principals = @(); CriticalNone = $false
    Critical = @(); Extensions = @(); CaFingerprint = $null; CaUsing = $null; PublicKeyFingerprint = $null; Raw = $text }
  $section = $null
  foreach ($line in ($text -split "\r?\n")) {
    if ($line -match '^\s+Type:\s+(\S+)\s+(\w+) certificate') { $c.Type = $Matches[1]; $c.Kind = $Matches[2]; $section = $null }
    elseif ($line -match '^\s+Public key:\s+\S+\s+(\S+)') { $c.PublicKeyFingerprint = $Matches[1]; $section = $null }
    elseif ($line -match '^\s+Signing CA:\s+\S+\s+(\S+)\s+\(using (\S+)\)') { $c.CaFingerprint = $Matches[1]; $c.CaUsing = $Matches[2]; $section = $null }
    elseif ($line -match '^\s+Key ID:\s+"(.*)"\s*$') { $c.KeyId = $Matches[1]; $section = $null }
    elseif ($line -match '^\s+Valid:\s+from\s+(\S+)\s+to\s+(\S+)') {
      $conv = {
        param($s)
        $t = [DateTime]::ParseExact($s, "yyyy-MM-dd'T'HH:mm:ss", [Globalization.CultureInfo]::InvariantCulture)
        [DateTime]::SpecifyKind($t, [DateTimeKind]::Local).ToUniversalTime()
      }
      $c.From = & $conv $Matches[1]; $c.To = & $conv $Matches[2]; $section = $null
    }
    elseif ($line -match '^\s+Serial:') { $section = $null }
    elseif ($line -match '^\s+Principals:\s*$') { $section = 'Principals' }
    elseif ($line -match '^\s+Critical Options:\s*(.*?)\s*$') {
      if ($Matches[1] -eq '(none)') { $c.CriticalNone = $true; $section = $null } else { $section = 'Critical' }
    }
    elseif ($line -match '^\s+Extensions:\s*(.*?)\s*$') { $section = 'Extensions' }
    elseif ($section -and $line -match '^\s+(\S.*?)\s*$') { $c[$section] = @($c[$section]) + $Matches[1] }
  }
  $c
}

# The fingerprint (the SHA256 word) of a public key file.
function Get-CcFingerprint([string]$PublicKeyPath) {
  $out = (& (Get-CcTool 'ssh-keygen') -l -f $PublicKeyPath 2>&1 | Out-String).Trim()
  if ($out -match '^\d+\s+(\S+)\s') { $Matches[1] } else { $null }
}

# The cipher name and the KDF name of an OpenSSH private key (state.md, "The key"): both `none`
# means no passphrase. Reads the throwaway key's file; nothing of it is printed.
function Get-CcKeyEncryption([string]$Path) {
  $text = [IO.File]::ReadAllText($Path)
  $m = [regex]::Match($text, '-----BEGIN OPENSSH PRIVATE KEY-----\s*(.+?)\s*-----END OPENSSH PRIVATE KEY-----', 'Singleline')
  if (-not $m.Success) { return $null }
  $b = [Convert]::FromBase64String(($m.Groups[1].Value -replace '\s', ''))
  $magic = [Text.Encoding]::ASCII.GetBytes("openssh-key-v1`0")
  for ($i = 0; $i -lt $magic.Length; $i++) { if ($b[$i] -ne $magic[$i]) { return $null } }
  $pos = $magic.Length
  $read = {
    $len = ([int]$b[$script:pos] -shl 24) -bor ([int]$b[$script:pos + 1] -shl 16) -bor ([int]$b[$script:pos + 2] -shl 8) -bor [int]$b[$script:pos + 3]
    $s = [Text.Encoding]::ASCII.GetString($b, $script:pos + 4, $len)
    $script:pos += 4 + $len
    $s
  }
  $script:pos = $pos
  $cipher = & $read
  $kdf = & $read
  @{ Cipher = $cipher; Kdf = $kdf }
}

# The problems with an alias file's default ProxyCommand line, judged only by what docs/state.md,
# "The alias file", makes checkable from outside (D8). The line is `"<program>" <alias>`, or
# `<system folder>\cmd.exe /d /c "<program>" <alias>` when, and only when, the program is a .cmd or
# .bat; <program> is the full path of a file that exists, and <system folder> is %SystemRoot%\System32.
# It also refuses ssh's %n token, since the alias MUST be written literally. An empty list means
# none. The program's path comes from the program under test and points outside the home, where
# Assert-ConformancePath cannot go, so it is only tested for existence, and only after it is shown to
# be a full path on a local drive (docs/conformance.md, "The program under test is untrusted").
# Nothing is written, deleted, run or listed there, and no reason quotes the path.
function Get-CcDefaultProxyProblems {
  param([string]$AliasText, [string]$Alias)
  $p = [System.Collections.Generic.List[string]]::new()
  $m = [regex]::Match([string]$AliasText, '(?m)^  ProxyCommand ([^\r\n]*)$')
  if (-not $m.Success) { $p.Add('There is no ProxyCommand line'); return , $p }
  $raw = $m.Groups[1].Value
  # ssh tokens: %% is a literal percent sign, and %n would be the alias by token, not literally.
  $line = [System.Text.StringBuilder]::new()
  for ($i = 0; $i -lt $raw.Length; $i++) {
    if ($raw[$i] -ne '%') { [void]$line.Append($raw[$i]); continue }
    if ($i + 1 -lt $raw.Length -and $raw[$i + 1] -eq '%') { [void]$line.Append('%'); $i++; continue }
    if ($i + 1 -lt $raw.Length -and $raw[$i + 1] -ceq 'n') { $p.Add('The line uses the %n token') }
    [void]$line.Append('%')
  }
  $line = $line.ToString()
  if (-not $env:SystemRoot) { $p.Add('SystemRoot is not set in the check, so the system folder is unknown'); return , $p }
  $prefix = (Join-Path $env:SystemRoot 'System32\cmd.exe') + ' /d /c '
  $wrapped = $line.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
  $rest = if ($wrapped) { $line.Substring($prefix.Length) } else { $line }
  $pm = [regex]::Match($rest, '\A"([^"]+)" ([^ ]+)\z')
  if (-not $pm.Success) { $p.Add('The line is not a program path in double quotes, one space and the alias, with or without the system folder''s cmd.exe /d /c before it'); return , $p }
  $program = $pm.Groups[1].Value
  if ($pm.Groups[2].Value -cne $Alias) { $p.Add('The line does not end with one space and the alias, written literally') }
  $local = $program -cmatch '\A[A-Za-z]:\\[^*?<>|:"]+\z' -and $program -notmatch '(\A|\\)\.{1,2}(\\|\z)' -and -not $program.EndsWith('\')
  if (-not $local) { $p.Add('The program is not a full path on a local drive') }
  elseif (-not [IO.File]::Exists($program)) { $p.Add('The program is not a file that exists') }
  $batch = $program -imatch '\.(cmd|bat)\z'
  if ($batch -and -not $wrapped) { $p.Add('A .cmd or .bat program is not run through the system folder''s cmd.exe /d /c') }
  if ($wrapped -and -not $batch) { $p.Add('cmd.exe /d /c runs a program that is not a .cmd or .bat') }
  return , $p
}

# The lines `ssh -G -F <config file> <alias>` prints: the oracle for what an alias file means.
function Get-CcSshG([string]$ConfigFile, [string]$Alias) {
  @(& (Get-CcTool 'ssh') -G -F $ConfigFile $Alias 2>&1 | ForEach-Object { [string]$_ })
}

# True when text names no backend, location or machine (lease-api.md, "What it never says").
function Test-CcNoBackendWords([string]$Text) {
  $Text -notmatch '(?i)\bec2\b|\baws\b|amazon|\bssm\b|region|us-west|instance|\bi-[0-9a-f]{6,}'
}
