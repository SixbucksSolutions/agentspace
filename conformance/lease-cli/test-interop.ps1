#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 and D38 (E0.05-T02-S05, #0103): one lease driven through two lease CLIs,
# the program AGENTSPACE_CMD names (A) and the program AGENTSPACE_PEER_CMD names (B), in ONE throwaway home
# and ONE state folder. It enforces docs/state.md, "Between the prototype and the rewrite": the two MUST be
# able to share one home folder and one state folder, so that a lease made by one is read, renewed and
# released by the other. It also enforces docs/lease-api.md, "agentspace lease get", "renew", "release" and
# "list", and docs/state.md, "The lease state file".
#
# - Each way round (A then B, and B then A), the first program creates a ready lease; the other reads it
#   with `get` and `list`, compared parsed; the other renews it, and the first reads the new expires_at,
#   ttl_seconds and certificate; the first renews it again; the other releases it; the first reads it
#   released, with the other's released_at, its alias file and lease folder gone; the first releases it
#   again and prints the same document; and an unsatisfiable lease made by the first reads the same in
#   the other. After EVERY step the state file validates against its schema, and every lease either
#   program printed is in it exactly once.
# - A document a program printed MUST be the lease that was asked for: its id is compared exactly, a get is
#   compared in full with the document the writer printed, a renew with the document before it on every
#   field except expires_at and ttl_seconds, and a release with the last renew on every field except
#   state, ssh and released_at (lease-api.md, "renew" and "release").
# - It drives each program only as a separate process, through the harness (Invoke-ConformanceProgram),
#   in a throwaway home with a throwaway CA. It reads no real folder, loads no prototype code and names no
#   prototype path: a variable that is unset or empty takes its default from
#   conformance/prototype/defaults.ps1, so with both unset the prototype is run against itself.
# - The program under test is untrusted (docs/conformance.md): a path is built from a lease ID or a host
#   alias only after the document passed its schema, and then only through Get-CcLeaseDir and
#   Get-CcAliasPath, which check the ID and alias patterns and call Assert-ConformancePath. This check
#   writes and deletes nothing itself.
# - It checks MUSTs only, and a failure's reason never quotes a program's output: a reason names a
#   difference and gives an exit code and whether each stream is empty, and no more.
# - Nothing connects anywhere, and nothing here needs a box or AWS.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-interop.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Set either variable to a program that prints nothing and exits 0, and the run MUST fail.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$sec = 'state.md#between-the-prototype-and-the-rewrite'
$ttlRenewed = 5400
$haveKeygen = $true
try { [void](Get-CcTool 'ssh-keygen') } catch { $haveKeygen = $false }

# Every lease ID a program printed in a document that passed its schema, as a set.
$seen = @{}

