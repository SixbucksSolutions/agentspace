#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.04-T03-S03, #0086): concurrent lease commands. It enforces
# docs/lease-api.md, "Concurrency and atomicity" (commands MUST be safe at the same time from
# several processes, and a command's effect on a lease MUST be atomic), and docs/state.md,
# "Concurrency" (the lease state lock, and what two commands at once can see).
#
# What it does. Rounds A to E start several lease CLI processes at one instant, and then read the
# state file once they have all ended. A round is a Barrier with one party for each process: every
# party is a runspace that has started and is waiting, and each starts its process the moment the
# last party arrives. So the processes are released together, never one after the other.
#   A. Six `create`s that are unsatisfiable (a short change of the state file), and a `list` reader.
#   B. Three ready `create`s at once, which hold the state lock for seconds, with a `list` reader.
#   C. Renews of four different leases at once, each with its own TTL.
#   D. Two renews of one lease at once, with different TTLs.
#   E. Two ready `create`s and two renews, all at once.
#   G. A gated round. The check holds the state lock itself, as state.md says an implementation does,
#      and starts ready and unsatisfiable `create`s, `renew`s and a `release`. It waits until every
#      process is running and none has finished, and only then lets go of the lock, so that all of
#      them contend for it at the same moment.
#   F. The check holds the state lock itself again: `get` and `list` MUST NOT wait, and a `create`, a
#      `renew` and a `release` that cannot start within 30 seconds MUST each fail with `internal`,
#      change nothing, and say to try again (lease-api.md, "Concurrency and atomicity").
#
# What a lost update looks like. A command that does not hold the state lock reads the file, changes
# its copy, and writes the whole file back. Two such commands that overlap each start from the same
# file, so the second write drops the first one's change: a `create` the caller was told succeeded has
# no record, or a `renew`'s new expires_at is replaced by the old one. After every round the check does
# not trust a command's own report: it reads the state file and demands that
#   - it validates against its schema;
#   - every lease a `create` printed (and so told the caller was made) is in it exactly once, and no
#     other record is;
#   - every `renew` that printed an expiry is in the record, and the certificate on disk, the
#     record and the printed document agree on the expiry and the TTL.
# A lost update breaks the second or the third. The check also records the time each process started
# and ended, with its own clock, and fails a round whose processes did not all overlap, so that a run
# of processes that happened one after another cannot pass.
#
# What a pass shows, and what it does not. It is evidence, not proof. A ready `create` and a `renew`
# run `ssh-keygen` while they hold the lock, which takes seconds against a few milliseconds of skew
# between the starts, so rounds B to E overlap their processes in time. That does not make their
# interleaving certain: a build that checked the lock and let go of it before it wrote could pass
# them whenever the processes happened not to collide. Round G narrows that gap and does not close
# it. Each of its processes is waiting on a lock another party holds, and all are let in together, so
# a build that does not keep the lock for the whole of its read, change and write has every process
# writing at once. A race that needs one particular interleaving may still pass any number of runs.
# So a pass means that no run so far lost an update, and not that none can.
#
# Nothing connects anywhere. The throwaway home has its own CA, state, ssh folder and leases.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-concurrency.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$conc = 'lease-api.md#concurrency-and-atomicity'
$harnessPath = Join-Path $PSScriptRoot 'harness.ps1'

# Starts one lease CLI process for each job, as a background job, and returns the job. A job is
# @{ Tag = <text>; Args = <string[]> }. Each runspace records its tag in $Started just before it starts
# its process, and its result holds the process's start and end on this machine's clock. With
# -UseBarrier every runspace waits for all the others first, so the processes start at one instant.
# A runspace writes nothing; it calls Assert-ConformancePath on the home's state folder before it runs
# anything, so that a home object that points outside the throwaway root stops it.
function Start-CcJobs {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][object[]]$Jobs, [Parameter(Mandatory)]$Started, [switch]$UseBarrier)
  $barrier = if ($UseBarrier) { [System.Threading.Barrier]::new($Jobs.Count) } else { $null }
  $hp = $harnessPath
  $proxy = $script:CcProxy
  $Jobs | ForEach-Object -ThrottleLimit $Jobs.Count -AsJob -Parallel {
    $harness = $using:hp
    . $harness
    $b = $using:barrier
    $st = $using:Started
    $proxyCommand = $using:proxy
    $labObj = $using:Lab
    $job = $_
    [void](Assert-ConformancePath -ConformanceHome $labObj -Path $labObj.StateDir)
    $arrived = $true
    if ($b) { $arrived = $b.SignalAndWait(120000) }
    $started = [DateTime]::UtcNow
    $st[[string]$job.Tag] = $started
    $r = Invoke-AgentspaceCli -ConformanceHome $labObj -Arguments (@('lease') + $job.Args) -Environment @{ AGENTSPACE_PROXY_COMMAND = $proxyCommand }
    $ended = [DateTime]::UtcNow
    [pscustomobject]@{ Tag = $job.Tag; Arrived = $arrived; Started = $started; Ended = $ended; ExitCode = $r.ExitCode; Stdout = $r.Stdout; Stderr = $r.Stderr }
  }
}

# Starts one lease CLI process for each job, all at one instant, and returns each one's run and
# its start and end on this machine's clock.
function Invoke-CcRound {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][object[]]$Jobs)
  $started = [System.Collections.Concurrent.ConcurrentDictionary[string, datetime]]::new()
  $job = Start-CcJobs -Lab $Lab -Jobs $Jobs -Started $started -UseBarrier
  Receive-Job -Job $job -Wait -AutoRemoveJob
}

# The tokens (each a text that one job's command line holds) that some process on this machine's
# command line holds, or $null when the machine's process list cannot be read.
function Get-CcSeenTokens([string[]]$Tokens) {
  try {
    $lines = @(Get-CimInstance -ClassName Win32_Process -Property CommandLine -ErrorAction Stop | ForEach-Object { [string]$_.CommandLine })
  } catch { return $null }
  @($Tokens | Where-Object { $t = $_; @($lines | Where-Object { $_.Contains($t) }).Count -gt 0 })
}

# Holds the state lock (state.md, "The lease state lock": leases.lock opened with no sharing), starts
# every job, waits until all of them are running and none has finished, and lets go of the lock. A
# job is @{ Tag; Args; Token }, where Token is a text that the job's command line holds and no other
# job's does. Returns Results (as Invoke-CcRound), ReleasedAt (read just before the lock was let go),
# AllSeen (every process was found on the machine's process list while the lock was held, or, when the
# list cannot be read, every runspace had started and six seconds had passed), and Seen.
function Invoke-CcGatedRound {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][object[]]$Jobs)
  $lockPath = Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $Lab.StateDir 'leases.lock')
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.StateDir) | Out-Null
  $tokens = @($Jobs | ForEach-Object { [string]$_.Token })
  $started = [System.Collections.Concurrent.ConcurrentDictionary[string, datetime]]::new()
  $held = [System.IO.FileStream]::new($lockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
  $job = $null; $releasedAt = $null; $allSeen = $false; $seen = @()
  try {
    $job = Start-CcJobs -Lab $Lab -Jobs $Jobs -Started $started
    # Every runspace has begun, and every process is on the process list: the commands are running.
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $deadline) {
      if ($started.Count -eq $Jobs.Count) {
        $seen = Get-CcSeenTokens $tokens
        if ($null -eq $seen) { Start-Sleep -Seconds 6; $allSeen = $true; $seen = $tokens; break }
        if ($seen.Count -eq $tokens.Count) { $allSeen = $true; break }
      }
      Start-Sleep -Milliseconds 250
    }
    # Time for a command that does not wait for the lock to finish, so that it is seen to have.
    Start-Sleep -Seconds 2
  }
  finally {
    $releasedAt = [DateTime]::UtcNow
    $held.Dispose()
  }
  $results = @(Receive-Job -Job $job -Wait -AutoRemoveJob)
  [pscustomobject]@{ Results = $results; ReleasedAt = $releasedAt; AllSeen = $allSeen; Seen = @($seen) }
}

# True when every process of a round was running at one moment: the last start is before the first end.
function Test-CcOverlap($Results) {
  $lastStart = ($Results | ForEach-Object { $_.Started } | Measure-Object -Maximum).Maximum
  $firstEnd = ($Results | ForEach-Object { $_.Ended } | Measure-Object -Minimum).Minimum
  $lastStart -lt $firstEnd
}

# The records in the state file, by ID.
function Get-CcRecordsById($Lab) {
  $s = Read-CcState $Lab
  $map = @{}
  if ($s) { foreach ($r in $s['leases']) { $map[[string]$r['id']] = $r } }
  $map
}

function Get-CcRecordCount($Lab) { $s = Read-CcState $Lab; if ($s) { @($s['leases']).Count } else { 0 } }

$lab = New-ConformanceHome
try {
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$conc, concurrent processes" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {
    $problems = @{ Report = [System.Collections.Generic.List[string]]::new(); Valid = [System.Collections.Generic.List[string]]::new()
      Records = [System.Collections.Generic.List[string]]::new(); Reader = [System.Collections.Generic.List[string]]::new()
      Overlap = [System.Collections.Generic.List[string]]::new(); Files = [System.Collections.Generic.List[string]]::new()
      Renew = [System.Collections.Generic.List[string]]::new() }
    $told = [System.Collections.Generic.List[string]]::new()   # every ID a create printed
    $leaseSchema = 'lease.output.schema.json'

    # What every round checks of the state file and the creates in it.
    function Confirm-CcRound([string]$Phase, [int]$Round, $Results, [int]$ExpectedNewCreates) {
      $tag = "$Phase round $Round"
      $newIds = [System.Collections.Generic.List[string]]::new()
      foreach ($x in $Results | Where-Object { $_.Tag -like 'create*' }) {
        $doc = ConvertFrom-CcJson $x.Stdout
        if (-not (Test-CcSuccess $x)) { $problems.Report.Add("$tag $($x.Tag): $(Format-CcRun $x)"); continue }
        $newIds.Add([string]$doc['id']); $told.Add([string]$doc['id'])
      }
      if ($newIds.Count -ne $ExpectedNewCreates) {
        $problems.Report.Add("$tag`: $($newIds.Count) of $ExpectedNewCreates creates reported success.")
        $problems.Records.Add("$tag`: $($newIds.Count) of $ExpectedNewCreates creates reported a lease, so the records cannot be compared.")
      }
      if (@($newIds | Sort-Object -Unique).Count -ne $newIds.Count) { $problems.Report.Add("$tag`: two creates printed one ID.") }
      if (-not (Test-CcStateFile $lab)) { $problems.Valid.Add("$tag`: the state file does not validate against its schema."); return }
      $ids = @((Read-CcState $lab)['leases'] | ForEach-Object { [string]$_['id'] })
      foreach ($id in $told) {
        $n = @($ids | Where-Object { $_ -ceq $id }).Count
        if ($n -ne 1) { $problems.Records.Add("$tag`: lease $($id.Substring(0, 8)) is in the state file $n times, and a create reported it made.") }
      }
      if ($ids.Count -ne $told.Count) { $problems.Records.Add("$tag`: the state file holds $($ids.Count) records, and creates reported $($told.Count).") }
      if (-not (Test-CcOverlap $Results)) { $problems.Overlap.Add("$tag`: the processes did not all overlap.") }
      if (@($Results | Where-Object { -not $_.Arrived }).Count -ne 0) { $problems.Overlap.Add("$tag`: a process was not released with the others.") }
    }

    # ---- A. Unsatisfiable creates and a reader ------------------------------------------------------
    $roundsA = 8
    for ($round = 1; $round -le $roundsA; $round++) {
      $countBefore = Get-CcRecordCount $lab
      $jobs = @(1..6 | ForEach-Object { @{ Tag = "create $_"; Args = @('create', '--project', "conc$_", '--size', 'l') } }) + @(@{ Tag = 'reader list'; Args = @('list') })
      $res = Invoke-CcRound $lab $jobs
      Confirm-CcRound 'A' $round $res 6
      $reader = $res | Where-Object { $_.Tag -eq 'reader list' }
      $ldoc = ConvertFrom-CcJson $reader.Stdout
      if (-not (Test-CcSuccess $reader 'lease-list.output.schema.json')) { $problems.Reader.Add("A round $round`: $(Format-CcRun $reader)") }
      else {
        $n = @($ldoc['leases']).Count
        if ($n -lt $countBefore -or $n -gt $countBefore + 6) { $problems.Reader.Add("A round $round`: list showed $n leases, between $countBefore and $($countBefore + 6) expected.") }
      }
    }
    Test-Cc "$conc`: A. Six unsatisfiable creates at once, $roundsA rounds: every create reports a valid lease" {
      if ($problems.Report.Count -eq 0) { $true } else { $problems.Report[0] }
    }
    Test-Cc "$conc`: A. The processes of every round overlapped in time (own clock), so the rounds cannot pass by running one after another" {
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Overlap.Count -eq 0) { $true } else { $problems.Overlap[0] }
    }
    Test-Cc "state.md#the-lease-state-lock: A. The state file validates against its schema after every round" {
      if ($problems.Valid.Count -eq 0) { $true } else { $problems.Valid[0] }
    }
    Test-Cc "$conc`: A. Every lease a create reported is in the state file exactly once, and nothing else is (no lost update)" {
      if ($problems.Records.Count -eq 0) { $true } else { $problems.Records[0] }
    }
    Test-Cc "$conc`: A. A reader running at the same time always gets a valid list that is whole (get and list never see a half-written file)" {
      if ($problems.Reader.Count -eq 0) { $true } else { $problems.Reader[0] }
    }

    # ---- B. Ready creates, which hold the lock for seconds ---------------------------------------------
    foreach ($k in $problems.Keys) { $problems[$k].Clear() }
    Reset-CcSshDir $lab
    $readyIds = [System.Collections.Generic.List[string]]::new()
    $roundsB = 2
    for ($round = 1; $round -le $roundsB; $round++) {
      $countBefore = Get-CcRecordCount $lab
      $jobs = @(1..3 | ForEach-Object { @{ Tag = "create $_"; Args = @('create', '--project', "ready$_") } }) + @(@{ Tag = 'reader list'; Args = @('list') })
      $res = Invoke-CcRound $lab $jobs
      Confirm-CcRound 'B' $round $res 3
      foreach ($x in $res | Where-Object { $_.Tag -like 'create*' }) {
        $doc = ConvertFrom-CcJson $x.Stdout
        if ($doc -and (Test-CcSuccess $x) -and $doc['state'] -ceq 'ready') {
          $readyIds.Add([string]$doc['id'])
          $dir = Get-CcLeaseDir $lab ([string]$doc['id'])
          if (-not (Test-Path -LiteralPath (Join-Path $dir 'id_ed25519-cert.pub')) -or -not (Test-Path -LiteralPath (Get-CcAliasPath $lab ([string]$doc['ssh']['host_alias'])))) {
            $problems.Files.Add("B round $round`: lease $(([string]$doc['id']).Substring(0, 8)) lacks its certificate or alias file.")
          }
        } else { $problems.Files.Add("B round $round`: a create did not print a ready lease.") }
      }
      $reader = $res | Where-Object { $_.Tag -eq 'reader list' }
      $ldoc = ConvertFrom-CcJson $reader.Stdout
      if (-not (Test-CcSuccess $reader 'lease-list.output.schema.json')) { $problems.Reader.Add("B round $round`: $(Format-CcRun $reader)") }
      elseif (@($ldoc['leases']).Count -lt $countBefore -or @($ldoc['leases']).Count -gt $countBefore + 3) { $problems.Reader.Add("B round $round`: list showed $(@($ldoc['leases']).Count) leases.") }
    }
    $aliasFiles = Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')
    $configBytes = if (Test-Path -LiteralPath (Join-Path $lab.SshDir 'config')) { [IO.File]::ReadAllBytes((Join-Path $lab.SshDir 'config')) } else { $null }
    Test-Cc "$conc`: B. Three ready creates at once, $roundsB rounds: every create reports a valid lease, and each lease is in the state file exactly once" {
      if ($problems.Report.Count) { return $problems.Report[0] }
      if ($problems.Valid.Count) { return $problems.Valid[0] }
      if ($problems.Records.Count) { return $problems.Records[0] }
      $true
    }
    Test-Cc "$conc`: B. The processes of every round overlapped in time (own clock), and a reader at the same time got a whole list" {
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Overlap.Count) { return $problems.Overlap[0] }
      if ($problems.Reader.Count) { return $problems.Reader[0] }
      $true
    }
    Test-Cc "$conc`: B. Each lease has its own certificate and alias file, agentspace.d holds one alias file per lease, and the Include line is added once (state.md#the-include-line)" {
      if ($problems.Files.Count) { return $problems.Files[0] }
      if ($readyIds.Count -ne 3 * $roundsB) { return "$($readyIds.Count) ready leases were made." }
      if ($aliasFiles.Count -ne $readyIds.Count) { return "agentspace.d holds $($aliasFiles.Count) files for $($readyIds.Count) leases." }
      $cfg = if ($configBytes) { [Text.UTF8Encoding]::new($false).GetString($configBytes) } else { '' }
      if ($cfg -cne "Include agentspace.d/*`n") { return 'The config is not exactly one Include line.' }
      $true
    }

    # ---- C. Renews of different leases at once ---------------------------------------------------------
    foreach ($k in $problems.Keys) { $problems[$k].Clear() }
    $pool = @($readyIds | Select-Object -First 4)
    $havePool = $pool.Count -ge 4
    $noPool = 'Setup: fewer than four ready leases were made in phase B.'
    $roundsC = 3
    for ($round = 1; $havePool -and $round -le $roundsC; $round++) {
      $ttls = @{}
      $jobs = for ($i = 0; $i -lt $pool.Count; $i++) {
        $ttls[$pool[$i]] = 3600 + 600 * $i + 60 * $round
        @{ Tag = "renew $i"; Args = @('renew', $pool[$i], '--ttl', "$($ttls[$pool[$i]])s") }
      }
      $stateBefore = Get-CcRecordsById $lab
      $res = Invoke-CcRound $lab @($jobs)
      if (-not (Test-CcOverlap $res)) { $problems.Overlap.Add("C round $round`: the processes did not all overlap.") }
      if (-not (Test-CcStateFile $lab)) { $problems.Valid.Add("C round $round`: the state file does not validate.") }
      $stateAfter = Get-CcRecordsById $lab
      if ($stateAfter.Count -ne $stateBefore.Count) { $problems.Records.Add("C round $round`: the state file holds $($stateAfter.Count) records, and held $($stateBefore.Count).") }
      foreach ($x in $res) {
        $doc = ConvertFrom-CcJson $x.Stdout
        if (-not (Test-CcSuccess $x)) { $problems.Report.Add("C round $round $($x.Tag): $(Format-CcRun $x)"); continue }
        $rec = $stateAfter[[string]$doc['id']]
        if (-not $rec) { $problems.Records.Add("C round $round`: lease $($doc['id'].Substring(0, 8)) is gone from the state file."); continue }
        if ($rec['expires_at'] -cne $doc['expires_at'] -or $rec['ttl_seconds'] -ne $doc['ttl_seconds'] -or $doc['ttl_seconds'] -ne $ttls[[string]$doc['id']]) {
          $problems.Renew.Add("C round $round`: lease $($doc['id'].Substring(0, 8)): the record has $($rec['expires_at']) / $($rec['ttl_seconds']) s, the renew printed $($doc['expires_at']) / $($doc['ttl_seconds']) s, and $($ttls[[string]$doc['id']]) s was asked.")
        }
        $c = Get-CcCertificate (Join-Path (Get-CcLeaseDir $lab ([string]$doc['id'])) 'id_ed25519-cert.pub')
        if ((ConvertTo-CcTime $c.To) -cne $rec['expires_at']) { $problems.Renew.Add("C round $round`: lease $($doc['id'].Substring(0, 8)): the certificate's valid-before is not the record's expires_at.") }
        # Nothing but the expiry and the TTL may differ from before the round.
        $was = ConvertTo-CcCanonical $stateBefore[[string]$doc['id']]
        $copy = ConvertFrom-CcJson (ConvertTo-CcCanonical $rec)
        $copy['expires_at'] = $stateBefore[[string]$doc['id']]['expires_at']; $copy['ttl_seconds'] = $stateBefore[[string]$doc['id']]['ttl_seconds']
        if ((ConvertTo-CcCanonical $copy) -cne $was) { $problems.Renew.Add("C round $round`: lease $($doc['id'].Substring(0, 8)): a renew changed more than expires_at and ttl_seconds.") }
      }
    }
    Test-Cc "$conc`: C. Renews of four leases at once, $roundsC rounds: each renew reports a valid lease, and the state file stays valid and whole" {
      if (-not $havePool) { return $noPool }
      foreach ($p in 'Report', 'Valid', 'Records') { if ($problems[$p].Count) { return $problems[$p][0] } }
      $true
    }
    Test-Cc "$conc`: C. Every record, certificate and printed expiry agree after each round, so no renew's update was lost" {
      if (-not $havePool) { return $noPool }
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Renew.Count) { return $problems.Renew[0] }
      $true
    }
    Test-Cc "$conc`: C. The processes of every round overlapped in time (own clock)" {
      if (-not $havePool) { return $noPool }
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Overlap.Count) { return $problems.Overlap[0] }
      $true
    }

    # ---- D. Two renews of one lease at once ---------------------------------------------------------------
    foreach ($k in $problems.Keys) { $problems[$k].Clear() }
    $one = $pool[0]
    $roundsD = 3
    for ($round = 1; $havePool -and $round -le $roundsD; $round++) {
      $a = 3600 + 60 * $round; $b = 7200 + 60 * $round
      $res = Invoke-CcRound $lab @(@{ Tag = 'renew a'; Args = @('renew', $one, '--ttl', "$($a)s") }, @{ Tag = 'renew b'; Args = @('renew', $one, '--ttl', "$($b)s") })
      if (-not (Test-CcOverlap $res)) { $problems.Overlap.Add("D round $round`: the processes did not all overlap.") }
      if (-not (Test-CcStateFile $lab)) { $problems.Valid.Add("D round $round`: the state file does not validate.") }
      $docs = @($res | ForEach-Object { ConvertFrom-CcJson $_.Stdout })
      foreach ($x in $res) { if (-not (Test-CcSuccess $x)) { $problems.Report.Add("D round $round $($x.Tag): $(Format-CcRun $x)") } }
      $rec = (Get-CcRecordsById $lab)[$one]
      $c = Get-CcCertificate (Join-Path (Get-CcLeaseDir $lab $one) 'id_ed25519-cert.pub')
      # The lock makes one renew come last: the record is one printed document, whole, and the
      # certificate on disk is that same renew's.
      $match = @($docs | Where-Object { $_ -and $_['expires_at'] -ceq $rec['expires_at'] -and $_['ttl_seconds'] -eq $rec['ttl_seconds'] })
      if ($match.Count -eq 0) { $problems.Renew.Add("D round $round`: the record ($($rec['expires_at']), $($rec['ttl_seconds']) s) is neither renew's printed lease.") }
      if ((ConvertTo-CcTime $c.To) -cne $rec['expires_at']) { $problems.Renew.Add("D round $round`: the certificate's valid-before is not the record's expires_at.") }
      if (((Get-CcFileNames (Get-CcLeaseDir $lab $one)) -join '|') -cne 'id_ed25519|id_ed25519-cert.pub|id_ed25519.pub') { $problems.Files.Add("D round $round`: the lease folder holds other files.") }
    }
    Test-Cc "$conc`: D. Two renews of one lease at once, $roundsD rounds: both succeed, and the record, the certificate and one printed lease agree" {
      if (-not $havePool) { return $noPool }
      foreach ($p in 'Report', 'Valid', 'Renew', 'Files') { if ($problems[$p].Count) { return $problems[$p][0] } }
      $true
    }
    Test-Cc "$conc`: D. The processes of every round overlapped in time (own clock)" {
      if (-not $havePool) { return $noPool }
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Overlap.Count) { return $problems.Overlap[0] } else { $true }
    }

    # ---- E. Creates and renews at once ------------------------------------------------------------------------
    foreach ($k in $problems.Keys) { $problems[$k].Clear() }
    $roundsE = 2
    for ($round = 1; $havePool -and $round -le $roundsE; $round++) {
      $ttls = @{ $pool[1] = 4000 + 60 * $round; $pool[2] = 5000 + 60 * $round }
      $jobs = @(
        @{ Tag = 'create 1'; Args = @('create', '--project', 'mixa') }, @{ Tag = 'create 2'; Args = @('create', '--project', 'mixb') },
        @{ Tag = 'renew 1'; Args = @('renew', $pool[1], '--ttl', "$($ttls[$pool[1]])s") }, @{ Tag = 'renew 2'; Args = @('renew', $pool[2], '--ttl', "$($ttls[$pool[2]])s") })
      $res = Invoke-CcRound $lab $jobs
      Confirm-CcRound 'E' $round $res 2
      $stateAfter = Get-CcRecordsById $lab
      foreach ($x in $res | Where-Object { $_.Tag -like 'renew*' }) {
        $doc = ConvertFrom-CcJson $x.Stdout
        if (-not (Test-CcSuccess $x)) { $problems.Report.Add("E round $round $($x.Tag): $(Format-CcRun $x)"); continue }
        $rec = $stateAfter[[string]$doc['id']]
        if (-not $rec -or $rec['expires_at'] -cne $doc['expires_at'] -or $rec['ttl_seconds'] -ne $ttls[[string]$doc['id']]) { $problems.Renew.Add("E round $round`: lease $($doc['id'].Substring(0, 8)): the record does not carry the renew's expiry and TTL.") }
      }
    }
    Test-Cc "$conc`: E. Two creates and two renews at once, $roundsE rounds: every create is in the state file exactly once, and every renew's expiry is kept" {
      if (-not $havePool) { return $noPool }
      foreach ($p in 'Report', 'Valid', 'Records', 'Renew') { if ($problems[$p].Count) { return $problems[$p][0] } }
      $true
    }
    Test-Cc "$conc`: E. The processes of every round overlapped in time (own clock)" {
      if (-not $havePool) { return $noPool }
      if ($problems.Report.Count) { return "The commands did not all succeed: $($problems.Report[0])" }
      if ($problems.Overlap.Count) { return $problems.Overlap[0] } else { $true }
    }

    # ---- G. A gated round: the lock held by the check, every command waiting, then all let in ------------
    # state.md, "The lease state lock": a command that cannot open leases.lock waits and tries again. With
    # the lock held, the check starts ready creates, an unsatisfiable create, two renews and a release,
    # waits until every process is running and none has finished, and lets go. Every command MUST have
    # waited (none finished while the lock was held), and then all of them contend at once.
    foreach ($k in $problems.Keys) { $problems[$k].Clear() }
    $haveGate = $readyIds.Count -ge 6
    $noGate = 'Setup: fewer than six ready leases were made in phase B.'
    $gate = $null
    if ($haveGate) {
      $gRenewA = [string]$readyIds[0]; $gRenewB = [string]$readyIds[1]; $gRelease = [string]$readyIds[2]
      $gBefore = Get-CcRecordsById $lab
      $gJobs = @(
        @{ Tag = 'create ready 1'; Args = @('create', '--project', 'gatea'); Token = 'gatea' },
        @{ Tag = 'create ready 2'; Args = @('create', '--project', 'gateb'); Token = 'gateb' },
        @{ Tag = 'create unsatisfiable'; Args = @('create', '--project', 'gatec', '--size', 'l'); Token = 'gatec' },
        @{ Tag = 'renew 1'; Args = @('renew', $gRenewA, '--ttl', '5400s'); Token = $gRenewA },
        @{ Tag = 'renew 2'; Args = @('renew', $gRenewB, '--ttl', '5460s'); Token = $gRenewB },
        @{ Tag = 'release'; Args = @('release', $gRelease); Token = $gRelease })
      $gate = Invoke-CcGatedRound $lab $gJobs
    }
    $gRes = if ($gate) { @($gate.Results) } else { @() }
    $gTtl = @{ 'renew 1' = 5400; 'renew 2' = 5460 }
    Test-Cc "state.md#the-lease-state-lock: G. With the check holding the lock, every command waits: all six were running when it let go, and none had finished" {
      if (-not $haveGate) { return $noGate }
      if (-not $gate.AllSeen) { return "Only $($gate.Seen.Count) of $($gJobs.Count) processes were seen running within 15 seconds." }
      foreach ($x in $gRes) {
        if ($x.Started -ge $gate.ReleasedAt) { return "$($x.Tag) started after the lock was let go." }
        if ($x.Ended -lt $gate.ReleasedAt) { return "$($x.Tag) finished while the check held the lock: $(Format-CcRun $x)" }
      }
      if ($gRes.Count -ne $gJobs.Count) { return "$($gRes.Count) of $($gJobs.Count) commands reported." }
      $true
    }
    $gProblems = [System.Collections.Generic.List[string]]::new()
    $gRecordProblems = [System.Collections.Generic.List[string]]::new()
    if ($haveGate) {
      foreach ($x in $gRes) { if (-not (Test-CcSuccess $x)) { $gProblems.Add("$($x.Tag): $(Format-CcRun $x)") } }
      if (-not (Test-CcStateFile $lab)) { $gProblems.Add('The state file does not validate against its schema.') }
      else {
        $gAfter = Get-CcRecordsById $lab
        $newIds = @($gRes | Where-Object { $_.Tag -like 'create*' -and (Test-CcSuccess $_) } | ForEach-Object { [string](ConvertFrom-CcJson $_.Stdout)['id'] })
        $ids = @((Read-CcState $lab)['leases'] | ForEach-Object { [string]$_['id'] })
        foreach ($id in $newIds) {
          if (@($ids | Where-Object { $_ -ceq $id }).Count -ne 1) { $gRecordProblems.Add("A create reported lease $($id.Substring(0, 8)) and it is not in the state file exactly once.") }
        }
        if ($ids.Count -ne $gBefore.Count + 3) { $gRecordProblems.Add("The state file holds $($ids.Count) records, and held $($gBefore.Count) before three creates.") }
        foreach ($x in $gRes | Where-Object { $_.Tag -like 'renew*' }) {
          $doc = ConvertFrom-CcJson $x.Stdout
          if (-not (Test-CcSuccess $x)) { continue }
          $rec = $gAfter[[string]$doc['id']]
          if (-not $rec) { $gRecordProblems.Add("$($x.Tag): lease $($doc['id'].Substring(0, 8)) is gone from the state file."); continue }
          if ($rec['expires_at'] -cne $doc['expires_at'] -or $rec['ttl_seconds'] -ne $doc['ttl_seconds'] -or $doc['ttl_seconds'] -ne $gTtl[$x.Tag]) {
            $gRecordProblems.Add("$($x.Tag): the record has $($rec['expires_at']) / $($rec['ttl_seconds']) s, the renew printed $($doc['expires_at']) / $($doc['ttl_seconds']) s, and $($gTtl[$x.Tag]) s was asked.")
          }
          $c = Get-CcCertificate (Join-Path (Get-CcLeaseDir $lab ([string]$doc['id'])) 'id_ed25519-cert.pub')
          if ((ConvertTo-CcTime $c.To) -cne $rec['expires_at']) { $gRecordProblems.Add("$($x.Tag): the certificate's valid-before is not the record's expires_at.") }
          $copy = ConvertFrom-CcJson (ConvertTo-CcCanonical $rec)
          $copy['expires_at'] = $gBefore[[string]$doc['id']]['expires_at']; $copy['ttl_seconds'] = $gBefore[[string]$doc['id']]['ttl_seconds']
          if ((ConvertTo-CcCanonical $copy) -cne (ConvertTo-CcCanonical $gBefore[[string]$doc['id']])) { $gRecordProblems.Add("$($x.Tag): a renew changed more than expires_at and ttl_seconds.") }
        }
        $rel = $gAfter[$gRelease]
        if (-not $rel -or $rel['state'] -cne 'released' -or $rel.ContainsKey('ssh')) { $gRecordProblems.Add('The released lease is not recorded as released without ssh.') }
        elseif ((Test-Path -LiteralPath (Get-CcLeaseDir $lab $gRelease)) -or (Test-Path -LiteralPath (Get-CcAliasPath $lab ([string]$gBefore[$gRelease]['ssh']['host_alias'])))) { $gRecordProblems.Add('The released lease still has its folder or its alias file.') }
        foreach ($id in $gBefore.Keys) {
          if ($id -in $gRenewA, $gRenewB, $gRelease) { continue }
          if ((ConvertTo-CcCanonical $gAfter[$id]) -cne (ConvertTo-CcCanonical $gBefore[$id])) { $gRecordProblems.Add("Lease $($id.Substring(0, 8)), which no command named, changed.") }
        }
      }
    }
    Test-Cc "$conc`: G. Let in together after waiting, every command reports a valid success, and the state file validates" {
      if (-not $haveGate) { return $noGate }
      if ($gProblems.Count) { return $gProblems[0] }
      $true
    }
    Test-Cc "$conc`: G. Every created lease is in the state file exactly once, each renew's record, certificate and printed expiry agree, the released lease is released with its files gone, and no other lease changed (no lost update)" {
      if (-not $haveGate) { return $noGate }
      if ($gProblems.Count) { return "The commands did not all succeed: $($gProblems[0])" }
      if ($gRecordProblems.Count) { return $gRecordProblems[0] }
      $true
    }

    # ---- F. The lease state lock (state.md#the-lease-state-lock) --------------------------------------------
    # The check takes the lock as state.md says every implementation does: it opens leases.lock in the
    # state folder with no sharing. While it is held, `get` and `list` MUST NOT wait, and a `create`, a `renew`
    # and a `release` MUST each give up within 30 seconds with `internal`, changing nothing and saying to try
    # again (lease-api.md, "Concurrency and atomicity"). The three run at once, so that the wait is 30 seconds
    # and not 90.
    $lockPath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.StateDir 'leases.lock')
    $someId = $null
    $s0 = Read-CcState $lab
    if ($s0 -and @($s0['leases']).Count -gt 0) { $someId = [string]$s0['leases'][0]['id'] }
    $fRenewId = if ($haveGate) { [string]$readyIds[3] } else { $none }
    $fReleaseId = if ($haveGate) { [string]$readyIds[4] } else { $none }
    $fCertBefore = Get-CcFileHash (Join-Path (Get-CcLeaseDir $lab $fRenewId) 'id_ed25519-cert.pub')
    $fReleaseNames = (Get-CcFileNames (Get-CcLeaseDir $lab $fReleaseId)) -join '|'
    $held = $null
    $listRun = $null; $listSeconds = 99.0; $getRun = $null; $getSeconds = 99.0
    $fRes = @(); $stateHeld = $null; $stateAfterBusy = $null; $issuedHeld = $null; $issuedAfterBusy = $null
    try {
      New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $lab.StateDir) | Out-Null
      $held = [System.IO.FileStream]::new($lockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
      $sw = [System.Diagnostics.Stopwatch]::StartNew()
      $listRun = Invoke-CcLease $lab @('list'); $listSeconds = $sw.Elapsed.TotalSeconds
      $sw.Restart()
      $getRun = Invoke-CcLease $lab @('get', $(if ($someId) { $someId } else { $none })); $getSeconds = $sw.Elapsed.TotalSeconds
      $stateHeld = Get-CcStateCanonical $lab
      $issuedHeld = (Get-CcFileNames (Join-Path $lab.UserProfile '.agentspace\leases')) -join '|'
      $fStarted = [System.Collections.Concurrent.ConcurrentDictionary[string, datetime]]::new()
      $fJob = Start-CcJobs -Lab $lab -Started $fStarted -Jobs @(
        @{ Tag = 'create'; Args = @('create', '--project', 'locked', '--size', 'l') },
        @{ Tag = 'ready create'; Args = @('create', '--project', 'lockedready') },
        @{ Tag = 'renew'; Args = @('renew', $fRenewId) },
        @{ Tag = 'release'; Args = @('release', $fReleaseId) })
      $fRes = @(Receive-Job -Job $fJob -Wait -AutoRemoveJob)
      $stateAfterBusy = Get-CcStateCanonical $lab
      $issuedAfterBusy = (Get-CcFileNames (Join-Path $lab.UserProfile '.agentspace\leases')) -join '|'
    }
    finally { if ($held) { $held.Dispose() } }
    $fBy = @{}; foreach ($x in $fRes) { $fBy[[string]$x.Tag] = $x }
    $fFailsAsRequired = {
      param($Tag)
      $x = $fBy[$Tag]
      if (-not $x) { return "The $Tag did not report." }
      if (-not (Test-CcFailure $x 1 'internal')) { return "Not the expected failure: $(Format-CcRun $x)" }
      $secs = ($x.Ended - $x.Started).TotalSeconds
      if ($secs -gt 45) { return "It waited $([int]$secs) s." }
      # lease-api.md, "Concurrency and atomicity": it MUST say to try again. A build MAY word that as it likes.
      if ((ConvertFrom-CcJson $x.Stderr)['message'] -inotmatch 'try again|retry|again') { return 'The message does not say to try again.' }
      if ($stateAfterBusy -cne $stateHeld -or $issuedAfterBusy -cne $issuedHeld) { return 'The state file or the lease folders changed.' }
      $true
    }
    Test-Cc "$conc`: F. Get and list do not wait for a command that holds the state lock" {
      if (-not $someId) { return 'Setup: there was no lease to get.' }
      if (-not (Test-CcSuccess $listRun 'lease-list.output.schema.json')) { return "List was not a valid success: $(Format-CcRun $listRun)" }
      if (-not (Test-CcSuccess $getRun)) { return "Get was not a valid success: $(Format-CcRun $getRun)" }
      if ($listSeconds -gt 20 -or $getSeconds -gt 20) { return "List took $([int]$listSeconds) s and get $([int]$getSeconds) s while the lock was held." }
      $true
    }
    Test-Cc "$conc`: F. An unsatisfiable create that cannot get the lock in 30 seconds fails with internal, says to try again, and changes nothing" { & $fFailsAsRequired 'create' }
    Test-Cc "$conc`: F. A ready create that cannot get the lock in 30 seconds fails with internal, says to try again, and changes nothing" { & $fFailsAsRequired 'ready create' }
    Test-Cc "$conc`: F. A renew that cannot get the lock in 30 seconds fails with internal, says to try again, and leaves the certificate as it was" {
      if (-not $haveGate) { return $noGate }
      $r = & $fFailsAsRequired 'renew'
      if ($r -ne $true) { return $r }
      if ((Get-CcFileHash (Join-Path (Get-CcLeaseDir $lab $fRenewId) 'id_ed25519-cert.pub')) -ne $fCertBefore) { return 'The certificate changed.' }
      $true
    }
    Test-Cc "$conc`: F. A release that cannot get the lock in 30 seconds fails with internal, says to try again, and leaves the lease's files in place" {
      if (-not $haveGate) { return $noGate }
      $r = & $fFailsAsRequired 'release'
      if ($r -ne $true) { return $r }
      if (((Get-CcFileNames (Get-CcLeaseDir $lab $fReleaseId)) -join '|') -cne $fReleaseNames) { return "The lease folder holds '$((Get-CcFileNames (Get-CcLeaseDir $lab $fReleaseId)) -join '|')'." }
      $true
    }
    $after = Invoke-CcLease $lab @('create', '--project', 'locked', '--size', 'l')
    $afterRenew = Invoke-CcLease $lab @('renew', $fRenewId)
    $afterRelease = Invoke-CcLease $lab @('release', $fReleaseId)
    Test-Cc "$conc`: F. Once the lock is free, the same create, renew and release succeed (a failed command leaves nothing that blocks a later one)" {
      if (-not $haveGate) { return $noGate }
      if (-not (Test-CcSuccess $after)) { return "Create was not a valid success: $(Format-CcRun $after)" }
      if (-not (Test-CcSuccess $afterRenew)) { return "Renew was not a valid success: $(Format-CcRun $afterRenew)" }
      if (-not (Test-CcSuccess $afterRelease)) { return "Release was not a valid success: $(Format-CcRun $afterRelease)" }
      $true
    }
  }
}
catch {
  Write-ConformanceResult -Name 'test-lease-concurrency: no unexpected error' -Status FAIL -Reason ("It threw: " + ($_.Exception.Message -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun
