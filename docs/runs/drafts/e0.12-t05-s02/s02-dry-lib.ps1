# The drafter's helpers for E0.12-T05-S02's dry runs (dot-sourced by s02-dry-clean.ps1 and s02-dry-planted.ps1; drafter-owned). RFC 2119
# keywords; the capitals are load-bearing.
#
# - A lab is a conformance-harness throwaway home (conformance\lease-cli\harness.ps1) under target\runs\drafter\tmp inside the worktree, never the real temp
#   folder: TEMP and TMP are set to it and stay set. The step scripts run through scripts\live\wrap.ps1 with USERPROFILE, HOME, LOCALAPPDATA and APPDATA
#   moved into the home and the AWS variables removed (Get-ConformanceChildEnvironment's own), and S02_STANDIN_* naming the stand-ins.
# - No toolchain program (cargo, rustc, rustdoc, link.exe) is started in a moved home. The only programs started are pwsh (the wrapper and the
#   stand-ins) and, in the one real-build scenario, the S01 build's agentspace.exe from its own worktree, read and run there, never written.
# - The installed programs are never named: S02_PROGRAM_AGENTSPACE refuses the installed folder, and a stand-in answers every other tool.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$script:DDir = $PSScriptRoot
$script:DRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:DOut = Join-Path $script:DRepo 'target\runs\drafter'
$script:DTmp = Join-Path $script:DOut 'tmp'
New-Item -ItemType Directory -Force -Path $script:DOut, $script:DTmp | Out-Null
$env:TEMP = $script:DTmp
$env:TMP = $script:DTmp
$env:AGENTSPACE_OPERATOR_CMD = Join-Path $script:DDir 's02-standin-ca.cmd'
. (Join-Path $script:DRepo 'conformance\lease-cli\harness.ps1')
$script:DUtf8 = [Text.UTF8Encoding]::new($false)
$script:DAnswers = Join-Path $script:DDir 's02-standin-answers'
$script:DS01Dist = 'C:\Projects\agentspace\.claude\worktrees\t05-s01\target\dist'

function Write-DFile([string]$Path, [string]$Text) {
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [IO.File]::WriteAllBytes($Path, $script:DUtf8.GetBytes(($Text -replace "`r`n", "`n")))
}

function ConvertTo-DUtc([DateTime]$T) { return $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }

# The next free three-digit wrapper number in the drafter's folder, from 100 up for a driver's inner wrappers.
function Get-DNextNumber([int]$From) {
  $used = @(Get-ChildItem -LiteralPath $script:DOut -Filter '*.start' -Name -ErrorAction SilentlyContinue | ForEach-Object { if ($_ -match '^(\d{3})') { [int]$Matches[1] } })
  $n = $From
  while ($used -contains $n) { $n++ }
  return $n
}

function New-DLab { return (New-ConformanceHome) }

function Remove-DLab($Lab) {
  Remove-ConformanceHome -ConformanceHome $Lab
  Write-Host "Home removed: $(-not (Test-Path -LiteralPath $Lab.Root))"
}

# A scenario: a folder of the lab with its own answers (the clean ones, then the overlay), calls, counters, state, observer files and logs.
function New-DScenario {
  param($Lab, [string]$Name, [hashtable]$Overlay = @{}, [hashtable]$State = @{}, [string]$ProvLog, [string]$GuardLog, [string]$Edges, [double]$HoursOpen = 1.0, [hashtable]$Env = @{}, [string[]]$Remove = @())
  $root = Join-Path $Lab.Root "cases\$Name"
  if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
  $answers = Join-Path $root 'answers'
  New-Item -ItemType Directory -Force -Path $answers | Out-Null
  Copy-Item -Path (Join-Path $script:DAnswers '*') -Destination $answers -Recurse -Force
  foreach ($r in $Remove) { Remove-Item -LiteralPath (Join-Path $answers $r) -Force -ErrorAction SilentlyContinue }
  foreach ($k in $Overlay.Keys) { Write-DFile (Join-Path $answers $k) ([string]$Overlay[$k]) }
  $st = Join-Path $root 'state'
  New-Item -ItemType Directory -Force -Path $st | Out-Null
  foreach ($k in $State.Keys) { Write-DFile (Join-Path $st $k) ([string]$State[$k]) }
  $obs = Join-Path $root 'observe'
  New-Item -ItemType Directory -Force -Path $obs | Out-Null
  $start = [DateTime]::UtcNow.AddHours(-$HoursOpen)
  Write-DFile (Join-Path $obs 'hours.json') ('[{"host":"agentspace.windows-amd64","start_utc":"' + (ConvertTo-DUtc $start) + '","start_local":"x","end_utc":null,"end_local":null}]' + "`n")
  if (-not $PSBoundParameters.ContainsKey('Edges')) { $Edges = "2026-11-26T01:00:00Z 8:00:00pm agentspace.windows-amd64 unseen -> stopped type=m8a.large`n" }
  Write-DFile (Join-Path $obs 'edges.log') $Edges
  # The lab's own .agentspace logs, rewritten for the scenario.
  $up = $Lab.UserProfile
  $prov = Join-Path $up '.agentspace\hosts\agentspace.windows-amd64\provision.log'
  $guard = Join-Path $up '.agentspace\guard.log'
  Remove-Item -LiteralPath $prov, $guard -Force -ErrorAction SilentlyContinue
  if (-not $PSBoundParameters.ContainsKey('ProvLog')) { $ProvLog = "2026-11-26T20:00:00Z run begun`n2026-11-26T20:30:00Z run done`n" }
  if (-not $PSBoundParameters.ContainsKey('GuardLog')) { $GuardLog = "2026-11-26T21:10:00Z host=agentspace.windows-amd64 stopped since=2026-11-26T20:30:00Z`n2026-11-26T21:11:00Z host=agentspace.linux-amd64 unexplained since=2026-11-26T20:30:00Z stops=2026-11-26T21:15:00Z`n" }
  Write-DFile $prov $ProvLog
  Write-DFile $guard $GuardLog
  return [pscustomobject]@{ Name = $Name; Root = $root; Answers = $answers; State = $st; Observe = $obs; Calls = (Join-Path $root 'calls'); Prov = $prov; Guard = $guard; Env = $Env; Lab = $Lab }
}

# Runs one step script through the wrapper in a scenario. Returns Exit, Text (the masked file), Name (the wrapper's name).
function Invoke-DStep {
  param($Scenario, [string]$Step, [string]$Tag, [hashtable]$Extra = @{}, [switch]$RealBuild)
  $lab = $Scenario.Lab
  $script = Join-Path $script:DDir "s02-step-$Step.ps1"
  $n = Get-DNextNumber 100
  $name = ('{0:D3}-{1}-{2}' -f $n, $Tag, $Step)
  $vars = [ordered]@{}
  # The harness's own child environment moves the profile and clears AWS; it also names AGENTSPACE_* folders, which the real-build scenario needs.
  $base = Get-ConformanceChildEnvironment -ConformanceHome $lab -Override @{}
  foreach ($k in $base.Keys) { if ($k -cne 'PSModulePath') { $vars[$k] = $base[$k] } }
  $vars['S02_DRY'] = '1'
  $vars['S02_STATE'] = $Scenario.State
  $vars['S02_OBSERVE'] = $Scenario.Observe
  $vars['S02_STANDIN_DIR'] = $Scenario.Answers
  $vars['S02_STANDIN_CALLS'] = $Scenario.Calls
  $vars['S02_STANDIN_PROVLOG'] = $Scenario.Prov
  $vars['S02_STANDIN_OPERATOR'] = Join-Path $script:DDir 's02-standin-operator.ps1'
  $vars['S02_STANDIN_SSH'] = Join-Path $script:DDir 's02-standin-ssh.ps1'
  $vars['S02_STANDIN_SCP'] = Join-Path $script:DDir 's02-standin-scp.ps1'
  $vars['S02_STANDIN_AWS'] = Join-Path $script:DDir 's02-standin-aws.ps1'
  $vars['S02_STANDIN_LOCALSWIM'] = Join-Path $script:DDir 's02-standin-localswim.ps1'
  $vars['S02_ENVLIB'] = Join-Path $script:DDir 's02-standin-env.ps1'
  $vars['S02_POLL_SECONDS'] = '1'
  $vars['S02_MAX_SECONDS'] = '60'
  $vars['S02_IDLE_SECONDS'] = '2'
  $vars['S02_STOP_SECONDS'] = '60'
  $vars['S02_SLEEP_SECONDS'] = '6'
  $vars['S02_HOLD_SECONDS'] = '2'
  $vars['S02_P2_TEXT_FILE'] = Join-Path $Scenario.Root 'p2-text.txt'
  Write-DFile $vars['S02_P2_TEXT_FILE'] "S02 DRY FIXED TEXT 4242`n"
  if ($RealBuild) { $vars['S02_PROGRAM_AGENTSPACE'] = Join-Path $script:DS01Dist 'agentspace.exe'; $vars['AGENTSPACE_PROVISION_CMD'] = Initialize-ConformanceTripwire -ConformanceHome $lab }
  else { $vars['S02_STANDIN_AGENTSPACE'] = Join-Path $script:DDir 's02-standin-agentspace.ps1' }
  foreach ($k in $Scenario.Env.Keys) { $vars[$k] = $Scenario.Env[$k] }
  foreach ($k in $Extra.Keys) { $vars[$k] = $Extra[$k] }
  $saved = @{}
  foreach ($k in $vars.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $(if ($null -eq $vars[$k]) { $null } else { [string]$vars[$k] })) }
  try {
    & pwsh -NoProfile -File (Join-Path $script:DRepo 'scripts\live\wrap.ps1') -Out $script:DOut -Name $name -Script $script | Out-Null
  } finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
  }
  $exitFile = Join-Path $script:DOut "$name.exit"
  $code = -1
  if (Test-Path -LiteralPath $exitFile) { $code = [int](([IO.File]::ReadAllLines($exitFile) | Where-Object { $_ -like 'exit=*' } | Select-Object -First 1) -replace '^exit=', '') }
  $text = if (Test-Path -LiteralPath (Join-Path $script:DOut "$name.masked.txt")) { [IO.File]::ReadAllText((Join-Path $script:DOut "$name.masked.txt")) } else { '' }
  return [pscustomobject]@{ Exit = $code; Text = $text; Name = $name }
}

# The calls log of a scenario, as lines.
function Get-DCalls($Scenario) {
  $f = Join-Path $Scenario.Calls 'calls.log'
  if (-not (Test-Path -LiteralPath $f)) { return @() }
  return @([IO.File]::ReadAllLines($f))
}

$script:DBad = 0
$script:DCases = 0

# A case's verdict: the exit code (an exact one, or any nonzero), each substring required, each forbidden.
function Write-DVerdict {
  param([string]$Case, $R, [int]$ExitWant = -1, [string[]]$Must = @(), [string[]]$MustNot = @())
  $script:DCases++
  $bad = @()
  if ($ExitWant -ge 0) { if ($R.Exit -ne $ExitWant) { $bad += "exit $($R.Exit), expected $ExitWant" } }
  elseif ($R.Exit -eq 0) { $bad += 'exit 0, expected nonzero' }
  foreach ($m in $Must) { if (-not $R.Text.Contains($m)) { $bad += "output lacks '$m'" } }
  foreach ($m in $MustNot) { if ($R.Text.Contains($m)) { $bad += "output holds '$m'" } }
  if ($bad.Count -eq 0) { Write-Host "CASE-OK $Case $($R.Name) (exit $($R.Exit))"; return }
  $script:DBad++
  Write-Host "CASE-BAD $Case $($R.Name) : $($bad -join '; ')"
}