# Runs `agentspace lease <Arguments>` through the program a conformance variable names.
function Invoke-Interop {
  param([Parameter(Mandatory)][string]$Variable, [Parameter(Mandatory)][string[]]$Arguments)
  Invoke-ConformanceProgram -Variable $Variable -ConformanceHome $script:lab -Arguments (@('lease') + $Arguments) `
    -Environment @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy }
}

# What a failure reason may say of a run: the exit code and whether each stream is empty. Never the text.
function Get-InteropNote($Run) {
  if ($null -eq $Run) { return 'There was no run' }
  "exit $($Run.ExitCode), stdout $(if ([string]$Run.Stdout -eq '') { 'empty' } else { 'not empty' }), stderr $(if ([string]$Run.Stderr -eq '') { 'empty' } else { 'not empty' })"
}

# The parsed lease document of a run that succeeded as the contract says, or $null. Its ID is recorded as
# one the check must find in the state file.
function Get-InteropDoc($Run) {
  if (-not (Test-CcSuccess $Run)) { return $null }
  $doc = ConvertFrom-CcJson $Run.Stdout
  if ($doc -and $doc['id']) { $script:seen[[string]$doc['id']] = $true }
  $doc
}

function Get-InteropList($Run) {
  if (-not (Test-CcSuccess $Run 'lease-list.output.schema.json')) { return $null }
  $doc = ConvertFrom-CcJson $Run.Stdout
  if ($doc) { foreach ($l in @($doc['leases'])) { $script:seen[[string]$l['id']] = $true } }
  $doc
}

# The issue time of a certificate a document describes: expires_at minus ttl_seconds.
function Get-InteropIssue($Doc) {
  try { (ConvertFrom-CcTime $Doc['expires_at']).AddSeconds(-[double]$Doc['ttl_seconds']) } catch { [DateTime]::UtcNow }
}

# A time a program printed, parsed, or $null when it is not a valid time. The schema's pattern admits
# dates that do not exist (a 30th of February), and a parse failure's text would quote the program's
# output, so the failure is caught here and a reason says it in a fixed sentence.
function Get-InteropTime($Text) {
  try { ConvertFrom-CcTime ([string]$Text) } catch { $null }
}

# True when two lease documents are equal, parsed and canonical, on every field except the named ones.
# A field that is present in one and absent in the other counts as a difference, unless it is named.
function Test-InteropSameExcept($A, $B, [string[]]$Except) {
  $strip = {
    param($Doc)
    $c = @{}
    foreach ($k in $Doc.Keys) { if ($Except -cnotcontains [string]$k) { $c[$k] = $Doc[$k] } }
    $c
  }
  (ConvertTo-CcCanonical (& $strip $A)) -ceq (ConvertTo-CcCanonical (& $strip $B))
}

# Waits until the clock is past the given whole second, so that two certificates cannot share an issue
# time (each renew sets a new expiry from its own time), and a second release could not re-stamp
# released_at unseen. Bounded to 5 seconds, because a time a program printed is untrusted.
function Wait-InteropPast([DateTime]$T) {
  $limit = [DateTime]::UtcNow.AddSeconds(5)
  while ((Get-CcNowSecond) -le $T -and [DateTime]::UtcNow -lt $limit) { Start-Sleep -Milliseconds 100 }
}

# The state file, after a step: it validates against its schema, and each lease any program printed is in
# it exactly once.
function Test-InteropState([string]$Name) {
  Test-Cc $Name {
    if (-not (Test-CcStateFile $script:lab)) { return 'The state file does not validate against its schema.' }
    $st = Read-CcState $script:lab
    if (-not $st) { return 'The state file could not be read.' }
    foreach ($id in @($script:seen.Keys)) {
      $n = @($st['leases'] | Where-Object { $_['id'] -ceq $id }).Count
      if ($n -ne 1) { return "A lease a program printed is in the state file $n times, not once." }
    }
    $true
  }
}

# The certificate on disk, read with ssh-keygen: its valid-before is the lease's expires_at, and its Key ID
# is the lease's ID (state.md, "Renewal"). SKIPPED when no ssh-keygen is on the machine.
function Test-InteropCertificate([string]$Name, $State, [string]$ExpiresAt) {
  if (-not $script:haveKeygen) {
    Write-ConformanceResult -Name $Name -Status SKIPPED -Reason 'No ssh-keygen was found in System32\OpenSSH or on the PATH, so the certificate cannot be read.'
    return
  }
  Test-Cc $Name {
    if (-not $State.Id) { return 'Setup: no lease was made.' }
    if (-not $ExpiresAt) { return 'Setup: the step before made no renewal, so there is no expires_at to compare the certificate with.' }
    $c = Get-CcCertificate (Join-Path $State.Dir 'id_ed25519-cert.pub')
    if (-not $c.To) { return 'The certificate could not be read.' }
    if ((ConvertTo-CcTime $c.To) -cne $ExpiresAt) { return 'The certificate valid-before time differs from the expires_at the lease shows.' }
    if ($c.KeyId -cne $State.Id) { return 'The certificate Key ID is not the lease ID.' }
    $true
  }
}

function Invoke-InteropRound {
  param([int]$Round, [string]$First, [string]$Second, [string]$Project)
  $tag = "Round $Round ($First then $Second)"
  # What the steps hand to each other. A hashtable, so that a step's script block changes it in place.
  $s = @{ Id = $null; Doc = $null; Dir = $null; Alias = $null; Cert0 = $null; Cert1 = $null; Renew1 = $null; Renew2 = $null; Released = $null }

  # Step 1: the first program creates a ready lease.
  Test-Cc "$sec`: $tag, step 1: $First creates a ready lease with its folder, key and alias file" {
    $run1 = Invoke-Interop $First @('create', '--project', $Project)
    $doc = Get-InteropDoc $run1
    if (-not $doc -or $doc['state'] -cne 'ready') { return "The create was not a valid ready lease: $(Get-InteropNote $run1)." }
    $s.Doc = $doc; $s.Id = [string]$doc['id']
    $s.Dir = Get-CcLeaseDir $script:lab $s.Id
    $s.Alias = Get-CcAliasPath $script:lab ([string]$doc['ssh']['host_alias'])
    if (-not (Test-Path -LiteralPath (Join-Path $s.Dir 'id_ed25519') -PathType Leaf)) { return 'The lease has no key in its folder.' }
    if (-not (Test-Path -LiteralPath (Join-Path $s.Dir 'id_ed25519-cert.pub') -PathType Leaf)) { return 'The lease has no certificate in its folder.' }
    if (-not (Test-Path -LiteralPath (Join-Path $s.Dir 'id_ed25519.pub') -PathType Leaf)) { return 'The lease has no public key in its folder.' }
    if (-not (Test-Path -LiteralPath $s.Alias -PathType Leaf)) { return 'The lease has no alias file.' }
    # state.md, "The alias file": the first line starts `# agentspace lease <id>;`, the owner mark the other program relies on.
    $first = ([IO.File]::ReadAllText($s.Alias, [Text.UTF8Encoding]::new($false)) -split "`n")[0]
    if (-not $first.StartsWith("# agentspace lease $($s.Id);", [StringComparison]::Ordinal)) { return 'The first line of the alias file is not the lease owner mark.' }
    $s.Cert0 = Get-CcFileHash (Join-Path $s.Dir 'id_ed25519-cert.pub')
    $true
  }
  Test-InteropState "$sec`: $tag, step 1: The state file is valid and holds every printed lease once"

  # Step 2: the other program's get prints it with the same values.
  Test-Cc "$sec`: $tag, step 2: $Second get prints the lease $First made, with the same values" {
    if (-not $s.Id) { return 'Setup: step 1 made no lease.' }
    $g = Invoke-Interop $Second @('get', $s.Id)
    $gd = Get-InteropDoc $g
    if (-not $gd) { return "The get was not a valid lease document: $(Get-InteropNote $g)." }
    if ((ConvertTo-CcCanonical $gd) -cne (ConvertTo-CcCanonical $s.Doc)) { return 'The document the get printed differs, parsed, from the one the create printed.' }
    $true
  }
  Test-InteropState "$sec`: $tag, step 2: The state file is valid and holds every printed lease once"

  # Step 3: and so does its list.
  Test-Cc "$sec`: $tag, step 3: $Second list holds the lease $First made once, with the same values" {
    if (-not $s.Id) { return 'Setup: step 1 made no lease.' }
    $l = Invoke-Interop $Second @('list')
    $ld = Get-InteropList $l
    if (-not $ld) { return "The list was not a valid list document: $(Get-InteropNote $l)." }
    $hits = @($ld['leases'] | Where-Object { $_['id'] -ceq $s.Id })
    if ($hits.Count -ne 1) { return "The lease is in the list $($hits.Count) times, not once." }
    if ((ConvertTo-CcCanonical $hits[0]) -cne (ConvertTo-CcCanonical $s.Doc)) { return 'The list entry differs, parsed, from the document the create printed.' }
    $true
  }
  Test-InteropState "$sec`: $tag, step 3: The state file is valid and holds every printed lease once"

  # Step 4: the other program renews it, and the first program sees the renewal.
  Test-Cc "$sec`: $tag, step 4: $Second renews the lease with --ttl 90m, and $First get shows the new expires_at, ttl_seconds and certificate" {
    if (-not $s.Id) { return 'Setup: step 1 made no lease.' }
    Wait-InteropPast (Get-InteropIssue $s.Doc)
    $rn = Invoke-Interop $Second @('renew', $s.Id, '--ttl', '90m')
    $rd = Get-InteropDoc $rn
    if (-not $rd -or $rd['state'] -cne 'ready') { return "The renew was not a valid ready lease: $(Get-InteropNote $rn)." }
    if ($rd['id'] -cne $s.Id) { return 'The renew printed a different lease from the one asked for.' }
    if ($rd['ttl_seconds'] -ne $script:ttlRenewed) { return 'The renew did not set ttl_seconds to 5400.' }
    if ($rd['expires_at'] -ceq $s.Doc['expires_at']) { return 'The renew did not change expires_at.' }
    if (-not (Test-InteropSameExcept $rd $s.Doc @('expires_at', 'ttl_seconds'))) { return 'The renew changed a field other than expires_at and ttl_seconds (lease-api.md, renew: the key, alias, user, ID and created_at do not change).' }
    $s.Renew1 = $rd
    $g = Invoke-Interop $First @('get', $s.Id)
    $gd = Get-InteropDoc $g
    if (-not $gd) { return "The get was not a valid lease document: $(Get-InteropNote $g)." }
    if ($gd['id'] -cne $s.Id) { return 'The get printed a different lease from the one asked for.' }
    if ($gd['state'] -cne 'ready') { return 'The get after the renew did not show a ready lease.' }
    if ((ConvertTo-CcCanonical $gd) -cne (ConvertTo-CcCanonical $rd)) { return 'The document the get printed differs, parsed, from the one the renew printed.' }
    if ($gd['expires_at'] -cne $rd['expires_at']) { return "The get's expires_at is not the one the renew printed." }
    if ($gd['ttl_seconds'] -ne $rd['ttl_seconds']) { return "The get's ttl_seconds is not the one the renew printed." }
    $rec = Get-CcRecord $script:lab $s.Id
    if (-not $rec -or $rec['expires_at'] -cne $rd['expires_at'] -or $rec['ttl_seconds'] -ne $rd['ttl_seconds']) { return 'The state file does not record the renewed expires_at and ttl_seconds.' }
    $s.Cert1 = Get-CcFileHash (Join-Path $s.Dir 'id_ed25519-cert.pub')
    if ($s.Cert1 -eq 'absent' -or $s.Cert1 -eq $s.Cert0) { return 'The certificate on disk is not a new one.' }
    $true
  }
  Test-InteropCertificate "$sec`: $tag, step 4: The certificate on disk after $Second renewed it is valid until the new expires_at" $s ([string]$(if ($s.Renew1) { $s.Renew1['expires_at'] } else { '' }))
  Test-InteropState "$sec`: $tag, step 4: The state file is valid and holds every printed lease once"

  # Step 5: the first program renews it again, and the other sees it.
  Test-Cc "$sec`: $tag, step 5: $First renews the lease again, and $Second get shows the new expires_at, ttl_seconds and certificate" {
    if (-not $s.Renew1) { return 'Setup: step 4 made no renewal.' }
    Wait-InteropPast (Get-InteropIssue $s.Renew1)
    $rn = Invoke-Interop $First @('renew', $s.Id)
    $rd = Get-InteropDoc $rn
    if (-not $rd -or $rd['state'] -cne 'ready') { return "The renew was not a valid ready lease: $(Get-InteropNote $rn)." }
    if ($rd['ttl_seconds'] -ne $script:ttlRenewed) { return 'The renew without --ttl did not reuse the lease ttl_seconds.' }
    if ($rd['id'] -cne $s.Id) { return 'The renew printed a different lease from the one asked for.' }
    $t2 = Get-InteropTime $rd['expires_at']; $t1 = Get-InteropTime $s.Renew1['expires_at']
    if ($null -eq $t2 -or $null -eq $t1) { return 'An expires_at is not a valid time.' }
    if ($t2 -le $t1) { return 'The second renew did not give a later expires_at.' }
    if (-not (Test-InteropSameExcept $rd $s.Renew1 @('expires_at', 'ttl_seconds'))) { return 'The renew changed a field other than expires_at and ttl_seconds (lease-api.md, renew: the key, alias, user, ID and created_at do not change).' }
    $s.Renew2 = $rd
    $g = Invoke-Interop $Second @('get', $s.Id)
    $gd = Get-InteropDoc $g
    if (-not $gd) { return "The get was not a valid lease document: $(Get-InteropNote $g)." }
    if ($gd['id'] -cne $s.Id) { return 'The get printed a different lease from the one asked for.' }
    if ((ConvertTo-CcCanonical $gd) -cne (ConvertTo-CcCanonical $rd)) { return 'The document the get printed differs, parsed, from the one the renew printed.' }
    if ($gd['state'] -cne 'ready' -or $gd['expires_at'] -cne $rd['expires_at'] -or $gd['ttl_seconds'] -ne $rd['ttl_seconds']) { return "The get does not show the renew's expires_at and ttl_seconds on a ready lease." }
    $cert2 = Get-CcFileHash (Join-Path $s.Dir 'id_ed25519-cert.pub')
    if ($cert2 -eq 'absent' -or $cert2 -eq $s.Cert1) { return 'The certificate on disk is not a new one.' }
    $true
  }
  Test-InteropCertificate "$sec`: $tag, step 5: The certificate on disk after $First renewed it again is valid until the new expires_at" $s ([string]$(if ($s.Renew2) { $s.Renew2['expires_at'] } else { '' }))
  Test-InteropState "$sec`: $tag, step 5: The state file is valid and holds every printed lease once"

  # Step 6: the other program releases it, and the first program sees it released, with the files gone.
  Test-Cc "$sec`: $tag, step 6: $Second releases the lease, and $First get shows it released with that released_at, its alias file and its folder gone" {
    if (-not $s.Renew2) { return 'Setup: step 5 made no renewal.' }
    $rl = Invoke-Interop $Second @('release', $s.Id)
    $rd = Get-InteropDoc $rl
    if (-not $rd -or $rd['state'] -cne 'released' -or -not $rd['released_at']) { return "The release was not a valid released lease: $(Get-InteropNote $rl)." }
    if ($rd['id'] -cne $s.Id) { return 'The release printed a different lease from the one asked for.' }
    if (-not (Test-InteropSameExcept $rd $s.Renew2 @('state', 'ssh', 'released_at'))) { return 'The release changed a field other than state, ssh and released_at (lease-api.md, release: it drops ssh, keeps expires_at and sets released_at).' }
    $s.Released = $rd
    $g = Invoke-Interop $First @('get', $s.Id)
    $gd = Get-InteropDoc $g
    if (-not $gd) { return "The get was not a valid lease document: $(Get-InteropNote $g)." }
    if ($gd['id'] -cne $s.Id) { return 'The get printed a different lease from the one asked for.' }
    if ((ConvertTo-CcCanonical $gd) -cne (ConvertTo-CcCanonical $rd)) { return 'The document the get printed differs, parsed, from the one the release printed.' }
    if ($gd['state'] -cne 'released') { return 'The get after the release did not show a released lease.' }
    if ($gd['released_at'] -cne $rd['released_at']) { return "The get's released_at is not the one the release printed." }
    if ($gd.ContainsKey('ssh')) { return 'The released lease still shows ssh.' }
    if ($gd['expires_at'] -cne $s.Renew2['expires_at']) { return 'The released lease did not keep its expires_at.' }
    if (Test-Path -LiteralPath $s.Alias) { return 'The alias file is still there.' }
    if (Test-Path -LiteralPath $s.Dir) { return 'The lease folder is still there.' }
    $true
  }
  Test-InteropState "$sec`: $tag, step 6: The state file is valid and holds every printed lease once"

  # Step 7: the first program releases it again: exit 0 and the same document.
  Test-Cc "$sec`: $tag, step 7: $First releases it again, exits 0 and prints the same released_at" {
    if (-not $s.Released) { return 'Setup: step 6 made no release.' }
    $ra = Get-InteropTime $s.Released['released_at']
    if ($null -eq $ra) { return 'The released_at the first release printed is not a valid time.' }
    Wait-InteropPast $ra
    $rl = Invoke-Interop $First @('release', $s.Id)
    $rd = Get-InteropDoc $rl
    if (-not $rd) { return "The second release was not a valid lease document: $(Get-InteropNote $rl)." }
    if ($rd['id'] -cne $s.Id) { return 'The second release printed a different lease from the one asked for.' }
    if ($rd['state'] -cne 'released' -or $rd['released_at'] -cne $s.Released['released_at']) { return 'The second release did not print the first released_at.' }
    if ((ConvertTo-CcCanonical $rd) -cne (ConvertTo-CcCanonical $s.Released)) { return 'The second release printed a different document from the first.' }
    if ((Test-Path -LiteralPath $s.Alias) -or (Test-Path -LiteralPath $s.Dir)) { return 'The alias file or the lease folder came back.' }
    $true
  }
  Test-InteropState "$sec`: $tag, step 7: The state file is valid and holds every printed lease once"

  # Step 8: an unsatisfiable lease made by the first program reads the same in the other.
  Test-Cc "$sec`: $tag, step 8: An unsatisfiable lease $First made reads the same in $Second, by get and by list" {
    $cu = Invoke-Interop $First @('create', '--project', $Project, '--size', 'l')
    $ud = Get-InteropDoc $cu
    if (-not $ud -or $ud['state'] -cne 'unsatisfiable') { return "The create was not a valid unsatisfiable lease: $(Get-InteropNote $cu)." }
    $uid = [string]$ud['id']
    $g = Invoke-Interop $Second @('get', $uid)
    $gd = Get-InteropDoc $g
    if (-not $gd) { return "The get was not a valid lease document: $(Get-InteropNote $g)." }
    if ((ConvertTo-CcCanonical $gd) -cne (ConvertTo-CcCanonical $ud)) { return 'The get printed a different document from the one the create printed.' }
    $l = Invoke-Interop $Second @('list', '--state', 'unsatisfiable')
    $ld = Get-InteropList $l
    if (-not $ld) { return "The list was not a valid list document: $(Get-InteropNote $l)." }
    $hits = @($ld['leases'] | Where-Object { $_['id'] -ceq $uid })
    if ($hits.Count -ne 1) { return "The lease is in the list $($hits.Count) times, not once." }
    if ((ConvertTo-CcCanonical $hits[0]) -cne (ConvertTo-CcCanonical $ud)) { return 'The list entry differs from the document the create printed.' }
    $true
  }
  Test-InteropState "$sec`: $tag, step 8: The state file is valid and holds every printed lease once"
}

$lab = New-ConformanceHome
try {
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: One lease through two programs" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {
    Invoke-InteropRound -Round 1 -First 'AGENTSPACE_CMD' -Second 'AGENTSPACE_PEER_CMD' -Project 'interopa'
    Invoke-InteropRound -Round 2 -First 'AGENTSPACE_PEER_CMD' -Second 'AGENTSPACE_CMD' -Project 'interopb'
  }
}
catch {
  Write-ConformanceResult -Name 'No unexpected error in test-interop' -Status FAIL -Reason ("It threw: " + ($_.Exception.Message -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun
