# Writes the CLEAN answers of E0.12-T05-S02's stand-ins to target\t05\s02-standin-answers\ (drafter-owned; run it with pwsh -NoProfile -File).
# RFC 2119 keywords; the capitals are load-bearing. Every identifier here is made up: a lease ID and alias of the form the contract gives, instance and
# volume IDs of repeated letters, an account number and a user ID that a step MUST NOT print, and a Windows computer name of the EC2 form that the
# steps MUST replace. The times are one fixed afternoon's, consistent with each other (the provisioning run, the host's boot, the patch run, the
# push, Git Bash's install, the idle checks); the dry runs' driver makes the observer's files and the release's times from the real clock.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Join-Path $PSScriptRoot 's02-standin-answers'
if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
$utf8 = [Text.UTF8Encoding]::new($false)
function W([string]$Rel, [string]$Text) {
  $p = Join-Path $root $Rel
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $p) | Out-Null
  $t = ($Text -replace "`r`n", "`n")
  if ($Rel -match '\.(out|err|provlog)$' -and $t -ne '' -and -not $t.EndsWith("`n")) { $t += "`n" }
  [IO.File]::WriteAllBytes($p, $utf8.GetBytes($t))
}
$id = '3f2a9c1e-5b7d-4c8e-9a1b-2d3e4f5a6b7c'
$alias = 'agentspace-agentspace-3f2a9c1e'
$created = '2026-11-27T21:45:15Z'
$expires = '2026-11-28T01:49:20Z'
function Lease([string]$State, [bool]$Ssh, [bool]$Released) {
  $d = [ordered]@{ id = $id; project = 'agentspace'; state = $State; platform = 'windows/amd64'; profile = 'agent-windows'; size = 'm'; ttl_seconds = 14400; created_at = $created }
  if ($State -in 'ready', 'released', 'expired') { $d['expires_at'] = $expires }
  if ($Released) { $d['released_at'] = '2026-11-27T22:30:00Z' }
  if ($Ssh) { $d['ssh'] = [ordered]@{ host_alias = $alias; user = 'agent' } }
  return ((ConvertTo-Json -InputObject $d -Compress -Depth 5) + "`n")
}

