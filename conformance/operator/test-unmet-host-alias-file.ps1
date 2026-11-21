#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of a host's operator alias (docs/operator.md, "How the operator reaches a host",
# step 4; docs/state.md, "A host's operator alias", "The `Include` line", "Agentspace's host keys";
# "Which checks show it", the rows for the operator's alias file), run through the test seam. Story
# E0.07-T04-S02 (#0156). The command that writes it here is `bootstrap -Host other`, whose first ssh
# is a stand-in that records, for each call, whether the alias file and the operator's certificate
# already exist, so the "before its first ssh" order is shown too. It checks:
#   - the file's bytes, exactly: UTF-8 with no byte order mark, LF line endings, one final LF, the owner
#     mark `# agentspace host <project>; <comment>`, then the ten lines of state.md in order with the
#     key, certificate and known-hosts paths as the home gives them, `StrictHostKeyChecking
#     accept-new`, `HostKeyAlias agentspace-host.<project>`, and a ProxyCommand naming
#     agentspace-ssh-proxy.exe beside the operator, then the alias; no RemoteForward line;
#   - the Include line: no config is created holding the line and one LF alone; an existing config
#     gets the line first, every other byte kept (a byte order mark and CR LF endings too), after a
#     backup equal to the original; a line already there, in any spacing and case, changes nothing
#     and makes no backup; a line below the first Host fails the command with the Config message,
#     changes nothing and starts no ssh;
#   - a file at the alias's path that is not this host's: no owner mark, a lease's mark, another
#     host's mark, a folder: the command fails with the spec's message, leaves it as it is, starts no ssh;
#   - an own file with other bytes is replaced; an own file whose bytes are the new ones is not written
#     (its modification time stays);
#   - a lease's create and release, through the lease CLI, leave the operator's alias as it was (the
#     lease alias's name holds no dot, so a lease can never overwrite it: nothing more is reachable
#     from outside);
#   - the operator never writes agentspace's known_hosts, nor the user's known_hosts.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA, APPDATA and the ssh folder into the
# throwaway home and removes every AWS credential, and the check confirms first that `aws sts
# get-caller-identity` fails under that environment for every AWS CLI the operator can run (SKIPPED if
# not). The operator's certificate is signed by the throwaway CA. The real ssh-keygen makes the
# operator's key (the seam has none); nothing reaches a host. Every ID is made up. A failure reason
# never quotes the program's output.
#
# The programs under test are the ones AGENTSPACE_OPERATOR_CMD and AGENTSPACE_CMD name; unset, each
# takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-alias-file.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'operator.md#how-the-operator-reaches-a-host'
$aliasSec = 'state.md#a-hosts-operator-alias'
$includeSec = 'state.md#the-include-line'
$hostsSec = 'state.md#agentspaces-host-keys'
$project = 'other'
$aliasName = "agentspace-host.$project"

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the operator writes a host's alias"
  $operator = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $operator) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } elseif (-not $h.CaReady) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $h.CaReason
  } else {
    $S = New-HpStandIns $h
    $S.Ssh = New-OperatorStandIn -ConformanceHome $h -Name 'ssh-alias' -Body @(
      'set "D=%~dp0"',
      'echo %* >>"%D%ssh-full.log"',
      ('if exist "%AGENTSPACE_SSH_DIR%\agentspace.d\' + $aliasName + '" (echo alias-present >>"%D%ssh-state.log") else (echo alias-absent >>"%D%ssh-state.log")'),
      ('if exist "%USERPROFILE%\.agentspace\hosts\' + $project + '\id_ed25519-cert.pub" (echo cert-present >>"%D%ssh-state.log") else (echo cert-absent >>"%D%ssh-state.log")'),
      'findstr "^" >nul',
      'exit /b 0')
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-HpHostRecord $project 'ready' $HpIdOther))
    $agentspaceDir = Join-Path $h.UserProfile '.agentspace'
    $aliasDir = Join-Path $h.SshDir 'agentspace.d'
    $aliasPath = Join-Path $aliasDir $aliasName
    $configPath = Join-Path $h.SshDir 'config'
    $knownHosts = Join-Path $agentspaceDir 'known_hosts'
    $userKnownHosts = Join-Path $h.SshDir 'known_hosts'
    $programPath = Join-Path (Split-Path -Parent $operator) 'agentspace-ssh-proxy.exe'
    $utf8 = [Text.UTF8Encoding]::new($false)

    function Reset-Ssh {
      foreach ($f in Get-ChildItem -LiteralPath $h.SshDir -Force -ErrorAction SilentlyContinue) { Remove-HpPath $h $f.FullName }
      foreach ($f in Get-ChildItem -LiteralPath $S.Dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*.log' }) { Remove-HpPath $h $f.FullName }
    }
    function Invoke-Alias { Invoke-HpOperator $h $S @('bootstrap', '-Host', $project) }
    function Get-SshStateLog { @(Get-HpLines (Join-Path $S.Dir 'ssh-state.log')) }
    function Get-SshCalls { @(Get-HpLines (Join-Path $S.Dir 'ssh-full.log')).Count }
    function ConvertTo-AliasPath([string]$Path) { $Path -replace '\\', '/' -replace '%', '%%' }
    # The lines of the file after the owner mark, as state.md gives them.
    function Get-ExpectedAliasLines {
      $home2 = $h.UserProfile.TrimEnd('\', '/')
      @(
        "Host $aliasName",
        '  User ubuntu',
        ('  IdentityFile "' + (ConvertTo-AliasPath "$home2\.agentspace\hosts\$project\id_ed25519") + '"'),
        ('  CertificateFile "' + (ConvertTo-AliasPath "$home2\.agentspace\hosts\$project\id_ed25519-cert.pub") + '"'),
        '  IdentitiesOnly yes',
        "  HostKeyAlias $aliasName",
        ('  UserKnownHostsFile "' + (ConvertTo-AliasPath "$home2\.agentspace\known_hosts") + '"'),
        '  StrictHostKeyChecking accept-new',
        ('  ProxyCommand "' + ($programPath -replace '%', '%%') + '" ' + $aliasName),
        '  ServerAliveInterval 30',
        '  ServerAliveCountMax 4')
    }
    function Test-AliasBytes([string]$Path) {
      $p = New-HpProblems
      if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { $p.Add('The alias file does not exist'); return , $p }
      $bytes = [IO.File]::ReadAllBytes($Path)
      if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { $p.Add('The file starts with a byte order mark') }
      $text = $utf8.GetString($bytes)
      if ($text.Contains("`r")) { $p.Add('The file holds a CR') }
      if (-not $text.EndsWith("`n") -or $text.EndsWith("`n`n")) { $p.Add('The file does not end in exactly one LF') }
      $lines = @($text.TrimEnd("`n") -split "`n")
      $want = Get-ExpectedAliasLines
      if ($lines.Count -ne $want.Count + 1) { $p.Add("The file has $($lines.Count) lines, not $($want.Count + 1)") }
      else {
        if ($lines[0] -cnotmatch ('\A# agentspace host ' + [regex]::Escape($project) + '; ')) { $p.Add('The first line is not the owner mark "# agentspace host <project>; <comment>"') }
        for ($i = 0; $i -lt $want.Count; $i++) {
          $got = $lines[$i + 1]
          # The program's path is the operator's own, found as a full path: its case MAY differ in spelling.
          $same = if ($want[$i] -like '  ProxyCommand *') { $got -ceq $want[$i] -or $got -ieq $want[$i] } else { $got -ceq $want[$i] }
          if (-not $same) { $p.Add("Line $($i + 2) is not as state.md gives it (expected: $($want[$i].Trim().Split(' ')[0]))") }
        }
      }
      if ($text -match 'RemoteForward') { $p.Add('The file holds a RemoteForward line') }
      , $p
    }

    # ---- The first write: no config, no alias ----
    Reset-Ssh
    $khSeed = "agentspace-host.$project ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIconformanceplaceholderkeytext`n"
    Set-HpFile $h $knownHosts $khSeed
    $khHash = Get-HpBytesHash $knownHosts
    $r = Invoke-Alias
    $p = New-HpProblems
    if ($r.TimedOut) { $p.Add('It timed out') } elseif ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    if ((Get-SshCalls) -ne 1) { $p.Add("ssh was started $(Get-SshCalls) times, not once") }
    foreach ($x in (Test-AliasBytes $aliasPath)) { $p.Add($x) }
    Write-HpProblems "${aliasSec}: The operator's alias holds exactly the bytes state.md gives, for a host (no byte order mark, LF, the owner mark, accept-new, its own HostKeyAlias and known_hosts, the proxy program beside the operator, no RemoteForward)" $p

    $p = New-HpProblems
    $log = Get-SshStateLog
    if ($log.Count -lt 2 -or $log[0] -cne 'alias-present' -or $log[1] -cne 'cert-present') { $p.Add('The alias file and the operator''s certificate were not both there when the first ssh started') }
    Write-HpProblems "${sec}: The alias and the certificate are written before the command's first ssh (step 4)" $p

    $p = New-HpProblems
    $cfg = Get-HpText $configPath
    if ($null -eq $cfg) { $p.Add('No config file was made') }
    elseif ($cfg -cne "Include agentspace.d/*`n") { $p.Add('The new config is not exactly the Include line and one LF') }
    Write-HpProblems "${includeSec}: With no config, the operator creates it holding exactly the Include line and one LF" $p

    # ---- agentspace's known_hosts is never written ----
    $p = New-HpProblems
    if ((Get-HpBytesHash $knownHosts) -cne $khHash) { $p.Add('agentspace''s known_hosts changed') }
    if (Test-Path -LiteralPath $userKnownHosts) { $p.Add('The user''s known_hosts was made') }
    Write-HpProblems "${hostsSec}: The operator never writes agentspace's known_hosts, and never the user's for a host that is not the box" $p
    Remove-HpPath $h $knownHosts
    Reset-Ssh
    $r = Invoke-Alias
    $p = New-HpProblems
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    if (Test-Path -LiteralPath $knownHosts) { $p.Add('agentspace''s known_hosts was made by the operator') }
    if (Test-Path -LiteralPath $userKnownHosts) { $p.Add('The user''s known_hosts was made') }
    Write-HpProblems "${hostsSec}: With no known_hosts, the operator makes none (only ssh writes the file)" $p

    # ---- The Include line on each shape of config ----
    $includeCases = @(
      @{ N = 'an empty config (0 bytes) becomes the Include line and one LF'; Orig = [byte[]]@(); Want = $utf8.GetBytes("Include agentspace.d/*`n"); Backup = $true },
      @{ N = 'a config with Host blocks gets the line first, every other byte kept'; Orig = $utf8.GetBytes("Host x`n  HostName y`n"); Want = $utf8.GetBytes("Include agentspace.d/*`nHost x`n  HostName y`n"); Backup = $true },
      @{ N = 'a config with a byte order mark and CR LF endings gets the line after the mark with CR LF, every other byte kept'; Orig = ([byte[]]@(0xEF, 0xBB, 0xBF) + $utf8.GetBytes("Host x`r`n  HostName y`r`n")); Want = ([byte[]]@(0xEF, 0xBB, 0xBF) + $utf8.GetBytes("Include agentspace.d/*`r`nHost x`r`n  HostName y`r`n")); Backup = $true },
      @{ N = 'a config with no final line ending keeps none'; Orig = $utf8.GetBytes("Host x`n  HostName y"); Want = $utf8.GetBytes("Include agentspace.d/*`nHost x`n  HostName y"); Backup = $true },
      @{ N = 'a config that already has the line above its first Host is left byte for byte, with no backup'; Orig = $utf8.GetBytes("Include agentspace.d/*`nHost x`n  HostName y`n"); Want = $utf8.GetBytes("Include agentspace.d/*`nHost x`n  HostName y`n"); Backup = $false },
      @{ N = 'a config whose line has other spacing and case (include   AGENTSPACE.D/*) counts as found and is left byte for byte'; Orig = $utf8.GetBytes("include   AGENTSPACE.D/*`nHost x`n"); Want = $utf8.GetBytes("include   AGENTSPACE.D/*`nHost x`n"); Backup = $false }
    )
    foreach ($c in $includeCases) {
      Reset-Ssh
      Set-HpBytes $h $configPath $c.Orig
      $r = Invoke-Alias
      $p = New-HpProblems
      if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
      $got = if (Test-Path -LiteralPath $configPath -PathType Leaf) { [IO.File]::ReadAllBytes($configPath) } else { $null }
      if ($null -eq $got) { $p.Add('The config is gone') }
      elseif ([BitConverter]::ToString($got) -cne [BitConverter]::ToString([byte[]]$c.Want)) { $p.Add('The config''s bytes are not the original with the one line inserted') }
      $backups = @(Get-ChildItem -LiteralPath $h.SshDir -Filter 'config.agentspace-backup-*' -File -ErrorAction SilentlyContinue)
      if ($c.Backup) {
        $orig = $c.Orig
        if ($orig.Count -eq 0 -and $backups.Count -eq 0) { }  # An empty config has nothing to keep; a backup is allowed but not required here.
        elseif ($backups.Count -ne 1) { $p.Add("There are $($backups.Count) backups, not one") }
        elseif ([BitConverter]::ToString([IO.File]::ReadAllBytes($backups[0].FullName)) -cne [BitConverter]::ToString([byte[]]$orig)) { $p.Add('The backup is not equal to the original') }
      } elseif ($backups.Count -gt 0) { $p.Add('A backup was made though nothing was inserted') }
      if (-not (Test-Path -LiteralPath $aliasPath -PathType Leaf)) { $p.Add('The alias file was not written') }
      Write-HpProblems "${includeSec}: The operator, writing a host's alias, finds the Include line: $($c.N)" $p
    }

    # An Include line below the first Host fails the command, changing nothing.
    Reset-Ssh
    $orig = $utf8.GetBytes("Host x`n  HostName y`nInclude agentspace.d/*`n")
    Set-HpBytes $h $configPath $orig
    $r = Invoke-Alias
    $p = New-HpProblems
    if ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
    if (-not (@($r.Stderr -split "`r?`n") -cmatch '\Aagentspace: .*config cannot take agentspace''s Include line: .+\.\z')) { $p.Add('Stderr lacks "agentspace: <config> cannot take agentspace''s Include line: <why>."') }
    if ([BitConverter]::ToString([IO.File]::ReadAllBytes($configPath)) -cne [BitConverter]::ToString($orig)) { $p.Add('The config changed') }
    if (Test-Path -LiteralPath $aliasPath) { $p.Add('The alias file was written') }
    if ((Get-SshCalls) -ne 0) { $p.Add('ssh was started') }
    Write-HpProblems "${includeSec}: An Include line below the first Host fails the command with the Config message, changes nothing and starts no ssh" $p

    # ---- A file at the alias's path that is not this host's ----
    $leaseMark = "# agentspace lease 1111111a-1111-4111-8111-11111111111a; written by conformance`nHost agentspace-other-1a2b3c4d`n"
    $foreign = @(
      @{ N = 'a file with no owner mark'; Write = { Set-HpFile $h $aliasPath "Host $aliasName`n  User someone`n" } },
      @{ N = 'a file with a lease''s owner mark'; Write = { Set-HpFile $h $aliasPath $leaseMark } },
      @{ N = 'a file with another host''s owner mark'; Write = { Set-HpFile $h $aliasPath "# agentspace host another; written by conformance`nHost $aliasName`n" } },
      @{ N = 'an empty file'; Write = { Set-HpFile $h $aliasPath '' } },
      @{ N = 'a folder'; Write = { New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $aliasPath) | Out-Null } }
    )
    foreach ($c in $foreign) {
      Reset-Ssh
      New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $aliasDir) | Out-Null
      & $c.Write
      $before = Get-HpBytesHash $aliasPath
      $r = Invoke-Alias
      $p = New-HpProblems
      if ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
      $m = @($r.Stderr -split "`r?`n" | Where-Object { $_ -cmatch ("\Aagentspace: .+ is not agentspace's alias for '" + $project + "'; it was left as it is\.\z") })
      if ($m.Count -eq 0) { $p.Add('Stderr lacks "agentspace: <path> is not agentspace''s alias for ''<project>''; it was left as it is."') }
      if ((Get-HpBytesHash $aliasPath) -cne $before) { $p.Add('The file at the alias''s path changed') }
      if ($c.N -eq 'a folder' -and -not (Test-Path -LiteralPath $aliasPath -PathType Container)) { $p.Add('The folder is gone') }
      if ((Get-SshCalls) -ne 0) { $p.Add('ssh was started') }
      Write-HpProblems "${aliasSec}: $($c.N[0].ToString().ToUpperInvariant() + $c.N.Substring(1)) at the alias's path is refused with the spec's message, left as it is, and no ssh is started" $p
    }

    # ---- An own file: replaced when its bytes differ, untouched when they are the new ones ----
    Reset-Ssh
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $aliasDir) | Out-Null
    Set-HpFile $h $aliasPath "# agentspace host $project; an older install wrote this`nHost $aliasName`n  User ubuntu`n"
    $r = Invoke-Alias
    $p = New-HpProblems
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    foreach ($x in (Test-AliasBytes $aliasPath)) { $p.Add($x) }
    Write-HpProblems "${aliasSec}: An own file with other bytes is replaced by the new ones" $p

    $old = [DateTime]::new(2001, 1, 1, 0, 0, 0, [DateTimeKind]::Utc)
    [IO.File]::SetLastWriteTimeUtc((Assert-ConformancePath -ConformanceHome $h -Path $aliasPath), $old)
    $hashBefore = Get-HpBytesHash $aliasPath
    $r = Invoke-Alias
    $p = New-HpProblems
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    if ((Get-HpBytesHash $aliasPath) -cne $hashBefore) { $p.Add('The bytes changed') }
    if ([IO.File]::GetLastWriteTimeUtc($aliasPath) -ne $old) { $p.Add('The file was written again though its bytes were the new ones') }
    Write-HpProblems "${aliasSec}: An own file whose bytes are the new ones is not written again" $p

    # ---- A lease's create and release leave the operator's alias ----
    $nameL = "${aliasSec}: A lease's create and release leave the operator's alias as it was"
    $cli = Get-ConformanceProgram -Variable 'AGENTSPACE_CMD'
    if (-not $cli) {
      Write-ConformanceResult -Name $nameL -Status FAIL -Reason 'Neither AGENTSPACE_CMD nor its default names a program'
    } else {
      $aliasBefore = Get-HpBytesHash $aliasPath
      # A case that can reach `pending` MUST name a stand-in provisioning program, so the lease CLI never
      # starts the real operator (operator.md, "The test seam", step 8). The host is ready, so it is never run.
      $provisionStandIn = New-OperatorStandIn -ConformanceHome $h -Name 'provision-tripwire' -Body @('exit /b 0')
      $create = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', $project) -Environment @{ AGENTSPACE_PROVISION_CMD = $provisionStandIn }
      $doc = $create.Json
      $p = New-HpProblems
      if ((Get-StandInCallCount -Log (Join-Path $S.Dir 'provision-tripwire-calls.log')) -gt 0) { $p.Add('create started a provisioning run for a ready host') }
      $leaseAlias = $null
      if ($create.ExitCode -ne 0 -or $null -eq $doc -or -not (Test-ConformanceSchema -Json $create.Stdout.TrimEnd("`n") -Schema 'lease.output.schema.json') -or $doc['state'] -cne 'ready') {
        $p.Add('create did not return a ready lease for the host, so the case reached nothing')
      } else {
        $leaseAlias = [string]$doc['ssh']['host_alias']
        $leaseId = [string]$doc['id']
        if ($leaseAlias -cnotmatch '\Aagentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}\z') { $p.Add('The lease''s host_alias does not match its pattern, so no path is built from it'); $leaseAlias = $null }
      }
      if ($null -ne $leaseAlias) {
        $leaseAliasPath = Join-Path $aliasDir $leaseAlias
        [void](Assert-ConformancePath -ConformanceHome $h -Path $leaseAliasPath)
        if (-not (Test-Path -LiteralPath $leaseAliasPath -PathType Leaf)) { $p.Add('The lease''s own alias file was not written') }
        if ((Get-HpBytesHash $aliasPath) -cne $aliasBefore) { $p.Add('create changed the operator''s alias') }
        if ($leaseAlias -eq $aliasName) { $p.Add('The lease''s alias is the operator''s alias''s name') }
        $release = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'release', $leaseId)
        if ($release.ExitCode -ne 0) { $p.Add("release exited $($release.ExitCode), not 0") }
        if (Test-Path -LiteralPath $leaseAliasPath) { $p.Add('release left the lease''s alias file') }
        if (-not (Test-Path -LiteralPath $aliasPath -PathType Leaf)) { $p.Add('release removed the operator''s alias') }
        elseif ((Get-HpBytesHash $aliasPath) -cne $aliasBefore) { $p.Add('release changed the operator''s alias') }
      }
      Write-HpProblems $nameL $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-alias-file.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun
