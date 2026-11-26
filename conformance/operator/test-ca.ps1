#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `ca` command (docs/operator.md, "ca"; docs/state.md, "The CA"):
# it makes the SSH CA once, checks it on every later run and changes nothing, never repairs a
# half-made or mismatched CA, and refuses a folder that a OneDrive variable names. The program under
# test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# - `ca` makes no AWS call and touches no box, so no AWS CLI need exist (B-50). The three programs the
#   operator starts are named as tripwires through the test seam anyway, and the check ends by proving
#   that none of them was called, so that a build that ran one in `ca` is caught.
# - Every run is in a throwaway home (the harness's), where the harness's own CA is deleted first so
#   that `ca` has none to find. Every write and delete goes through Assert-ConformancePath.
# - ssh-keygen is a program outside agentspace (D34), and is the oracle for the key pair.
# - Not checked: a run elevated (the refusal needs an elevated shell, and the suite MUST NOT run
#   elevated), a machine with no `ssh-keygen.exe`, and the access rules of the CA's folder and key
#   (a loosened key needs a rule added to a file, which a check does not do). docs/conformance.md
#   ("Coverage of the operator and the keeper") says so.
# - A failure reason never quotes the program's output, which names the user and the computer.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-ca.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#ca'

function Invoke-Ca {
  param($Lab, [hashtable]$Trip, [string[]]$Arguments = @('ca'), [hashtable]$Extra = @{})
  Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $Lab -Arguments $Arguments -Environment ($Trip + $Extra) -TimeoutSeconds 120
}

function Get-CaHash([string]$Path) { if (Test-Path -LiteralPath $Path -PathType Leaf) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash } else { 'absent' } }

# Lines of a stderr, without the empty ones.
function Get-Lines([string]$Text) { , @($Text -split "`r?`n" | Where-Object { $_.Trim() -ne '' }) }

# Passes when no problem was listed, else fails with the list.
function Write-CaResult {
  param([string]$Name, [System.Collections.Generic.List[string]]$Problems)
  if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS } else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($Problems -join '; ')) }
}

# The problems with a run meant to be a successful `ca`: exit 0, nothing on stdout, no runtime error.
function Get-CaSuccessProblems($Run) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.TimedOut) { $p.Add('It timed out') }
  elseif ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not 0") }
  if ($Run.Stdout.Trim().Length -gt 0) { $p.Add('Stdout is not empty') }
  if ($Run.Stderr -match 'CategoryInfo|At line:|Exception') { $p.Add('Stderr shows a runtime error record') }
  , $p
}

# The problems with a run meant to be a refusal: exit 1, nothing on stdout, and a last line that
# starts "agentspace: " and holds every text given.
function Get-CaRefusalProblems($Run, [string[]]$Text) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.TimedOut) { $p.Add('It timed out') }
  elseif ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
  if ($Run.Stdout.Trim().Length -gt 0) { $p.Add('Stdout is not empty') }
  $lines = Get-Lines $Run.Stderr
  if ($lines.Count -eq 0 -or -not $lines[-1].StartsWith('agentspace: ')) { $p.Add('The last line on stderr does not start with "agentspace: "') }
  foreach ($t in $Text) { if (-not $Run.Stderr.Contains($t)) { $p.Add("Stderr lacks the text '$t'") } }
  if ($Run.Stderr -match 'CategoryInfo|At line:|Exception') { $p.Add('Stderr shows a runtime error record') }
  , $p
}

