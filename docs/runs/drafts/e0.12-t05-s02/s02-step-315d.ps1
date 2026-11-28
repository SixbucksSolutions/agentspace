# Step 315d of E0.12-T05-S02, measure (d): whether winget worked in the bootstrap's session, and which column of operator.md's "The tools" each
# tool took. The bootstrap's own output is not kept (a detached run's stdout is nobody's; operator.md, provision, "Its log"), so this reads what
# the host holds: `winget --version` in a key-based ssh session of agent (the bootstrap's own session type), winget's own log files and which
# package IDs they name, and each tool's file. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
# The columns are INFERRED and printed as such; a failure here is a failure to measure.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315d' 'measure (d): did winget work in a key-based ssh session, and which column did each tool take')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $h = Invoke-S02HostScript -Alias $alias -Name '315d' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    $exit = Get-S02First $q 'winget_version_exit'
    Write-S02Line "winget on the PATH: $(Get-S02First $q 'winget_on_path'); winget --version exit: '$exit'; first line: '$(Get-S02First $q 'winget_version')'"
    Assert-S02 ($null -ne $exit) 'The probe answered about winget.' 'The probe printed no winget answer.'
    $works = ($exit -ceq '0')
    Write-S02Line "RESULT (d): winget answers in a key-based ssh session of agent: $works"
    Write-S02Line "winget log files: $(Get-S02First $q 'winget_log_files'); first $(Format-S02Time (Get-S02First $q 'winget_log_first_utc')); last $(Format-S02Time (Get-S02First $q 'winget_log_last_utc'))"
    $mentions = @{}
    if ($q.By.ContainsKey('winget_log_mentions')) { foreach ($v in $q.By['winget_log_mentions']) { $f = $v -split '\|', 2; $mentions[$f[0]] = [int]$f[1]; Write-S02Line "winget logs naming $($f[0]): $($f[1]) file(s)" } }
    $ids = [ordered]@{ 'PowerShell 7' = 'Microsoft.PowerShell'; 'Git for Windows' = 'Git.Git'; 'Node' = 'OpenJS.NodeJS.LTS'; 'uv' = 'astral-sh.uv'; 'Claude Code' = ''; 'gh' = 'GitHub.cli' }
    $present = @{}
    if ($q.By.ContainsKey('tool_file')) { foreach ($v in $q.By['tool_file']) { $f = $v -split '\|', 3; $present[$f[0]] = ($f[1] -ceq 'True'); Write-S02Line "tool $($f[0]): file present $($f[1]); created $(Format-S02Time $f[2])" } }
    Write-S02Line "winget upgrade lines in patch.log (agentspace-tools.ps1 as SYSTEM): $(Get-S02First $q 'patch_log_winget_upgrade_lines')"
    foreach ($k in $ids.Keys) {
      Assert-S02 ($present.ContainsKey($k) -and $present[$k]) "$k is present." "$k is not present on the host."
      $id = $ids[$k]
      $col = if ($id -eq '') { 'own installer (the table lists no winget package for it)' }
             elseif ($works -and $mentions.ContainsKey($id) -and $mentions[$id] -gt 0) { 'winget (a winget log names the package)' }
             elseif ($works) { 'own installer (winget answers, but no winget log names the package)' }
             else { 'own installer (winget does not answer in this session type)' }
      Write-S02Line "column of The tools taken by ${k} (inferred): $col"
    }
    Write-S02Line 'NOTE: PowerShell 7 is put in place by install.ps1 step 1 (its own MSI) before the bootstrap runs, so the bootstrap finds it present.'
  })
