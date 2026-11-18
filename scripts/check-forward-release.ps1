#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Checks that sshd drops a dead client's session within a bound (0.02.T1.1, fixes #0044), and that a
# live idle session survives twice that bound. 0.02.T3.1 reuses this script.
#
#   pwsh -NoProfile -File scripts/check-forward-release.ps1 -Alias <alias> [-BoundSeconds <n>]
#
# A real run SHOULD use the default -BoundSeconds 120, the card's 2 minutes. sshd's own bound is 90 s,
# and a run with -BoundSeconds 90 passed only on the 5 s of sample slack (0.02.T1.1, run 1: 89.9 s).
#
# The caller creates the lease and passes its alias, which MUST be ready. Nothing here creates,
# renews or releases a lease, and nothing here edits ~/.ssh.
#
# Phase 1: starts its own holder, `ssh -N -o ExitOnForwardFailure=yes <alias>`, and waits until the
#   box shows 127.0.0.1:8811 and :53147 held. It kills that holder on the laptop (the holder and its
#   process tree), then watches the box until both ports are free and the holder's sshd-session is
#   gone. It passes when that happens within -BoundSeconds (plus one 5 s sample of slack), and fails
#   when it does not, or when -BoundSeconds plus 60 s pass without it.
# Phase 2: opens an idle `ssh -N -o ClearAllForwardings=yes <alias>`. It passes when, after twice
#   -BoundSeconds, that process is still alive on the laptop and its sshd-session is still on the box.
#
# One watcher connection serves both phases (D14: no repeated logins). It runs a loop on the box that
# prints, every 5 s: the box clock, the lines of `sudo ss -ltnp` for the two ports, and every
# `sshd-session: ubuntu` process with its parent. The script reads those lines asynchronously. The
# watcher carries ClearAllForwardings, so it never holds the ports itself. The tunnel accepts the
# loop's writes after the laptop side is gone, so the loop runs until a deadline computed from
# -BoundSeconds. Its output keeps sshd from probing until then, and sshd then drops the watcher's
# session within its ClientAlive bound. The watcher holds no ports at any time (0.02.T1.1 findings).
#
# Telling sessions apart: the first sample, taken before anything else starts, lists the watcher's own
# sshd-session processes and any others (a desktop-app connection, say). The holder is the pid that
# `ss` names as holding the ports, plus its parent. The idle session is the set of pids that appear
# after it starts and were not in the sample taken just before. Since all of this rests on the
# session lines, the check MUST fail when the first sample shows no sshd-session at all (the watcher's
# own is always there), and when a pid that holds the ports is not among that sample's sessions.
#
# A refused or dropped login fails the check once, and stops. It is never retried (D12, D14).
#
# This script MUST kill only the processes it started: each ssh.exe it launches and that process's
# descendants, found by walking ParentProcessId. A child is adopted only while its parent is still the
# recorded process (present, with the recorded start time), so a reused pid's children are never
# taken. The tree is walked only until the handle is first stopped; a later stop re-kills only the
# recorded pids whose start time still matches. It MUST clean them all up in the finally block, on
# failure and on Ctrl+C too.
#
# Output: one line per observation, starting with a UTC time (ISO 8601, Z). Exit 0 when both phases
# pass, 1 otherwise. It MUST NOT print an instance ID, account ID or token; ssh's stderr is
# redacted before it is shown.
#
# -SshExe, -ExtraWatchSeconds and -SampleSeconds exist for tests with a stand-in ssh. A real run MUST
# leave them at their defaults.

[CmdletBinding()]
param(
  [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')][string]$Alias,
  [ValidateRange(1, 3600)][int]$BoundSeconds = 120,
  [string]$SshExe = 'C:\Windows\System32\OpenSSH\ssh.exe',
  [ValidateRange(0, 3600)][int]$ExtraWatchSeconds = 60,
  [ValidateRange(1, 60)][int]$SampleSeconds = 5
)

$ErrorActionPreference = 'Stop'
$script:Ports = @('8811', '53147')
$script:ConnectWait = 90
$script:Handles = [Collections.Generic.List[object]]::new()
$script:MustLive = [Collections.Generic.List[string]]::new()
$script:Snaps = [Collections.Generic.Queue[object]]::new()
$script:Cur = $null
$script:LastSnap = $null
$script:LastShown = ''
$script:LastShownAt = [DateTime]::MinValue

function Write-Log([string]$Text) {
  $stamp = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
  [Console]::WriteLine("$stamp  $Text")
}

function Hide-Secrets([string]$Text) {
  $Text = $Text -replace 'i-[0-9a-f]{8,17}', '<instance>'
  $Text = $Text -replace 'arn:aws[a-z-]*:\S+', '<arn>'
  $Text -replace '\b\d{12}\b', '<account>'
}

function New-SshHandle([string]$Name, [string[]]$SshArgs) {
  $psi = [Diagnostics.ProcessStartInfo]::new($SshExe)
  foreach ($a in $SshArgs) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $p = [Diagnostics.Process]::Start($psi)
  $h = [pscustomobject]@{
    Name = $Name; Proc = $p; Started = $p.StartTime; OutTask = $p.StandardOutput.ReadLineAsync(); ErrTask = $p.StandardError.ReadLineAsync()
    ErrLines = [Collections.Generic.List[string]]::new(); Tree = @{}; Stopped = $false
  }
  $script:Handles.Add($h)
  $h
}

function Read-DoneTask($Task) {
  # Returns the finished line, or $null at end of stream or on a stream error.
  try { $Task.Result } catch { $null }
}

function Add-WatcherLine([string]$Line) {
  if ($Line -match '^BEGIN (\S+)') {
    $script:Cur = [pscustomobject]@{
      Time = $Matches[1]; Received = [DateTime]::UtcNow; Held = @{}
      Sessions = [Collections.Generic.List[object]]::new(); Errors = [Collections.Generic.List[string]]::new()
    }
  }
  elseif ($null -eq $script:Cur) { return }
  elseif ($Line -match '^L (.*)') {
    $t = $Matches[1]
    if ($t -match '127\.0\.0\.1:(\d+)\s') {
      $port = $Matches[1]
      $script:Cur.Held[$port] = @([regex]::Matches($t, 'pid=(\d+)') | ForEach-Object { [int]$_.Groups[1].Value })
    }
  }
  elseif ($Line -match '^S\s*(\d+)\s+(\d+)\s+(.*)') {
    $script:Cur.Sessions.Add([pscustomobject]@{ Pid = [int]$Matches[1]; Ppid = [int]$Matches[2]; Args = $Matches[3] })
  }
  elseif ($Line -match '^X (.*)') { $script:Cur.Errors.Add($Matches[1]) }
  elseif ($Line -eq 'END') {
    $script:Cur.Received = [DateTime]::UtcNow
    $script:Snaps.Enqueue($script:Cur)
    $script:Cur = $null
  }
}

function Update-Handle($H) {
  while ($H.OutTask -and $H.OutTask.IsCompleted) {
    $line = Read-DoneTask $H.OutTask
    if ($null -eq $line) { $H.OutTask = $null; break }
    if ($H.Name -eq 'watcher') { Add-WatcherLine $line }
    $H.OutTask = $H.Proc.StandardOutput.ReadLineAsync()
  }
  while ($H.ErrTask -and $H.ErrTask.IsCompleted) {
    $line = Read-DoneTask $H.ErrTask
    if ($null -eq $line) { $H.ErrTask = $null; break }
    $H.ErrLines.Add($line)
    $H.ErrTask = $H.Proc.StandardError.ReadLineAsync()
  }
}

function Assert-Alive {
  foreach ($name in $script:MustLive) {
    $h = $script:Handles | Where-Object Name -EQ $name | Select-Object -First 1
    if ($h.Proc.HasExited) {
      Start-Sleep -Milliseconds 300
      Update-Handle $h
      $err = Hide-Secrets (($h.ErrLines | Select-Object -Last 4) -join ' | ')
      throw "The $($h.Name) ssh process ended with exit code $($h.Proc.ExitCode). Its stderr: $err. The login is not retried (D14)."
    }
  }
}

function Show-Snapshot($S) {
  $parts = foreach ($p in $script:Ports) {
    if ($S.Held.ContainsKey($p)) { "port $p held by pid $($S.Held[$p] -join ',')" } else { "port $p free" }
  }
  $pids = ($S.Sessions | ForEach-Object { $_.Pid } | Sort-Object) -join ' '
  $text = "Box sample at $($S.Time): $($parts -join '; '); sshd-session pids: $(if ($pids) { $pids } else { 'none' })"
  $sig = ($text -replace '^Box sample at \S+', '')
  $now = [DateTime]::UtcNow
  if ($sig -ne $script:LastShown -or ($now - $script:LastShownAt).TotalSeconds -ge 30) {
    Write-Log $text
    $script:LastShown = $sig
    $script:LastShownAt = $now
  }
}

function Wait-Snapshot([scriptblock]$Cond, [int]$TimeoutSec, [string]$What) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($true) {
    foreach ($h in $script:Handles) { Update-Handle $h }
    while ($script:Snaps.Count -gt 0) {
      $snap = $script:Snaps.Dequeue()
      $script:LastSnap = $snap
      Show-Snapshot $snap
      if ($snap.Errors.Count) { throw "The box loop reported: $($snap.Errors -join '; ')" }
      if ([bool](& $Cond $snap)) { return $snap }
    }
    Assert-Alive
    if ($sw.Elapsed.TotalSeconds -ge $TimeoutSec) { throw "Timed out after $TimeoutSec s waiting for: $What." }
    Start-Sleep -Milliseconds 200
  }
}

function Update-Tree($H) {
  # Adds the live descendants of everything already tracked for this handle (and of its root) to its
  # tree. A child is adopted only while its parent is still the recorded process: present in the
  # current list with the recorded creation date. A recorded pid that has died and been reused by
  # another process therefore never brings that process's children in.
  $all = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CreationDate)
  $created = @{}
  foreach ($p in $all) { $created[[int]$p.ProcessId] = $p.CreationDate }
  $root = $H.Proc.Id
  if (-not $H.Tree.ContainsKey($root) -and $created.ContainsKey($root) -and
    [Math]::Abs(($created[$root] - $H.Started).TotalSeconds) -lt 2) { $H.Tree[$root] = $created[$root] }
  $grew = $true
  while ($grew) {
    $grew = $false
    foreach ($p in $all) {
      $id = [int]$p.ProcessId
      $parent = [int]$p.ParentProcessId
      if ($H.Tree.ContainsKey($id) -or -not $H.Tree.ContainsKey($parent)) { continue }
      # The parent's pid now belongs to another process, or the parent is gone.
      if (-not $created.ContainsKey($parent) -or $created[$parent] -ne $H.Tree[$parent]) { continue }
      # A child is never older than its parent; an older one means the parent's pid was reused.
      if ($p.CreationDate -lt $H.Tree[$parent]) { continue }
      if ($id -eq $PID) { continue }
      $H.Tree[$id] = $p.CreationDate
      $grew = $true
    }
  }
}

function Stop-Handle($H) {
  # The tree is walked only before the first stop. After it, its pids are dead and MAY be reused, so a
  # second stop re-kills only the recorded pids whose start time still matches.
  if (-not $H.Stopped) { try { Update-Tree $H } catch { } }
  foreach ($id in @($H.Tree.Keys)) {
    if ($id -eq $PID) { continue }
    try {
      $proc = Get-Process -Id $id -ErrorAction Stop
      # Only the process that was in the tree: a reused pid has another start time.
      if ([Math]::Abs(($proc.StartTime - $H.Tree[$id]).TotalSeconds) -lt 2) { $proc.Kill() }
    }
    catch { }
  }
  try { if (-not $H.Proc.HasExited) { $H.Proc.Kill() } } catch { }
  $H.Stopped = $true
}

