#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.04-T03-S03, #0086): the driver as a caller sees it. The driver
# boundary is not visible from outside the CLI except through the CLI (docs/driver.md, "The
# boundary"), so this check drives the CLI only, and looks at what it prints and the files it
# leaves. It enforces docs/driver.md, "Unsatisfiable requests", "Place a lease", "What a driver
# never exposes", "The driver's name" and "Failure"; docs/lease-api.md, "Fields", "States",
# "agentspace lease create" and "Concurrency and atomicity"; and docs/state.md, "The lease state
# file" and "Creation, in order".
#
# - No program is loaded and no driver is named. A failed alias step is made with a file where the
#   alias folder `agentspace.d` has to be, which state.md says makes the alias write fail.
# - It reads the lease state file, which state.md documents, only to learn what the driver's
#   stored name is, so it can look for that name in every output.
# - Nothing connects anywhere.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-driver-outcomes.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$unsat = 'driver.md#unsatisfiable-requests'
$none = '00000000-0000-4000-8000-000000000000'

# What a failed or unsatisfiable create must leave alone: the lease folders and, unless -LeasesOnly,
# the ssh folder. A failed alias step MAY leave the `Include` line it added (state.md, "Creation,
# in order"), so that case compares the lease folders only.
function Get-CcIssued($Lab, [switch]$LeasesOnly) {
  $leasesDir = Join-Path $Lab.UserProfile '.agentspace\leases'
  $ssh = if ($LeasesOnly) { '' } else { (Get-CcFileNames $Lab.SshDir) -join '|' }
  "$((Get-CcFileNames $leasesDir) -join '|');$ssh"
}

# True when a message names no absolute path, no user name, no other program's output (lease-api.md,
# "Exit codes and errors", an `internal` message).
function Test-CcCleanMessage($Lab, [string]$Message) {
  if (-not $Message -or $Message.StartsWith(' ') -or $Message -match '[\x00-\x1f\x7f]') { return $false }
  if ($Message -match '(?i)[a-z]:[\\/]' -or $Message.Contains($Lab.Root) -or $Message.Contains($Lab.UserProfile)) { return $false }
  if ($Message -match '(?i)ssh-keygen|Exception|at line|\bCategoryInfo\b') { return $false }
  Test-CcNoBackendWords $Message
}

$lab = New-ConformanceHome
try {

  # ---- Unsatisfiable requests (driver.md#unsatisfiable-requests) -----------------------------------
  # No credential is needed, so these run whether or not the throwaway CA could be made.
  $cases = @(
    @{ Name = 'An unsupported platform'; Args = @('--platform', 'linux/amd64'); Codes = @('unsupported_platform'); Values = @("'linux/amd64'") }
    @{ Name = 'A Windows platform'; Args = @('--platform', 'windows/amd64'); Codes = @('unsupported_platform'); Values = @("'windows/amd64'") }
    @{ Name = 'A versioned profile'; Args = @('--profile', 'agent-linux@1'); Codes = @('unsupported_profile'); Values = @("'agent-linux@1'") }
    @{ Name = 'A size no profile has'; Args = @('--size', 'l'); Codes = @('unsupported_size'); Values = @("'l'") }
    @{ Name = 'A platform and a size'; Args = @('--platform', 'linux/amd64', '--size', 'l'); Codes = @('unsupported_platform', 'unsupported_size'); Values = @("'linux/amd64'", "'l'") }
    @{ Name = 'All three, given in upper case and with a padded version'; Args = @('--size', 'L', '--profile', 'Agent-Linux@03', '--platform', 'Linux/AMD64')
      Codes = @('unsupported_platform', 'unsupported_profile', 'unsupported_size'); Values = @("'linux/amd64'", "'agent-linux@3'", "'l'") }
  )
  foreach ($case in $cases) {
    $issuedBefore = Get-CcIssued $lab
    $run = Invoke-CcLease $lab (@('create', '--project', 'drvtest') + $case.Args)
    $doc = ConvertFrom-CcJson $run.Stdout
    $issuedAfter = Get-CcIssued $lab
    Test-Cc "$unsat`: $($case.Name) is a successful create (exit 0) that validates and is unsatisfiable, with the reasons in order and nothing else" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($doc['state'] -cne 'unsatisfiable') { return "The state is '$($doc['state'])'." }
      $codes = @($doc['reasons'] | ForEach-Object { $_['code'] }) -join ','
      if ($codes -cne ($case.Codes -join ',')) { return "The reason codes are '$codes'." }
      foreach ($k in 'ssh', 'expires_at', 'released_at') { if ($doc.ContainsKey($k)) { return "The document has $k." } }
      $true
    }
    Test-Cc "$unsat`: $($case.Name): each message is one sentence, quotes the normalized value and names no backend (D1)" {
      if (-not $doc -or -not $doc['reasons']) { return 'There are no reasons.' }
      $i = 0
      foreach ($reason in $doc['reasons']) {
        $m = [string]$reason['message']
        if ($m -notmatch '^[^\r\n]+\.$') { return "Message '$m' is not one sentence." }
        if (-not $m.Contains($case.Values[$i])) { return "Message '$m' does not quote $($case.Values[$i])." }
        if (-not (Test-CcNoBackendWords $m)) { return "Message '$m' names a backend." }
        $i++
      }
      $true
    }
    Test-Cc "$unsat`: $($case.Name) issues nothing: no lease folder, no alias file, no ssh folder content (state.md#creation-in-order)" {
      if (-not (Test-CcSuccess $run)) { return "There was no valid lease: $(Format-CcRun $run)" }
      if ($issuedAfter -cne $issuedBefore) { return "The lease or ssh folders changed from '$issuedBefore' to '$issuedAfter'." }
      $true
    }
    $got = Invoke-CcLease $lab @('get', $(if ($doc) { [string]$doc['id'] } else { $none }))
    Test-Cc "lease-api.md#agentspace-lease-create: $($case.Name) is recorded, and get returns the same document" {
      if (-not $doc) { return 'There was no lease.' }
      if (-not (Test-CcSuccess $got)) { return "Get was not a valid success: $(Format-CcRun $got)" }
      if ((ConvertTo-CcCanonical (ConvertFrom-CcJson $got.Stdout)) -cne (ConvertTo-CcCanonical $doc)) { return 'Get printed another document.' }
      $true
    }
  }
  Test-Cc "lease-api.md#states: The state file holds each unsatisfiable lease, and validates against its schema" {
    $s = Read-CcState $lab
    if (-not $s -or @($s['leases']).Count -ne $cases.Count) { return "The state file holds $(@($s['leases']).Count) records, not $($cases.Count)." }
    if (@($s['leases'] | Where-Object { $_['state'] -cne 'unsatisfiable' }).Count -ne 0) { return 'A record is not unsatisfiable.' }
    if (-not (Test-CcStateFile $lab)) { return 'The state file does not validate.' }
    $true
  }

  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$unsat and driver.md#what-a-driver-never-exposes, for a ready lease" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {
    # ---- The output has no driver name (driver.md#what-a-driver-never-exposes, #the-drivers-name) ------
    $r = New-CcReadyLease $lab 'drvtest'
    $d = $r.Doc
    $id = if ($d) { [string]$d['id'] } else { $none }
    $runs = [System.Collections.Generic.List[object]]::new()
    $runs.Add($r.Run)
    $runs.Add((Invoke-CcLease $lab @('get', $id)))
    $runs.Add((Invoke-CcLease $lab @('list')))
    Start-Sleep -Milliseconds 1100
    $runs.Add((Invoke-CcLease $lab @('renew', $id)))
    $runs.Add((Invoke-CcLease $lab @('release', $id)))
    $runs.Add((Invoke-CcLease $lab @('get', '3b6c1d2e-5f40-4a8b-9c7d-1e2f3a4b5c6d')))
    $stored = Get-CcRecord $lab $id
    $driverName = if ($stored) { [string]$stored['driver'] } else { '' }
    Test-Cc "state.md#the-lease-state-file: A ready lease's record carries a non-empty driver name" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if (-not $driverName -or $driverName -match '[\x00-\x1f\x7f]') { return 'The record has no driver name, or one with a control character.' }
      $true
    }
    # Not named $name: Test-Cc has a parameter of that name, which a block run by it would read instead.
    Test-Cc "driver.md#the-drivers-name: No output of create, get, list, renew, release or a not_found names the driver, or has a driver member" {
      if (-not $driverName) { return 'There is no stored driver name to look for.' }
      $pattern = '(?i)(?<![0-9a-z])' + [regex]::Escape($driverName) + '(?![0-9a-z])'
      $n = 0
      foreach ($x in $runs) {
        $n++
        $all = [string]$x.Stdout + [string]$x.Stderr
        if ($all.Length -eq 0) { return "Run $n printed nothing." }
        if ($all -match $pattern) { return "Run $n names the driver." }
        if ($all -match '"driver"\s*:') { return "Run $n has a driver member." }
      }
      $true
    }
    Test-Cc "driver.md#what-a-driver-never-exposes: No output names a backend, a machine or an absolute path on the caller's machine" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      $n = 0
      foreach ($x in $runs) {
        $n++
        $all = [string]$x.Stdout + [string]$x.Stderr
        if ($all.Length -eq 0) { return "Run $n printed nothing." }
        if (-not (Test-CcNoBackendWords $all)) { return "Run $n names a backend." }
        if ($all -match '(?i)[a-z]:[\\/]' -or $all.Contains($lab.Root)) { return "Run $n holds an absolute path." }
      }
      $true
    }

    # ---- A failed alias step (driver.md#place-a-lease, lease-api.md#concurrency-and-atomicity) ---------
    # A file where the alias folder has to be makes the alias step fail after the credential is made.
    Reset-CcSshDir $lab
    $blocker = Join-Path $lab.SshDir 'agentspace.d'
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $lab -Path $blocker), "not a folder`n")
    $issuedBefore = Get-CcIssued $lab -LeasesOnly
    $stateBefore = Get-CcStateCanonical $lab
    $run = Invoke-CcLease $lab @('create', '--project', 'drvtest')
    $issuedAfter = Get-CcIssued $lab -LeasesOnly
    $err = ConvertFrom-CcJson $run.Stderr
    Test-Cc "lease-api.md#concurrency-and-atomicity: A failed alias step is internal (exit 1), with nothing on stdout and one valid error document" {
      if (-not (Test-CcFailure $run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }
    Test-Cc "driver.md#failure: The failed alias step's message is in agentspace's own words: no absolute path, no user name, no other program's text, no backend" {
      if (-not $err) { return 'There was no error document.' }
      if (-not (Test-CcCleanMessage $lab ([string]$err['message']))) { return "The message is '$($err['message'])'." }
      $true
    }
    Test-Cc "lease-api.md#concurrency-and-atomicity: A failed alias step leaves no lease folder, no key, no certificate, no alias file and no record" {
      if (-not (Test-CcFailure $run 1 'internal')) { return "Setup: create did not fail with a valid internal error: $(Format-CcRun $run)" }
      if ($issuedAfter -cne $issuedBefore) { return "The lease or ssh folders changed from '$issuedBefore' to '$issuedAfter'." }
      if ((Get-CcStateCanonical $lab) -cne $stateBefore) { return 'The state file changed.' }
      if ((Get-Content -LiteralPath $blocker -Raw) -cne "not a folder`n") { return 'The file in the way was changed.' }
      $true
    }
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $blocker) -Force
    $later = New-CcReadyLease $lab 'drvtest'
    Test-Cc "lease-api.md#concurrency-and-atomicity: A later create is not blocked by anything the failed one left" {
      if ($later.Ok) { $true } else { "The create gave $(Format-CcRun $later.Run)" }
    }

    # ---- An unsatisfiable answer needs no CA (state.md#creation-in-order) ---------------------------------
    $caDir = Join-Path $lab.UserProfile '.agentspace\ca'
    if (Test-Path -LiteralPath $caDir) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $caDir) -Recurse -Force }
    $run = Invoke-CcLease $lab @('create', '--project', 'drvtest', '--size', 'l')
    Test-Cc "state.md#creation-in-order: An unsatisfiable create runs no CA check, so a machine with no CA still gets the answer" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((ConvertFrom-CcJson $run.Stdout)['state'] -cne 'unsatisfiable') { return 'The state is not unsatisfiable.' }
      if (Test-Path -LiteralPath $caDir) { return 'A CA was created.' }
      $true
    }
  }
}
catch {
  Write-ConformanceResult -Name 'No unexpected error in test-lease-driver-outcomes' -Status FAIL -Reason ("It threw: " + ($_.Exception.Message -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun
