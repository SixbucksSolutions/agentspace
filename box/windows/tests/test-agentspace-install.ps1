#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests box/windows/install.ps1's sshd_config edit (E0.12-T03-S07, #0260; D7, D95, D96; #0193's Windows
# twin; WB22), written from docs/operator.md, "sshd's settings on a Windows host" and "The tests'
# seams", and not from the script.
#
# Seams (operator.md, The tests' seams): AGENTSPACE_INSTALL_ONLY set to sshd-config,
# AGENTSPACE_SSHD_CONFIG (the sshd_config file, beside which the CA file is agentspace_ca.pub) and
# AGENTSPACE_SSHD_TEST_PROGRAM, set together: step 9's edit runs alone, `sshd.exe -t -f <file>` is the
# program named given the temporary file's full path as its one argument, `sshd -T` is the same program
# given the one argument -T, and in place of restarting sshd the script writes `sshd-restarted` beside
# the file. The program is a .cmd stand-in made here. The staged copy of box/windows (beside which the
# CA's public key is, as sync stages it) is made in the throwaway home. The script's folder is
# -ScriptDir (default: box/windows beside this folder).
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'install'
$resolvedDir = Resolve-WtScriptDir $ScriptDir
$sourcePath = Join-Path $resolvedDir 'install.ps1'
$testHome = New-WtHome
$latin1 = [Text.Encoding]::Latin1

# operator.md's lines, exactly (N11), without the line endings.
$agentspaceLines = @(
  '# agentspace begin: installed by agentspace-operator sync from box/windows/sshd_config.agentspace; every sync replaces these lines (D7, D95, D96)',
  'TrustedUserCAKeys __PROGRAMDATA__/ssh/agentspace_ca.pub',
  'PubkeyAuthentication yes',
  'PasswordAuthentication no',
  'KbdInteractiveAuthentication no',
  'AllowUsers agent',
  'LoginGraceTime 30',
  'ClientAliveInterval 15',
  'ClientAliveCountMax 5',
  '# agentspace end'
)
$exemptLine = 'PerSourcePenaltyExemptList 127.0.0.1,::1'
$probeWith = "port 22`nlogingracetime 30`npersourcepenalties crash:1 authfail:1`npersourcepenaltyexemptlist 127.0.0.1,::1`nclientaliveinterval 15`n"
$probeWithout = "port 22`nlogingracetime 30`npersourcepenalties`nclientaliveinterval 15`n"

