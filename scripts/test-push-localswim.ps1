# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for Push-Localswim in scripts\agentspace.ps1 (card #0026) and for the alias it
# sends over (card #0057). It loads Push-Localswim, Get-BotEnvHash and Test-AliasName from the
# script's syntax tree, points them at a throwaway receipt, a dummy token and a stand-in for ssh,
# and checks that a send goes to the right alias and is recorded only when the stand-in confirms
# the file it wrote. It then runs a copy of agentspace.ps1 with bad aliases, and ssh-proxy.cmd
# against a stand-in agentspace.ps1, to check that a bad alias stops each of them before anything
# else runs. Nothing reaches the box or AWS, and the real token and hash file are never touched.
# It MUST run in Windows PowerShell 5.1, as agentspace.ps1 does:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-push-localswim.ps1
$ErrorActionPreference = 'Stop'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}

$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'agentspace.ps1'), [ref]$null, [ref]$null)
foreach ($name in 'Push-Localswim', 'Get-BotEnvHash', 'Test-AliasName') {
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
    return 0;
  }
}
'@
  $env:AGENTSPACE_TEST_BOX = $box
  $env:AGENTSPACE_TEST_ARGS = $sshArgs

  # The script variables Push-Localswim reads.
  $LocalswimReceipt = Join-Path $work 'service.json'
  $PushedFile = Join-Path $work 'state\localswim-pushed.sha256'
  $Ssh = $fakeSsh
  $NoForward = @('-o', 'ClearAllForwardings=yes')
  $PushTimeoutSec = 5
  $Taskkill = Join-Path $env:SystemRoot 'System32\taskkill.exe'
  $id = 'i-0123456789abcdef0'
  $leaseAlias = 'agentspace-swimcoach-a62cb513'
  $sha = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes("$id`n$expected"))).Replace('-', '')
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
  Push-Localswim $id
  Check 'A wrong hash from the box records nothing' (-not (Test-Path $PushedFile))
  Check 'A wrong hash from the box warns' (($script:said -join ' ') -match 'did not confirm')

  $env:AGENTSPACE_TEST_SSH = 'fail'
  $script:said = @()
  Push-Localswim $id
  Check 'A failed ssh records nothing' (-not (Test-Path $PushedFile))
  Check 'A failed ssh warns' (($script:said -join ' ') -match 'did not confirm')

  $env:AGENTSPACE_TEST_SSH = 'hang'
  $script:said = @()
  $start = Get-Date
  Push-Localswim $id
  $took = ((Get-Date) - $start).TotalSeconds
  Check 'A hung ssh is stopped at the time limit' ($took -lt 15) "took $took s"
  Check 'A hung ssh records nothing' (-not (Test-Path $PushedFile))
  Check 'A hung ssh warns' (($script:said -join ' ') -match 'was stopped')

  $env:AGENTSPACE_TEST_SSH = 'right'
  $script:said = @()
  Remove-Item $sshArgs -ErrorAction SilentlyContinue
  Push-Localswim $id
  Check 'A confirmed send is recorded' ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A confirmed send warns about nothing' (-not (($script:said -join ' ') -match 'agentspace:'))
  Check 'The stand-in box got exactly the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  $got = Get-SshArgs
  Check 'Without -Alias the push goes to agentspace, clearing forwards' (
    $got.Count -eq 4 -and $got[0] -eq '-o' -and $got[1] -eq 'ClearAllForwardings=yes' -and $got[2] -eq 'agentspace') ($got -join ' | ')

  $script:said = @()
  Remove-Item $sshArgs, $box -ErrorAction SilentlyContinue
  Push-Localswim $id -Force -Alias $leaseAlias
  $got = Get-SshArgs
  Check 'With -Alias the push goes to that alias, clearing forwards' (
    $got.Count -eq 4 -and $got[0] -eq '-o' -and $got[1] -eq 'ClearAllForwardings=yes' -and $got[2] -eq $leaseAlias) ($got -join ' | ')
  Check 'With -Alias the stand-in box got exactly the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)
  Check 'With -Alias a confirmed send is recorded' ((Get-Content $PushedFile -Raw) -eq $sha)

  foreach ($bad in 'a;b', 'a b', 'a&b', '') {
    $script:said = @()
    Remove-Item $sshArgs -ErrorAction SilentlyContinue
    Push-Localswim $id -Force -Alias $bad
    Check "Push-Localswim -Alias '$bad' starts no ssh" ($null -eq (Get-SshArgs))
    Check "Push-Localswim -Alias '$bad' warns" (($script:said -join ' ') -match 'alias for the localswim token is not valid')
  }
  Check 'A refused alias leaves the record as it was' ((Get-Content $PushedFile -Raw) -eq $sha)

  $env:AGENTSPACE_TEST_SSH = 'fail'
  $script:said = @()
  Push-Localswim $id
  Check 'An unchanged token is not sent again' ($script:said.Count -eq 0)

  $script:said = @()
  Push-Localswim $id -Force
  Check 'A forced send that fails leaves the record as it was' ((Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A forced send that fails warns' (($script:said -join ' ') -match 'did not confirm')

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

  # ssh-proxy.cmd beside a stand-in agentspace.ps1 that records its arguments and fails, so the
  # proxy stops there and never starts the tunnel.
  $proxyDir = Join-Path $work 'proxy\scripts'
  New-Item -ItemType Directory $proxyDir | Out-Null
  Copy-Item (Join-Path $PSScriptRoot 'ssh-proxy.cmd') $proxyDir
  Set-Content (Join-Path $proxyDir 'agentspace.ps1') @'
Set-Content -Path $env:AGENTSPACE_TEST_UP -Value ($args -join ' ') -NoNewline
exit 1
'@
  $proxy = Join-Path $proxyDir 'ssh-proxy.cmd'
  $upArgs = Join-Path $work 'up-args.txt'
  $proxyTemp = Join-Path $work 'tmp'
  New-Item -ItemType Directory $proxyTemp | Out-Null
  $proxyEnv = @{ AGENTSPACE_TEST_UP = $upArgs; TEMP = $proxyTemp; TMP = $proxyTemp }
  $cmd = Join-Path $env:SystemRoot 'System32\cmd.exe'
  # The two ProxyCommand lines exactly as Terry's entry and a lease alias write them.
  $shaped = [ordered]@{
    "/d /c `"$proxy`""             = 'up'
    "/d /c `"$proxy`" $leaseAlias" = "up -Alias $leaseAlias"
  }
  foreach ($line in $shaped.Keys) {
    Remove-Item $upArgs -ErrorAction SilentlyContinue
    $r = Invoke-Child $cmd $line $proxyEnv
    $got = if (Test-Path $upArgs) { Get-Content $upArgs -Raw } else { '(not run)' }
    $label = $line -replace '^.*ssh-proxy\.cmd"\s*', ''
    $label = if ($label) { "ssh-proxy.cmd $label" } else { 'ssh-proxy.cmd with no argument' }
    Check "$label runs agentspace.ps1 $($shaped[$line])" ($got -eq $shaped[$line]) $got
    Check "$label stops when up fails, writing nothing to stdout" ($r.Code -eq 1 -and $r.Out -eq '') "exit $($r.Code), stdout '$($r.Out)'"
  }
  # Arguments a cmd line can hand the proxy that MUST NOT reach agentspace.ps1 or run anything.
  # /s keeps each one exactly as written here; INJECTED in stdout or stderr would mean a command ran.
  $hostile = @('a;b', 'a b', '"a b"', '"a&b"', 'a^&b', '"a|b"', 'a^>b', '"a^b"', '""', '-Fx', '"a!b"',
    '"a%AGENTSPACE_TEST_UNSET%b"', 'a^&b"c&echo INJECTED"', '"x"a^&echo INJECTED"', 'a"b', "$leaseAlias extra")
  foreach ($arg in $hostile) {
    Remove-Item $upArgs -ErrorAction SilentlyContinue
    $r = Invoke-Child $cmd "/d /s /c `"`"$proxy`" $arg`"" $proxyEnv
    Check "ssh-proxy.cmd $arg is refused before agentspace.ps1 runs" (-not (Test-Path $upArgs) -and $r.Code -eq 1 -and
      $r.Err -match 'ssh-proxy\.cmd takes at most one argument') "exit $($r.Code), stderr '$($r.Err.Trim())'"
    Check "ssh-proxy.cmd $arg runs nothing and writes nothing to stdout" ($r.Out -eq '' -and $r.Err -notmatch 'INJECTED' -and
      -not (Test-Path (Join-Path $work 'b'))) "stdout '$($r.Out)'"
  }
  Check 'ssh-proxy.cmd leaves no folder of its own behind' (@(Get-ChildItem $proxyTemp).Count -eq 0)
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  Remove-Item Env:\AGENTSPACE_TEST_SSH, Env:\AGENTSPACE_TEST_BOX, Env:\AGENTSPACE_TEST_ARGS -ErrorAction SilentlyContinue
  # A hung stand-in that was not stopped can hold its folder open for a few seconds.
  for ($n = 0; $n -lt 30 -and (Test-Path $work); $n++) {
    try { Remove-Item -Recurse -Force $work } catch { Start-Sleep -Seconds 1 }
  }
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'
