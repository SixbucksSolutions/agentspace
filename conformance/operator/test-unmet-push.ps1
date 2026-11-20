#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the localswim token push that the operator's `up` makes (docs/operator.md,
# "The token push"), run through the test seam (docs/operator.md, "The test seam").
#
# EXPECTED TO FAIL AGAINST THE PROTOTYPE, because it does not meet the seam: it runs the AWS CLI and
# `ssh` by fixed paths and reads none of AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD, AGENTSPACE_SCP_CMD
# or AGENTSPACE_TIMEZONE (card #0085; docs/operator.md, "Requirements the prototype does not meet
# yet"). Until a build calls the stand-ins, the file reports `FAIL ... seam not met` and checks
# nothing more. A rewrite MUST pass every result line. The result lines still say FAIL: nothing turns
# a FAIL into a pass. Every file in conformance/operator/ not named test-unmet-* passes against the
# prototype.
#
# What it does, when the seam is met: it writes into a throwaway home a fake localswim receipt and a
# fake Bot credential (DPAPI-protected for the current user, as localswim's is), the instance-ID
# cache, a stand-in `aws` that reports a running instance launched long ago, and a stand-in `ssh`
# that behaves as the box does (docs/operator.md, "What the box does") and records its arguments, its
# standard input and its environment. It then runs `up -Alias <alias>` in each of the cases below and
# checks only what it can see: the recorded call, the stamp file in the home, the messages on stderr,
# the exit code and stdout.
#
#   1. The first push: the ssh arguments, the two lines on stdin, the stamp, the messages, exit 0.
#   2. An unchanged token and zone: nothing is sent.
#   3. A changed zone, and a deleted stamp: each makes the push due again.
#   4. A nested `up`, and a missing receipt: no push, and no message for the receipt.
#   5. A credential that cannot be read: the fixed warning, and no ssh.
#   6. An ssh that exits 255, a box that refuses the zone, and a zone that fails the check: each
#      warns as the spec says, `up` still exits 0, and no stamp is written when it must not be.
#   7. An ssh that hangs: it is stopped after 20 seconds, with the fixed warning, and `up` exits 0.
#   8. Secrecy: the token is on the ssh's standard input only, and in no argument, no environment
#      variable the stand-in sees, and no message on stderr.
#
# SAFETY. The token is a made-up value. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA
# into the throwaway home and removes every AWS credential, so a mistaken run cannot find Terry's
# receipt or token. The alias is a name no ssh configuration holds, so an ssh that ignores the seam
# connects nowhere. Before any program that can reach AWS starts, the check confirms under the same
# environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and
# reports SKIPPED, running nothing, if it does not.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/prototype/defaults.ps1 (docs/conformance.md, "Naming the program under test").
# A failure reason never quotes the program's output, which can hold an instance ID.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-push.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#the-token-push'
$seamSec = 'operator.md#the-test-seam'
$alias = 'conformance-push-alias'
$fakeId = 'i-' + ('0' * 17)
$tokenValue = 'conformance-fake-token-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
$baseUrl = 'http://127.0.0.1:18811'
$content = "LOCALSWIM_URL=$baseUrl`nLOCALSWIM_TOKEN=$tokenValue`n"

function Get-Sha256Hex([string]$Text) {
  $b = [Text.UTF8Encoding]::new($false).GetBytes($Text)
  [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData($b)).Replace('-', '').ToLowerInvariant()
}

# The stand-in ssh: a Windows PowerShell 5.1 script that a .cmd starts, so that it sees the arguments
# as a program would. It records every call, acts as the box (docs/operator.md, "What the box does")
# according to the scenario file beside it, and never reads or writes anything outside its own folder.
$sshStandInScript = @'
$dir = $PSScriptRoot
$scenario = (Get-Content -LiteralPath (Join-Path $dir 'ssh-scenario.txt') -Raw).Trim()
$zone = (Get-Content -LiteralPath (Join-Path $dir 'ssh-zone.txt') -Raw).Trim()
$n = @(Get-ChildItem -LiteralPath $dir -Filter 'ssh-stdin-*.bin').Count + 1
$m = New-Object IO.MemoryStream
[Console]::OpenStandardInput().CopyTo($m)
$bytes = $m.ToArray()
[IO.File]::WriteAllBytes((Join-Path $dir "ssh-stdin-$n.bin"), $bytes)
[IO.File]::AppendAllText((Join-Path $dir 'ssh-args.log'), ((ConvertTo-Json -InputObject @($args) -Compress) + "`n"))
[IO.File]::WriteAllText((Join-Path $dir "ssh-env-$n.txt"), ((Get-ChildItem Env: | ForEach-Object { $_.Name + '=' + $_.Value }) -join "`n"))
if ($scenario -eq 'hang') { Start-Sleep -Seconds 120; exit 0 }
if ($scenario -eq 'fail') { exit 255 }
$text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
$lines = @($text -split "`n" | Where-Object { $_ -ne '' })
$norm = (($lines -join "`n") + "`n")
$sha = [Security.Cryptography.SHA256]::Create()
$hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
[Console]::Out.Write($hash + '  -' + "`n")
if ($scenario -eq 'zonefailed') { [Console]::Out.Write('zone-failed' + "`n") }
elseif ($zone -ne '') { [Console]::Out.Write('zone ' + $zone + "`n") }
exit 0
'@

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name "${seamSec}: up runs the stand-ins (the push's checks)" -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name "${seamSec}: up runs the stand-ins (the push's checks)" -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    # The throwaway home: the instance-ID cache, a fake receipt and a fake DPAPI-protected credential.
    $agentspaceDir = Join-Path $h.UserProfile '.agentspace'
    # The cache and the stamp are the rewrite's JSON files (D40; docs/state.md).
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $agentspaceDir 'instance-id.json')), ('{"version":0,"instance_id":"' + $fakeId + '"}'))
    $stamp = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $agentspaceDir 'localswim-pushed.json')
    $lsDir = Join-Path $h.UserProfile 'AppData\Local\localswim-v2'
    $credDir = Join-Path $lsDir 'cred'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $credDir) | Out-Null
    $receipt = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $lsDir 'service.json')
    $botXml = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $credDir 'bot.xml')
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path $receipt), (ConvertTo-Json -Compress -InputObject @{ base = $baseUrl; credential_directory = $credDir }))
    $secure = ConvertTo-SecureString -String $tokenValue -AsPlainText -Force
    [pscredential]::new('bot', $secure) | Export-Clixml -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $botXml)

    # The stand-ins: a running instance launched long ago, so that `up` does not wait.
    $aws = New-OperatorStandIn -ConformanceHome $h -Name 'aws' -Body @(
      'if /i "%~1"=="ec2" if /i "%~2"=="describe-instances" goto describe',
      'exit /b 0',
      ':describe',
      'echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
      'exit /b 0')
    $sshPs1 = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.Root 'standins\ssh-standin.ps1')
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Split-Path -Parent $sshPs1)) | Out-Null
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path $sshPs1), $sshStandInScript)
    $ssh = New-OperatorStandIn -ConformanceHome $h -Name 'ssh' -Body @(
      'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0ssh-standin.ps1" %*',
      'exit /b %ERRORLEVEL%')
    $scp = New-OperatorStandIn -ConformanceHome $h -Name 'scp' -Body @('exit /b 255')
    $standins = Split-Path -Parent $ssh
    $sshCalls = Join-Path $standins 'ssh-calls.log'
    $awsCalls = Join-Path $standins 'aws-calls.log'
    $scpCalls = Join-Path $standins 'scp-calls.log'

    function Set-Scenario([string]$Scenario, [string]$Zone) {
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standins 'ssh-scenario.txt')), $Scenario)
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $standins 'ssh-zone.txt')), $Zone)
    }
    function Invoke-Up([string]$Zone, [hashtable]$Extra = @{}, [string[]]$Arguments = @('up', '-Alias', $alias), [int]$Timeout = 120) {
      $e = @{ AGENTSPACE_AWS_CMD = $aws; AGENTSPACE_SSH_CMD = $ssh; AGENTSPACE_SCP_CMD = $scp; AGENTSPACE_TIMEZONE = $Zone }
      foreach ($k in $Extra.Keys) { $e[$k] = $Extra[$k] }
      Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments $Arguments -Environment $e -TimeoutSeconds $Timeout
    }
    # The arguments of the n-th ssh call, as the stand-in recorded them.
    function Get-SshArgs([int]$N) {
      $log = Join-Path $standins 'ssh-args.log'
      if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { return $null }
      $line = @(Get-Content -LiteralPath $log | Where-Object { $_.Trim() -ne '' })[$N - 1]
      if (-not $line) { return $null }
      , @(ConvertFrom-Json -InputObject $line)
    }
    # The stamp that localswim-pushed.json holds, {"version":0,"stamp":"<hex>"} (docs/state.md, The
    # push stamp); the file's whole text when it is not that document, so that no check below passes on it.
    function Get-StampText {
      if (-not (Test-Path -LiteralPath $stamp -PathType Leaf)) { return $null }
      $raw = [IO.File]::ReadAllText($stamp)
      try { $doc = ConvertFrom-Json -InputObject $raw -ErrorAction Stop } catch { return $raw }
      if ($doc -isnot [pscustomobject]) { return $raw }
      $names = @($doc.PSObject.Properties.Name)
      if ($names.Count -ne 2 -or $names -cnotcontains 'version' -or $names -cnotcontains 'stamp') { return $raw }
      if ($doc.version -isnot [long] -or $doc.version -ne 0 -or $doc.stamp -isnot [string]) { return $raw }
      $doc.stamp
    }
    function Get-Problems($Run) {
      $p = [System.Collections.Generic.List[string]]::new()
      if ($Run.TimedOut) { $p.Add('It timed out') }
      elseif ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not 0") }
      if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
      , $p
    }
    function Write-Problems([string]$Name, $Problems) {
      if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS } else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
    }
    $zoneA = 'America/New_York'
    $zoneB = 'Europe/Paris'
    function Get-ExpectedStamp([string]$Zone) { Get-Sha256Hex ($fakeId + "`n" + $Zone + "`n" + $content) }

    # ---- 1. The first push ----
    Set-Scenario 'ok' $zoneA
    $r = Invoke-Up $zoneA
    if ((Get-StandInCallCount -Log $awsCalls) -eq 0 -or (Get-StandInCallCount -Log $sshCalls) -eq 0) {
      # The seam is not met, so nothing below can be shown. One result says so, as the D1 file does.
      Write-ConformanceResult -Name "${seamSec}: up runs AGENTSPACE_AWS_CMD and AGENTSPACE_SSH_CMD" -Status FAIL -Reason 'seam not met (a stand-in was not used)'
    } else {
      Write-ConformanceResult -Name "${seamSec}: up runs AGENTSPACE_AWS_CMD and AGENTSPACE_SSH_CMD" -Status PASS

      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne 1) { $p.Add("ssh was started $(Get-StandInCallCount -Log $sshCalls) times, not once") }
      Write-Problems "${sec}: The first up exits 0, writes nothing to stdout and starts one ssh" $p

      $a = Get-SshArgs 1
      $p = [System.Collections.Generic.List[string]]::new()
      if ($null -eq $a) { $p.Add('No ssh call was recorded') }
      else {
        $want = @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $alias)
        if ($a.Count -ne 6) { $p.Add("The ssh got $($a.Count) arguments, not 6") }
        else {
          for ($i = 0; $i -lt 5; $i++) { if ($a[$i] -cne $want[$i]) { $p.Add("Argument $($i + 1) is not as the spec gives it") } }
          if (-not ([string]$a[5]).Contains($zoneA)) { $p.Add('The remote command does not name the zone') }
        }
      }
      Write-Problems "${sec}: The push's ssh gets -o ClearAllForwardings=yes -o BatchMode=yes <alias> and one remote command that names the zone (What the laptop sends)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      $stdinFile = Join-Path $standins 'ssh-stdin-1.bin'
      if (-not (Test-Path -LiteralPath $stdinFile)) { $p.Add('No standard input was recorded') }
      else {
        $got = [IO.File]::ReadAllBytes($stdinFile)
        $skip = if ($got.Length -ge 3 -and $got[0] -eq 0xEF -and $got[1] -eq 0xBB -and $got[2] -eq 0xBF) { 3 } else { 0 }
        $text = [Text.UTF8Encoding]::new($false).GetString($got, $skip, $got.Length - $skip)
        if ($text -cne $content) { $p.Add('The standard input is not exactly the two lines LOCALSWIM_URL=<url> and LOCALSWIM_TOKEN=<token>, each ending in LF') }
      }
      Write-Problems "${sec}: The ssh's standard input is exactly the two content lines, each ending in LF, with at most a byte order mark ahead (What the laptop sends)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      $stampText = Get-StampText
      if ($null -eq $stampText) { $p.Add('No stamp file was written') }
      else {
        if ($stampText -cnotmatch '\A[0-9A-F]{64}\z') { $p.Add('The stamp file is not {"version":0,"stamp":"<64 uppercase hex digits>"}') }
        elseif ($stampText.ToLowerInvariant() -cne (Get-ExpectedStamp $zoneA)) { $p.Add('The stamp is not the SHA-256 of <instance ID> LF <zone> LF <content>') }
      }
      Write-Problems "${sec}: The stamp is the SHA-256 of the instance ID, the zone and the content, written once the box confirmed (The stamp)" $p

      $lines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
      $p = [System.Collections.Generic.List[string]]::new()
      if (-not ($lines -ccontains 'Sending the localswim Bot token to the instance...')) { $p.Add('Stderr lacks the sending message') }
      if ($lines | Where-Object { $_.StartsWith('agentspace: ') -and $_ -match 'did not confirm|did not take|Could not' }) { $p.Add('Stderr holds a push warning on a push that succeeded') }
      Write-Problems "${sec}: A confirmed push prints the sending message and no warning" $p

      # ---- 8. Secrecy, from the first push ----
      $p = [System.Collections.Generic.List[string]]::new()
      $a1 = Get-SshArgs 1
      if ($a1 -and ($a1 -join ' ').Contains($tokenValue)) { $p.Add('An ssh argument holds the token') }
      $envFile = Join-Path $standins 'ssh-env-1.txt'
      if ((Test-Path -LiteralPath $envFile) -and ([IO.File]::ReadAllText($envFile)).Contains($tokenValue)) { $p.Add('An environment variable the ssh sees holds the token') }
      if ($r.Stderr.Contains($tokenValue) -or $r.Stdout.Contains($tokenValue)) { $p.Add('A message holds the token') }
      if ((Get-StampText) -and (Get-StampText).Contains($tokenValue)) { $p.Add('The stamp holds the token') }
      Write-Problems "${sec}: The token is on the ssh's standard input only: in no argument, no environment variable, no message and not in the stamp (Secrecy)" $p

      # ---- 2. Unchanged: nothing is sent ----
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $stampBefore = Get-StampText
      $r = Invoke-Up $zoneA
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore) { $p.Add('An unchanged token and zone started an ssh') }
      if ((Get-StampText) -cne $stampBefore) { $p.Add('The stamp changed') }
      if ($r.Stderr.Contains('Sending the localswim Bot token')) { $p.Add('The sending message was printed') }
      Write-Problems "${sec}: A push that is not due sends nothing and prints nothing about it (The stamp)" $p

      # ---- 3. A changed zone, then a deleted stamp, each make the push due ----
      Set-Scenario 'ok' $zoneB
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore + 1) { $p.Add('A changed zone did not start one ssh') }
      $a = Get-SshArgs ($callsBefore + 1)
      if ($null -eq $a -or $a.Count -ne 6 -or -not ([string]$a[5]).Contains($zoneB)) { $p.Add('The remote command does not name the new zone') }
      if ((Get-StampText) -and (Get-StampText).ToLowerInvariant() -cne (Get-ExpectedStamp $zoneB)) { $p.Add('The stamp is not the new zone''s') }
      Write-Problems "${sec}: A changed zone makes the push due, and the stamp then covers the new zone (The stamp)" $p

      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $stamp) -Force -ErrorAction SilentlyContinue
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore + 1) { $p.Add('A missing stamp did not start one ssh') }
      if ((Get-StampText) -and (Get-StampText).ToLowerInvariant() -cne (Get-ExpectedStamp $zoneB)) { $p.Add('The stamp did not come back equal') }
      Write-Problems "${sec}: A deleted stamp makes the push due, and the stamp comes back equal (The stamp)" $p

      # ---- 3b. The zone: legacy names are mapped, and a zone that fails the check is never sent ----
      # AGENTSPACE_TIMEZONE replaces the laptop's zone before the legacy names are mapped and the
      # check of "The time zone", step 3, is made (docs/operator.md, "The test seam"), so both can be
      # reached from outside. The Windows IDs of step 1 cannot: the seam replaces them.
      $legacy = [ordered]@{
        'Asia/Calcutta' = 'Asia/Kolkata'; 'Asia/Katmandu' = 'Asia/Kathmandu'; 'Asia/Rangoon' = 'Asia/Yangon'
        'Europe/Kiev' = 'Europe/Kyiv'; 'America/Buenos_Aires' = 'America/Argentina/Buenos_Aires'
        'America/Godthab' = 'America/Nuuk'; 'America/Indianapolis' = 'America/Indiana/Indianapolis'
      }
      $p = [System.Collections.Generic.List[string]]::new()
      foreach ($old in $legacy.Keys) {
        $new = $legacy[$old]
        Set-Scenario 'ok' $new
        $callsBefore = Get-StandInCallCount -Log $sshCalls
        $r = Invoke-Up $old
        $a = Get-SshArgs ($callsBefore + 1)
        if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore + 1 -or $null -eq $a -or $a.Count -ne 6) { $p.Add("$old : no push was made"); continue }
        if (-not ([string]$a[5]).Contains($new)) { $p.Add("$old : the remote command does not name $new") }
        if (([string]$a[5]).Contains($old)) { $p.Add("$old : the remote command still names the legacy name") }
        if ((Get-StampText) -and (Get-StampText).ToLowerInvariant() -cne (Get-ExpectedStamp $new)) { $p.Add("$old : the stamp does not cover $new") }
      }
      Write-Problems "${sec}: The seven legacy zone names are sent as their current names, and the stamp covers the current name (The time zone, step 2)" $p

      $p = [System.Collections.Generic.List[string]]::new()
      foreach ($bad in '-America/New_York', 'America/New York', 'a;b', 'America//New_York', 'Europe/Paris/', '/Europe/Paris', 'Europe/Pa$ris') {
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $stamp) -Force -ErrorAction SilentlyContinue
        Set-Scenario 'ok' ''
        $callsBefore = Get-StandInCallCount -Log $sshCalls
        $r = Invoke-Up $bad
        $a = Get-SshArgs ($callsBefore + 1)
        if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore + 1 -or $null -eq $a -or $a.Count -ne 6) { $p.Add("'$bad' : the token was not pushed"); continue }
        if (([string]$a[5]).Contains($bad)) { $p.Add("'$bad' : it is on the box's command line") }
        if (-not (@($r.Stderr -split "`r?`n") -ccontains 'agentspace: Could not work out the laptop''s time zone; times on the box stay in its own zone.')) { $p.Add("'$bad' : no warning") }
        if ((Get-StampText) -and (Get-StampText).ToLowerInvariant() -cne (Get-ExpectedStamp '')) { $p.Add("'$bad' : the stamp does not carry an empty zone") }
      }
      Write-Problems "${sec}: A zone that fails the zone check (a leading dash, a space, a semicolon, an empty step, a trailing or leading slash, a dollar sign) is never sent, and the stamp carries an empty zone (The time zone, step 3)" $p

      # ---- 4. A nested up pushes nothing; a missing receipt is silent ----
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up $zoneB -Extra @{ AGENTSPACE_NESTED = '1' }
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $stamp) -Force -ErrorAction SilentlyContinue
      $r2 = Invoke-Up $zoneB -Extra @{ AGENTSPACE_NESTED = '1' }
      $p = Get-Problems $r2
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore) { $p.Add('A nested up started an ssh') }
      if ($null -ne (Get-StampText)) { $p.Add('A nested up wrote a stamp') }
      Write-Problems "${sec}: A nested up (AGENTSPACE_NESTED set) does not push (When it runs)" $p

      Rename-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $receipt) -NewName 'service.json.away'
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore) { $p.Add('A missing receipt started an ssh') }
      if ($r.Stderr -match 'localswim|token') { $p.Add('A missing receipt printed a message about localswim') }
      Write-Problems "${sec}: A missing localswim receipt skips the push without a message (What the laptop reads)" $p
      Rename-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path "$receipt.away") -NewName 'service.json'

      # ---- 5. A credential that cannot be read ----
      $botBackup = Join-Path $h.Root 'bot.xml.keep'
      Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $botXml) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $botBackup)
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path $botXml), 'not a credential')
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore) { $p.Add('An unreadable credential started an ssh') }
      $warn = 'agentspace: Could not read the localswim Bot token; the board will not work on the box this session.'
      if (-not (@($r.Stderr -split "`r?`n") -ccontains $warn)) { $p.Add('Stderr lacks the fixed "Could not read the localswim Bot token" warning') }
      Write-Problems "${sec}: A credential that cannot be read warns with the fixed text, sends nothing, and still exits 0 (What the laptop reads)" $p
      Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $botBackup) -Destination (Assert-ConformancePath -ConformanceHome $h -Path $botXml) -Force

      # ---- 6. The box does not confirm ----
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $stamp) -Force -ErrorAction SilentlyContinue
      Set-Scenario 'fail' $zoneB
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      $warn = 'agentspace: The instance did not confirm the localswim token; the board will not work on the box this session.'
      if (-not (@($r.Stderr -split "`r?`n") -ccontains $warn)) { $p.Add('Stderr lacks the fixed "did not confirm" warning') }
      if ($null -ne (Get-StampText)) { $p.Add('A stamp was written') }
      Write-Problems "${sec}: An ssh that exits 255 warns with the fixed text, writes no stamp, and up still exits 0 (What the laptop checks)" $p

      Set-Scenario 'zonefailed' $zoneB
      $r = Invoke-Up $zoneB
      $p = Get-Problems $r
      $warn = "agentspace: The instance did not take the time zone ${zoneB}; times on the box stay in its own zone."
      if (-not (@($r.Stderr -split "`r?`n") -ccontains $warn)) { $p.Add('Stderr lacks the fixed "did not take the time zone" warning') }
      if ($null -ne (Get-StampText)) { $p.Add('A stamp was written') }
      Write-Problems "${sec}: A box that answers zone-failed warns with the fixed text, writes no stamp, and up still exits 0 (What the laptop checks)" $p

      # A zone that fails the zone check is not sent, and the push goes on without one.
      Set-Scenario 'ok' ''
      $callsBefore = Get-StandInCallCount -Log $sshCalls
      $r = Invoke-Up 'not a zone'
      $p = Get-Problems $r
      if ((Get-StandInCallCount -Log $sshCalls) -ne $callsBefore + 1) { $p.Add('The token was not pushed') }
      $a = Get-SshArgs ($callsBefore + 1)
      if ($null -ne $a -and $a.Count -eq 6 -and ([string]$a[5]).Contains('not a zone')) { $p.Add('The bad zone is on the box''s command line') }
      $warn = 'agentspace: Could not work out the laptop''s time zone; times on the box stay in its own zone.'
      if (-not (@($r.Stderr -split "`r?`n") -ccontains $warn)) { $p.Add('Stderr lacks the fixed "Could not work out the laptop''s time zone" warning') }
      if ((Get-StampText) -and (Get-StampText).ToLowerInvariant() -cne (Get-ExpectedStamp '')) { $p.Add('The stamp does not carry the empty zone') }
      Write-Problems "${sec}: A zone that fails the check is not sent: the token is pushed, the fixed warning is printed, and the stamp carries an empty zone (The time zone)" $p

      # ---- 7. A hung ssh is stopped after 20 seconds ----
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $stamp) -Force -ErrorAction SilentlyContinue
      Set-Scenario 'hang' $zoneB
      $clock = [Diagnostics.Stopwatch]::StartNew()
      $r = Invoke-Up $zoneB -Timeout 100
      $clock.Stop()
      $p = Get-Problems $r
      $warn = 'agentspace: Sending the localswim token took over 20 seconds and was stopped; the board will not work on the box this session.'
      if (-not (@($r.Stderr -split "`r?`n") -ccontains $warn)) { $p.Add('Stderr lacks the fixed "took over 20 seconds" warning') }
      if ($clock.Elapsed.TotalSeconds -lt 19) { $p.Add('It gave up before 20 seconds') }
      if ($clock.Elapsed.TotalSeconds -gt 60) { $p.Add('It took more than 60 seconds, so the hung ssh was not stopped at the limit') }
      if ($null -ne (Get-StampText)) { $p.Add('A stamp was written') }
      Write-Problems "${sec}: An ssh that hangs is stopped after 20 seconds, with the fixed warning, no stamp, and up still exits 0 (What the laptop sends, step 4)" $p

      # ---- the push starts no scp ----
      Write-Problems "${seamSec}: up starts no scp" $(if ((Get-StandInCallCount -Log $scpCalls) -eq 0) { , [System.Collections.Generic.List[string]]::new() } else { , [System.Collections.Generic.List[string]]@('AGENTSPACE_SCP_CMD was called, and up copies no file') })
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-push.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
if ($global:ConformanceCounts['FAIL'] -gt 0) {
  Write-Host 'NOTE This file is expected to fail against the prototype (the test seam, card #0085); a rewrite MUST pass it.'
}
Exit-ConformanceRun
