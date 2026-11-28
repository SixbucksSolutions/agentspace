# The drafter's CLEAN dry run of E0.12-T05-S02's step scripts (drafter-owned): every step in order, in one conformance-harness home, against
# the stand-ins' clean answers, each through scripts\live\wrap.ps1; then the clean variants (both answers of measure (b), winget not answering,
# an inconclusive measure (c)) and the real S01 build's `lease list` under step 302. Every wrapper MUST exit 0. Run it through scripts\live\wrap.ps1 too.
# RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-dry-lib.ps1')
$lab = New-DLab
$clean0 = 0
try {
  Write-Host "Lab ready; CaReady: $($lab.CaReady)"
  function Run-Clean($Sc, [string]$Step, [string[]]$Must = @(), [string[]]$MustNot = @(), [string]$Tag = 'clean', [switch]$RealBuild) {
    $r = Invoke-DStep -Scenario $Sc -Step $Step -Tag $Tag -RealBuild:$RealBuild
    Write-DVerdict -Case "$Tag $Step" -R $r -ExitWant 0 -Must (@('PASS: every check of this step held.') + $Must) -MustNot (@('FINDING:', 'STOP:', 'EC2AMAZ', 'dryrun-token', '123456789012', 'AIDAEXAMPLE') + $MustNot)
    if ($r.Exit -eq 0) { $script:clean0++ }
  }
  $sc = New-DScenario -Lab $lab -Name 'clean'
  Run-Clean $sc '300' @('mask-proof exit: 0', 'COMPUTERNAME removed: True')
  Run-Clean $sc '301' @('aws sts get-caller-identity exit: 0') @('arn:aws')
  Run-Clean $sc '302' @('Host agentspace.linux-amd64: state stopped', 'Total leases: 2', 'Leases by state: released=2', '<instance-id>') @('i-0aaaa')
  Run-Clean $sc '310' @('Command: agentspace lease create --platform windows/amd64 --project agentspace --ttl 4h', "ttl_seconds: '14400'", 'provision.log lines before the create: 2') @('3f2a9c1e')
  Run-Clean $sc '311' @('Ready at read 3', 'run done: ', 'Phase times', 'sync done took') @('3f2a9c1e')
  Run-Clean $sc '312' @('ssh.host_alias present: True', 'The alias has the form') @('agentspace-agentspace-3f2a9c1e')
  Run-Clean $sc '313' @('<operator alias> ssh -G exit: 0; a ProxyCommand line exists: True', '<alias> ssh -G exit: 0; a ProxyCommand line exists: True', '<operator alias> ssh true exit: 0', '<alias> ssh true exit: 0') @('SECRETPROXYPATH', 'SECRETKEYPATH', 'user agent', 'hostname', 'agentspace-agentspace-3f2a9c1e')
  Run-Clean $sc '314' @('tool git: found True', 'tool claude: found True', 'localswim --help exited 0', 'DefaultShell is Git Bash', "provision.log lines holding 'Installed:': 0", 'NOTE: a detached run copies no program output') @('SECRET')
  Run-Clean $sc '316' @('The install printed its step 10 line', '<fingerprint>') @('SHA256:abcdef')
  Run-Clean $sc '315a' @('Run 1 took 500 s', 'holds exactly one Tools: version-summary line')
  Run-Clean $sc '315c' @('RESULT (c): bot.env was created before Git Bash existed')
  Run-Clean $sc '315d' @('RESULT (d): winget answers in a key-based ssh session of agent: True', 'column of The tools taken by Git for Windows (inferred): winget', 'column of The tools taken by Claude Code (inferred): own installer')
  Run-Clean $sc '315f' @('Checks by kind: SSH session open 3; Session Manager session open 0; Patching 1; CPU work 0; Idle for 2', 'the most: 2; the limit is 6')
  Run-Clean $sc '315g' @('event 6005: 1 time(s)', 'WindowsUpdateClient events since boot: 2')
  Run-Clean $sc '315h' @('sync done came', 'the wait was answered')
  Run-Clean $sc '315i' @('install-3: True', 'localswim-4: True', 'bootstrap-Claude Code: True', 'winget as SYSTEM:', 'The trigger repeats every 5 minutes', 'Every finished patch run holds exactly one')
  Run-Clean $sc '320' @('icacls claude: <win-host>\agent', 'hosts.yml exists (gh fell back to its file)')
  Run-Clean $sc '330' @('Comments on #0267: 2', 'Comments holding the fixed text: 1', 'LOCALSWIM_TOKEN was cleared') @('S02 DRY FIXED TEXT')
  Run-Clean $sc '340' @('The answer is released.', 'The release''s end time is recorded for step 341.')
  # The guard's stop, written after the release by the driver, from the release's own end time.
  $rel = [DateTime]::ParseExact(([IO.File]::ReadAllText((Join-Path $sc.State 'release-end.txt'))).Trim(), "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
  [IO.File]::AppendAllText($sc.Guard, ((ConvertTo-DUtc $rel.AddMinutes(3)) + ' host=agentspace.windows-amd64 unexplained since=' + (ConvertTo-DUtc $rel.AddMinutes(-1)) + ' stops=' + (ConvertTo-DUtc $rel.AddMinutes(15)) + "`n" + (ConvertTo-DUtc $rel.AddMinutes(17)) + ' host=agentspace.windows-amd64 stopped since=' + (ConvertTo-DUtc $rel.AddMinutes(-1)) + "`n"))
  [IO.File]::AppendAllText((Join-Path $sc.Observe 'edges.log'), ((ConvertTo-DUtc $rel.AddMinutes(18)) + " 9:00:00pm agentspace.windows-amd64 running -> stopped type=m8a.large`n"))
  Run-Clean $sc '341' @('Stop edge seen by the observer:', 'guard.log new line classes: stopped 1; unexplained 1; failed 0; sign-in has expired 0', 'StateReason.Code: Client.UserInitiatedShutdown', 'The host was stopped within D56''s 25 minutes')
  Run-Clean $sc '350' @('Windows host in the registry: True', 'Host agentspace.windows-amd64 is stopped.')

  $calls = Get-DCalls $sc
  Write-Host ("Calls logged: {0}; REFUSED or UNKNOWN: {1}" -f $calls.Count, @($calls | Where-Object { $_ -match 'REFUSED|UNKNOWN' }).Count)
  $script:DCases++
  if (@($calls | Where-Object { $_ -match 'REFUSED|UNKNOWN' }).Count -ne 0) { $script:DBad++; Write-Host 'CASE-BAD calls: a stand-in refused or did not know a call.' } else { Write-Host 'CASE-OK calls: no stand-in refused or failed to know a call.' }
  foreach ($want in 'agentspace lease create platform=windows/amd64 project=agentspace ttl=4h', 'operator status -All', 'operator status -Host agentspace -Platform windows/amd64', 'ssh -G operator', 'ssh -G lease', 'ssh lease shell clearfwd=True', 'ssh lease tool-claude clearfwd=True', 'scp lease .s02-host-314.ps1', 'env Set-LocalswimEnv', 'env Clear-LocalswimEnv', 'aws ec2 describe-instances (stack-filtered, State.Name and StateReason.Code)') {
    $script:DCases++
    if (@($calls | Where-Object { $_ -ceq $want }).Count -ge 1) { Write-Host "CASE-OK calls: has '$want'" } else { $script:DBad++; Write-Host "CASE-BAD calls: lacks '$want'" }
  }
  $script:DCases++
  $sets = @($calls | Where-Object { $_ -ceq 'env Set-LocalswimEnv' }).Count; $clears = @($calls | Where-Object { $_ -ceq 'env Clear-LocalswimEnv' }).Count
  if ($sets -eq 1 -and $clears -eq 1) { Write-Host 'CASE-OK calls: the credential was set once and cleared once.' } else { $script:DBad++; Write-Host "CASE-BAD calls: set $sets, cleared $clears" }
  $script:DCases++
  $op = @($calls | Where-Object { $_ -match '^ssh operator true|^scp operator' }).Count
  Write-Host "Calls by the operator alias in the main run: $(@($calls | Where-Object { $_ -match '^ssh operator' }).Count) ssh, $(@($calls | Where-Object { $_ -match '^scp operator' }).Count) scp (320's host script)"

  # Clean variants.
  $st = @{ 'lease-w.id' = "3f2a9c1e-5b7d-4c8e-9a1b-2d3e4f5a6b7c`n"; 'lease-w.alias' = "agentspace-agentspace-3f2a9c1e`n"; 'provlog-baseline.txt' = "2`n" }
  $provFull = "2026-11-26T20:00:00Z run begun`n2026-11-26T20:30:00Z run done`n2026-11-27T21:45:20Z run begun`n2026-11-27T21:47:40Z deploy done`n2026-11-27T21:49:10Z start done`n2026-11-27T21:59:10Z sync done`n2026-11-27T22:06:00Z bootstrap done`n2026-11-27T22:06:01Z record done`n2026-11-27T22:06:02Z place done placed=1`n2026-11-27T22:06:02Z run done`n"
  $v = New-DScenario -Lab $lab -Name 'clean-315b-survived' -State $st
  Run-Clean $v '315b' @('sshd before: state Running; process ID 1111', 'RESULT (b): a restart of sshd did NOT end the open session', 'The process ID changed') -Tag 'clean2'
  $v = New-DScenario -Lab $lab -Name 'clean-315b-ended' -State $st -Overlay @{ 'ssh\sleep.exit' = '255'; 'ssh\sleep.delay' = '4' }
  Run-Clean $v '315b' @('RESULT (b): a restart of sshd ENDED the open session: exit 255') -Tag 'clean2'
  $v = New-DScenario -Lab $lab -Name 'clean-315d-nowinget' -State $st -Overlay @{ 'ssh\host-315d.out' = "S02 winget_on_path=False`nS02 winget_version_exit=error:Win32Exception`nS02 winget_version=`nS02 winget_log_files=no log folder`nS02 tool_file=PowerShell 7|True|2026-11-27T21:49:50Z`nS02 tool_file=Git for Windows|True|2026-11-27T22:05:30Z`nS02 tool_file=Node|True|2026-11-27T22:02:10Z`nS02 tool_file=uv|True|2026-11-27T22:03:00Z`nS02 tool_file=Claude Code|True|2026-11-27T22:03:40Z`nS02 tool_file=gh|True|2026-11-27T22:04:30Z`n" }
  Run-Clean $v '315d' @('RESULT (d): winget answers in a key-based ssh session of agent: False', 'column of The tools taken by Git for Windows (inferred): own installer (winget does not answer') -Tag 'clean2'
  $v = New-DScenario -Lab $lab -Name 'clean-315c-inconclusive' -State $st -ProvLog $provFull -Overlay @{ 'ssh\host-315c.out' = "S02 botenv_created_utc=2026-11-27T22:30:00Z`nS02 botenv_written_utc=2026-11-27T22:30:00Z`nS02 botenv_lines=2`nS02 botenv_url_line=1`nS02 botenv_token_line=1`nS02 gitbash_created_utc=2026-11-27T22:05:30Z`nS02 defaultshell=C:\Program Files\Git\bin\bash.exe`nS02 timezone=Eastern Standard Time`n" }
  Run-Clean $v '315c' @('RESULT (c): inconclusive.') -Tag 'clean2'
  $v = New-DScenario -Lab $lab -Name 'clean-315h' -State $st -ProvLog $provFull
  Run-Clean $v '315h' @('sync done came', 'the wait was answered') -Tag 'clean2'
  $v = New-DScenario -Lab $lab -Name 'clean-315c-order' -State $st -ProvLog $provFull
  Run-Clean $v '315c' @('RESULT (c): bot.env was created before Git Bash existed', 'sync (and its push) came before the bootstrap') -Tag 'clean2'
  # The real S01 build's `lease list` and the stand-in operator under step 302, in a home with no lease.
  $v = New-DScenario -Lab $lab -Name 'clean-realbuild-302'
  Run-Clean $v '302' @('Total leases: 0', 'Leases by state: none') -Tag 'clean2-real' -RealBuild
} finally {
  Remove-DLab $lab
}
Write-Host "Clean dry run: $($script:DCases) checks, $($script:DBad) not as designed; wrappers that exited 0: $clean0"
exit $(if ($script:DBad -eq 0) { 0 } else { 1 })