# --- agentspace ---
W 'agentspace\lease-create.out' (Lease 'pending' $false $false)
W 'agentspace\lease-get.1.out' (Lease 'pending' $false $false)
W 'agentspace\lease-get.2.out' (Lease 'pending' $false $false)
W 'agentspace\lease-get.3.out' (Lease 'ready' $true $false)
W 'agentspace\lease-get.1.provlog' "2026-11-27T21:45:20Z run begun`n2026-11-27T21:47:40Z deploy done`n"
W 'agentspace\lease-get.2.provlog' "2026-11-27T21:49:10Z start done`n2026-11-27T21:59:10Z sync done`n"
W 'agentspace\lease-get.3.provlog' "2026-11-27T22:06:00Z bootstrap done`n2026-11-27T22:06:01Z record done`n2026-11-27T22:06:02Z place done placed=1`n2026-11-27T22:06:02Z run done`n"
W 'agentspace\lease-release.out' (Lease 'released' $false $true)
$other = [ordered]@{ id = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'; project = 'agentspace'; state = 'released'; platform = 'linux/amd64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800; created_at = '2026-11-27T17:00:00Z'; expires_at = '2026-11-28T01:00:00Z'; released_at = '2026-11-27T19:00:00Z' }
$win = [ordered]@{ id = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'; project = 'agentspace'; state = 'released'; platform = 'windows/amd64'; profile = 'agent-windows'; size = 'm'; ttl_seconds = 14400; created_at = '2026-11-26T20:00:00Z'; expires_at = '2026-11-27T00:00:00Z'; released_at = '2026-11-26T21:00:00Z' }
W 'agentspace\lease-list.out' ((ConvertTo-Json -InputObject ([ordered]@{ leases = @($other, $win) }) -Compress -Depth 6) + "`n")

# --- operator ---
function HostBlock([string]$Name, [string]$Stack, [string]$State, [string]$Plat, [string]$Type, [int]$Gib, [string]$Letter, [string]$Ssm) {
  $t = "Host      $Name ($Stack), ready`n"
  if ($Plat) { $t += "Platform  $Plat`n" }
  $t += "Instance  i-0$($Letter * 16)`nState     $State`nType      $Type`nDisk      $Gib GiB gp3 (vol-0$($Letter * 16))`nSSM       $Ssm`nLeases    0 live`n"
  return $t
}
$statusAll = (HostBlock 'swimcoach' 'agentbox' 'stopped' '' 'm9g.large' 8 'a' '-') + (HostBlock 'agentspace' 'agentspace-host-agentspace' 'stopped' '' 'm9g.large' 8 'b' '-') +
  (HostBlock 'agentspace.linux-amd64' 'agentspace-linux-amd64-host-agentspace' 'stopped' 'linux/amd64' 'm8a.large' 8 'c' '-') +
  (HostBlock 'agentspace.windows-amd64' 'agentspace-windows-amd64-host-agentspace' 'stopped' 'windows/amd64' 'm8a.large' 30 'd' '-')
W 'operator\status-all.err' $statusAll
W 'operator\status-host.err' "Platform  windows/amd64`nInstance  i-0$('d' * 16)`nState     running`nType      m8a.large`nDisk      30 GiB gp3 (vol-0$('d' * 16))`nSSM       online`n"
W 'operator\sync.out' @'
Building the localswim CLI for windows/amd64...
Ready: i-0dddddddddddddddd
Installed: the machine PATH holds agentspace's and agent's tool folders (already there)
Installed: the login shell is C:\Program Files\Git\bin\bash.exe
Installed: idle check (task Ready), patch at boot (task Ready)
Installed: sshd_config holds the agentspace lines (already there)
Installed: SSH trusts the agentspace CA (SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG), password login closed, agent the only login
Installed: login grace time 30 seconds; this sshd has no PerSourcePenalties
Installed: a session whose client has gone is dropped about 90 seconds after it goes quiet (probe every 15 seconds, 5 unanswered)
Patch run: finished this boot; not started again
'@

# --- aws ---
W 'aws\sts.out' "{`"UserId`": `"AIDAEXAMPLE0123456789`", `"Account`": `"123456789012`", `"Arn`": `"arn:aws:iam::123456789012:user/example`"}`n"
W 'aws\describe.out' "stopped`tClient.UserInitiatedShutdown`n"

# --- ssh ---
foreach ($k in 'G-op', 'G-lease') {
  W "ssh\$k.out" "host $k`nuser agent`nhostname i-0$('d' * 16)`nport 22`nproxycommand SECRETPROXYPATH agentspace-ssh-proxy %n`nidentityfile SECRETKEYPATH`n"
}
W 'ssh\true.exit' '0'
W 'ssh\shell.out' "S02 shell0=bash`nS02 bash=5.2.37(1)-release`n"
W 'ssh\tool-git.out' "/mingw64/bin/git`ngit version 2.52.0.windows.1`n"
W 'ssh\tool-pwsh.out' "/c/Program Files/PowerShell/7/pwsh`nPowerShell 7.5.4`n"
W 'ssh\tool-node.out' "/c/Program Files/nodejs/node`nv22.21.1`n"
W 'ssh\tool-uv.out' "/c/Users/agent/.local/bin/uv`nuv 0.9.13`n"
W 'ssh\tool-claude.out' "/c/Users/agent/.local/bin/claude`n2.1.50 (Claude Code)`n"
W 'ssh\tool-gh.out' "/c/Program Files/GitHub CLI/gh`ngh version 2.83.0`n"
W 'ssh\localswim.out' "/c/ProgramData/agentspace/bin/localswim`nS02 localswim_exit=0`n"
W 'ssh\localswim-why.out' "localswim: The board at http://127.0.0.1:8811 is unreachable. It is forwarded from the laptop only while a connection holds it. Stop and report; do not work without a card.`n"
W 'ssh\host-314.out' @'
S02 defaultshell=C:\Program Files\Git\bin\bash.exe
S02 defaultshell_file_exists=True
S02 gitbash_expected=C:\Program Files\Git\bin\bash.exe
S02 firstboot_lines=9
S02 firstboot=2026-11-27T21:46:03Z first boot begun
S02 firstboot=2026-11-27T21:46:04Z step 1 done: Windows Update installs and restarts nothing by itself
S02 firstboot=2026-11-27T21:46:06Z step 2 done: agent is a member of Administrators
S02 firstboot=2026-11-27T21:46:12Z step 3 done: sshd starts automatically, and has made its keys and sshd_config
S02 firstboot=2026-11-27T21:46:12Z step 4 done: sshd trusts the agentspace CA
S02 firstboot=2026-11-27T21:46:13Z step 5 done: administrators_authorized_keys holds the user's key
S02 firstboot=2026-11-27T21:46:14Z step 6 done: sshd_config holds the agentspace lines
S02 firstboot=2026-11-27T21:46:14Z step 7 done: the login shell is Windows PowerShell 5.1
S02 firstboot=2026-11-27T21:46:16Z step 8 done: sshd started; first boot done
S02 horizon_bytes=11
S02 horizon_has_cr=False
S02 horizon_form=digits-then-one-LF
S02 horizon_written_utc=2026-11-27T22:10:00Z
'@
W 'ssh\host-pid.1.out' "S02 sshd_state=Running`nS02 sshd_pid=1111`nS02 sshd_start_utc=2026-11-27T21:46:14Z`n"
W 'ssh\host-pid.2.out' "S02 sshd_state=Running`nS02 sshd_pid=2222`nS02 sshd_start_utc=2026-11-27T22:12:00Z`n"
W 'ssh\sleep.exit' '0'
W 'ssh\restart.exit' '0'
W 'ssh\host-315a.out' @'
S02 boot_utc=2026-11-27T21:46:00Z
S02 now_utc=2026-11-27T22:20:00Z
S02 marker_patched=written 2026-11-27T21:58:50Z
S02 marker_patch-failed=missing
S02 patch_task_last_run_utc=2026-11-27T21:50:30Z
S02 patch_task_last_result=0
S02 patch_log_lines=7
S02 tools_line=2026-11-27T21:58:49Z Tools: Node v22.21.1, uv 0.9.13, gh 2.83.0, Git 2.52.0, Claude Code 2.1.50, PowerShell 7.5.4
S02 run=1|2026-11-27T21:50:30Z|2026-11-27T21:58:50Z|500|finished|tools=1|summary=1|Search result code 2, 14 updates
S02 runs=1
'@
W 'ssh\host-315c.out' @'
S02 botenv_created_utc=2026-11-27T21:52:00Z
S02 botenv_written_utc=2026-11-27T21:52:00Z
S02 botenv_lines=2
S02 botenv_url_line=1
S02 botenv_token_line=1
S02 gitbash_created_utc=2026-11-27T22:05:30Z
S02 defaultshell=C:\Program Files\Git\bin\bash.exe
S02 timezone=Eastern Standard Time
'@
W 'ssh\host-315d.out' @'
S02 winget_on_path=True
S02 winget_version_exit=0
S02 winget_version=v1.11.510
S02 winget_log_files=4
S02 winget_log_first_utc=2026-11-27T22:00:10Z
S02 winget_log_last_utc=2026-11-27T22:05:20Z
S02 winget_log_mentions=Microsoft.PowerShell|0
S02 winget_log_mentions=Git.Git|2
S02 winget_log_mentions=OpenJS.NodeJS.LTS|2
S02 winget_log_mentions=astral-sh.uv|2
S02 winget_log_mentions=GitHub.cli|2
S02 tool_file=PowerShell 7|True|2026-11-27T21:49:50Z
S02 tool_file=Git for Windows|True|2026-11-27T22:05:30Z
S02 tool_file=Node|True|2026-11-27T22:02:10Z
S02 tool_file=uv|True|2026-11-27T22:03:00Z
S02 tool_file=Claude Code|True|2026-11-27T22:03:40Z
S02 tool_file=gh|True|2026-11-27T22:04:30Z
S02 patch_log_winget_upgrade_lines=0
'@
W 'ssh\host-315f.out' @'
S02 boot_utc=2026-11-27T21:46:00Z
S02 now_utc=2026-11-27T22:20:00Z
S02 uptime_seconds=2040
S02 idle_log=present
S02 idle_lines_since_boot=6
S02 idle=2026-11-27T21:51:00Z Active: SSH session open
S02 idle=2026-11-27T21:56:00Z Active: Patching
S02 idle=2026-11-27T22:01:00Z Active: SSH session open
S02 idle=2026-11-27T22:06:00Z Active: SSH session open
S02 idle=2026-11-27T22:11:00Z Idle for 5 min of 60 (CPU 2 s since last check)
S02 idle=2026-11-27T22:16:00Z Idle for 10 min of 60 (CPU 1 s since last check)
S02 idle_task_state=Ready
S02 idle_task_last_run_utc=2026-11-27T22:17:00Z
S02 idle_task_next_run_utc=2026-11-27T22:22:00Z
S02 idle_task_last_result=0
S02 idle_task_missed_runs=0
S02 idle_trigger_class=MSFT_TaskBootTrigger
S02 idle_trigger_delay=PT5M
S02 idle_trigger_interval=PT5M
S02 idle_trigger_duration=
S02 idle_trigger_stop_at_end=False
S02 idle_task_registered_utc=2026-11-27T21:49:40Z
'@
W 'ssh\host-315g.out' @'
S02 boot_utc=2026-11-27T21:46:00Z
S02 now_utc=2026-11-27T22:20:00Z
S02 sysevent=1074|0|
S02 sysevent=6005|1|2026-11-27T21:46:09Z
S02 sysevent=6006|0|
S02 sysevent=6008|0|
S02 sysevent=41|0|
S02 sysevent_1074_all_time=0
S02 wuclient_events=2
S02 wuclient=19|1|2026-11-27T21:55:00Z|2026-11-27T21:55:00Z
S02 wuclient=43|1|2026-11-27T21:54:00Z|2026-11-27T21:54:00Z
'@
$evid = @(
  'install-1|True|pwsh.exe present; this probe runs under PowerShell 7.5.4', 'install-2|True|agentspace_ca.pub is 112 byte(s)', 'install-3|True|inheritance off: True; SYSTEM full: True; Administrators full: True; Users read and execute: True; no other account: True',
  'install-4|True|missing: none; idle.conf settings: IDLE_MINUTES=60 BUSY_CPU_SECONDS=6 HORIZON_GRACE_MINUTES=15', 'install-5|True|NoAutoUpdate=1; NoAutoRebootWithLoggedOnUsers=1', 'install-6|True|agentspace bin=True; agent .local\bin=True; agent npm=True',
  'install-7|True|DefaultShell is C:\Program Files\Git\bin\bash.exe', 'install-8|True|agentspace-boot=Ready,SYSTEM-highest-pwsh7:True; agentspace-patch=Ready,SYSTEM-highest-pwsh7:True; agentspace-idle=Ready,SYSTEM-highest-pwsh7:True',
  'install-10|True|patched written 2026-11-27T21:58:50Z; boot 2026-11-27T21:46:00Z; patch.log has a start line: True',
  'localswim-1|True|the CLI is installed', 'localswim-2|True|bot.env present, 120 byte(s)', 'localswim-3|True|the wrapper reads the lines with StartsWith and has 0 Invoke-Expression or dot-source',
  'localswim-4|True|a TCP connection to the board at 127.0.0.1:8811 from the host', 'localswim-5|True|the wrapper ran the CLI with --help: exit 0; the token echoed: False',
  'bootstrap-PowerShell 7|True|file present, created 2026-11-27T21:49:50Z', 'bootstrap-Git for Windows|True|file present, created 2026-11-27T22:05:30Z', 'bootstrap-Node|True|file present, created 2026-11-27T22:02:10Z',
  'bootstrap-uv|True|file present, created 2026-11-27T22:03:00Z', 'bootstrap-Claude Code|True|file present, created 2026-11-27T22:03:40Z', 'bootstrap-gh|True|file present, created 2026-11-27T22:04:30Z',
  'bootstrap-login-shell|True|DefaultShell is C:\Program Files\Git\bin\bash.exe', 'bootstrap-update-policy|True|NoAutoUpdate=1; NoAutoRebootWithLoggedOnUsers=1')
W 'ssh\host-315i.out' ((($evid | ForEach-Object { "S02 evid=$_" }) -join "`n") + "`nS02 system_winget_who=NT AUTHORITY\SYSTEM`nS02 system_winget_exit=-1073741515`nS02 system_winget_out=`nS02 system_winget_task_removed=True`n")
W 'ssh\host-320.out' "S02 exists=claude|True`nS02 size=claude|462`nS02 acl=claude|EC2AMAZ-K3F9Q2L\agent|I)(F`nS02 acl=claude|BUILTIN\Administrators|I)(F`nS02 acl=claude|NT AUTHORITY\SYSTEM|I)(F`nS02 exists=gh|True`nS02 size=gh|96`nS02 acl=gh|EC2AMAZ-K3F9Q2L\agent|I)(F`nS02 acl=gh|BUILTIN\Administrators|I)(F`nS02 acl=gh|NT AUTHORITY\SYSTEM|I)(F`n"

# --- scp ---
W 'scp\scp.exit' '0'

# --- localswim ---
W 'localswim\comment-list.out' "{`"comments`":[{`"id`":`"c1`",`"text`":`"An ordinary comment.`"},{`"id`":`"c2`",`"text`":`"S02 DRY FIXED TEXT 4242`"}]}`n"
"Clean answers written under target\t05\s02-standin-answers: $(@(Get-ChildItem -LiteralPath $root -Recurse -File).Count) file(s)."