# The oracle: what the edit MUST produce from the old bytes, written from the spec's steps 1 to 4 and 3.
# Bytes are handled as Latin-1 text so each byte is one character. Returns the new bytes, or the line
# number (from 1) of a begin line with no end line after it.
function Get-WtExpectedConfig {
  param([Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Old, [switch]$Exempt)
  $text = $latin1.GetString($Old)
  $parts = [Collections.Generic.List[string]]::new()
  $start = 0
  while ($start -lt $text.Length) {
    $nl = $text.IndexOf("`n", $start)
    if ($nl -lt 0) { $parts.Add($text.Substring($start)); break }
    $parts.Add($text.Substring($start, $nl - $start + 1))
    $start = $nl + 1
  }
  $kept = [Text.StringBuilder]::new()
  $inBlock = $false
  $beginAt = 0
  $n = 0
  foreach ($p in $parts) {
    $n++
    $body = $p
    if ($body.EndsWith("`n")) { $body = $body.Substring(0, $body.Length - 1) }
    if ($body.EndsWith("`r")) { $body = $body.Substring(0, $body.Length - 1) }
    if (-not $inBlock) {
      if ($body.StartsWith('# agentspace begin:', [StringComparison]::Ordinal)) { $inBlock = $true; $beginAt = $n; continue }
      [void]$kept.Append($p)
    } else {
      if ($body -ceq '# agentspace end') { $inBlock = $false }
    }
  }
  if ($inBlock) { return $beginAt }
  $block = [Text.StringBuilder]::new()
  foreach ($l in $agentspaceLines) {
    if ($Exempt -and $l -ceq '# agentspace end') { [void]$block.Append($exemptLine + "`r`n") }
    [void]$block.Append($l + "`r`n")
  }
  return , $latin1.GetBytes($block.ToString() + $kept.ToString())
}

function Get-WtSddl { param([string]$Path) return (Get-Acl -LiteralPath $Path).Sddl }

# A case: the staged copy of box/windows with the new CA's public key beside install.ps1, the ssh
# folder holding sshd_config and the old CA file, and the stand-in program. -NoCa leaves the CA out of
# the stage (for a case that must not get as far as the edit).
function New-WtInstall {
  param([Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Config, [string]$Probe = '', [int]$TestExit = 0, [switch]$NoCa, [string]$OldCa = "OLD-CA-KEY`n")
  $dir = New-WtCase -TestHome $testHome
  $stage = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'stage')
  foreach ($f in (Get-ChildItem -LiteralPath $resolvedDir -File -Force)) {
    Copy-Item -LiteralPath $f.FullName -Destination (Assert-ConformancePath -ConformanceHome $testHome -Path (Join-Path $stage $f.Name))
  }
  $caText = "ssh-ed25519 NEW-CA-KEY-FOR-TESTS agentspace-test-ca`n"
  if (-not $NoCa) { Write-WtText -TestHome $testHome -Path (Join-Path $stage 'agentspace_ca.pub') -Text $caText }
  $ssh = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'ssh')
  Write-WtBytes -TestHome $testHome -Path (Join-Path $ssh 'sshd_config') -Bytes $Config
  Write-WtText -TestHome $testHome -Path (Join-Path $ssh 'agentspace_ca.pub') -Text $OldCa
  $standin = New-WtSshdStandIn -TestHome $testHome -Folder (Join-Path $dir 'standin') -TestExit $TestExit -ProbeOutput $Probe
  return [pscustomobject]@{
    Dir = $dir; Stage = $stage; Script = Join-Path $stage 'install.ps1'
    Ssh = $ssh; Config = Join-Path $ssh 'sshd_config'; Ca = Join-Path $ssh 'agentspace_ca.pub'
    StageCa = Join-Path $stage 'agentspace_ca.pub'; NewCaBytes = $script:WtUtf8.GetBytes($caText)
    Standin = $standin; Calls = Join-Path (Split-Path -Parent $standin) 'calls.txt'; Copy = Join-Path (Split-Path -Parent $standin) 'test-copy.txt'
    Restarted = Join-Path $ssh 'sshd-restarted'
  }
}

function Get-WtInstallSeam {
  param($Case)
  return @{ AGENTSPACE_INSTALL_ONLY = 'sshd-config'; AGENTSPACE_SSHD_CONFIG = $Case.Config; AGENTSPACE_SSHD_TEST_PROGRAM = $Case.Standin }
}

function Invoke-WtInstall {
  param([Parameter(Mandatory)]$Case, [hashtable]$Seam = $null)
  if ($null -eq $Seam) { $Seam = Get-WtInstallSeam -Case $Case }
  return Invoke-WtScript -TestHome $testHome -ScriptPath $Case.Script -Seam $Seam
}

# A run that MUST have written the file: the expected bytes, the line, sshd restarted, exit 0.
function Assert-WtEdited {
  param($Case, $Result, [byte[]]$Old, [switch]$Exempt)
  Assert-WtEqual $Result.ExitCode 0 'Exit code'
  if ($Exempt) { $expected = Get-WtExpectedConfig -Old $Old -Exempt } else { $expected = Get-WtExpectedConfig -Old $Old }
  Assert-WtBytes -Actual (Read-WtBytes -Path $Case.Config) -Expected $expected -What 'sshd_config'
  Assert-WtStdoutLine -Result $Result -Pattern '^Installed: sshd_config holds the agentspace lines \(added\)$'
  Assert-Wt ($null -ne (Read-WtUnixLine -Path $Case.Restarted)) ("sshd MUST be restarted (sshd-restarted written beside the file). {0}" -f (Format-WtRun $Result))
}

$defaultConfig = $latin1.GetBytes("#Port 22`r`n#AddressFamily any`r`nSubsystem sftp sftp-server.exe`r`n`r`nMatch Group administrators`r`n       AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys`r`n")

try {
  Assert-Wt (Test-Path -LiteralPath $sourcePath -PathType Leaf) 'The script under test is missing: install.ps1'
  $realBefore = Get-WtRealMark

  # --- The lines themselves ------------------------------------------------------------------------------------
  Test-WtCase 'Lines: sshd_config.agentspace holds exactly operator.md''s lines, nothing before or after' {
    $p = Join-Path $resolvedDir 'sshd_config.agentspace'
    Assert-Wt (Test-Path -LiteralPath $p -PathType Leaf) 'sshd_config.agentspace is missing'
    $got = (Read-WtText -Path $p) -replace "`r`n", "`n"
    Assert-WtEqual $got (($agentspaceLines -join "`n") + "`n") 'The file'
  }
  Test-WtCase 'Lines: The file holds no PerSourcePenaltyExemptList line: the install adds it when sshd knows the keyword' {
    Assert-Wt (-not ((Read-WtText -Path (Join-Path $resolvedDir 'sshd_config.agentspace')) -match 'PerSourcePenaltyExemptList')) 'The file MUST NOT hold the exemption line'
  }

  # --- The edit ------------------------------------------------------------------------------------------------------
  Test-WtCase 'Edit: A file with no agentspace lines gets them at the top, each ending CR LF, and every line after byte for byte' {
    $c = New-WtInstall -Config $defaultConfig
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $defaultConfig
  }
  Test-WtCase 'Edit: The lines go above the first Match line, which is kept with its block' {
    $c = New-WtInstall -Config $defaultConfig
    [void](Invoke-WtInstall -Case $c)
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt ($t.IndexOf('# agentspace end') -ge 0 -and $t.IndexOf('# agentspace end') -lt $t.IndexOf('Match Group administrators')) 'The agentspace lines MUST be above the Match line'
  }
  Test-WtCase 'Edit: The file is read as bytes: LF lines, CR LF lines, a byte that is no UTF-8 and a last line with no ending are kept' {
    $old = [byte[]]([Text.Encoding]::ASCII.GetBytes("# comment lf`nPort 22`r`n# caf") + @([byte]0xE9) + [Text.Encoding]::ASCII.GetBytes("`r`n`n`n# last line with no ending"))
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
  }
  Test-WtCase 'Edit: An empty file gets the lines alone' {
    $c = New-WtInstall -Config ([byte[]]@())
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old ([byte[]]@())
  }
  Test-WtCase 'Edit: A block at the top is replaced by the new lines, and its own old lines are gone' {
    $old = $latin1.GetBytes("# agentspace begin: old`r`nAllowUsers someone`r`nLoginGraceTime 120`r`n# agentspace end`r`nPort 22`r`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt (-not $t.Contains('AllowUsers someone')) 'A line of the old block MUST be removed'
    Assert-Wt (-not $t.Contains('LoginGraceTime 120')) 'A line of the old block MUST be removed'
  }
  Test-WtCase 'Edit: A block in the middle or at the end is removed wherever it is, and the new lines go to the top' {
    $old = $latin1.GetBytes("Port 22`nSubsystem sftp x`n# agentspace begin: old`nAllowUsers someone`n# agentspace end`nMatch Group administrators`n  X yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt (-not $t.Contains('AllowUsers someone')) 'A line of the old block MUST be removed'
  }
  Test-WtCase 'Edit: A block with LF endings is found, and one with CR LF endings is found' {
    $old = $latin1.GetBytes("Port 22`r`n# agentspace begin: a`r`nA 1`r`n# agentspace end`r`n# agentspace begin: b`nB 1`n# agentspace end`nLast yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt (-not $t.Contains('A 1') -and -not $t.Contains('B 1')) 'Both old blocks MUST be removed'
  }
  Test-WtCase 'Edit: Every block is removed, not only the first' {
    $old = $latin1.GetBytes("# agentspace begin: one`nX 1`n# agentspace end`nPort 22`n# agentspace begin: two`nY 1`n# agentspace end`nLast yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-WtEqual ([regex]::Matches($t, '# agentspace begin:').Count) 1 'Begin lines in the new file'
  }
  Test-WtCase 'Edit: A block with no lines between begin and end is removed' {
    $old = $latin1.GetBytes("Port 22`n# agentspace begin: empty`n# agentspace end`nLast yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
  }
  Test-WtCase 'Edit: Lines between begin and end are removed whatever they say, including another begin line' {
    $old = $latin1.GetBytes("Port 22`n# agentspace begin: one`n# agentspace begin: nested`nZ 1`n# agentspace end`nAfter yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt ($t.Contains('After yes') -and -not $t.Contains('Z 1')) 'The nested begin line is between the two, so it goes, and the line after the first end stays'
  }
  Test-WtCase 'Edit: An end line with no begin line before it is an ordinary line and is kept' {
    $old = $latin1.GetBytes("Port 22`n# agentspace end`nLast yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-WtEqual ([regex]::Matches($t, '(?m)^# agentspace end\r?$').Count) 2 'End lines in the new file (the block''s own and the kept one)'
  }
  Test-WtCase 'Edit: A line that only holds the begin text later in the line, or after a space, is no begin line' {
    $old = $latin1.GetBytes("Port 22`n  # agentspace begin: indented`nNote # agentspace begin: not at the start`nLast yes`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
  }
  Test-WtCase 'Edit: A begin line with no end line after it fails the install, with the line''s number, and nothing is written' {
    $old = $latin1.GetBytes("Port 22`n#note`n# agentspace begin: lonely`nAllowUsers x`n")
    $c = New-WtInstall -Config $old
    $oldCa = Read-WtBytes -Path $c.Ca
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr -match "install\.ps1: line 3 of .+ starts agentspace's lines with no end line after it; the file is left as it was, so fix it by hand and run agentspace-operator sync again") ("stderr MUST give the message for line 3. {0}" -f (Format-WtRun $r))
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $old -What 'sshd_config'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Ca) -Expected $oldCa -What 'The CA file'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted'
    Assert-WtEqual ((Get-WtNames -Path $c.Ssh) -join ',') 'agentspace_ca.pub,sshd_config' 'Files beside sshd_config'
  }
  Test-WtCase 'Edit: With one good block then a begin with no end, the second begin''s number is named' {
    $old = $latin1.GetBytes("# agentspace begin: ok`nA 1`n# agentspace end`nPort 22`n# agentspace begin: lonely`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr -match 'install\.ps1: line 5 of .+ starts agentspace''s lines with no end line after it') ("stderr MUST name line 5. {0}" -f (Format-WtRun $r))
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $old -What 'sshd_config'
  }
  Test-WtCase 'Edit: An end line with trailing text is not an end line, so the block has no end' {
    $old = $latin1.GetBytes("# agentspace begin: x`nA 1`n# agentspace end now`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr -match 'install\.ps1: line 1 of .+ starts agentspace''s lines with no end line after it') ("stderr MUST name line 1. {0}" -f (Format-WtRun $r))
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $old -What 'sshd_config'
  }
  Test-WtCase 'Edit: An unclosed begin does not touch the old CA file either' {
    $old = $latin1.GetBytes("# agentspace begin: x`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-WtEqual (Read-WtText -Path $c.Ca) "OLD-CA-KEY`n" 'The CA file'
  }
  Test-WtCase 'Edit: Success prints Installed: on stdout and nothing on stderr' {
    $c = New-WtInstall -Config $defaultConfig
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.Stderr.Trim() '' 'stderr'
    Assert-Wt ($r.Stdout -match '(?m)^Installed: ') 'stdout MUST hold an Installed: line'
  }
  Test-WtCase 'Edit: A failure prints its install.ps1: line on stderr and not on stdout' {
    $c = New-WtInstall -Config $defaultConfig -TestExit 1
    $r = Invoke-WtInstall -Case $c
    Assert-Wt ($r.Stderr.Contains('install.ps1: ')) 'The failure line MUST be on stderr'
    Assert-Wt (-not $r.Stdout.Contains('install.ps1: ')) 'The failure line MUST NOT be on stdout'
  }

  # --- Unchanged: no write, no restart ------------------------------------------------------------------------------------
  Test-WtCase 'Unchanged: A second run finds the lines there, writes nothing, restarts nothing and says so' {
    $c = New-WtInstall -Config $defaultConfig
    $r1 = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r1 -Old $defaultConfig
    $afterFirst = Read-WtBytes -Path $c.Config
    Set-WtWritten -TestHome $testHome -Path $c.Config -WrittenUnix 1600000000
    Remove-WtFile -TestHome $testHome -Path $c.Restarted
    $r2 = Invoke-WtInstall -Case $c
    Assert-WtEqual $r2.ExitCode 0 'Exit code'
    Assert-WtStdoutLine -Result $r2 -Pattern '^Installed: sshd_config holds the agentspace lines \(already there\)$'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $afterFirst -What 'sshd_config'
    Assert-WtEqual ([DateTimeOffset]::new((Get-Item -LiteralPath $c.Config).LastWriteTimeUtc).ToUnixTimeSeconds()) 1600000000 'sshd_config last write (an unchanged file is not written)'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted when the bytes did not change'
  }
  Test-WtCase 'Unchanged: A file that already holds the new lines byte for byte is not written' {
    $already = Get-WtExpectedConfig -Old $defaultConfig
    $c = New-WtInstall -Config $already
    Set-WtWritten -TestHome $testHome -Path $c.Config -WrittenUnix 1600000000
    $r = Invoke-WtInstall -Case $c
    Assert-WtStdoutLine -Result $r -Pattern '^Installed: sshd_config holds the agentspace lines \(already there\)$'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted'
    Assert-WtEqual ([DateTimeOffset]::new((Get-Item -LiteralPath $c.Config).LastWriteTimeUtc).ToUnixTimeSeconds()) 1600000000 'sshd_config last write'
  }
  Test-WtCase 'Unchanged: Blocks whose lines differ from the new ones make a change' {
    $old = $latin1.GetBytes((($agentspaceLines | ForEach-Object { if ($_ -ceq 'LoginGraceTime 30') { 'LoginGraceTime 60' } else { $_ } }) -join "`r`n") + "`r`nPort 22`r`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
  }
  Test-WtCase 'Unchanged: Lines with LF endings instead of CR LF differ in bytes, so they are replaced and sshd restarts' {
    $old = $latin1.GetBytes((($agentspaceLines) -join "`n") + "`nPort 22`n")
    $c = New-WtInstall -Config $old
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
  }

  # --- The CA's public key --------------------------------------------------------------------------------------------------
  Test-WtCase 'CA: A run that edits the file replaces the CA file, whole, with the staged one' {
    $c = New-WtInstall -Config $defaultConfig
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $defaultConfig
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Ca) -Expected $c.NewCaBytes -What 'agentspace_ca.pub'
  }
  Test-WtCase 'CA: A run that finds the lines already there still replaces the CA file, with no restart' {
    $already = Get-WtExpectedConfig -Old $defaultConfig
    $c = New-WtInstall -Config $already
    $r = Invoke-WtInstall -Case $c
    Assert-WtStdoutLine -Result $r -Pattern '\(already there\)$'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Ca) -Expected $c.NewCaBytes -What 'agentspace_ca.pub'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted for a CA change alone'
  }
  Test-WtCase 'CA: A new CA drops the old one, it never adds to it' {
    $c = New-WtInstall -Config $defaultConfig -OldCa "ssh-ed25519 THE-OLD-CA-KEY old-ca`nssh-ed25519 ANOTHER-OLD-KEY other`n"
    [void](Invoke-WtInstall -Case $c)
    $t = Read-WtText -Path $c.Ca
    Assert-Wt (-not $t.Contains('THE-OLD-CA-KEY') -and -not $t.Contains('ANOTHER-OLD-KEY')) 'The old CA keys MUST be gone'
    Assert-Wt ($t.Contains('NEW-CA-KEY-FOR-TESTS')) 'The staged CA key MUST be there'
  }
  Test-WtCase 'CA: A missing CA file beside sshd_config is made' {
    $c = New-WtInstall -Config $defaultConfig
    Remove-WtFile -TestHome $testHome -Path $c.Ca
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Ca) -Expected $c.NewCaBytes -What 'agentspace_ca.pub'
  }

  # --- sshd -t before the move, and the failed test's transaction ---------------------------------------------------------
  Test-WtCase 'Test: The program is given the temporary file''s full path as its one argument, beside sshd_config' {
    $c = New-WtInstall -Config $defaultConfig
    [void](Invoke-WtInstall -Case $c)
    $calls = @(Get-WtLines -Text (Read-WtText -Path $c.Calls) | Where-Object { $_ -notmatch '^\[-T\]' })
    Assert-WtEqual $calls.Count 1 'Calls of the program that are not -T'
    Assert-Wt ($calls[0] -match '^\[(.+)\] \[\]$') ("The program MUST be given exactly one argument: {0}" -f $calls[0])
    $arg = $Matches[1]
    Assert-Wt ([IO.Path]::IsPathRooted($arg)) 'The argument MUST be a full path'
    Assert-WtEqual (Split-Path -Parent $arg).TrimEnd('\') (Split-Path -Parent $c.Config).TrimEnd('\') 'The temporary file''s folder'
    Assert-Wt ($arg -ne $c.Config) 'The argument MUST be the temporary file, not sshd_config'
  }
  Test-WtCase 'Test: The file the program was given held the new bytes, and they are what sshd_config holds after' {
    $c = New-WtInstall -Config $defaultConfig
    [void](Invoke-WtInstall -Case $c)
    $expected = Get-WtExpectedConfig -Old $defaultConfig
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Copy) -Expected $expected -What 'The file as the sshd -t stand-in saw it'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $expected -What 'sshd_config'
  }
  Test-WtCase 'Test: After a passing test nothing but sshd_config, the CA file and sshd-restarted is left beside the file' {
    $c = New-WtInstall -Config $defaultConfig
    [void](Invoke-WtInstall -Case $c)
    Assert-WtEqual ((Get-WtNames -Path $c.Ssh) -join ',') ((@('agentspace_ca.pub', 'sshd-restarted', 'sshd_config') | Sort-Object) -join ',') 'Files beside sshd_config'
  }
  Test-WtCase 'Test: A failing test keeps both files, removes the temporary file, restarts nothing and exits 1 with its line' {
    $c = New-WtInstall -Config $defaultConfig -TestExit 1
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains('install.ps1: sshd -t rejected the new sshd_config; the old files were kept and sshd was not restarted')) ("stderr MUST hold the line. {0}" -f (Format-WtRun $r))
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $defaultConfig -What 'sshd_config'
    Assert-WtEqual (Read-WtText -Path $c.Ca) "OLD-CA-KEY`n" 'The old CA file MUST be put back'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted'
    Assert-WtEqual ((Get-WtNames -Path $c.Ssh) -join ',') 'agentspace_ca.pub,sshd_config' 'Files beside sshd_config (the temporary file MUST be removed)'
  }
  Test-WtCase 'Test: A failing test keeps the old CA file byte for byte, with several keys in it' {
    $oldCa = "ssh-ed25519 KEY-ONE one`r`nssh-ed25519 KEY-TWO two"
    $c = New-WtInstall -Config $defaultConfig -TestExit 3 -OldCa $oldCa
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Ca) -Expected $script:WtUtf8.GetBytes($oldCa) -What 'The CA file'
  }
  Test-WtCase 'Test: A failing test leaves a config that already held the lines as it was too' {
    $already = Get-WtExpectedConfig -Old $defaultConfig
    $c = New-WtInstall -Config $already -TestExit 1
    $r = Invoke-WtInstall -Case $c
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $already -What 'sshd_config'
  }
  Test-WtCase 'Test: The sshd_config''s own access rules are on the new file' {
    $c = New-WtInstall -Config $defaultConfig
    $acl = Get-Acl -LiteralPath $c.Config
    $acl.SetAccessRuleProtection($true, $false)
    $me = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($me, 'FullControl', 'Allow'))
    Set-Acl -LiteralPath (Assert-ConformancePath -ConformanceHome $testHome -Path $c.Config) -AclObject $acl
    $before = Get-WtSddl -Path $c.Config
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    # The access rules name accounts, so a reason says only that they differ, never what they are.
    Assert-Wt ((Get-WtSddl -Path $c.Config) -ceq $before) 'sshd_config''s access rules after the edit are not the old file''s (the temporary file MUST take the rules of sshd_config)'
  }

  # --- PerSourcePenaltyExemptList (WB22) --------------------------------------------------------------------------------------
  Test-WtCase 'Penalties: With the keyword in the probe''s output the exemption line is written before the end line' {
    $c = New-WtInstall -Config $defaultConfig -Probe $probeWith
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $defaultConfig -Exempt
    $t = $latin1.GetString((Read-WtBytes -Path $c.Config))
    Assert-Wt ($t.Contains("$exemptLine`r`n# agentspace end`r`n")) 'The exemption line MUST be the last line before # agentspace end'
  }
  Test-WtCase 'Penalties: Without the keyword in the probe''s output no exemption line is written' {
    $c = New-WtInstall -Config $defaultConfig -Probe $probeWithout
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $defaultConfig
    Assert-Wt (-not ((Read-WtText -Path $c.Config).Contains('PerSourcePenaltyExemptList'))) 'No exemption line MUST be written'
  }
  Test-WtCase 'Penalties: A probe with no output at all writes no exemption line' {
    $c = New-WtInstall -Config $defaultConfig -Probe ''
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $defaultConfig
  }
  Test-WtCase 'Penalties: The probe is the program given the one argument -T' {
    $c = New-WtInstall -Config $defaultConfig -Probe $probeWith
    [void](Invoke-WtInstall -Case $c)
    $calls = @(Get-WtLines -Text (Read-WtText -Path $c.Calls))
    Assert-Wt ($calls -contains '[-T] []') ("The program MUST be run once with the one argument -T: {0}" -f ($calls -join ' '))
  }
  Test-WtCase 'Penalties: A host that gains the keyword has its lines rewritten and sshd restarted' {
    $c = New-WtInstall -Config $defaultConfig -Probe $probeWithout
    [void](Invoke-WtInstall -Case $c)
    Remove-WtFile -TestHome $testHome -Path $c.Restarted
    Write-WtText -TestHome $testHome -Path (Join-Path (Split-Path -Parent $c.Standin) 'probe-output.txt') -Text $probeWith
    $r = Invoke-WtInstall -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    $expected = Get-WtExpectedConfig -Old $defaultConfig -Exempt
    Assert-WtBytes -Actual (Read-WtBytes -Path $c.Config) -Expected $expected -What 'sshd_config'
    Assert-Wt ($null -ne (Read-WtUnixLine -Path $c.Restarted)) 'sshd MUST be restarted: the lines'' bytes changed'
  }
  Test-WtCase 'Penalties: With the keyword, a second run changes nothing and restarts nothing' {
    $c = New-WtInstall -Config $defaultConfig -Probe $probeWith
    [void](Invoke-WtInstall -Case $c)
    Remove-WtFile -TestHome $testHome -Path $c.Restarted
    $r = Invoke-WtInstall -Case $c
    Assert-WtStdoutLine -Result $r -Pattern '\(already there\)$'
    Assert-Wt (-not (Test-Path -LiteralPath $c.Restarted)) 'sshd MUST NOT be restarted'
  }
  Test-WtCase 'Penalties: An old block that held the exemption line is replaced whole, not added to' {
    $old = $latin1.GetBytes((($agentspaceLines[0..($agentspaceLines.Count - 2)] + $exemptLine + $agentspaceLines[-1]) -join "`r`n") + "`r`nPort 22`r`n")
    $c = New-WtInstall -Config $old -Probe $probeWithout
    $r = Invoke-WtInstall -Case $c
    Assert-WtEdited -Case $c -Result $r -Old $old
    Assert-Wt (-not ((Read-WtText -Path $c.Config).Contains('PerSourcePenaltyExemptList'))) 'The old exemption line MUST go when this sshd lacks the keyword'
  }

  # --- The seam is all or nothing --------------------------------------------------------------------------------------------------
  $installNames = @('AGENTSPACE_INSTALL_ONLY', 'AGENTSPACE_SSHD_CONFIG', 'AGENTSPACE_SSHD_TEST_PROGRAM')
  foreach ($subset in @(@(0), @(1), @(2), @(0, 1), @(0, 2), @(1, 2))) {
    $label = ($subset | ForEach-Object { $installNames[$_] -replace '^AGENTSPACE_', '' }) -join ' and '
    $subsetCopy = @($subset)
    Test-WtCase ("Seam: Only $label set is refused, nothing done") {
      # The stage holds no CA, so a script that ignored the seam would stop at its own CA check.
      $c = New-WtInstall -Config $defaultConfig -NoCa
      $all = Get-WtInstallSeam -Case $c
      $seam = @{ AGENTSPACE_INSTALL_ONLY = $null; AGENTSPACE_SSHD_CONFIG = $null; AGENTSPACE_SSHD_TEST_PROGRAM = $null }
      foreach ($i in $subsetCopy) { $seam[$installNames[$i]] = $all[$installNames[$i]] }
      $before = Get-WtSnapshot -Path $c.Ssh
      $r = Invoke-WtInstall -Case $c -Seam $seam
      Assert-WtSeamRefusal -Result $r -ScriptName $scriptName -Variables $installNames
      Assert-WtEqual (Get-WtSnapshot -Path $c.Ssh) $before 'The ssh folder'
      Assert-Wt (-not (Test-Path -LiteralPath $c.Calls)) 'The stand-in program MUST NOT be run'
    }
  }
  Test-WtCase 'Seam: Under it steps 1 to 8 and 10 are skipped: the install makes no folder in the home but its own stage' {
    $c = New-WtInstall -Config $defaultConfig
    $before = (Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)
    [void](Invoke-WtInstall -Case $c)
    Assert-WtEqual ((Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)) $before 'The home''s .agentspace and .ssh'
  }
  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun
