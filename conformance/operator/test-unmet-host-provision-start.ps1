#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the first steps of the operator's `provision`, and of the steps that fail on the
# host registry (docs/operator.md, "provision", steps 1, 2, 3 and 8; "The host registry, as the operator
# uses it"; "Command line", the host checks; "Which checks show it"), run through the test seam.
# Card #0156 (E0.07-T04-S02).
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `provision`: every case reports a FAIL there.
# A v0.07 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home with the stand-ins of
# conformance/operator/host-provision-common.ps1, and checks for each refusal the exit code, the exact
# message on stderr, an empty stdout, that no `aws`, `ssh`, `scp` or `go` stand-in was started, and
# what was and was not written (the log, the host's folder, the lock, the registry's bytes, the lease
# state file).
#
#   1. The -Host checks that apply to provision, in their order, before anything is read or written.
#   2. Step 1: a project with no record that is not on the project list is refused with the list's
#      path, writing nothing, no log and no host folder included; a project list that is missing,
#      empty or lists others; the normalized name in the message.
#   3. Step 1: a project list that cannot be read, for a project with no record, fails with the
#      list's message, writing nothing (invalid JSON, no `projects`, a name listed twice, a folder).
#   4. Step 2: the log is opened before the lock, and after step 1. A held provision.lock gives one
#      `run failed` line, exit 1, no record and nothing else; a folder at the log's path gives exit 1,
#      the log's message, no record and no lock taken; the log's failure wins over a held lock, and
#      step 1's refusal over both.
#   5. Steps 3 and 8 on the registry: hosts.lock held past its 30 seconds, and a write that fails
#      (hosts.json read-only), each at the start of the run (`run failed`, nothing written) and at its
#      end (`record failed`, the record left as step 3 and step 4 left it: `creating` with the
#      instance ID for a host that was never ready, `ready` for one that was).
#
# SAFETY. See test-unmet-host-provision.ps1. The cases that hold hosts.lock, provision.lock or a read-only
# registry do it through the check's own handles and attributes at a path the assert checked, and release
# them in a finally before the home is removed.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision-start.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#provision'
$cmd = 'operator.md#command-line'
$hr = 'operator.md#the-host-registry-as-the-operator-uses-it'
$leaseId = '11111111-1111-4111-8111-111111111111'
$oldTime = '2026-01-01T00:00:00Z'
$already = "A provisioning run for 'agentspace' is already running."

# What a refusal MUST leave alone: the registry's bytes, the lease state file and the home's folders.
function Add-NothingWrittenProblems($Problems, $H, $S, $HostsBefore, $LeasesBefore, [switch]$NoHostFolder) {
  if (@(Get-HpCalls $S).Count -gt 0) { $Problems.Add('A stand-in aws, ssh, scp or go was started') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $H)) $HostsBefore)) { $Problems.Add('hosts.json was written') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $H)) $LeasesBefore)) { $Problems.Add('The lease state file was written') }
  if ($NoHostFolder -and (Test-Path -LiteralPath (Join-Path $H.UserProfile '.agentspace\hosts'))) { $Problems.Add('A hosts\ folder was made') }
  if (@(Get-HpOperatorFolders $H).Count -ne 0) { $Problems.Add('An operator\<hex> folder was made') }
}
function New-StartHome($H, $S, [string]$HostsText = (Get-HpHostsText -BoxId $script:HpBoxId)) {
  Initialize-HpHome -H $H -HostsText $HostsText
  Set-HpLeases $H @((New-HpLeaseRecord -Id $leaseId))
  Set-HpScenario -H $H -S $S -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace'); rules = @((Get-HpBootstrapRule)) }
}

# ---- 1. The -Host checks that apply to provision -----------------------------------------------------------
Invoke-HpInHome -Name "${cmd}: provision's host checks" -Body {
  param($h, $s)
  New-StartHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  $cases = @(
    @{ A = @('provision'); M = 'agentspace: provision needs -Host <project>.'; W = 'provision with no -Host' },
    @{ A = @('provision', '-All'); M = 'agentspace: -All applies to status, down and sync alone.'; W = 'provision -All, which check 2 refuses before check 4' },
    @{ A = @('provision', '-Host', 'agentspace', '-All'); M = 'agentspace: -All applies to status, down and sync alone.'; W = 'provision -Host with -All, which check 2 refuses before check 3' },
    @{ A = @('provision', '-Host', 'agentspace', '-InstanceType', 'm9g.xlarge'); M = 'agentspace: -InstanceType applies to deploy alone.'; W = 'provision -InstanceType' },
    @{ A = @('provision', '-Host', 'bad!'); M = 'agentspace: -Host MUST be a project name of 1 to 32 letters, digits and ''-'' that starts and ends with a letter or digit, not ''bad!''.'; W = 'provision -Host with a value that is not a project name' }
  )
  foreach ($c in $cases) {
    $r = Invoke-HpOperator -H $h -S $s -Arguments $c.A
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($r.Stderr -cne "$($c.M)`n") { $p.Add('Stderr is not exactly the spec''s message') }
    if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
    Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore -NoHostFolder
    Write-HpProblems "${cmd}: $($c.W) fails with the spec's message, exit 1, and starts, reads and writes nothing (the host checks, in their order)" $p
  }
}

# ---- 2. Step 1: an unlisted project with no record -------------------------------------------------------
foreach ($c in @(
    @{ N = 'a project list that lists other projects'; Text = '{"version":0,"projects":[{"name":"other","approved":"2026-11-20"}]}' + "`n" },
    @{ N = 'a project list that lists none'; Text = '{"version":0,"projects":[]}' + "`n" },
    @{ N = 'no project list at all'; Text = $null })) {
  $c2 = $c
  Invoke-HpInHome -Name "${sec}: a project with no record that is not listed, $($c2.N)" -Body {
    param($h, $s)
    New-StartHome $h $s
    $projectsFile = Join-Path $h.UserProfile '.agentspace\projects.json'
    if ($null -eq $c2.Text) { Remove-HpPath -H $h -Path $projectsFile } else { Write-HpFile -H $h -Path $projectsFile -Text $c2.Text }
    $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'Zebra')
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $m = [regex]::Match($r.Stderr, '^agentspace: Project ''zebra'' has no host and is not in (.+), so no host is made for it\.\n$')
    if (-not $m.Success) { $p.Add('Stderr is not agentspace: Project ''zebra'' has no host and is not in <path>, so no host is made for it.') }
    elseif (-not (Test-HpSamePath $m.Groups[1].Value $projectsFile)) { $p.Add('The path in the message is not the project list''s full path') }
    if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
    Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore -NoHostFolder
    Write-HpProblems "${sec}: $($c2.N): a project with no record that is not listed is refused with the list's path, the normalized name, exit 1, writing nothing, no log and no host folder included (step 1; D54)" $p
  }
}

# ---- 3. Step 1: a project list that cannot be read --------------------------------------------------------------
foreach ($c in @(
    @{ N = 'is not JSON'; Text = 'not json {' },
    @{ N = 'has no projects member'; Text = '{"version":0}' },
    @{ N = 'lists one name twice'; Text = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"},{"name":"agentspace","approved":"2026-11-21"}]}' },
    @{ N = 'is a folder'; Text = $null })) {
  $c2 = $c
  Invoke-HpInHome -Name "${sec}: a project list that $($c2.N), for a project with no record" -Body {
    param($h, $s)
    New-StartHome $h $s
    $projectsFile = Join-Path $h.UserProfile '.agentspace\projects.json'
    if ($null -eq $c2.Text) { Remove-HpPath -H $h -Path $projectsFile; New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $projectsFile) | Out-Null }
    else { Write-HpFile -H $h -Path $projectsFile -Text $c2.Text }
    $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    $m = [regex]::Match($r.Stderr, '^agentspace: The project list (.+) cannot be read: (.+)\.\n$')
    if (-not $m.Success) { $p.Add('Stderr is not agentspace: The project list <path> cannot be read: <reason>.') }
    else {
      if (-not (Test-HpSamePath $m.Groups[1].Value $projectsFile)) { $p.Add('The path in the message is not the project list''s full path') }
      if ($m.Groups[2].Value.Trim() -eq '') { $p.Add('The message gives no reason') }
    }
    if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
    Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore -NoHostFolder
    Write-HpProblems "${sec}: a project list that $($c2.N) fails provision for a project with no record with the list's message, exit 1, writing nothing (step 1; The host registry, as the operator uses it)" $p
  }
}

# ---- 4. Step 2: the log, then the lock ---------------------------------------------------------------------
foreach ($c in @(
    @{ N = 'no record'; Hosts = $null },
    @{ N = 'a creating record'; Hosts = (Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'creating' '' $oldTime))) })) {
  $c2 = $c
  Invoke-HpInHome -Name "${sec}: a held provision.lock, with $($c2.N)" -Body {
    param($h, $s)
    if ($c2.Hosts) { New-StartHome $h $s $c2.Hosts } else { New-StartHome $h $s }
    $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    $held = Hold-HpLock -H $h -Path (Get-HpLockFile $h 'agentspace')
    try { $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') } finally { $held.Dispose() }
    $log = Read-HpLog $h 'agentspace'
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($r.Stderr -cne "agentspace: $already`n") { $p.Add('Stderr is not exactly agentspace: A provisioning run for ''<project>'' is already running.') }
    if ($log.Lines.Count -ne 1) { $p.Add("The log holds $($log.Lines.Count) lines, not the one run failed line") }
    elseif (-not $log.Lines[0].Ok -or $log.Lines[0].Step -ne 'run' -or $log.Lines[0].Outcome -ne 'failed' -or $log.Lines[0].Rest -cne " reason=$already") { $p.Add('The one log line is not <time> run failed reason=A provisioning run for ''<project>'' is already running.') }
    Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore
    Write-HpProblems "${sec}: a provision.lock another run holds fails the run: one run failed line in the log with no run begun, the message, exit 1, no record and nothing else written, with $($c2.N) (step 2; P18)" $p
  }
}

Invoke-HpInHome -Name "${sec}: a folder at the log's path" -Body {
  param($h, $s)
  New-StartHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  $logPath = Get-HpLogFile $h 'agentspace'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $logPath) | Out-Null
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  $m = [regex]::Match($r.Stderr, '^agentspace: Cannot open the provisioning log (.+): (.+)\.\n$')
  if (-not $m.Success) { $p.Add('Stderr is not agentspace: Cannot open the provisioning log <path>: <error>.') }
  elseif (-not (Test-HpSamePath $m.Groups[1].Value $logPath)) { $p.Add('The path in the message is not the log''s full path') }
  if (Test-Path -LiteralPath (Get-HpLockFile $h 'agentspace')) { $p.Add('provision.lock was made: no lock is taken when the log cannot be opened') }
  if (-not (Test-Path -LiteralPath $logPath -PathType Container)) { $p.Add('The folder at the log''s path was removed or replaced') }
  Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore
  Write-HpProblems "${sec}: a run that cannot open its log (a folder at its path) exits 1 and writes nothing else: the message with the path, no record, no lock taken, no file (step 2; P19)" $p
}

Invoke-HpInHome -Name "${sec}: the order of step 1, the log and the lock" -Body {
  param($h, $s)
  New-StartHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  # The log is opened before the lock: a folder at the log's path and a held lock give the log's message.
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Get-HpLogFile $h 'agentspace')) | Out-Null
  $held = Hold-HpLock -H $h -Path (Get-HpLockFile $h 'agentspace')
  try { $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') } finally { $held.Dispose() }
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if ($r.Stderr -notmatch '^agentspace: Cannot open the provisioning log ') { $p.Add('With a log that cannot be opened and a lock held, the message is not the log''s: the log is opened first') }
  Write-HpProblems "${sec}: the log is opened before the lock is taken: a log that cannot be opened is reported before a held lock (step 2)" $p

  # Step 1 comes before the log: an unlisted project is refused though its log's path is a folder.
  $zebraLog = Get-HpLogFile $h 'zebra'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $zebraLog) | Out-Null
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'zebra')
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if ($r.Stderr -notmatch '^agentspace: Project ''zebra'' has no host and is not in ') { $p.Add('The message is not the refusal of an unlisted project: step 1 comes before the log') }
  if (Test-Path -LiteralPath (Get-HpLockFile $h 'zebra')) { $p.Add('A lock was made for the refused project') }
  if (-not (Test-Path -LiteralPath $zebraLog -PathType Container)) { $p.Add('The folder at the log''s path was removed or replaced') }
  Write-HpProblems "${sec}: the project list is checked before the log is opened: an unlisted project is refused, its log never touched and no lock made (step 1, then step 2)" $p
}

# ---- 5. Steps 3 and 8 on the registry --------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: hosts.lock held past its 30 seconds at step 3" -Body {
  param($h, $s)
  New-StartHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  $lockPath = Join-Path $h.UserProfile '.agentspace\hosts.lock'
  $held = Hold-HpLock -H $h -Path $lockPath
  $clock = [Diagnostics.Stopwatch]::StartNew()
  try { $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') } finally { $held.Dispose() }
  $clock.Stop()
  $log = Read-HpLog $h 'agentspace'
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  $m = [regex]::Match($r.Stderr, '^agentspace: The host registry''s lock (.+) is held by another program; try again\.\n$')
  if (-not $m.Success) { $p.Add('Stderr is not agentspace: The host registry''s lock <path> is held by another program; try again.') }
  elseif (-not (Test-HpSamePath $m.Groups[1].Value $lockPath)) { $p.Add('The path in the message is not hosts.lock''s full path') }
  if ($clock.Elapsed.TotalSeconds -lt 25) { $p.Add('It gave up before the lock''s 30 seconds') }
  if ($log.Lines.Count -ne 1 -or -not $log.Lines[0].Ok -or $log.Lines[0].Step -ne 'run' -or $log.Lines[0].Outcome -ne 'failed' -or $log.Lines[0].Rest -notmatch '^ reason=The host registry''s lock .+ is held by another program; try again\.$') { $p.Add('The log is not the one line run failed reason=<the registry''s message>') }
  if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run') }
  Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore
  Write-HpProblems "${sec}: a hosts.lock held past its 30 seconds fails step 3: run failed with the registry's message in the log, the message, exit 1, no record, nothing else written, provision.lock released (step 3; The host registry, as the operator uses it)" $p
}

Invoke-HpInHome -Name "${sec}: a registry write that fails at step 3" -Body {
  param($h, $s)
  New-StartHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  $file = Get-HpHostsFile $h
  [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::ReadOnly)
  try { $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') }
  finally { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::Normal) }
  $log = Read-HpLog $h 'agentspace'
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  $m = [regex]::Match($r.Stderr, '^agentspace: Cannot write (.+)\.\n$')
  if (-not $m.Success) { $p.Add('Stderr is not agentspace: Cannot write <path>.') }
  elseif (-not (Test-HpSamePath $m.Groups[1].Value $file)) { $p.Add('The path in the message is not the registry''s full path') }
  if ($log.Lines.Count -ne 1 -or $log.Lines[0].Step -ne 'run' -or $log.Lines[0].Outcome -ne 'failed' -or $log.Lines[0].Rest -notmatch '^ reason=Cannot write .+\.$') { $p.Add('The log is not the one line run failed reason=Cannot write <path>.') }
  Add-NothingWrittenProblems $p $h $s $hostsBefore $leasesBefore
  Write-HpProblems "${sec}: a registry write that fails at step 3 (hosts.json read-only) logs run failed with the message, prints it, exits 1, and leaves the record as it was (step 3)" $p
}

