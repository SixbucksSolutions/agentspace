# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for Push-Localswim in scripts\agentspace.ps1 (card #0026) and for the alias it
# sends over (card #0057). It loads Push-Localswim, Get-BotEnvHash, Test-AliasName, Get-LaptopZone
# and ConvertTo-CurrentZone from the script's syntax tree, points them at a throwaway receipt, a dummy token and a stand-in for ssh,
# and checks that a send goes to the right alias and is recorded only when the stand-in confirms
# the file it wrote. It then runs a copy of agentspace.ps1 with bad aliases, to check that a bad
# alias stops it before anything else runs, and copies of ssh-proxy.cmd with a stand-in tunnel, to
# check the arguments they pass to up and that they exit with the tunnel's code. Nothing reaches
# the box or AWS, and the real token and hash file are never touched.
# It MUST run in Windows PowerShell 5.1, as agentspace.ps1 does:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-push-localswim.ps1
$ErrorActionPreference = 'Stop'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}

$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'agentspace.ps1'), [ref]$null, [ref]$null)
foreach ($name in 'Push-Localswim', 'Get-BotEnvHash', 'Test-AliasName', 'Get-LaptopZone', 'ConvertTo-CurrentZone') {
  $f = $ast.Find({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name }, $true)
  if (-not $f) { throw "agentspace.ps1 has no function $name." }
  . ([scriptblock]::Create($f.Extent.Text))
}
$script:said = @()
function Say([string]$Text) { $script:said += $Text }

# Runs a process with its own environment changes (a $null value removes the variable) and
# returns its exit code, stdout and stderr.
function Invoke-Child([string]$Exe, [string]$Arguments, [hashtable]$Environment = @{}) {
  $psi = New-Object Diagnostics.ProcessStartInfo $Exe
  $psi.Arguments = $Arguments
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.WorkingDirectory = $work
  foreach ($key in $Environment.Keys) {
    if ($null -eq $Environment[$key]) { $psi.EnvironmentVariables.Remove($key) }
    else { $psi.EnvironmentVariables[$key] = $Environment[$key] }
  }
  $p = [Diagnostics.Process]::Start($psi)
  try {
    $err = $p.StandardError.ReadToEndAsync()
    $out = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{ Code = $p.ExitCode; Out = $out; Err = $err.Result }
  } finally { $p.Dispose() }
}

# The arguments the stand-in ssh was last started with, one per line, or $null if it never ran.
function Get-SshArgs {
  if (Test-Path $sshArgs) { @(Get-Content $sshArgs) } else { $null }
}

$work = Join-Path ([IO.Path]::GetTempPath()) "agentspace-test-push-$(Get-Date -Format yyyyMMddHHmmss)"
New-Item -ItemType Directory $work | Out-Null
try {
  # A throwaway receipt and DPAPI credential file, shaped like localswim's own.
  $credential = New-Object Management.Automation.PSCredential 'Bot', (ConvertTo-SecureString 'dummy-not-a-token' -AsPlainText -Force)
  $credential | Export-Clixml (Join-Path $work 'bot.xml')
  @{ base = 'http://127.0.0.1:8811'; credential_directory = $work } | ConvertTo-Json | Set-Content (Join-Path $work 'service.json')
  $expected = "LOCALSWIM_URL=http://127.0.0.1:8811`nLOCALSWIM_TOKEN=dummy-not-a-token`n"

  # The stand-in for ssh, built as an executable so that it records its arguments exactly as it
  # was given them. Mode right plays the box: it reads two lines, writes them as the box would
  # and prints their SHA-256. The others answer with a wrong hash, fail, or hang.
  $box = Join-Path $work 'bot.env'
  $sshArgs = Join-Path $work 'ssh-args.txt'
  $fakeSsh = Join-Path $work 'fake-ssh.exe'
  Add-Type -OutputAssembly $fakeSsh -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
public static class FakeSsh {
  public static int Main(string[] args) {
    File.WriteAllLines(Environment.GetEnvironmentVariable("AGENTSPACE_TEST_ARGS"), args);
    string mode = Environment.GetEnvironmentVariable("AGENTSPACE_TEST_SSH");
    if (mode == "fail") return 255;
    if (mode == "wrong") { Console.Out.WriteLine(new string('0', 64) + "  -"); return 0; }
    if (mode == "hang") { System.Threading.Thread.Sleep(30000); return 0; }
    Stream input = Console.OpenStandardInput();
    List<byte> read = new List<byte>();
    int lines = 0;
    while (lines < 2) {
      int b = input.ReadByte();
      if (b < 0) break;
      read.Add((byte)b);
      if (b == 10) lines++;
    }
    List<string> kept = new List<string>();
    foreach (string line in Encoding.UTF8.GetString(read.ToArray()).Replace("\r", "").Split('\n')) {
      string l = line.TrimStart((char)0xFEFF);
      if (l.Length > 0) kept.Add(l);
    }
    byte[] bytes = Encoding.UTF8.GetBytes(string.Join("\n", kept.ToArray()) + "\n");
    File.WriteAllBytes(Environment.GetEnvironmentVariable("AGENTSPACE_TEST_BOX"), bytes);
    StringBuilder hash = new StringBuilder();
    foreach (byte h in System.Security.Cryptography.SHA256.Create().ComputeHash(bytes)) hash.Append(h.ToString("x2"));
    Console.Out.WriteLine(hash.ToString() + "  -");
    // The zone line, as the box prints it after the hash: the zone it finds after set-timezone in
    // the remote command, answered as AGENTSPACE_TEST_ZONE says (taken, refused or no line).
    string zoneMode = Environment.GetEnvironmentVariable("AGENTSPACE_TEST_ZONE");
    System.Text.RegularExpressions.Match m = System.Text.RegularExpressions.Regex.Match(string.Join(" ", args), @"set-timezone\s+([A-Za-z0-9_+/-]+)");
    if (zoneMode == "failed" && m.Success) Console.Out.WriteLine("zone-failed");
    else if (zoneMode == "ok" && m.Success) Console.Out.WriteLine("zone " + m.Groups[1].Value);
    return 0;
  }
}
'@
  $env:AGENTSPACE_TEST_BOX = $box
  $env:AGENTSPACE_TEST_ARGS = $sshArgs
  $env:AGENTSPACE_TEST_ZONE = 'ok'

  # The script variables Push-Localswim reads.
  $LocalswimReceipt = Join-Path $work 'service.json'
  $PushedFile = Join-Path $work 'state\localswim-pushed.sha256'
  $Ssh = $fakeSsh
  $NoForward = @('-o', 'ClearAllForwardings=yes')
  $PushTimeoutSec = 5
  $Taskkill = Join-Path $env:SystemRoot 'System32\taskkill.exe'
  $id = 'i-0123456789abcdef0'
  $leaseAlias = 'agentspace-swimcoach-a62cb513'
  # The zones the tests send with -TimeZone; the laptop's own zone is never changed or relied on.
  $zone = 'America/Los_Angeles'
  $zone2 = 'America/New_York'
  function Get-Stamp([string]$Zone) {
    [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes("$id`n$Zone`n$expected"))).Replace('-', '')
  }
  $sha = Get-Stamp $zone
  $badAliases = @('a;b', 'a b', 'a&b', '', 'a|b', 'a"b', '-Fx', "a`n", 'a/b', 'a%b', [string][char]0x212A)

  Check 'Get-BotEnvHash ignores CRs, a BOM and blank lines' (
    (Get-BotEnvHash ([char]0xFEFF + "LOCALSWIM_URL=http://127.0.0.1:8811`r`n`r`nLOCALSWIM_TOKEN=dummy-not-a-token`r`n")) -eq (Get-BotEnvHash $expected))

  foreach ($good in 'agentspace', $leaseAlias, 'A.b_c-9') {
    Check "Test-AliasName accepts '$good'" (Test-AliasName $good)
  }
  foreach ($bad in $badAliases) {
    $shown = if ($bad -eq [string][char]0x212A) { 'U+212A, the Kelvin sign' } else { "'$($bad -replace "`n", '\n')'" }
    Check "Test-AliasName refuses $shown" (-not (Test-AliasName $bad))
  }

  $env:AGENTSPACE_TEST_SSH = 'wrong'
  $script:said = @()
  Push-Localswim $id -TimeZone $zone
  Check 'A wrong hash from the box records nothing' (-not (Test-Path $PushedFile))
  Check 'A wrong hash from the box warns' (($script:said -join ' ') -match 'did not confirm')

  $env:AGENTSPACE_TEST_SSH = 'fail'
  $script:said = @()
  Push-Localswim $id -TimeZone $zone
  Check 'A failed ssh records nothing' (-not (Test-Path $PushedFile))
  Check 'A failed ssh warns' (($script:said -join ' ') -match 'did not confirm')

  $env:AGENTSPACE_TEST_SSH = 'hang'
  $script:said = @()
  $start = Get-Date
  Push-Localswim $id -TimeZone $zone
  $took = ((Get-Date) - $start).TotalSeconds
  Check 'A hung ssh is stopped at the time limit' ($took -lt 15) "took $took s"
  Check 'A hung ssh records nothing' (-not (Test-Path $PushedFile))
  Check 'A hung ssh warns' (($script:said -join ' ') -match 'was stopped')

  $env:AGENTSPACE_TEST_SSH = 'right'
  $script:said = @()
  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  Push-Localswim $id -TimeZone $zone
  Check 'A confirmed send is recorded' ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A confirmed send warns about nothing' (-not (($script:said -join ' ') -match 'agentspace:'))
  Check 'The stand-in box got exactly the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  $got = Get-SshArgs
  Check 'Without -Alias the push goes to agentspace, clearing forwards, in batch mode' (
    ($got[0..4] -join ' ') -ceq '-o ClearAllForwardings=yes -o BatchMode=yes agentspace' -and $got.Count -eq 6) ($got -join ' | ')

  $script:said = @()
  Remove-Item $sshArgs, $box -ErrorAction SilentlyContinue
  Push-Localswim $id -Force -Alias $leaseAlias -TimeZone $zone
  $got = Get-SshArgs
  Check 'With -Alias the push goes to that alias, clearing forwards, in batch mode' (
    ($got[0..4] -join ' ') -ceq "-o ClearAllForwardings=yes -o BatchMode=yes $leaseAlias" -and $got.Count -eq 6) ($got -join ' | ')
  Check 'With -Alias the stand-in box got exactly the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  Check 'With -Alias a confirmed send is recorded' ((Get-Content $PushedFile -Raw) -eq $sha)

  foreach ($bad in 'a;b', 'a b', 'a&b', '') {
    $script:said = @()
    Remove-Item $sshArgs -ErrorAction SilentlyContinue
    Push-Localswim $id -Force -Alias $bad -TimeZone $zone
    Check "Push-Localswim -Alias '$bad' starts no ssh" ($null -eq (Get-SshArgs))
    Check "Push-Localswim -Alias '$bad' warns" (($script:said -join ' ') -match 'alias for the localswim token is not valid')
  }
  Check 'A refused alias leaves the record as it was' ((Get-Content $PushedFile -Raw) -eq $sha)

  $env:AGENTSPACE_TEST_SSH = 'fail'
  $script:said = @()
  Push-Localswim $id -TimeZone $zone
  Check 'An unchanged token is not sent again' ($script:said.Count -eq 0)

  $script:said = @()
  Push-Localswim $id -Force -TimeZone $zone
  Check 'A forced send that fails leaves the record as it was' ((Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A forced send that fails warns' (($script:said -join ' ') -match 'did not confirm')

  # The time zone (card #0061, D32). A zone is part of the stamp, goes to the box only when valid,
  # and is confirmed by the box's own zone line, which never changes whether the token counts.
  $badZoneMessage = "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone."
  function Test-ZoneSent([string]$Zone) {
    $a = Get-SshArgs
    $null -ne $a -and (($a -join ' ') -match "set-timezone\s+$([regex]::Escape($Zone))(\s|$|[^A-Za-z0-9_+/-])")
  }
  function Reset-Stamp { Remove-Item $PushedFile, $sshArgs, $box -ErrorAction SilentlyContinue; $script:said = @() }

  $env:AGENTSPACE_TEST_SSH = 'right'
  $env:AGENTSPACE_TEST_ZONE = 'ok'
  Check 'The stamp differs by zone, and an empty zone differs from a named one' ($sha -ne (Get-Stamp $zone2) -and $sha -ne (Get-Stamp ''))

  Reset-Stamp
  Push-Localswim $id -TimeZone $zone
  Check 'A valid zone is sent to the box in the remote command' (Test-ZoneSent $zone) ((Get-SshArgs) -join ' | ')
  Check 'A valid zone the box confirms writes the stamp with the zone in it' ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone))
  Check 'A valid zone the box confirms warns about nothing' (-not (($script:said -join ' ') -match 'agentspace:'))
  Check 'A valid zone leaves the token as the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)

  $script:said = @()
  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  Push-Localswim $id -TimeZone $zone
  Check 'The same zone is not pushed again' ($null -eq (Get-SshArgs) -and $script:said.Count -eq 0)

  $script:said = @()
  Push-Localswim $id -TimeZone $zone2
  Check 'A changed zone makes the push due again' ($null -ne (Get-SshArgs) -and (Test-ZoneSent $zone2)) ((Get-SshArgs) -join ' | ')
  Check 'A changed zone writes the stamp for the new zone' ((Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone2))

  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  Push-Localswim $id -TimeZone $zone
  Check 'Changing the zone back makes the push due again' ($null -ne (Get-SshArgs) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone))

  # A leading '-' would reach timedatectl as an option (-H <host>), and the Kelvin sign (U+212A)
  # matches [A-Za-z] in a case-insensitive match.
  $kelvinZone = 'Europe/' + [char]0x212A + 'yiv'
  foreach ($bad in 'Not a zone', 'America/New_York;reboot', 'a$(id)', '../etc', 'A//B', '/America/New_York', 'America/New_York/', "America/New_York`n", 'a"b', "a'b", 'a`b',
    '-H', '-Hhost', '--help', $kelvinZone) {
    $shown = if ($bad -ceq $kelvinZone) { "'Europe/Kyiv' with its K as U+212A, the Kelvin sign," } else { "'$($bad -replace "`n", '\n')'" }
    Reset-Stamp
    Push-Localswim $id -TimeZone $bad
    $a = Get-SshArgs
    Check "An invalid zone $shown still pushes the token" ($null -ne $a -and (Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
    Check "An invalid zone $shown sends no zone to the box" ($null -ne $a -and -not (($a -join ' ') -match 'timedatectl|set-timezone')) ($a -join ' | ')
    Check "An invalid zone $shown says the zone could not be worked out" ($script:said -contains $badZoneMessage) ($script:said -join ' | ')
    Check "An invalid zone $shown records the stamp with an empty zone" ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp ''))
  }

  Reset-Stamp
  Push-Localswim $id -TimeZone ''
  $a = Get-SshArgs
  Check 'An empty zone sends no zone and says so' ($null -ne $a -and -not (($a -join ' ') -match 'set-timezone') -and ($script:said -contains $badZoneMessage))
  Check 'An empty zone still writes the token and the stamp with an empty zone' ((Test-Path $box) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp ''))

  # A stamp with an empty zone is stale once a valid zone can be sent, so the zone is asked for later.
  $script:said = @()
  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  Push-Localswim $id -TimeZone $zone
  Check 'A valid zone after a push without one is due again' ($null -ne (Get-SshArgs) -and (Test-ZoneSent $zone) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone))

  # A box that answers zone-failed.
  $env:AGENTSPACE_TEST_ZONE = 'failed'
  Reset-Stamp
  Push-Localswim $id -TimeZone $zone
  Check 'A zone-failed answer says the instance did not take the zone' ($script:said -contains "agentspace: The instance did not take the time zone $zone; times on the box stay in its own zone.") ($script:said -join ' | ')
  Check 'A zone-failed answer does not warn that the token was unconfirmed' (-not (($script:said -join ' ') -match 'did not confirm'))
  Check 'A zone-failed answer leaves the token written on the box' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  Check 'A zone-failed answer writes no stamp' (-not (Test-Path $PushedFile))
  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  $script:said = @()
  Push-Localswim $id -TimeZone $zone
  Check 'A box that refused the zone is asked again on the next connection' ($null -ne (Get-SshArgs) -and (Test-ZoneSent $zone))

  # A zone-failed answer must not disturb a stamp that was already there for another zone.
  $env:AGENTSPACE_TEST_ZONE = 'ok'
  Reset-Stamp
  Push-Localswim $id -TimeZone $zone
  $env:AGENTSPACE_TEST_ZONE = 'failed'
  $script:said = @()
  Push-Localswim $id -TimeZone $zone2
  Check 'A zone-failed answer leaves the earlier stamp as it was' ((Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone))
  Check 'A zone-failed answer names the zone that was sent' ($script:said -contains "agentspace: The instance did not take the time zone $zone2; times on the box stay in its own zone.") ($script:said -join ' | ')

  # A box that prints no zone line at all has not confirmed the zone.
  $env:AGENTSPACE_TEST_ZONE = 'none'
  Reset-Stamp
  Push-Localswim $id -TimeZone $zone
  Check 'A box that prints no zone line leaves no stamp when a zone was sent' (-not (Test-Path $PushedFile))
  Check 'A box that prints no zone line does not warn that the token was unconfirmed' (-not (($script:said -join ' ') -match 'did not confirm'))

  # With no zone sent, no zone line is asked of the box, so a missing one is no failure.
  Reset-Stamp
  Push-Localswim $id -TimeZone 'not a zone'
  Check 'With no zone sent, the token alone confirms the push' ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp ''))

  # The zone line never changes whether the token counts as confirmed.
  $env:AGENTSPACE_TEST_ZONE = 'ok'
  foreach ($mode in 'wrong', 'fail') {
    $env:AGENTSPACE_TEST_SSH = $mode
    Reset-Stamp
    Push-Localswim $id -TimeZone $zone
    Check "A $mode answer with a valid zone records nothing" (-not (Test-Path $PushedFile))
    Check "A $mode answer with a valid zone warns that the token was not confirmed" (($script:said -join ' ') -match 'did not confirm')
  }
  $env:AGENTSPACE_TEST_SSH = 'right'
  Reset-Stamp
  Push-Localswim $id -Force -TimeZone $zone
  Check 'A forced push sends the zone and records the stamp' ((Test-ZoneSent $zone) -and (Get-Content $PushedFile -Raw) -ceq (Get-Stamp $zone))

  # Legacy names (D32). The map is checked through ConvertTo-CurrentZone and Get-LaptopZone, which
  # take the name or Windows ID as an argument, so nothing here depends on the laptop's own zone.
  $legacy = [ordered]@{
    'Asia/Calcutta' = 'Asia/Kolkata'; 'Asia/Katmandu' = 'Asia/Kathmandu'; 'Asia/Rangoon' = 'Asia/Yangon'
    'Europe/Kiev' = 'Europe/Kyiv'; 'America/Buenos_Aires' = 'America/Argentina/Buenos_Aires'
    'America/Godthab' = 'America/Nuuk'; 'America/Indianapolis' = 'America/Indiana/Indianapolis'
  }
  foreach ($old in $legacy.Keys) {
    $got = ConvertTo-CurrentZone $old
    Check "The legacy zone $old maps to $($legacy[$old])" ($got -ceq $legacy[$old]) $got
  }
  foreach ($current in 'America/Los_Angeles', 'Asia/Kolkata', 'Etc/UTC', 'America/Indiana/Indianapolis') {
    $got = ConvertTo-CurrentZone $current
    Check "The current zone $current passes through unchanged" ($got -ceq $current) $got
  }
  # Windows IDs whose CLDR name is legacy, converted by this runtime's path and then mapped.
  $windowsIds = [ordered]@{
    'Pacific Standard Time' = 'America/Los_Angeles'; 'India Standard Time' = 'Asia/Kolkata'
    'Nepal Standard Time' = 'Asia/Kathmandu'; 'Myanmar Standard Time' = 'Asia/Yangon'
    'FLE Standard Time' = 'Europe/Kyiv'; 'Argentina Standard Time' = 'America/Argentina/Buenos_Aires'
    'Greenland Standard Time' = 'America/Nuuk'; 'US Eastern Standard Time' = 'America/Indiana/Indianapolis'
  }
  foreach ($w in $windowsIds.Keys) {
    $got = Get-LaptopZone $w
    Check "Get-LaptopZone '$w' gives $($windowsIds[$w])" ($got -ceq $windowsIds[$w]) $got
  }
  $got = Get-LaptopZone 'No Such Standard Time'
  Check 'Get-LaptopZone gives nothing for an unknown Windows ID' (-not $got) $got
  # -TimeZone is for tests, and is sent as given, legacy or not.
  Reset-Stamp
  Push-Localswim $id -TimeZone 'Asia/Calcutta'
  Check 'A legacy zone given with -TimeZone is sent as given' (Test-ZoneSent 'Asia/Calcutta') ((Get-SshArgs) -join ' | ')

  # With no -TimeZone the laptop's own zone is read, and the test never changes it.
  $laptopBefore = (Get-TimeZone).Id
  Reset-Stamp
  Push-Localswim $id
  Check 'A push with no -TimeZone still sends the token' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  Check 'A push with no -TimeZone leaves the laptop zone as it was' ((Get-TimeZone).Id -eq $laptopBefore)
  Remove-Item $PushedFile -ErrorAction SilentlyContinue
  $env:AGENTSPACE_TEST_ZONE = 'ok'


  # A copy of agentspace.ps1 in a throwaway repo, run as ssh-proxy.cmd runs it. Its environment
  # points the receipt and state at the throwaway folder and leaves the AWS CLI no credentials,
  # so even a broken check could reach neither the token nor AWS.
  $repo = Join-Path $work 'repo'
  New-Item -ItemType Directory (Join-Path $repo 'scripts') | Out-Null
  Copy-Item (Join-Path $PSScriptRoot 'agentspace.ps1') (Join-Path $repo 'scripts\agentspace.ps1')
  Set-Content (Join-Path $repo '.instance-id') $id -NoNewline
  $isolated = @{
    LOCALAPPDATA = $work; USERPROFILE = $work; AGENTSPACE_NESTED = $null
    AWS_CONFIG_FILE = (Join-Path $work 'no-aws-config'); AWS_SHARED_CREDENTIALS_FILE = (Join-Path $work 'no-aws-credentials')
    AWS_EC2_METADATA_DISABLED = 'true'; AWS_PROFILE = $null; AWS_ACCESS_KEY_ID = $null; AWS_SECRET_ACCESS_KEY = $null; AWS_SESSION_TOKEN = $null
  }
  $ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  $run = "-NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $repo 'scripts\agentspace.ps1')`""
  foreach ($bad in '"a;b"', '"a b"', '"a&b"', '""', '-Fx') {
    $r = Invoke-Child $ps "$run up -Alias $bad" $isolated
    Check "agentspace.ps1 up -Alias $bad fails" ($r.Code -ne 0) "exit $($r.Code)"
    Check "agentspace.ps1 up -Alias $bad fails on the alias, before anything runs" ($r.Err -match '-Alias MUST be an SSH host alias') $r.Err
    Check "agentspace.ps1 up -Alias $bad writes nothing to stdout" ($r.Out -eq '') $r.Out
  }
  $r = Invoke-Child $ps "$run status -Alias $leaseAlias" $isolated
  Check 'agentspace.ps1 refuses -Alias on a command other than up' ($r.Code -ne 0 -and $r.Err -match '-Alias applies to up alone') $r.Err

  # Copies of ssh-proxy.cmd whose aws line is a stand-in tunnel that exits 7, so a run that reaches
  # the tunnel is told apart from one that stops at up, and the tunnel's exit code is checked.
  $proxyLines = [IO.File]::ReadAllLines((Join-Path $PSScriptRoot 'ssh-proxy.cmd'))
  $awsAt = @(for ($i = 0; $i -lt $proxyLines.Count; $i++) {
      if ($proxyLines[$i].StartsWith('"C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session ')) { $i } })
  if ($awsAt.Count -ne 1) { throw "ssh-proxy.cmd has $($awsAt.Count) aws start-session lines; the test expects exactly one." }
  $proxyLines[$awsAt[0]] = 'C:\Windows\System32\cmd.exe /d /c "exit 7"'
  function New-ProxyCopy([string]$Dir) {
    New-Item -ItemType Directory (Join-Path $Dir 'scripts') | Out-Null
    [IO.File]::WriteAllText((Join-Path $Dir 'scripts\ssh-proxy.cmd'), (($proxyLines -join "`r`n") + "`r`n"), [Text.Encoding]::ASCII)
    Set-Content (Join-Path $Dir '.instance-id') $id -NoNewline
    Join-Path $Dir 'scripts\ssh-proxy.cmd'
  }
  $cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'

  # The first copy sits beside a stand-in agentspace.ps1 that records its arguments and exits with
  # AGENTSPACE_TEST_UP_EXIT.
  $proxy = New-ProxyCopy (Join-Path $work 'proxy')
  Set-Content (Join-Path (Split-Path $proxy) 'agentspace.ps1') @'
Set-Content -Path $env:AGENTSPACE_TEST_UP -Value ($args -join ' ') -NoNewline
exit [int]$env:AGENTSPACE_TEST_UP_EXIT
'@
  $upArgs = Join-Path $work 'up-args.txt'
  # The two ProxyCommand lines exactly as Terry's entry and a lease alias write them.
  $shaped = [ordered]@{
    "/d /c `"$proxy`""             = 'up'
    "/d /c `"$proxy`" $leaseAlias" = "up -Alias $leaseAlias"
  }
  foreach ($line in $shaped.Keys) {
    $label = $line -replace '^.*ssh-proxy\.cmd"\s*', ''
    $label = if ($label) { "ssh-proxy.cmd $label" } else { 'ssh-proxy.cmd with no argument' }

    Remove-Item $upArgs -ErrorAction SilentlyContinue
    $r = Invoke-Child $cmd $line @{ AGENTSPACE_TEST_UP = $upArgs; AGENTSPACE_TEST_UP_EXIT = '1' }
    $got = if (Test-Path $upArgs) { Get-Content $upArgs -Raw } else { '(not run)' }
    Check "$label runs agentspace.ps1 $($shaped[$line])" ($got -eq $shaped[$line]) $got
    Check "$label stops when up fails, starting no tunnel and writing nothing to stdout" ($r.Code -eq 1 -and $r.Out -eq '') "exit $($r.Code), stdout '$($r.Out)'"

    Remove-Item $upArgs -ErrorAction SilentlyContinue
    $r = Invoke-Child $cmd $line @{ AGENTSPACE_TEST_UP = $upArgs; AGENTSPACE_TEST_UP_EXIT = '0' }
    $got = if (Test-Path $upArgs) { Get-Content $upArgs -Raw } else { '(not run)' }
    Check "$label runs the tunnel after up succeeds" ($got -eq $shaped[$line]) $got
    Check "$label exits with the tunnel's code" ($r.Code -eq 7) "exit $($r.Code)"
  }
  # A second argument is refused before agentspace.ps1 runs. /s keeps each line exactly as written
  # here. Cmd splits an argument at ';' as it does at a space.
  foreach ($arg in "$leaseAlias extra", 'a;b') {
    Remove-Item $upArgs -ErrorAction SilentlyContinue
    $r = Invoke-Child $cmd "/d /s /c `"`"$proxy`" $arg`"" @{ AGENTSPACE_TEST_UP = $upArgs; AGENTSPACE_TEST_UP_EXIT = '0' }
    Check "ssh-proxy.cmd $arg is refused before agentspace.ps1 runs" (-not (Test-Path $upArgs) -and $r.Code -eq 1 -and
      $r.Err -match 'ssh-proxy\.cmd takes at most one argument') "exit $($r.Code), stderr '$($r.Err.Trim())'"
    Check "ssh-proxy.cmd $arg writes nothing to stdout" ($r.Out -eq '') "stdout '$($r.Out)'"
  }

  # The second copy sits beside the real agentspace.ps1, run in the isolated environment above. A
  # bad alias MUST stop up on its alias check, before any ssh or tunnel. Arguments with cmd
  # metacharacters are not tried: the outer cmd.exe parses the whole ProxyCommand line before
  # ssh-proxy.cmd runs, so whoever can put one there already runs commands. The line MUST come
  # from lease-alias.ps1, whose exact alias check is the gate (ssh-proxy.cmd's header).
  $realProxy = New-ProxyCopy (Join-Path $work 'proxy-real')
  Copy-Item (Join-Path $PSScriptRoot 'agentspace.ps1') (Split-Path $realProxy)
  foreach ($arg in '-Fx', '""', '"a b"', 'a/b') {
    $r = Invoke-Child $cmd "/d /s /c `"`"$realProxy`" $arg`"" $isolated
    Check "ssh-proxy.cmd $arg fails in up on the alias, starting no tunnel" ($r.Code -eq 1 -and
      $r.Err -match '-Alias MUST be an SSH host alias') "exit $($r.Code), stderr '$($r.Err.Trim())'"
    Check "ssh-proxy.cmd $arg writes nothing to stdout" ($r.Out -eq '') "stdout '$($r.Out)'"
  }
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  Remove-Item Env:\AGENTSPACE_TEST_SSH, Env:\AGENTSPACE_TEST_BOX, Env:\AGENTSPACE_TEST_ARGS, Env:\AGENTSPACE_TEST_ZONE -ErrorAction SilentlyContinue
  # A hung stand-in that was not stopped can hold its folder open for a few seconds.
  for ($n = 0; $n -lt 30 -and (Test-Path $work); $n++) {
    try { Remove-Item -Recurse -Force $work } catch { Start-Sleep -Seconds 1 }
  }
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'
