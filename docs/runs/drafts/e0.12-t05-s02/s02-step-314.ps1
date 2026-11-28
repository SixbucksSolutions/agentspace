# Step 314 of E0.12-T05-S02: over the lease's alias, in Git Bash. One ssh per command (BatchMode, ConnectTimeout, no forwarding): the
# login shell is Git Bash; each agent-windows tool (git, pwsh, node, uv, claude, gh) and localswim is found by `command -v` and answers
# its version or help (which also shows measure (e): the machine PATH and agent's tool folders reach a new ssh session); sshd's DefaultShell
# is Git Bash's path; first-boot.log's lines; the horizon file's bytes; and the `Installed:` lines of provision.log. Run through
# scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# provision.log holds the provisioning run's own step lines alone, so a detached run leaves no `Installed:` line in it
# (operator.md, provision, "Its log"; `sync`'s stdout is the run's, which nothing reads). The count is printed, 0 is a NOTE, and step 316
# is the optional by-hand `sync` that prints them.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '314' 'over the lease alias: Git Bash, the tools, localswim, DefaultShell, first-boot.log, the horizon')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'

    # The login shell.
    $r = Invoke-S02Ssh -Alias $alias -Command 'echo S02 shell0=$0; echo S02 bash=$BASH_VERSION # s02:shell' -TimeoutSeconds 120
    $p = ConvertFrom-S02Probe $r.Stdout
    $bash = Get-S02First $p 'bash'
    $shell0 = Get-S02First $p 'shell0'
    Write-S02Line "login shell: exit $($r.ExitCode); `$0 = '$shell0'; BASH_VERSION = '$bash'"
    Assert-S02 ($r.ExitCode -eq 0) 'The shell probe exited 0.' "The shell probe exited $($r.ExitCode)."
    Assert-S02 ($null -ne $bash -and $bash -cmatch '^\d+\.\d+') 'The login shell is Git Bash (BASH_VERSION is set).' 'BASH_VERSION is empty: the login shell is not Git Bash.'

    # The tools.
    foreach ($tool in 'git', 'pwsh', 'node', 'uv', 'claude', 'gh') {
      $t = Invoke-S02Ssh -Alias $alias -Command "command -v $tool && $tool --version # s02:tool-$tool" -TimeoutSeconds 120
      $lines = @($t.Stdout -split "\r?\n" | Where-Object { $_ -ne '' })
      $found = ($t.ExitCode -eq 0 -and $lines.Count -ge 1)
      $path = if ($lines.Count -ge 1) { $lines[0] } else { '' }
      $version = if ($lines.Count -ge 2) { $lines[1] } else { '' }
      Write-S02Line "tool ${tool}: found $found; path '$path'; version '$version'; exit $($t.ExitCode)"
      Assert-S02 $found "$tool is found by command -v in a new ssh session." "$tool is not found by command -v (exit $($t.ExitCode))."
      Assert-S02 ($version -cmatch '\d+\.\d+') "$tool prints a version." "$tool printed no version."
    }

    # localswim.
    $l = Invoke-S02Ssh -Alias $alias -Command 'command -v localswim; localswim --help > /dev/null 2>&1; echo S02 localswim_exit=$? # s02:localswim' -TimeoutSeconds 120
    $lp = ConvertFrom-S02Probe $l.Stdout
    $lexit = Get-S02First $lp 'localswim_exit'
    $lpath = @($l.Stdout -split "\r?\n" | Where-Object { $_ -ne '' -and -not $_.StartsWith('S02 ') } | Select-Object -First 1)
    Write-S02Line "localswim: path '$(if ($lpath.Count) { $lpath[0] } else { '' })'; --help exit '$lexit'"
    Assert-S02 ($lpath.Count -eq 1) 'localswim is found by command -v.' 'localswim is not found by command -v.'
    if ($lexit -ne '0') {
      $w = Invoke-S02Ssh -Alias $alias -Command 'localswim --help 2>&1 | head -n 2 # s02:localswim-why' -TimeoutSeconds 120
      foreach ($line in ($w.Stdout -split "\r?\n")) { if ($line -ne '') { Write-S02Line ('localswim says: ' + $line) } }
      $known = 'localswim: The CLI is not installed|localswim: No Bot credentials yet|localswim: The board at .* is unreachable'
      $why = if ($w.Stdout -cmatch $known) { 'one of the wrapper''s own refusals (operator.md, The board''s wrapper, steps 1, 2 and 4)' } else { 'an unknown answer' }
      Add-S02Finding "localswim --help did not exit 0 ($lexit); its answer is $why."
    } else {
      Write-S02Line 'ok: localswim --help exited 0 (the wrapper reached the CLI; no board was read).'
    }

    # sshd's DefaultShell, the first-boot log and the horizon, from one host script.
    $h = Invoke-S02HostScript -Alias $alias -Name '314' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    Assert-S02 ($h.Run.ExitCode -eq 0) 'The host script exited 0.' "The host script exited $($h.Run.ExitCode)."
    $ds = Get-S02First $q 'defaultshell'
    $want = Get-S02First $q 'gitbash_expected'
    Write-S02Line "sshd DefaultShell: '$ds'; the file exists: $(Get-S02First $q 'defaultshell_file_exists')"
    Assert-S02 ($null -ne $ds -and $null -ne $want -and [string]::Equals($ds, $want, [StringComparison]::OrdinalIgnoreCase)) 'DefaultShell is Git Bash''s path, C:\Program Files\Git\bin\bash.exe.' "DefaultShell is '$ds', expected C:\Program Files\Git\bin\bash.exe."
    Assert-S02 ((Get-S02First $q 'defaultshell_file_exists') -ceq 'True') 'The DefaultShell file exists.' 'The DefaultShell file does not exist.'

    $count = Get-S02First $q 'firstboot_lines'
    Write-S02Line "first-boot.log lines: $count"
    if ($q.By.ContainsKey('firstboot')) { foreach ($v in $q.By['firstboot']) { Write-S02Line ('first-boot: ' + (Format-S02LogLine $v)) } }
    $fb = @(if ($q.By.ContainsKey('firstboot')) { $q.By['firstboot'] })
    Assert-S02 ($fb.Count -gt 0) 'first-boot.log has lines.' 'first-boot.log is missing or empty.'
    $steps = @('first boot begun', 'step 1 done:', 'step 2 done:', 'step 3 done:', 'step 4 done:', 'step 5 done:', 'step 6 done:', 'step 7 done:', 'step 8 done:')
    foreach ($s in $steps) {
      Assert-S02 (@($fb | Where-Object { $_.Contains($s) }).Count -ge 1) "first-boot.log has '$s'." "first-boot.log has no '$s' line."
    }
    $bad = @($fb | Where-Object { $_ -cmatch '(?i)\bfailed\b' })
    Assert-S02 ($bad.Count -eq 0) 'first-boot.log has no failed line.' "first-boot.log has $($bad.Count) failed line(s)."

    $bytes = Get-S02First $q 'horizon_bytes'
    $cr = Get-S02First $q 'horizon_has_cr'
    $form = Get-S02First $q 'horizon_form'
    Write-S02Line "horizon file: $bytes byte(s); holds 0x0D: $cr; form: $form"
    Assert-S02 ($bytes -ne 'missing' -and $null -ne $bytes) 'The horizon file exists.' 'The horizon file is missing; the guard sends it at its next run, at most 5 minutes after the host runs, so run this step again later.'
    if ($bytes -ne 'missing' -and $null -ne $bytes) {
      Assert-S02 ($cr -ceq 'False') 'The horizon file holds no 0x0D.' 'The horizon file holds a 0x0D.'
      Assert-S02 ($form -ceq 'digits-then-one-LF' -or $form -ceq 'digits-alone') 'The horizon file is the digits then one LF, or the digits alone.' "The horizon file's form is '$form'."
    }

    # sync's Installed: lines, from provision.log.
    $log = @(Read-S02LogLines (Get-S02ProvisionLogPath))
    $inst = @($log | Where-Object { $_.Contains('Installed:') })
    Write-S02Line "provision.log lines holding 'Installed:': $($inst.Count)"
    foreach ($i in $inst) { Write-S02Line ('log: ' + (Format-S02LogLine $i)) }
    if ($inst.Count -eq 0) { Write-S02Line 'NOTE: a detached run copies no program output into provision.log (operator.md, provision, "Its log"); step 316 runs sync by hand and prints the Installed: lines.' }
  })
