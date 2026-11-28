# Step 315c of E0.12-T05-S02, measure (c): whether Windows PowerShell 5.1, as the login shell, passes the token push's stdin on. It does NOT
# change DefaultShell. It reads, from the host and from provision.log, which shell the provisioning run's push ran under and whether the push
# succeeded: the run's order (`sync done` before `bootstrap done`), bot.env's two lines on the host, and bot.env's creation time against
# the time Git Bash appeared. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# operator.md says (Windows hosts, The remote command on Windows): "with Windows PowerShell 5.1 as the login shell, that it is passed on to a program
# the shell starts is unmeasured". The first boot writes Windows PowerShell 5.1 as DefaultShell and the bootstrap writes Git Bash once Git is
# installed (What a Windows host is), and a provisioning run's sync, with its push, comes before its bootstrap (provision). So a bot.env created
# before Git Bash existed was written by a push that ran under PowerShell 5.1, with its stdin passed on. A bot.env created after Git Bash
# appeared (a later push replaces the file) shows nothing about 5.1: the result is then inconclusive and a direct test is proposed.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315c' 'measure (c): the push''s stdin under Windows PowerShell 5.1 as the login shell, from files and logs')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $docLine = @(Select-String -LiteralPath (Join-Path $script:S02Repo 'docs\operator.md') -Pattern 'is unmeasured' -SimpleMatch | Where-Object { $_.Line -match 'PowerShell 5\.1 as the login shell' } | Select-Object -First 1)
    Write-S02Line ("operator.md line naming the measure: " + $(if ($docLine.Count) { $docLine[0].LineNumber } else { 'not found' }))
    $log = @(Read-S02LogLines (Get-S02ProvisionLogPath))
    $syncDone = @($log | Where-Object { $_ -cmatch '^\S+ sync done' } | Select-Object -Last 1)
    $bootDone = @($log | Where-Object { $_ -cmatch '^\S+ bootstrap done' } | Select-Object -Last 1)
    Assert-S02 ($syncDone.Count -eq 1 -and $bootDone.Count -eq 1) 'provision.log shows sync done and bootstrap done.' 'provision.log shows no sync done or no bootstrap done line.'
    if ($syncDone.Count -eq 1 -and $bootDone.Count -eq 1) {
      $ts = ConvertFrom-S02Utc ($syncDone[0].Split(' ')[0])
      $tb = ConvertFrom-S02Utc ($bootDone[0].Split(' ')[0])
      Write-S02Line ('sync done ' + (Format-S02Time $syncDone[0].Split(' ')[0]) + '; bootstrap done ' + (Format-S02Time $bootDone[0].Split(' ')[0]))
      Assert-S02 ($null -ne $ts -and $null -ne $tb -and $ts -le $tb) 'sync (and its push) came before the bootstrap (and its switch to Git Bash).' 'sync done is after bootstrap done.'
    }
    $h = Invoke-S02HostScript -Alias $alias -Name '315c' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    $lines = Get-S02First $q 'botenv_lines'
    Write-S02Line "bot.env lines: $lines; LOCALSWIM_URL lines: $(Get-S02First $q 'botenv_url_line'); LOCALSWIM_TOKEN lines: $(Get-S02First $q 'botenv_token_line')"
    Write-S02Line "bot.env created $(Format-S02Time (Get-S02First $q 'botenv_created_utc')); written $(Format-S02Time (Get-S02First $q 'botenv_written_utc'))"
    Write-S02Line "Git Bash appeared $(Format-S02Time (Get-S02First $q 'gitbash_created_utc')); DefaultShell now '$(Get-S02First $q 'defaultshell')'; host zone '$(Get-S02First $q 'timezone')'"
    Assert-S02 ($lines -ceq '2') 'bot.env holds two lines.' "bot.env holds '$lines' lines, expected 2."
    Assert-S02 ((Get-S02First $q 'botenv_url_line') -ceq '1' -and (Get-S02First $q 'botenv_token_line') -ceq '1') 'bot.env has one LOCALSWIM_URL line and one LOCALSWIM_TOKEN line, each with a value.' 'bot.env lacks a LOCALSWIM_URL or LOCALSWIM_TOKEN line with a value: the push''s stdin did not arrive whole.'
    $created = ConvertFrom-S02Utc (Get-S02First $q 'botenv_created_utc')
    $bash = ConvertFrom-S02Utc (Get-S02First $q 'gitbash_created_utc')
    if ($null -ne $created -and $null -ne $bash) {
      if ($created -lt $bash) { Write-S02Line 'RESULT (c): bot.env was created before Git Bash existed, so its push ran under Windows PowerShell 5.1 as the login shell, and its stdin was passed on (both lines arrived).' }
      else { Write-S02Line 'RESULT (c): inconclusive. bot.env was created after Git Bash appeared, so a later push (under Git Bash) wrote it; the first push, under 5.1, left no trace on the host. A direct test needs DefaultShell set to 5.1 for one connection (the report names it).' }
    } else {
      Add-S02Finding 'bot.env''s or Git Bash''s creation time could not be read.'
    }
  })