# Step 8: the run reaches bootstrap's ssh, which stops at a hook; the check then breaks the registry and
# lets the run go on to record its outcome.
foreach ($c in @(
    @{ N = 'hosts.lock held past its 30 seconds'; Seed = 'creating'; How = 'lock' },
    @{ N = 'hosts.lock held past its 30 seconds, for a ready host'; Seed = 'ready'; How = 'lock' },
    @{ N = 'a registry write that fails'; Seed = 'creating'; How = 'readonly' })) {
  $c2 = $c
  Invoke-HpInHome -Name "${sec}: step 8, $($c2.N)" -Body {
    param($h, $s)
    $ready = $c2.Seed -eq 'ready'
    $rec = if ($ready) { New-HpRecord 'agentspace' 'ready' $script:HpHostId $oldTime } else { New-HpRecord 'agentspace' 'creating' '' $oldTime }
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @($rec))
    Set-HpLeases $h @((New-HpLeaseRecord -Id $leaseId))
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    $aws = if ($ready) { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists } else { Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId }
    Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = $aws; rules = @(@{ kind = 'ssh'; class = 'bootstrap'; hook = 'boot' }) }
    $file = Get-HpHostsFile $h
    $lockPath = Join-Path $h.UserProfile '.agentspace\hosts.lock'
    $job = Start-HpOperatorJob -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $held = $null
    $reached = Wait-HpHook -S $s -Name 'boot' -Job $job
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
      if ($reached) {
        if ($c2.How -eq 'lock') { $held = Hold-HpLock -H $h -Path $lockPath }
        else { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::ReadOnly) }
        Release-HpHook -H $h -S $s -Name 'boot'
      }
      $r = Wait-HpOperatorJob $job
    } finally {
      if ($held) { $held.Dispose() }
      if (Test-Path -LiteralPath $file -PathType Leaf) { [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $file), [IO.FileAttributes]::Normal) }
    }
    $clock.Stop()
    $log = Read-HpLog $h 'agentspace'
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    if (-not $reached) { $p.Add('The run never reached bootstrap, so step 8 was not reached') }
    Add-HpRunProblems $p $r 1
    if ($c2.How -eq 'lock') {
      if ($lines.Count -eq 0 -or $lines[-1] -notmatch '^agentspace: The host registry''s lock .+ is held by another program; try again\.$') { $p.Add('The last line of stderr is not the registry''s lock message') }
      if ($clock.Elapsed.TotalSeconds -lt 25) { $p.Add('It gave up before the lock''s 30 seconds') }
    } else {
      if ($lines.Count -eq 0 -or $lines[-1] -notmatch '^agentspace: Cannot write .+\.$') { $p.Add('The last line of stderr is not agentspace: Cannot write <path>.') }
    }
    $shape = Get-HpLogShape $log
    if ($shape -cne 'run begun,deploy done,start done,sync done,bootstrap done,record failed') { $p.Add('The log is not run begun, deploy, start, sync and bootstrap done, then record failed, with no place line and no run done') }
    elseif ($log.Lines[-1].Rest -notmatch '^ reason=(The host registry''s lock .+ is held by another program; try again\.|Cannot write .+\.)$') { $p.Add('The record failed line''s reason is not the registry''s message') }
    $fin = Get-HpRecord $h 'agentspace'
    $wantState = if ($ready) { 'ready' } else { 'creating' }
    if ($null -eq $fin -or $fin['state'] -cne $wantState -or $fin['instance_id'] -cne $script:HpHostId) { $p.Add("The record is not left as step 3 and step 4 left it: $wantState with the instance ID") }
    elseif (-not (Test-HpTimeBetween $fin['last_attempt_at'] ([DateTime]::UtcNow.AddMinutes(-10)) ([DateTime]::UtcNow))) { $p.Add('last_attempt_at is not the run''s start') }
    if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run') }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $h)) $leasesBefore)) { $p.Add('The lease state file changed: place_pending ran') }
    Write-HpProblems "${sec}: step 8 with $($c2.N): record failed with the registry's message in the log, the message printed, exit 1, the record left $wantState with its instance ID, no place line, provision.lock released (step 8)" $p
  }
}

Exit-ConformanceRun
