# Step 320 of E0.12-T05-S02, after Terry's sign-ins (README, Signing in on a new Windows host, steps 3 to 6, done by Terry and not by the runner):
# the two icacls checks re-read over the OPERATOR alias, masked, which MUST name no account but agent, Administrators and SYSTEM; whether gh fell
# back to its own file (hosts.yml exists: True or False); and whether Claude Code's .credentials.json exists. No file's content is read or printed.
# It signs the operator's one-hour certificate again first, as README's step 3 says. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '320' 'after Terry''s sign-ins: the icacls checks, hosts.yml and .credentials.json exist')) { exit 5 }
exit (Invoke-S02Main {
    $r = Invoke-S02Tool -Tool 'operator' -Arguments @('status', '-Host', 'agentspace', '-Platform', 'windows/amd64') -TimeoutSeconds 240
    Write-S02Line "status -Host agentspace -Platform windows/amd64 exit: $($r.ExitCode) (signs the operator's certificate again)"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'The operator certificate was signed.' "status -Host exited $($r.ExitCode)."
    $h = Invoke-S02HostScript -Alias $script:S02OperatorAlias -Name '320' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    $exists = @{}
    if ($q.By.ContainsKey('exists')) { foreach ($v in $q.By['exists']) { $f = $v -split '\|', 2; $exists[$f[0]] = $f[1] } }
    Write-S02Line "Claude Code's .credentials.json exists: $($exists['claude'])"
    Write-S02Line "gh fell back to its file (hosts.yml exists): $($exists['gh'])"
    Assert-S02 ($exists['claude'] -ceq 'True') '.credentials.json exists.' '.credentials.json does not exist on the host: Claude Code kept no sign-in file.'
    Assert-S02 ($exists['gh'] -ceq 'True') 'hosts.yml exists (gh fell back to its file).' 'hosts.yml does not exist: gh did not fall back to its file, and README step 5 changes.'
    foreach ($k in 'claude', 'gh') {
      $acl = @()
      if ($q.By.ContainsKey('acl')) { foreach ($v in $q.By['acl']) { $f = $v -split '\|', 3; if ($f[0] -ceq $k) { $acl += [pscustomobject]@{ Account = $f[1]; Flags = $f[2] } } } }
      foreach ($a in $acl) { Write-S02Line "icacls $k`: $($a.Account):($($a.Flags)" }
      if ($exists[$k] -ceq 'True') {
        Assert-S02 ($acl.Count -ge 1) "icacls for $k printed its access list." "icacls for $k printed no access list."
        $bad = @($acl | Where-Object {
            $name = ConvertTo-S02Safe $_.Account
            -not ($name -ceq 'BUILTIN\Administrators' -or $name -ceq 'NT AUTHORITY\SYSTEM' -or ($name -cmatch '^[^\\]+\\agent$' -and $name -cnotmatch '^(BUILTIN|NT AUTHORITY)\\'))
          })
        Assert-S02 ($bad.Count -eq 0) "icacls for $k names no account but agent, Administrators and SYSTEM." "icacls for $k names $($bad.Count) other account(s): $(($bad | ForEach-Object { $_.Account }) -join ', ')."
      }
    }
  })