function New-BoxLoop([int]$MaxSeconds) {
  $loop = @'
MAX=__MAX__
while [ "$SECONDS" -lt "$MAX" ]; do
  echo "BEGIN $(date -u +%FT%TZ)"
  if o=$(sudo -n ss -ltnp 2>&1); then
    printf '%s\n' "$o" | grep -E '127\.0\.0\.1:(8811|53147)[[:space:]]' | sed 's/^/L /'
  else
    echo "X ss failed"
  fi
  ps -eo pid=,ppid=,args= | grep '[s]shd-session: ubuntu' | sed 's/^ */S /'
  echo END
  sleep __SAMPLE__
done
'@
  $loop = $loop.Replace('__MAX__', "$MaxSeconds").Replace('__SAMPLE__', "$SampleSeconds").Replace("`r`n", "`n")
  $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($loop))
  # No quotes in the command, so the laptop's and the box's shells have nothing to mangle.
  "echo $b64 | base64 -d | bash"
}

$code = 1
$boxLoopStarted = $false
try {
  $bound = $BoundSeconds
  $limit = $bound + $ExtraWatchSeconds
  $maxBox = 3 * $bound + $ExtraWatchSeconds + 2 * $script:ConnectWait + 120
  Write-Log "Check started: alias $Alias, bound $bound s, idle hold $(2 * $bound) s."

  # The watcher.
  $watcher = New-SshHandle 'watcher' @('-T', '-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=30',
    '-o', 'ServerAliveInterval=15', $Alias, (New-BoxLoop $maxBox))
  $script:MustLive.Add('watcher')
  $first = Wait-Snapshot { param($s) $true } $script:ConnectWait 'the watcher connection to send its first sample'
  $boxLoopStarted = $true
  if ($first.Held.Count) { throw "The ports were already held before the check started ($(@($first.Held.Keys) -join ', ')); free them first." }
  if (-not $first.Sessions.Count) {
    throw 'The first box sample showed no sshd-session, not even the watcher''s own, so the check cannot tell sessions apart.'
  }
  $baseline = @($first.Sessions | ForEach-Object { $_.Pid })
  Write-Log "Watcher connected; it and other logins show sshd-session pids: $($baseline -join ' ')."

  # Phase 1.
  Write-Log 'Phase 1 started: a holder with the forwards, then killed on the laptop.'
  $holder = New-SshHandle 'holder' @('-N', '-o', 'ExitOnForwardFailure=yes', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=30', $Alias)
  $script:MustLive.Add('holder')
  $held = Wait-Snapshot { param($s) $s.Held.ContainsKey('8811') -and $s.Held.ContainsKey('53147') } $script:ConnectWait 'the holder to hold both ports on the box'
  # A holder that lost its forwards exits under ExitOnForwardFailure; fail with its stderr.
  Assert-Alive
  $holderPids = [Collections.Generic.HashSet[int]]::new()
  foreach ($k in $held.Held.Keys) { foreach ($i in $held.Held[$k]) { [void]$holderPids.Add($i) } }
  $samplePids = @($held.Sessions | ForEach-Object { $_.Pid } | Sort-Object)
  $unseen = @($holderPids | Where-Object { $_ -notin $samplePids } | Sort-Object)
  if (-not $holderPids.Count -or $unseen.Count) {
    throw "The pids holding the ports ($(($holderPids | Sort-Object) -join ' ')) are not all among that sample's sshd-session pids ($($samplePids -join ' ')), so the check cannot tell the holder's session apart."
  }
  # The privileged parent of the process that holds the ports belongs to the holder too.
  foreach ($s in @($held.Sessions)) { if ($holderPids.Contains($s.Pid)) { [void]$holderPids.Add($s.Ppid) } }
  Write-Log "Holder holds 127.0.0.1:8811 and :53147 on the box (sshd-session pids: $(($holderPids | Sort-Object) -join ' '))."

  Update-Tree $holder
  $script:MustLive.Remove('holder') | Out-Null
  $killedAt = [Diagnostics.Stopwatch]::StartNew()
  Stop-Handle $holder
  Write-Log "Holder killed on the laptop ($($holder.Tree.Count) processes in its tree); watching the box for up to $limit s."

  $freed = Wait-Snapshot {
    param($s)
    -not $s.Held.Count -and -not (@($s.Sessions | Where-Object { $holderPids.Contains($_.Pid) }).Count)
  } $limit 'both ports to be free and the holder''s sshd-session to be gone'
  $elapsed = [Math]::Round(($freed.Received - [DateTime]::UtcNow).TotalSeconds + $killedAt.Elapsed.TotalSeconds, 1)
  $allowed = $bound + $SampleSeconds
  if ($elapsed -gt $allowed) {
    throw "Phase 1 failed: the ports were free $elapsed s after the kill, past the bound of $bound s (plus $SampleSeconds s of sampling)."
  }
  Write-Log "Phase 1 passed: both ports free and the holder's sshd-session gone $elapsed s after the kill (bound $bound s)."

  # Phase 2.
  Write-Log "Phase 2 started: an idle session with no forwards, watched for $(2 * $bound) s."
  $before = @($script:LastSnap.Sessions | ForEach-Object { $_.Pid })
  $idle = New-SshHandle 'idle' @('-N', '-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=30', $Alias)
  $script:MustLive.Add('idle')
  $newPids = { param($s) @($s.Sessions | ForEach-Object { $_.Pid } | Where-Object { $_ -notin $before }) }
  [void](Wait-Snapshot { param($s) (& $newPids $s).Count -gt 0 } $script:ConnectWait 'the idle session to show an sshd-session on the box')
  # One more sample, so the session's whole set of processes (privileged parent and child) is in.
  $settled = Wait-Snapshot { param($s) $true } ($SampleSeconds * 3 + 30) 'a second sample of the idle session'
  $idlePids = @(& $newPids $settled)
  if (-not $idlePids.Count) { throw 'The idle session''s sshd-session vanished before it had settled.' }
  Update-Tree $idle
  Write-Log "Idle session is up; its sshd-session pids: $($idlePids -join ' ')."
  $holdTimer = [Diagnostics.Stopwatch]::StartNew()
  $hold = 2 * $bound
  $last = Wait-Snapshot {
    param($s)
    $now = @($s.Sessions | ForEach-Object { $_.Pid })
    $gone = @($idlePids | Where-Object { $_ -notin $now })
    if ($gone.Count) { throw "Phase 2 failed: the idle session's sshd-session (pid $($gone -join ',')) was gone after $([Math]::Round($holdTimer.Elapsed.TotalSeconds, 1)) s." }
    $holdTimer.Elapsed.TotalSeconds -ge $hold
  } ($hold + 60) 'the idle hold time to pass'
  if ($idle.Proc.HasExited) { throw "Phase 2 failed: the idle ssh process ended (exit code $($idle.Proc.ExitCode))." }
  Write-Log "Phase 2 passed: after $([Math]::Round($holdTimer.Elapsed.TotalSeconds, 1)) s the idle session is still connected and its sshd-session (pid $($idlePids -join ',')) is still on the box."

  Write-Log 'Result: Pass.'
  $code = 0
}
catch {
  Write-Log "Failed: $(Hide-Secrets $_.Exception.Message)"
  Write-Log 'Result: Fail.'
  $code = 1
}
finally {
  foreach ($h in @($script:Handles)) {
    try { Stop-Handle $h } catch { }
    try { $h.Proc.StandardInput.Close() } catch { }
  }
  $line = "Cleanup done: $($script:Handles.Count) ssh process trees stopped."
  if ($boxLoopStarted) {
    $line += " The box loop runs until its deadline ($maxBox s after it started), and sshd drops the watcher's session after that."
  }
  Write-Log $line
}
exit $code
