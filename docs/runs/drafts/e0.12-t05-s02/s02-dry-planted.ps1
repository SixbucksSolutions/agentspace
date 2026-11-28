# The drafter's PLANTED dry runs of E0.12-T05-S02's step scripts (drafter-owned): each checker shown to fail on a planted answer, each through
# scripts\live\wrap.ps1 in a conformance-harness home, against the stand-ins. A planted case MUST exit nonzero (the step's own exit: 1 for a finding, 2, 3,
# 5 or 6 where the step defines it) with a FINDING: or STOP: line naming the planted fault. The planted answers are also written, whole, under
# target\t05\planted-s02-<case>\ (a planted copy of an answer is named planted-*, never test-*). -Group A to F picks a part of the list so that no
# run lasts more than a couple of minutes. Run it through scripts\live\wrap.ps1 too. RFC 2119 keywords; the capitals are load-bearing.
param([Parameter(Mandatory)][ValidateSet('A', 'B', 'C', 'D', 'E', 'F')][string]$Group)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-dry-lib.ps1')

function CA([string]$Rel) { return [IO.File]::ReadAllText((Join-Path $script:DAnswers $Rel)) }
function Rep1([string]$Text, [string]$Pattern, [string]$With) { return ([regex]::new($Pattern)).Replace($Text, $With, 1) }
function RepLast([string]$Text, [string]$Literal, [string]$With) { $i = $Text.LastIndexOf($Literal); return $Text.Substring(0, $i) + $With + $Text.Substring($i + $Literal.Length) }
function Doc([string]$State, [bool]$Ssh = $false, [string]$Alias = 'agentspace-agentspace-3f2a9c1e', [string]$User = 'agent', [string]$Platform = 'windows/amd64', [string]$Profile = 'agent-windows', [int]$Ttl = 14400, [string]$Size = 'm', [switch]$SizedFor, [switch]$Released) {
  $d = [ordered]@{ id = '3f2a9c1e-5b7d-4c8e-9a1b-2d3e4f5a6b7c'; project = 'agentspace'; state = $State; platform = $Platform; profile = $Profile; size = $Size }
  if ($SizedFor) { $d['sized_for'] = [ordered]@{ plan = 'v0.12'; turn = 'T05'; agents = 3 } }
  $d['ttl_seconds'] = $Ttl
  $d['created_at'] = '2026-11-27T21:45:15Z'
  if ($State -in 'ready', 'released', 'expired') { $d['expires_at'] = '2026-11-28T01:49:20Z' }
  if ($Released) { $d['released_at'] = '2026-11-27T22:30:00Z' }
  if ($Ssh) { $d['ssh'] = [ordered]@{ host_alias = $Alias; user = $User } }
  return ((ConvertTo-Json -InputObject $d -Compress -Depth 5) + "`n")
}
$idv = "3f2a9c1e-5b7d-4c8e-9a1b-2d3e4f5a6b7c`n"
$S311 = @{ 'lease-w.id' = $idv; 'provlog-baseline.txt' = "2`n" }
$S312 = @{ 'lease-w.id' = $idv; 'lease-w.alias' = "agentspace-agentspace-3f2a9c1e`n"; 'provlog-baseline.txt' = "2`n" }
$provOld = "2026-11-26T20:00:00Z run begun`n2026-11-26T20:30:00Z run done`n"
$provFull = $provOld + "2026-11-27T21:45:20Z run begun`n2026-11-27T21:47:40Z deploy done`n2026-11-27T21:49:10Z start done`n2026-11-27T21:59:10Z sync done`n2026-11-27T22:06:00Z bootstrap done`n2026-11-27T22:06:01Z record done`n2026-11-27T22:06:02Z place done placed=1`n2026-11-27T22:06:02Z run done`n"
$dir = Join-Path $script:DDir ''
$planted = Join-Path $script:DDir 'planted-s02-maskproof-fail.ps1'
Write-DFile $planted "Write-Output 'Mask proof: 4 passed, 1 failed.'`nexit 1`n"

$lab = New-DLab
$script:N = 0
try {
  Write-Host "Lab ready; CaReady: $($lab.CaReady); group $Group"
  # One planted case: its own scenario, the step run through the wrapper, the verdict. The overlay is also kept under target\t05\planted-s02-<case>.
  function P {
    param([string]$Case, [string]$Step, [hashtable]$Overlay = @{}, [hashtable]$State = @{}, [string]$ProvLog, [hashtable]$Env = @{}, [double]$Hours = 1.0, [int]$ExitWant = -1, [string[]]$Must = @(), [string[]]$MustNot = @(), [string[]]$Remove = @(), [string]$GuardLog, [string]$Edges, [switch]$RealBuild, [switch]$Calls0)
    $args2 = @{ Lab = $lab; Name = $Case; Overlay = $Overlay; State = $State; HoursOpen = $Hours; Env = $Env; Remove = $Remove }
    if ($PSBoundParameters.ContainsKey('ProvLog')) { $args2['ProvLog'] = $ProvLog }
    if ($PSBoundParameters.ContainsKey('GuardLog')) { $args2['GuardLog'] = $GuardLog }
    if ($PSBoundParameters.ContainsKey('Edges')) { $args2['Edges'] = $Edges }
    $sc = New-DScenario @args2
    $keep = Join-Path $script:DDir "planted-s02-$Case"
    if (Test-Path -LiteralPath $keep) { Remove-Item -LiteralPath $keep -Recurse -Force }
    foreach ($k in $Overlay.Keys) { Write-DFile (Join-Path $keep $k) ([string]$Overlay[$k]) }
    Write-DFile (Join-Path $keep 'case.txt') ("step $Step`nexit want $ExitWant`nmust: " + ($Must -join ' | ') + "`n")
    $r = Invoke-DStep -Scenario $sc -Step $Step -Tag 'planted' -RealBuild:$RealBuild
    Write-DVerdict -Case "$Case (step $Step)" -R $r -ExitWant $ExitWant -Must $Must -MustNot (@('EC2AMAZ', 'dryrun-token', '123456789012', 'AIDAEXAMPLE', 'SECRETPROXYPATH') + $MustNot)
    if ($Calls0) {
      $script:DCases++
      if (@(Get-DCalls $sc).Count -eq 0) { Write-Host "CASE-OK $Case no tool was called" } else { $script:DBad++; Write-Host "CASE-BAD $Case a tool was called" }
    }
    $script:N++
    return $sc
  }
  function Edge341([double]$MinutesAfter, [DateTime]$Rel) { return ((ConvertTo-DUtc $Rel.AddMinutes($MinutesAfter)) + " 9:00:00pm agentspace.windows-amd64 running -> stopped type=m8a.large`n") }
  function New341([string]$Case, [hashtable]$Overlay = @{}, [double]$EdgeAfter = 18, [string]$GuardNew = $null, [hashtable]$Env = @{}, [double]$Hours = 1.0, [int]$ExitWant = -1, [string[]]$Must = @(), [string[]]$MustNot = @(), [bool]$HasEdge = $true) {
    $rel = [DateTime]::UtcNow.AddMinutes(-40)
    $old = "2026-11-26T21:10:00Z host=agentspace.windows-amd64 stopped since=2026-11-26T20:30:00Z`n2026-11-26T21:11:00Z host=agentspace.linux-amd64 unexplained since=2026-11-26T20:30:00Z stops=2026-11-26T21:15:00Z`n"
    if ($null -eq $GuardNew) { $GuardNew = (ConvertTo-DUtc $rel.AddMinutes(3)) + ' host=agentspace.windows-amd64 unexplained since=' + (ConvertTo-DUtc $rel.AddMinutes(-1)) + ' stops=' + (ConvertTo-DUtc $rel.AddMinutes(15)) + "`n" + (ConvertTo-DUtc $rel.AddMinutes(17)) + ' host=agentspace.windows-amd64 stopped since=' + (ConvertTo-DUtc $rel.AddMinutes(-1)) + "`n" }
    $edges = "2026-11-26T01:00:00Z 8:00:00pm agentspace.windows-amd64 unseen -> stopped type=m8a.large`n"
    if ($HasEdge) { $edges += (Edge341 $EdgeAfter $rel) }
    $state = @{ 'release-end.txt' = ((ConvertTo-DUtc $rel) + "`n"); 'guard-baseline.txt' = "2`n" }
    return (P -Case $Case -Step '341' -Overlay $Overlay -State $state -GuardLog ($old + $GuardNew) -Edges $edges -Env $Env -Hours $Hours -ExitWant $ExitWant -Must $Must -MustNot $MustNot)
  }

  if ($Group -eq 'A') {
    [void](P 'p300-proof-fails' '300' -Env @{ S02_MASKPROOF = $planted } -Must @('FINDING: The mask proof exited 1', 'FINDING: The proof does not report'))
    [void](P 'p301-signin-expired' '301' -Overlay @{ 'aws\sts.exit' = '255' } -Must @('FINDING: The AWS sign-in does not hold (exit 255)', 'Needs Terry'))
    [void](P 'p302-swimcoach-running' '302' -Overlay @{ 'operator\status-all.err' = (Rep1 (CA 'operator\status-all.err') '(?m)^State     stopped' 'State     running') } -Must @('FINDING: Host swimcoach is running, not stopped'))
    [void](P 'p302-windows-running' '302' -Overlay @{ 'operator\status-all.err' = (RepLast (CA 'operator\status-all.err') 'State     stopped' 'State     running') } -Must @('FINDING: Host agentspace.windows-amd64 is running, not stopped'))
    [void](P 'p302-lease-ready' '302' -Overlay @{ 'agentspace\lease-list.out' = ('{"leases":[' + (Doc 'ready' $true).TrimEnd("`n") + ']}' + "`n") } -Must @('FINDING: 1 lease(s) are ready or pending'))
    [void](P 'p302-status-fails' '302' -Overlay @{ 'operator\status-all.exit' = '1' } -Must @('FINDING: status -All exited 1'))
    [void](P 'p302-no-hosts' '302' -Overlay @{ 'operator\status-all.err' = '' } -Must @('FINDING: No Host line was read'))
    [void](P 'p302-list-fails' '302' -Overlay @{ 'agentspace\lease-list.exit' = '1' } -Must @('FINDING: lease list exited 1'))
    [void](P 'p310-ready-doc' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'ready' $true) } -Must @("FINDING: The lease's state is 'ready', expected 'pending'"))
    [void](P 'p310-arm64' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'pending' -Platform 'linux/arm64') } -Must @("FINDING: The platform is 'linux/arm64'"))
    [void](P 'p310-profile' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'pending' -Profile 'agent-linux') } -Must @("FINDING: The profile is 'agent-linux'"))
    [void](P 'p310-ttl' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'pending' -Ttl 28800) } -Must @("FINDING: ttl_seconds is '28800', expected 14400"))
    [void](P 'p310-size' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'pending' -Size 'l') } -Must @("FINDING: The size is 'l'"))
    [void](P 'p310-sized-for' '310' -Overlay @{ 'agentspace\lease-create.out' = (Doc 'pending' -SizedFor) } -Must @('FINDING: The members are'))
    [void](P 'p310-exit1' '310' -Overlay @{ 'agentspace\lease-create.exit' = '1' } -Must @('FINDING: create exited 1'))
    [void](P 'p310-not-json' '310' -Overlay @{ 'agentspace\lease-create.out' = "not json`n" } -Must @('FINDING: create did not print a JSON object'))
    [void](P 'p310-state-exists' '310' -State @{ 'lease-w.json' = "{}`n" } -ExitWant 5 -Must @('STOP: a state file of a lease W exists already') -Calls0)
    [void](P 'p-real-guard' '302' -Env @{ S02_DRY = $null; AGENTSPACE_PLANT = '1' } -ExitWant 5 -Must @('STOP: the live run holds') -Calls0)
  }
  if ($Group -eq 'B') {
    $unsat = '{"id":"3f2a9c1e-5b7d-4c8e-9a1b-2d3e4f5a6b7c","project":"agentspace","state":"unsatisfiable","platform":"windows/amd64","profile":"agent-windows","size":"m","ttl_seconds":14400,"created_at":"2026-11-27T21:45:15Z","reasons":[{"code":"no_capacity","message":"No capacity."}]}' + "`n"
    [void](P 'p311-failed-line' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.2.provlog' = "2026-11-27T21:49:10Z start done`n2026-11-27T21:55:00Z sync failed reason=Host 'agentspace': the patch wait gave up`n" } -ExitWant 2 -Must @("FINDING: provision.log says 'sync failed'"))
    [void](P 'p311-unsatisfiable' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.2.out' = $unsat } -ExitWant 2 -Must @("FINDING: The lease is 'unsatisfiable', not ready"))
    [void](P 'p311-expired' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.3.out' = (Doc 'expired' $false) } -ExitWant 2 -Must @("FINDING: The lease is 'expired', not ready"))
    [void](P 'p311-bound' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.3.out' = (Doc 'pending') } -Env @{ S02_MAX_SECONDS = '4' } -ExitWant 3 -Must @('FINDING: The lease is still ''pending'' after 4 s'))
    [void](P 'p311-hours' '311' -State $S311 -Hours 5.2 -ExitWant 6 -Must @("FINDING: windows/amd64's instance hours reached 5.2"))
    [void](P 'p311-get-fails' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.2.exit' = '1' } -ExitWant 1 -Must @('FINDING: lease get exit 1 with no readable state'))
    [void](P 'p311-missing-phase' '311' -State $S311 -Overlay @{ 'agentspace\lease-get.2.provlog' = "2026-11-27T21:49:10Z start done`n" } -ExitWant 1 -Must @("FINDING: The log shows no 'sync done' line"))
    [void](P 'p311-fewer-than-baseline' '311' -State @{ 'lease-w.id' = $idv; 'provlog-baseline.txt' = "9`n" } -ExitWant 1 -Must @('FINDING: provision.log has'))
    [void](P 'p311-no-state' '311' -ExitWant 5 -Must @('STOP: no lease W state file') -Calls0)
    [void](P 'p312-pending' '312' -State $S312 -Must @("FINDING: The lease's state is 'pending'"))
    [void](P 'p312-no-ssh' '312' -State $S312 -Overlay @{ 'agentspace\lease-get.1.out' = (Doc 'ready' $false) } -Must @('FINDING: ssh.host_alias is missing', 'FINDING: The members are'))
    [void](P 'p312-wrong-user' '312' -State $S312 -Overlay @{ 'agentspace\lease-get.1.out' = (Doc 'ready' $true -User 'ubuntu') } -Must @("FINDING: ssh.user is 'ubuntu', expected 'agent'"))
    [void](P 'p312-wrong-profile' '312' -State $S312 -Overlay @{ 'agentspace\lease-get.1.out' = (Doc 'ready' $true -Profile 'agent-linux') } -Must @("FINDING: The profile is 'agent-linux'"))
    [void](P 'p312-alias-form' '312' -State $S312 -Overlay @{ 'agentspace\lease-get.1.out' = (Doc 'ready' $true -Alias 'agentspace-wrong') } -Must @('FINDING: The alias does not have the form'))
    [void](P 'p312-no-state' '312' -ExitWant 5 -Must @('STOP: no lease W state file') -Calls0)
    [void](P 'p313-no-proxy-operator' '313' -State $S312 -Overlay @{ 'ssh\G-op.out' = "host x`nuser agent`nport 22`n" } -Must @('FINDING: ssh -G for the operator alias shows no ProxyCommand line'))
    [void](P 'p313-no-proxy-lease' '313' -State $S312 -Overlay @{ 'ssh\G-lease.out' = "host x`nuser agent`nport 22`n" } -Must @('FINDING: ssh -G for the lease alias shows no ProxyCommand line'))
    [void](P 'p313-true-operator-fails' '313' -State $S312 -Overlay @{ 'ssh\true.1.exit' = '255'; 'ssh\true.2.exit' = '0' } -Must @('FINDING: ssh <operator alias> true exited 255'))
    [void](P 'p313-true-lease-fails' '313' -State $S312 -Overlay @{ 'ssh\true.1.exit' = '0'; 'ssh\true.2.exit' = '255' } -Must @('FINDING: ssh <lease alias> true exited 255'))
    [void](P 'p313-status-fails' '313' -State $S312 -Overlay @{ 'operator\status-host.exit' = '1' } -Must @('FINDING: status -Host exited 1'))
    [void](P 'p313-ssm-not-online' '313' -State $S312 -Overlay @{ 'operator\status-host.err' = (CA 'operator\status-host.err').Replace('SSM       online', 'SSM       not yet') } -Must @('FINDING: The report has no "SSM       online" line'))
    [void](P 'p313-no-platform-line' '313' -State $S312 -Overlay @{ 'operator\status-host.err' = (CA 'operator\status-host.err').Replace("Platform  windows/amd64`n", '') } -Must @('FINDING: The report''s first line is not'))
    [void](P 'p313-not-running' '313' -State $S312 -Overlay @{ 'operator\status-host.err' = (CA 'operator\status-host.err').Replace('State     running', 'State     stopped') } -Must @('FINDING: The report has no "State     running" line'))
    [void](P 'p313-no-alias-state' '313' -ExitWant 5 -Must @('STOP: no lease alias state file') -Calls0)
  }
  if ($Group -eq 'C') {
    $h314 = CA 'ssh\host-314.out'
    [void](P 'p314-no-bash' '314' -State $S312 -Overlay @{ 'ssh\shell.out' = "S02 shell0=cmd`nS02 bash=`n" } -Must @('FINDING: BASH_VERSION is empty'))
    [void](P 'p314-tool-missing' '314' -State $S312 -Overlay @{ 'ssh\tool-uv.exit' = '1' } -Remove @('ssh\tool-uv.out') -Must @('FINDING: uv is not found by command -v'))
    [void](P 'p314-tool-no-version' '314' -State $S312 -Overlay @{ 'ssh\tool-gh.out' = "/c/Program Files/GitHub CLI/gh`n" } -Must @('FINDING: gh printed no version'))
    [void](P 'p314-localswim-refused' '314' -State $S312 -Overlay @{ 'ssh\localswim.out' = "/c/ProgramData/agentspace/bin/localswim`nS02 localswim_exit=1`n" } -Must @("FINDING: localswim --help did not exit 0 (1); its answer is one of the wrapper's own refusals"))
    [void](P 'p314-localswim-missing' '314' -State $S312 -Overlay @{ 'ssh\localswim.out' = "S02 localswim_exit=127`n"; 'ssh\localswim-why.out' = "bash: localswim: command not found`n" } -Must @('FINDING: localswim is not found by command -v', 'its answer is an unknown answer'))
    [void](P 'p314-defaultshell-powershell' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = $h314.Replace('S02 defaultshell=C:\Program Files\Git\bin\bash.exe', 'S02 defaultshell=C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe') } -Must @('FINDING: DefaultShell is'))
    [void](P 'p314-defaultshell-missing-file' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = $h314.Replace('defaultshell_file_exists=True', 'defaultshell_file_exists=False') } -Must @('FINDING: The DefaultShell file does not exist'))
    [void](P 'p314-firstboot-failed' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = $h314.Replace('step 4 done: sshd trusts the agentspace CA', 'step 4 failed: CaPublicKey is empty, so sshd was not started') } -Must @('FINDING: first-boot.log has 1 failed line', "FINDING: first-boot.log has no 'step 4 done:' line"))
    [void](P 'p314-firstboot-short' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = (($h314 -split "`n" | Where-Object { $_ -notmatch 'step 8 done' }) -join "`n") } -Must @("FINDING: first-boot.log has no 'step 8 done:' line"))
    [void](P 'p314-firstboot-missing' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = (($h314 -split "`n" | Where-Object { $_ -notmatch 'firstboot' }) -join "`n").Replace('S02 firstboot_lines=9', 'S02 firstboot_lines=missing') } -Must @('FINDING: first-boot.log is missing or empty'))
    [void](P 'p314-horizon-cr' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = $h314.Replace('horizon_has_cr=False', 'horizon_has_cr=True').Replace('digits-then-one-LF', 'other') } -Must @('FINDING: The horizon file holds a 0x0D', "FINDING: The horizon file's form is 'other'"))
    [void](P 'p314-horizon-missing' '314' -State $S312 -Overlay @{ 'ssh\host-314.out' = (($h314 -split "`n" | Where-Object { $_ -notmatch 'horizon' }) -join "`n") + "`nS02 horizon_bytes=missing`n" } -Must @('FINDING: The horizon file is missing'))
    [void](P 'p314-scp-fails' '314' -State $S312 -Overlay @{ 'scp\scp.exit' = '1' } -Must @('FINDING: The copy of host script 314 failed (scp exit 1)'))
    [void](P 'p314-hostscript-exit' '314' -State $S312 -Overlay @{ 'ssh\host-314.exit' = '1' } -Must @('FINDING: Host script 314 exited 1'))
    [void](P 'p314-no-alias-state' '314' -ExitWant 5 -Must @('STOP: no lease alias state file') -Calls0)
    [void](P 'p316-missing-patch-line' '316' -State $S312 -Overlay @{ 'operator\sync.out' = ((CA 'operator\sync.out') -split "`n" | Where-Object { $_ -notmatch '^Patch run:' }) -join "`n" } -Must @('FINDING: The install printed no step 10 line'))
    [void](P 'p316-exit1' '316' -State $S312 -Overlay @{ 'operator\sync.exit' = '1' } -Must @('FINDING: sync exited 1'))
    [void](P 'p316-error-line' '316' -State $S312 -Overlay @{ 'operator\sync.out' = (CA 'operator\sync.out') + "install.ps1: sshd -t rejected the new sshd_config; the old files were kept and sshd was not restarted`n" } -Must @('FINDING: The install printed an install.ps1: error line'))
    [void](P 'p316-added-not-already' '316' -State $S312 -Overlay @{ 'operator\sync.out' = (CA 'operator\sync.out').Replace('(already there)', '(added)') } -Must @('FINDING: The install printed no step 6 line', 'FINDING: The install printed no step 9, sshd_config line'))
    [void](P 'p316-after-signin' '316' -State ($S312 + @{ 'signin-asked.flag' = "1`n" }) -ExitWant 5 -Must @('STOP: Terry has been asked to sign in') -Calls0)
    $h320 = CA 'ssh\host-320.out'
    [void](P 'p320-extra-account' '320' -Overlay @{ 'ssh\host-320.out' = $h320 + "S02 acl=gh|BUILTIN\Users|I)(RX`n" } -Must @('FINDING: icacls for gh names 1 other account(s)'))
    [void](P 'p320-other-user' '320' -Overlay @{ 'ssh\host-320.out' = $h320.Replace('claude|EC2AMAZ-K3F9Q2L\agent|', 'claude|EC2AMAZ-K3F9Q2L\agent2|') } -Must @('FINDING: icacls for claude names 1 other account(s)'))
    [void](P 'p320-no-hosts-yml' '320' -Overlay @{ 'ssh\host-320.out' = (($h320 -split "`n" | Where-Object { $_ -notmatch '\|gh\|' -and $_ -notmatch '=gh\|' }) -join "`n") + "`nS02 exists=gh|False`n" } -Must @('FINDING: hosts.yml does not exist'))
    [void](P 'p320-no-credentials' '320' -Overlay @{ 'ssh\host-320.out' = (($h320 -split "`n" | Where-Object { $_ -notmatch '=claude\|' }) -join "`n") + "`nS02 exists=claude|False`n" } -Must @('FINDING: .credentials.json does not exist on the host'))
    [void](P 'p320-status-fails' '320' -Overlay @{ 'operator\status-host.exit' = '1' } -Must @('FINDING: status -Host exited 1'))
    [void](P 'p320-empty-acl' '320' -Overlay @{ 'ssh\host-320.out' = (($h320 -split "`n" | Where-Object { $_ -notmatch 'acl=gh' }) -join "`n") } -Must @('FINDING: icacls for gh printed no access list'))
    [void](P 'p330-no-text' '330' -Overlay @{ 'localswim\comment-list.out' = '{"comments":[{"id":"c1","text":"An ordinary comment."}]}' + "`n" } -Must @('FINDING: No comment holds the fixed text'))
    [void](P 'p330-no-comments' '330' -Overlay @{ 'localswim\comment-list.out' = '{"comments":[]}' + "`n" } -Must @('FINDING: #0267 has no comment'))
    [void](P 'p330-cli-fails' '330' -Overlay @{ 'localswim\comment-list.exit' = '1' } -Must @('FINDING: The read exited 1'))
    [void](P 'p330-not-json' '330' -Overlay @{ 'localswim\comment-list.out' = "not json`n" } -Must @("FINDING: The CLI's output is not JSON"))
    $emptyFile = Join-Path $lab.Root 'empty-fixed.txt'
    Write-DFile $emptyFile "`n"
    [void](P 'p330-no-fixed-file' '330' -Env @{ S02_P2_TEXT_FILE = (Join-Path $lab.Root 'no-such-fixed.txt') } -ExitWant 5 -Must @('STOP: target\t05\s02-p2-fixed-text.txt is missing') -Calls0)
    [void](P 'p330-empty-fixed-file' '330' -Env @{ S02_P2_TEXT_FILE = $emptyFile } -ExitWant 5 -Must @('STOP: target\t05\s02-p2-fixed-text.txt is empty') -Calls0)
  }
  if ($Group -eq 'D') {
    $a = CA 'ssh\host-315a.out'
    [void](P 'p315a-past-up-wait' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('|500|finished', '|1000|finished') } -Must @("FINDING: Run 1 took 1000 s, past up's 15-minute wait"))
    [void](P 'p315a-past-task-limit' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('|500|finished', '|1300|finished') } -Must @("past up's 15-minute wait and past the task's 20-minute limit"))
    [void](P 'p315a-failed-run' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('|500|finished', '|500|failed') } -Must @("FINDING: Run 1 is 'failed', not finished"))
    [void](P 'p315a-two-summary-lines' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('summary=1', 'summary=2') } -Must @('FINDING: Run 1 holds 2 Tools: version-summary lines, expected 1'))
    [void](P 'p315a-no-marker' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('marker_patched=written 2026-11-27T21:58:50Z', 'marker_patched=missing') } -Must @('FINDING: The patched marker is missing'))
    [void](P 'p315a-failed-marker' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('marker_patch-failed=missing', 'marker_patch-failed=written 2026-11-27T21:59:00Z') } -Must @('FINDING: A patch-failed marker exists'))
    [void](P 'p315a-task-result' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('patch_task_last_result=0', 'patch_task_last_result=1') } -Must @("FINDING: The patch task's last result is '1'"))
    [void](P 'p315a-no-run-this-boot' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = $a.Replace('run=1|2026-11-27T21:50:30Z', 'run=1|2026-11-27T21:30:00Z') } -Must @('FINDING: No patch run started since this boot'))
    [void](P 'p315a-no-run' '315a' -State $S312 -Overlay @{ 'ssh\host-315a.out' = (($a -split "`n" | Where-Object { $_ -notmatch '^S02 run' -and $_ -notmatch 'tools_line' }) -join "`n") } -Must @('FINDING: patch.log holds no patch run'))
    [void](P 'p315b-session-gone' '315b' -State $S312 -Overlay @{ 'ssh\sleep.exit' = '255'; 'ssh\sleep.delay' = '1' } -Must @('FINDING: Session 1 was gone before the restart began'))
    [void](P 'p315b-same-pid' '315b' -State $S312 -Overlay @{ 'ssh\host-pid.2.out' = "S02 sshd_state=Running`nS02 sshd_pid=1111`nS02 sshd_start_utc=2026-11-27T21:46:14Z`n" } -Must @("FINDING: sshd's process ID did not change"))
    [void](P 'p315b-not-running' '315b' -State $S312 -Overlay @{ 'ssh\host-pid.2.out' = "S02 sshd_state=Stopped`nS02 sshd_pid=0`nS02 sshd_start_utc=unknown`n" } -Must @("FINDING: sshd is 'Stopped' after the restart"))
    [void](P 'p315b-before-copy-fails' '315b' -State $S312 -Overlay @{ 'scp\scp.exit' = '1' } -Must @('FINDING: The copy of host script pid failed'))
    [void](P 'p315b-after-signin-asked' '315b' -State ($S312 + @{ 'signin-asked.flag' = "1`n" }) -ExitWant 5 -Must @('STOP: Terry has been asked to sign in') -Calls0)
    $c = CA 'ssh\host-315c.out'
    [void](P 'p315c-botenv-short' '315c' -State $S312 -ProvLog $provFull -Overlay @{ 'ssh\host-315c.out' = $c.Replace('botenv_lines=2', 'botenv_lines=1').Replace('botenv_token_line=1', 'botenv_token_line=0') } -Must @("FINDING: bot.env holds '1' lines", 'FINDING: bot.env lacks a LOCALSWIM_URL or LOCALSWIM_TOKEN line'))
    [void](P 'p315c-order' '315c' -State $S312 -ProvLog ($provOld + "2026-11-27T21:59:00Z bootstrap done`n2026-11-27T22:06:00Z sync done`n") -Must @('FINDING: sync done is after bootstrap done'))
    [void](P 'p315c-no-log-lines' '315c' -State $S312 -Must @('FINDING: provision.log shows no sync done or no bootstrap done line'))
    [void](P 'p315c-no-time' '315c' -State $S312 -ProvLog $provFull -Overlay @{ 'ssh\host-315c.out' = $c.Replace('gitbash_created_utc=2026-11-27T22:05:30Z', 'gitbash_created_utc=missing') } -Must @("FINDING: bot.env's or Git Bash's creation time could not be read"))
    $d = CA 'ssh\host-315d.out'
    [void](P 'p315d-no-winget-answer' '315d' -State $S312 -Overlay @{ 'ssh\host-315d.out' = (($d -split "`n" | Where-Object { $_ -notmatch 'winget_version_exit' }) -join "`n") } -Must @('FINDING: The probe printed no winget answer'))
    [void](P 'p315d-tool-missing' '315d' -State $S312 -Overlay @{ 'ssh\host-315d.out' = $d.Replace('tool_file=uv|True|2026-11-27T22:03:00Z', 'tool_file=uv|False|') } -Must @('FINDING: uv is not present on the host'))
    [void](P 'p315d-no-tools' '315d' -State $S312 -Overlay @{ 'ssh\host-315d.out' = (($d -split "`n" | Where-Object { $_ -notmatch 'tool_file' }) -join "`n") } -Must @('FINDING: Git for Windows is not present on the host'))
  }
  if ($Group -eq 'E') {
    $f = CA 'ssh\host-315f.out'
    $lines = $f -split "`n"
    [void](P 'p315f-cpu-work' '315f' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('2026-11-27T22:06:00Z Active: SSH session open', '2026-11-27T22:06:00Z Active: CPU work: 9 s since last check') } -Must @('FINDING: 1 check(s) with no session counted as active for CPU work'))
    [void](P 'p315f-no-idle-window' '315f' -State $S312 -Overlay @{ 'ssh\host-315f.out' = (($lines | Where-Object { $_ -notmatch 'Idle for' }) -join "`n") } -Must @('FINDING: Only 0 idle-window check(s) were observed'))
    [void](P 'p315f-one-idle' '315f' -State $S312 -Overlay @{ 'ssh\host-315f.out' = (($lines | Where-Object { $_ -notmatch '22:16:00Z Idle for' }) -join "`n") } -Must @('FINDING: Only 1 idle-window check(s) were observed'))
    [void](P 'p315f-no-lines' '315f' -State $S312 -Overlay @{ 'ssh\host-315f.out' = (($lines | Where-Object { $_ -notmatch 'S02 idle=' }) -join "`n").Replace('idle_lines_since_boot=6', 'idle_lines_since_boot=0') } -Must @('FINDING: idle.log has no line since this boot'))
    [void](P 'p315f-odd-line' '315f' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('S02 idle_task_state=Ready', "S02 idle=2026-11-27T22:17:30Z Something else`nS02 idle_task_state=Ready") } -Must @("FINDING: 1 line(s) are none of the idle check's forms"))
    [void](P 'p315f-hours' '315f' -State $S312 -Hours 5.0 -ExitWant 6 -Must @("FINDING: windows/amd64's instance hours reached"))
    $g = CA 'ssh\host-315g.out'
    [void](P 'p315g-restart-1074' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = $g.Replace('sysevent=1074|0|', 'sysevent=1074|1|2026-11-27T21:59:00Z') } -Must @("FINDING: Event 1074 count is 1, expected 0"))
    [void](P 'p315g-unexpected-6008' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = $g.Replace('sysevent=6008|0|', 'sysevent=6008|1|2026-11-27T21:46:10Z') } -Must @('FINDING: Event 6008 count is 1, expected 0'))
    [void](P 'p315g-kernel-power-41' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = $g.Replace('sysevent=41|0|', 'sysevent=41|2|2026-11-27T21:59:00Z;2026-11-27T22:00:00Z') } -Must @('FINDING: Event 41 count is 2, expected 0'))
    [void](P 'p315g-no-boot-event' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = $g.Replace('sysevent=6005|1|2026-11-27T21:46:09Z', 'sysevent=6005|0|') } -Must @('FINDING: Event 6005 count is 0, expected 1'))
    [void](P 'p315g-event-unread' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = (($g -split "`n" | Where-Object { $_ -notmatch 'sysevent=6006' }) -join "`n") } -Must @('FINDING: Event 6006 was not read'))
    [void](P 'p315g-wuclient-unread' '315g' -State $S312 -Overlay @{ 'ssh\host-315g.out' = (($g -split "`n" | Where-Object { $_ -notmatch 'wuclient' }) -join "`n") } -Must @('FINDING: The WindowsUpdateClient source was not read'))
    [void](P 'p315h-sync-before-patch-end' '315h' -State $S312 -ProvLog ($provFull.Replace('21:59:10Z sync done', '21:55:00Z sync done')) -Must @('FINDING: sync done came 230 s BEFORE the patch run ended'))
    [void](P 'p315h-at-the-limit' '315h' -State $S312 -ProvLog $provFull -Overlay @{ 'ssh\host-315a.out' = (CA 'ssh\host-315a.out').Replace('run=1|2026-11-27T21:50:30Z', 'run=1|2026-11-27T21:25:00Z') } -Must @("FINDING: sync done came 2050 s after the patch run began: at the wait's 30-minute limit"))
    [void](P 'p315h-patch-failed-text' '315h' -State $S312 -ProvLog ($provFull.Replace('sync done', "sync done; the patch run failed, see the host's journal")) -Must @('FINDING: sync did not end with a plain "sync done"'))
    [void](P 'p315h-no-sync-line' '315h' -State $S312 -Must @('FINDING: provision.log shows no sync line since the baseline'))
    [void](P 'p315h-first-run-failed' '315h' -State $S312 -ProvLog $provFull -Overlay @{ 'ssh\host-315a.out' = (CA 'ssh\host-315a.out').Replace('|500|finished', '|500|failed') } -Must @("FINDING: The first patch run is 'failed'"))
    $i = CA 'ssh\host-315i.out'
    [void](P 'p315i-evidence-false' '315i' -State $S312 -Overlay @{ 'ssh\host-315i.out' = $i.Replace('install-6|True', 'install-6|False') } -Must @('FINDING: install-6 is NOT shown'))
    [void](P 'p315i-evidence-missing' '315i' -State $S312 -Overlay @{ 'ssh\host-315i.out' = (($i -split "`n" | Where-Object { $_ -notmatch 'localswim-4' }) -join "`n") } -Must @('FINDING: The host printed no evidence line for localswim-4'))
    [void](P 'p315i-bootstrap-tool-false' '315i' -State $S312 -Overlay @{ 'ssh\host-315i.out' = $i.Replace('bootstrap-gh|True|file present', 'bootstrap-gh|False|file missing') } -Must @('FINDING: bootstrap-gh is NOT shown'))
    [void](P 'p315i-winget-no-answer' '315i' -State $S312 -Overlay @{ 'ssh\host-315i.out' = $i.Replace('system_winget_exit=-1073741515', 'system_winget_exit=no-answer') } -Must @('FINDING: The SYSTEM winget probe gave no answer'))
    [void](P 'p315i-task-left-behind' '315i' -State $S312 -Overlay @{ 'ssh\host-315i.out' = $i.Replace('system_winget_task_removed=True', 'system_winget_task_removed=False') } -Must @('FINDING: The transient task was not removed'))
    [void](P 'p315i-idle-never-ran' '315i' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('idle_task_last_run_utc=2026-11-27T22:17:00Z', 'idle_task_last_run_utc=1999-11-30T00:00:00Z').Replace('idle_task_next_run_utc=2026-11-27T22:22:00Z', 'idle_task_next_run_utc=none') } -Must @('FINDING: The idle task has NOT run since this boot', 'AtStartup trigger registered after the boot'))
    [void](P 'p315i-trigger-interval' '315i' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('idle_trigger_interval=PT5M', 'idle_trigger_interval=PT10M') } -Must @("FINDING: The trigger's repetition interval is 'PT10M'"))
    [void](P 'p315i-trigger-duration' '315i' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('idle_trigger_duration=', 'idle_trigger_duration=PT1H') } -Must @("FINDING: The repetition's duration is 'PT1H'"))
    [void](P 'p315i-repetition-stopped' '315i' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('idle_task_last_run_utc=2026-11-27T22:17:00Z', 'idle_task_last_run_utc=2026-11-27T22:05:00Z') } -Must @('FINDING: The last run was 900 s ago'))
    [void](P 'p315i-two-tools-lines' '315i' -State $S312 -Overlay @{ 'ssh\host-315a.out' = (CA 'ssh\host-315a.out').Replace('summary=1', 'summary=2') } -Must @('FINDING: 1 finished patch run(s) do not hold exactly one Tools: version-summary line'))
    [void](P 'p315i-too-few-idle-lines' '315i' -State $S312 -Overlay @{ 'ssh\host-315f.out' = $f.Replace('idle_lines_since_boot=6', 'idle_lines_since_boot=1') } -Must @('FINDING: idle.log holds 1 line(s) since boot, fewer than the 4'))
  }
  if ($Group -eq 'F') {
    $relDoc = Doc 'released' $false -Released
    [void](P 'p340-not-released' '340' -State $S311 -Overlay @{ 'agentspace\lease-release.out' = (Doc 'ready' $true) } -Must @("FINDING: The answer's state is 'ready', expected 'released'"))
    [void](P 'p340-exit1' '340' -State $S311 -Overlay @{ 'agentspace\lease-release.exit' = '1' } -Must @('FINDING: release exited 1'))
    [void](P 'p340-ssh-left' '340' -State $S311 -Overlay @{ 'agentspace\lease-release.out' = (Doc 'released' $true -Released) } -Must @('FINDING: The released lease still has an ssh object'))
    [void](P 'p340-hot-list' '340' -State $S311 -Overlay @{ 'agentspace\lease-list.out' = ('{"leases":[' + (Doc 'ready' $true).TrimEnd("`n") + ']}' + "`n") } -Must @('FINDING: 1 lease(s) are ready or pending after the release'))
    [void](P 'p340-already-released' '340' -State ($S311 + @{ 'release-end.txt' = "2026-11-27T22:30:00Z`n" }) -ExitWant 5 -Must @('STOP: a release time is recorded already') -Calls0)
    [void](P 'p340-no-state' '340' -ExitWant 5 -Must @('STOP: no lease W state file') -Calls0)
    [void](New341 -Case 'p341-bound-no-edge' -HasEdge $false -Env @{ S02_STOP_SECONDS = '3'; S02_POLL_SECONDS = '1' } -ExitWant 3 -Must @('FINDING: No `agentspace.windows-amd64 -> stopped` edge appeared'))
    [void](New341 -Case 'p341-late-edge' -EdgeAfter 30 -Must @("FINDING: The host was seen stopped 30 minutes after the release, past D56's 25"))
    [void](New341 -Case 'p341-no-guard-stopped-line' -GuardNew '' -Must @('FINDING: guard.log shows no `stopped` line since the release'))
    $relGuard = [DateTime]::UtcNow.AddMinutes(-40)
    [void](New341 -Case 'p341-guard-failed-line' -GuardNew ((ConvertTo-DUtc $relGuard.AddMinutes(3)) + " failed step=describe reason=AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.`n" + (ConvertTo-DUtc $relGuard.AddMinutes(17)) + " host=agentspace.windows-amd64 stopped since=2026-11-27T21:00:00Z`n") -Must @('FINDING: guard.log shows a failed or sign-in line since the release', 'failed 1; sign-in has expired 1'))
    [void](New341 -Case 'p341-instance-initiated' -Overlay @{ 'aws\describe.out' = "stopped`tClient.InstanceInitiatedShutdown`n" } -Must @("FINDING: StateReason.Code is 'Client.InstanceInitiatedShutdown'"))
    [void](New341 -Case 'p341-still-running' -Overlay @{ 'aws\describe.out' = "running`tNone`n" } -Must @("FINDING: The instance is 'running', not stopped"))
    [void](New341 -Case 'p341-two-instances' -Overlay @{ 'aws\describe.out' = "stopped`tClient.UserInitiatedShutdown`nrunning`tNone`n" } -Must @('FINDING: describe-instances returned 2 row(s), expected 1'))
    [void](New341 -Case 'p341-aws-fails' -Overlay @{ 'aws\describe.exit' = '254' } -Must @('FINDING: describe-instances exited 254'))
    [void](New341 -Case 'p341-hours' -Hours 5.0 -ExitWant 6 -Must @("FINDING: windows/amd64's instance hours reached"))
    [void](P 'p341-no-release-state' '341' -ExitWant 5 -Must @('STOP: no release time recorded') -Calls0)
    [void](P 'p341-no-guard-baseline' '341' -State @{ 'release-end.txt' = "2026-11-27T22:30:00Z`n" } -ExitWant 5 -Must @('STOP: no guard.log baseline recorded') -Calls0)
    $sa = CA 'operator\status-all.err'
    [void](P 'p350-linux-running' '350' -Overlay @{ 'operator\status-all.err' = $sa.Replace("Platform  linux/amd64`nInstance  i-0cccccccccccccccc`nState     stopped", "Platform  linux/amd64`nInstance  i-0cccccccccccccccc`nState     running") } -Must @('FINDING: Host agentspace.linux-amd64 is running, not stopped'))
    [void](P 'p350-windows-running' '350' -Overlay @{ 'operator\status-all.err' = (RepLast $sa 'State     stopped' 'State     running') } -Must @('FINDING: Host agentspace.windows-amd64 is running, not stopped'))
    [void](P 'p350-ready-lease' '350' -Overlay @{ 'agentspace\lease-list.out' = ('{"leases":[' + (Doc 'ready' $true).TrimEnd("`n") + ']}' + "`n") } -Must @('FINDING: 1 lease(s) are ready or pending'))
    [void](P 'p350-no-windows-host' '350' -Overlay @{ 'operator\status-all.err' = $sa.Substring(0, $sa.IndexOf('Host      agentspace.windows-amd64')) } -Must @('FINDING: The Windows host is in the registry 0 time(s), expected 1'))
    [void](P 'p350-status-fails' '350' -Overlay @{ 'operator\status-all.exit' = '1' } -Must @('FINDING: status -All exited 1'))
    # The real S01 build, in the same scenario: its answers differ from a Windows host's only where no host exists, so each check on the form
    # (--ttl 4h, windows/amd64, agent-windows, 14400) holds and only the state does not.
    $sc = P 'p-realbuild-310' '310' -RealBuild -Must @("FINDING: The lease's state is 'unsatisfiable', expected 'pending'", 'Command: agentspace lease create --platform windows/amd64 --project agentspace --ttl 4h', "platform: 'windows/amd64'; profile: 'agent-windows'", "ttl_seconds: '14400'") -MustNot @('FINDING: The platform', 'FINDING: The profile', 'FINDING: ttl_seconds', 'FINDING: The size', 'FINDING: The id')
    $r = Invoke-DStep -Scenario $sc -Step '311' -Tag 'planted' -RealBuild
    Write-DVerdict -Case 'p-realbuild-311' -R $r -ExitWant 2 -Must @("FINDING: The lease is 'unsatisfiable', not ready") -MustNot @('SECRETPROXYPATH', '3f2a9c1e')
    $r = Invoke-DStep -Scenario $sc -Step '312' -Tag 'planted' -RealBuild
    Write-DVerdict -Case 'p-realbuild-312' -R $r -ExitWant 1 -Must @("FINDING: The lease's state is 'unsatisfiable', expected 'ready'", 'FINDING: ssh.host_alias is missing')
  }
} finally {
  Remove-DLab $lab
}
Write-Host "Planted group ${Group}: $($script:DCases) checks, $($script:DBad) not as designed"
exit $(if ($script:DBad -eq 0) { 0 } else { 1 })
