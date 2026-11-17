# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for Push-Localswim in scripts\agentspace.ps1 (card #0026). It loads that function
# and Get-BotEnvHash from the script's syntax tree, points them at a throwaway receipt, a dummy
# token and a stand-in for ssh, and checks that a send is recorded only when the stand-in confirms
# the file it wrote. Nothing reaches the box, and the real token and hash file are never touched.
# It MUST run in Windows PowerShell 5.1, as agentspace.ps1 does:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-push-localswim.ps1
$ErrorActionPreference = 'Stop'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}

$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'agentspace.ps1'), [ref]$null, [ref]$null)
foreach ($name in 'Push-Localswim', 'Get-BotEnvHash') {
  $f = $ast.Find({ param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name }, $true)
  if (-not $f) { throw "agentspace.ps1 has no function $name." }
  . ([scriptblock]::Create($f.Extent.Text))
}
$script:said = @()
function Say([string]$Text) { $script:said += $Text }

$work = Join-Path ([IO.Path]::GetTempPath()) "agentspace-test-push-$(Get-Date -Format yyyyMMddHHmmss)"
New-Item -ItemType Directory $work | Out-Null
try {
  # A throwaway receipt and DPAPI credential file, shaped like localswim's own.
  $credential = New-Object Management.Automation.PSCredential 'Bot', (ConvertTo-SecureString 'dummy-not-a-token' -AsPlainText -Force)
  $credential | Export-Clixml (Join-Path $work 'bot.xml')
  @{ base = 'http://127.0.0.1:8811'; credential_directory = $work } | ConvertTo-Json | Set-Content (Join-Path $work 'service.json')
  $expected = "LOCALSWIM_URL=http://127.0.0.1:8811`nLOCALSWIM_TOKEN=dummy-not-a-token`n"

  # The stand-in for ssh. Mode right plays the box: it reads two lines, writes them as the box
  # would and prints their SHA-256. The others answer with a wrong hash, fail, or hang.
  $box = Join-Path $work 'bot.env'
  Set-Content (Join-Path $work 'fake-ssh.ps1') @'
$lines = @()
for ($i = 0; $i -lt 2; $i++) { $l = [Console]::In.ReadLine(); if ($null -eq $l) { break }; $lines += $l }
$text = (($lines | ForEach-Object { ($_ -replace "`r", '').TrimStart([char]0xFEFF) } | Where-Object { $_ }) -join "`n") + "`n"
$bytes = [Text.Encoding]::UTF8.GetBytes($text)
[IO.File]::WriteAllBytes($env:AGENTSPACE_TEST_BOX, $bytes)
$hash = -join ([Security.Cryptography.SHA256]::Create().ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
[Console]::Out.WriteLine("$hash  -")
'@
  Set-Content (Join-Path $work 'fake-ssh.cmd') @'
@echo off
if "%AGENTSPACE_TEST_SSH%"=="fail" exit /b 255
if "%AGENTSPACE_TEST_SSH%"=="wrong" (echo 0000000000000000000000000000000000000000000000000000000000000000  -& exit /b 0)
if "%AGENTSPACE_TEST_SSH%"=="hang" (ping -n 30 127.0.0.1 >nul & exit /b 0)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-ssh.ps1"
'@
  $env:AGENTSPACE_TEST_BOX = $box

  # The script variables Push-Localswim reads.
  $LocalswimReceipt = Join-Path $work 'service.json'
  $PushedFile = Join-Path $work 'state\localswim-pushed.sha256'
  $Ssh = Join-Path $work 'fake-ssh.cmd'
  $NoForward = @('-o', 'ClearAllForwardings=yes')
  $PushTimeoutSec = 5
  $Taskkill = Join-Path $env:SystemRoot 'System32\taskkill.exe'
  $id = 'i-0123456789abcdef0'
  $sha = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes("$id`n$expected"))).Replace('-', '')

  Check 'Get-BotEnvHash ignores CRs, a BOM and blank lines' (
    (Get-BotEnvHash ([char]0xFEFF + "LOCALSWIM_URL=http://127.0.0.1:8811`r`n`r`nLOCALSWIM_TOKEN=dummy-not-a-token`r`n")) -eq (Get-BotEnvHash $expected))

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
  Push-Localswim $id
  Check 'A confirmed send is recorded' ((Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A confirmed send warns about nothing' (-not (($script:said -join ' ') -match 'agentspace:'))
  Check 'The stand-in box got exactly the two lines' ((Test-Path $box) -and [IO.File]::ReadAllText($box) -eq $expected)

  $env:AGENTSPACE_TEST_SSH = 'fail'
  $script:said = @()
  Push-Localswim $id
  Check 'An unchanged token is not sent again' ($script:said.Count -eq 0)

  $script:said = @()
  Push-Localswim $id -Force
  Check 'A forced send that fails leaves the record as it was' ((Get-Content $PushedFile -Raw) -eq $sha)
  Check 'A forced send that fails warns' (($script:said -join ' ') -match 'did not confirm')
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  Remove-Item Env:\AGENTSPACE_TEST_SSH, Env:\AGENTSPACE_TEST_BOX -ErrorAction SilentlyContinue
  # The hang case's ping can outlive the test by a few seconds and hold its folder open.
  for ($n = 0; $n -lt 30 -and (Test-Path $work); $n++) {
    try { Remove-Item -Recurse -Force $work } catch { Start-Sleep -Seconds 1 }
  }
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'