$h = New-ConformanceHome
try {
  $caDir = Join-Path $h.UserProfile '.agentspace\ca'
  $key = Join-Path $caDir 'agentspace_ca'
  $pub = Join-Path $caDir 'agentspace_ca.pub'
  $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
  $keygen = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
  if (-not (Test-Path -LiteralPath $keygen -PathType Leaf)) {
    Write-ConformanceResult -Name "${sec}: ca makes the CA" -Status SKIPPED -Reason 'ssh-keygen.exe is not in the Windows OpenSSH folder, so the key pair cannot be read'
  } else {
    # Start with no CA: the harness made one (its `ca -Quiet` is not a result of this check, and a
    # program that makes none fails the first-run results below), which is deleted here.
    if (Test-Path -LiteralPath $caDir) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $caDir) -Recurse -Force }

    # ---- A CA folder inside a OneDrive folder is refused, and nothing is made ----
    $r = Invoke-Ca $h $trip -Extra @{ OneDrive = $h.UserProfile }
    $p = Get-CaRefusalProblems $r @('is inside', 'MUST NOT live in a synced folder or in the repo')
    if (Test-Path -LiteralPath $caDir) { $p.Add('A CA folder was made') }
    Write-CaResult "${sec}: ca refuses a CA folder inside the OneDrive folder, with exit 1, agentspace's message and nothing made" $p

    # ---- The first run makes the CA ----
    $r = Invoke-Ca $h $trip
    $p = Get-CaSuccessProblems $r
    $lines = Get-Lines $r.Stderr
    if (-not ($lines | Where-Object { $_ -like 'Creating the agentspace SSH CA in *...' })) { $p.Add('Stderr lacks the "Creating the agentspace SSH CA in <folder>..." line') }
    if (-not ($lines -ccontains 'Created the agentspace SSH CA.')) { $p.Add('Stderr lacks "Created the agentspace SSH CA."') }
    foreach ($label in 'Private key', 'Public key', 'Fingerprint') {
      if (-not ($lines | Where-Object { $_ -match "^$label {2,}\S" })) { $p.Add("Stderr lacks the '$label' line") }
    }
    Write-CaResult "${sec}: The first ca makes the CA and prints its four lines to stderr, with exit 0 and nothing on stdout" $p

    $p = [System.Collections.Generic.List[string]]::new()
    $madeFiles = (Test-Path -LiteralPath $key -PathType Leaf) -and (Test-Path -LiteralPath $pub -PathType Leaf)
    if (-not $madeFiles) { $p.Add('The CA folder lacks agentspace_ca or agentspace_ca.pub') }
    else {
      $out = & $keygen -y -P '' -f $key 2>&1
      if ($LASTEXITCODE -ne 0) { $p.Add('The private key does not load with an empty passphrase') }
      else {
        $derived = ([string]($out | Select-Object -First 1)).Trim().Split(' ')
        $stored = ([IO.File]::ReadAllText($pub)).Trim().Split(' ')
        if ($derived[0] -cne 'ssh-ed25519') { $p.Add('The key is not ed25519') }
        if ($derived.Count -lt 2 -or $stored.Count -lt 2 -or $derived[1] -cne $stored[1]) { $p.Add('The public key is not the private key''s own') }
      }
      $fpLine = & $keygen -l -f $pub 2>&1
      $fp = ([string]($fpLine | Select-Object -First 1)).Split(' ')[1]
      if (-not $fp -or -not ($lines | Where-Object { $_ -match '^Fingerprint {2,}' -and $_.Contains($fp) })) { $p.Add('The printed fingerprint is not the public key''s') }
    }
    Write-CaResult "${sec}: The CA is an ed25519 pair with no passphrase, and the printed fingerprint is its own (state.md#the-ca)" $p

    # ---- A later run checks and changes nothing ----
    $before = (Get-CaHash $key) + '|' + (Get-CaHash $pub)
    $times = (Get-Item -LiteralPath $key).LastWriteTimeUtc.Ticks.ToString() + '|' + (Get-Item -LiteralPath $pub).LastWriteTimeUtc.Ticks.ToString()
    $r = Invoke-Ca $h $trip
    $p = Get-CaSuccessProblems $r
    $lines = Get-Lines $r.Stderr
    if (-not ($lines -ccontains 'The agentspace SSH CA already exists; nothing changed.')) { $p.Add('Stderr lacks "The agentspace SSH CA already exists; nothing changed."') }
    if ($lines -ccontains 'Created the agentspace SSH CA.') { $p.Add('Stderr says it created the CA') }
    if ((Get-CaHash $key) + '|' + (Get-CaHash $pub) -cne $before) { $p.Add('A key file changed') }
    $timesAfter = (Get-Item -LiteralPath $key).LastWriteTimeUtc.Ticks.ToString() + '|' + (Get-Item -LiteralPath $pub).LastWriteTimeUtc.Ticks.ToString()
    if ($timesAfter -cne $times) { $p.Add('A key file was written to') }
    Write-CaResult "${sec}: ca on an existing, sound CA exits 0, says nothing changed, and changes neither file" $p

    $r = Invoke-Ca $h $trip -Arguments @('ca', '-Quiet')
    $p = Get-CaSuccessProblems $r
    if ($r.Stderr.Trim().Length -gt 0) { $p.Add('-Quiet left a message on stderr') }
    Write-CaResult "${sec}: ca -Quiet prints nothing at all" $p

    # ---- A half-made CA is refused, and never repaired ----
    $backup = Join-Path $h.Root 'ca-backup'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $backup) | Out-Null

    # A file is moved aside and moved back, never copied back: a copy gets new access rules, and ca
    # refuses a private key that is not the user's alone.
    $keyAside = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $backup 'agentspace_ca')
    $pubAside = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $backup 'agentspace_ca.pub')
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $pub) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $pubAside)
    $keyBefore = Get-CaHash $key
    $r = Invoke-Ca $h $trip
    $p = Get-CaRefusalProblems $r @('exists but', 'does not repair a half-made CA')
    if (Test-Path -LiteralPath $pub) { $p.Add('The public key was made again') }
    if ((Get-CaHash $key) -cne $keyBefore) { $p.Add('The private key changed') }
    Write-CaResult "${sec}: A private key with no public key is refused with exit 1 and is not repaired" $p
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $pubAside) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $pub)

    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $key) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $keyAside)
    $pubBefore = Get-CaHash $pub
    $r = Invoke-Ca $h $trip
    $p = Get-CaRefusalProblems $r @('exists but', 'private key is lost')
    if (Test-Path -LiteralPath $key) { $p.Add('A private key was made') }
    if ((Get-CaHash $pub) -cne $pubBefore) { $p.Add('The public key changed') }
    Write-CaResult "${sec}: A public key with no private key is refused with exit 1, and no CA is made over it" $p
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $keyAside) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $key)

    # ---- A public key that is not the private key's own is refused, and not repaired ----
    $other = Join-Path $h.Root 'other-key'
    & $keygen -q -t ed25519 -N '' -f (Assert-ConformancePath -ConformanceHome $h -Path $other) | Out-Null
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $pub) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $pubAside)
    Copy-Item -LiteralPath "$other.pub" -Destination (Assert-ConformancePath -ConformanceHome $h -Path $pub)
    $pubBefore = Get-CaHash $pub
    $keyBefore = Get-CaHash $key
    $r = Invoke-Ca $h $trip
    $p = Get-CaRefusalProblems $r @('is not the ed25519 public key of', 'does not repair a mismatched pair')
    if ((Get-CaHash $pub) -cne $pubBefore -or (Get-CaHash $key) -cne $keyBefore) { $p.Add('A key file changed') }
    Write-CaResult "${sec}: A public key that is not the private key's own is refused with exit 1 and is not repaired" $p
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $pub) -Force
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $pubAside) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $pub)
    $r = Invoke-Ca $h $trip
    Write-CaResult "${sec}: With the pair put back by hand, ca passes again" (Get-CaSuccessProblems $r)

    # ---- ca makes no AWS, ssh or scp call ----
    $p = [System.Collections.Generic.List[string]]::new()
    foreach ($n in 'aws', 'ssh', 'scp') {
      $log = Join-Path $h.Root "standins\$n-tripwire-calls.log"
      if ((Get-StandInCallCount -Log $log) -gt 0) { $p.Add("ca started the $n program") }
    }
    Write-CaResult "${sec}: ca starts no AWS CLI, ssh or scp" $p
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-ca.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun
