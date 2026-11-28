# The helpers of the drafter's dry runs of the v0.12-T05 linux/amd64 check (dot-sourced by s01-dry-clean.ps1 and
# s01-dry-planted.ps1). Drafter-owned. RFC 2119 keywords; the capitals are load-bearing.
#
# - A lab is a conformance-harness throwaway home under target\t05\s01-tmp (TEMP and TMP point there, never at the real temp
#   folder). The driver moves USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the home, removes the AWS variables, and
#   restores them in its finally. It runs no cargo, rustc, rustdoc or link.exe, and no installed program.
# - A case is one step script run through scripts\live\wrap.ps1 (-Out target\runs\drafter), with S01_DRY=1 and the stand-ins
#   named. Its folder under target\t05\s01-dry\<tag>\ holds the raw state, the stand-ins' files and the observer's files.
#   A case's verdict reads only the wrapper's .masked.txt and .exit files.
# - Every case appends `<wrapper name> <expected exit>` to target\runs\drafter\expected-exits.txt, which s01-dry-count.ps1
#   compares with the .exit files.
$script:DRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:DDir = $PSScriptRoot
$script:DTmp = Join-Path $script:DDir 's01-tmp'
$script:DCases = Join-Path $script:DDir 's01-dry'
$script:DOut = Join-Path $script:DRepo 'target\runs\drafter'
New-Item -ItemType Directory -Force -Path $script:DTmp, $script:DCases, $script:DOut | Out-Null
$env:TEMP = $script:DTmp
$env:TMP = $script:DTmp
$env:AGENTSPACE_OPERATOR_CMD = Join-Path $script:DDir 's01-standin-noca.cmd'
. (Join-Path $script:DRepo 'conformance\lease-cli\harness.ps1')
. (Join-Path $script:DDir 's01-standin-lib.ps1')
$script:DUtf8 = [Text.UTF8Encoding]::new($false)
$script:DInv = [Globalization.CultureInfo]::InvariantCulture
$script:DSaved = @{}
$script:DAmd = 'agentspace.linux-amd64'

function New-S01Lab { return (New-ConformanceHome) }

# Moves the process's profile variables into the lab's home, keeping the real values for Restore-S01Env.
function Enter-S01Home {
  param($Lab)
  foreach ($n in 'USERPROFILE', 'HOME', 'LOCALAPPDATA', 'APPDATA', 'AWS_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN', 'AWS_CONFIG_FILE', 'AWS_SHARED_CREDENTIALS_FILE', 'AWS_EC2_METADATA_DISABLED', 'AGENTSPACE_OPERATOR_CMD') {
    $script:DSaved[$n] = [Environment]::GetEnvironmentVariable($n)
  }
  $env:USERPROFILE = $Lab.UserProfile
  $env:HOME = $Lab.UserProfile
  $env:LOCALAPPDATA = Join-Path $Lab.UserProfile 'AppData\Local'
  $env:APPDATA = Join-Path $Lab.UserProfile 'AppData\Roaming'
  foreach ($n in 'AWS_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN') { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
  $env:AWS_CONFIG_FILE = Join-Path $Lab.UserProfile 'no-aws\config'
  $env:AWS_SHARED_CREDENTIALS_FILE = Join-Path $Lab.UserProfile 'no-aws\credentials'
  $env:AWS_EC2_METADATA_DISABLED = 'true'
  Remove-Item Env:AGENTSPACE_OPERATOR_CMD -ErrorAction SilentlyContinue
}
# [Environment]::SetEnvironmentVariable with $null or an empty string leaves an empty variable in this runtime, so a variable is
# removed with Remove-Item.
function Set-DEnv([string]$Name, [string]$Value) {
  if ([string]::IsNullOrEmpty($Value)) { Remove-Item "Env:$Name" -ErrorAction SilentlyContinue } else { Set-Item "Env:$Name" -Value $Value }
}
function Restore-S01Env {
  foreach ($n in @($script:DSaved.Keys)) { Set-DEnv $n $script:DSaved[$n] }
}


# The real operator's `ca` refuses a folder inside a git working tree, and the drafter's scratch MUST be inside the worktree's
# target\, so a home's CA is made by hand: ssh-keygen, then icacls to the current user alone, as `ca` makes it (operator.md, ca,
# step 4). Nothing of either tool's output is printed (icacls names the user).
function Add-S01HandCa {
  param($Lab)
  $dir = Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $Lab.UserProfile '.agentspace\ca')
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  & icacls.exe $dir /inheritance:r /grant:r "*${sid}:(OI)(CI)F" 2>&1 | Out-Null
  $key = Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $dir 'agentspace_ca')
  & ssh-keygen.exe -q -t ed25519 -N '' -C 'agentspace-ca <computer name>' -f $key 2>&1 | Out-Null
  & icacls.exe $key /reset 2>&1 | Out-Null
  & icacls.exe $key /inheritance:d 2>&1 | Out-Null
  $Lab.CaReady = ((Test-Path -LiteralPath $key) -and (Test-Path -LiteralPath ($key + '.pub')))
  return $Lab.CaReady
}
# --- Files of a case ---------------------------------------------------------------------------------------------

function New-S01CaseDir([string]$Tag) {
  $d = Join-Path $script:DCases $Tag
  if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
  foreach ($s in 'state', 'standin', 'observe') { New-Item -ItemType Directory -Force -Path (Join-Path $d $s) | Out-Null }
  return $d
}
function Write-S01File([string]$Path, [string]$Text) { [IO.File]::WriteAllBytes($Path, $script:DUtf8.GetBytes($Text)) }
function Format-DTime([DateTimeOffset]$t) { return $t.ToString('yyyy-MM-ddTHH:mm:ssZ', $script:DInv) }

# The observer's files. Hours: an open interval of the amd64 host that began $OpenMinutes ago (none when 0), plus a closed
# 3-hour interval of the arm64 host, which MUST NOT count. $Hours of 'missing' leaves hours.json out; 'stale' ages it.
function Write-S01Observe {
  param([string]$Case, [double]$OpenMinutes = 20, [string]$Hours = 'ok', [string[]]$Edges = @())
  $dir = Join-Path $Case 'observe'
  $now = [DateTimeOffset]::UtcNow
  $items = @()
  $items += [ordered]@{ host = 'agentspace'; start_utc = (Format-DTime $now.AddHours(-8)); start_local = '1:00pm'; end_utc = (Format-DTime $now.AddHours(-5)); end_local = '4:00pm' }
  if ($OpenMinutes -gt 0) { $items += [ordered]@{ host = $script:DAmd; start_utc = (Format-DTime $now.AddMinutes(-$OpenMinutes)); start_local = '9:00pm'; end_utc = $null; end_local = $null } }
  $f = Join-Path $dir 'hours.json'
  if ($Hours -cne 'missing') {
    Write-S01File $f (ConvertTo-Json -InputObject @($items) -Depth 4)
    if ($Hours -ceq 'stale') { (Get-Item -LiteralPath $f).LastWriteTimeUtc = [DateTime]::UtcNow.AddMinutes(-30) }
  }
  Write-S01File (Join-Path $dir 'edges.log') (($Edges | ForEach-Object { $_ + "`n" }) -join '')
}
function New-S01EdgeLine([DateTimeOffset]$At, [string]$From = 'stopping') {
  return ('{0} {1} {2} {3} -> stopped type=m8a.xlarge' -f (Format-DTime $At), '9:50pm', $script:DAmd, $From)
}

# Seeds the stand-ins' store and the state folder as the clean flow leaves them just before a step.
function Initialize-S01Flow {
  param([string]$Case, [ValidateSet('none', 'pending', 'ready', 'two')][string]$Leases = 'none', [string]$Phase = 'before')
  $env:S01_STANDIN_DIR = Join-Path $Case 'standin'
  Write-S01File (Join-Path $Case 'standin\phase.txt') $Phase
  $store = [System.Collections.Generic.List[object]]::new()
  if ($Leases -ne 'none') {
    $a = New-SlLease -State $(if ($Leases -ceq 'pending') { 'pending' } else { 'ready' }) -Size 'm'
    $store.Add($a)
    Write-S01File (Join-Path $Case 'state\lease-A.id') $a['id']
    if ($Leases -ne 'pending') { Write-S01File (Join-Path $Case 'state\doc-212.json') ((ConvertTo-Json -InputObject (Get-SlPublic $a) -Depth 6 -Compress) + "`n") }
    if ($Leases -ceq 'two') {
      $b = New-SlLease -State 'ready' -Size 's'
      $store.Add($b)
      Write-S01File (Join-Path $Case 'state\lease-B.id') $b['id']
    }
  }
  [IO.File]::WriteAllText((Join-Path $Case 'standin\leases.json'), (ConvertTo-Json -InputObject @($store) -Depth 10), $script:DUtf8)
  Remove-Item Env:S01_STANDIN_DIR -ErrorAction SilentlyContinue
}

# The home's own logs (the steps' defaults, under %USERPROFILE%\.agentspace of the throwaway home).
function Write-S01HomeLogs {
  param($Lab, [string[]]$Provision = @(), [string[]]$Guard = @())
  $d = Join-Path $Lab.UserProfile '.agentspace'
  $h = Join-Path $d ('hosts\' + $script:DAmd)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $h) | Out-Null
  Write-S01File (Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $h 'provision.log')) (($Provision | ForEach-Object { $_ + "`n" }) -join '')
  Write-S01File (Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $d 'guard.log')) (($Guard | ForEach-Object { $_ + "`n" }) -join '')
}
$script:DProvisionOk = @(
  '2026-11-27T21:40:01Z run started project=agentspace platform=linux/amd64',
  '2026-11-27T21:44:10Z deploy done',
  '2026-11-27T21:46:30Z sync done',
  '2026-11-27T21:47:00Z place done',
  '2026-11-27T21:47:01Z run done'
)

# --- Running a case --------------------------------------------------------------------------------------------------

# Runs one step script through the wrapper. Returns Exit and Text. -Plant sets S01_PLANT. -Live leaves S01_DRY unset.
function Invoke-S01Case {
  param(
    $Lab, [string]$Case, [string]$Name, [string]$Step, [string]$Plant = '', [switch]$Live, [hashtable]$EnvVars = @{},
    [int]$ExpectExit = 0, [string[]]$Must = @(), [string[]]$MustNot = @(), [switch]$NoStandIns
  )
  $standin = {
    param($leaf) Join-Path $script:DDir $leaf
  }
  $set = [ordered]@{
    S01_STATE_DIR = (Join-Path $Case 'state'); S01_STANDIN_DIR = (Join-Path $Case 'standin'); S01_OBSERVE_DIR = (Join-Path $Case 'observe')
    S01_POLL_SECONDS = '1'; S01_BOUND_SECONDS = '' ; S01_PLANT = $Plant
  }
  if ($Live) { $set = [ordered]@{ S01_STATE_DIR = (Join-Path $Case 'state') } }
  if (-not $Live) {
    $set['S01_DRY'] = '1'
    $set['S01_AGENTSPACE'] = & $standin 's01-standin-agentspace.ps1'
    $set['S01_OPERATOR'] = & $standin 's01-standin-operator.ps1'
    $set['S01_SSH'] = & $standin 's01-standin-ssh.ps1'
    $set['S01_AWS'] = & $standin 's01-standin-aws.ps1'
    $set['S01_MASKPROOF'] = & $standin 's01-standin-maskproof.ps1'
  }
  if ($NoStandIns) { foreach ($k in 'S01_AGENTSPACE', 'S01_OPERATOR', 'S01_SSH', 'S01_AWS', 'S01_MASKPROOF') { $set.Remove($k) } }
  foreach ($k in $EnvVars.Keys) { $set[$k] = $EnvVars[$k] }
  $old = @{}
  foreach ($k in $set.Keys) { $old[$k] = [Environment]::GetEnvironmentVariable($k); Set-DEnv $k ([string]$set[$k]) }
  try {
    & pwsh -NoProfile -File (Join-Path $script:DRepo 'scripts\live\wrap.ps1') -Out $script:DOut -Name $Name -Script (Join-Path $script:DDir "s01-step-$Step.ps1") | Out-Null
  } finally {
    foreach ($k in $set.Keys) { Set-DEnv $k ([string]$old[$k]) }
  }
  Add-Content -LiteralPath (Join-Path $script:DOut 'expected-exits.txt') -Value "$Name $ExpectExit"
  $exitFile = Join-Path $script:DOut "$Name.exit"
  $code = -1
  if (Test-Path -LiteralPath $exitFile) { $code = [int](([IO.File]::ReadAllLines($exitFile) | Where-Object { $_ -like 'exit=*' } | Select-Object -First 1) -replace '^exit=', '') }
  $text = [IO.File]::ReadAllText((Join-Path $script:DOut "$Name.masked.txt"))
  $bad = @()
  if ($code -ne $ExpectExit) { $bad += "exit $code, expected $ExpectExit" }
  foreach ($m in $Must) { if (-not $text.Contains($m)) { $bad += "output lacks '$m'" } }
  foreach ($m in $MustNot) { if ($text.Contains($m)) { $bad += "output holds '$m'" } }
  if ($ExpectExit -ne 0 -and -not $text.Contains('FINDING:')) { $bad += 'a nonzero exit prints no FINDING line' }
  if ($bad.Count -eq 0) { Write-Host "CASE-OK $Name (step $Step, plant '$Plant'): exit $code"; return [pscustomobject]@{ Ok = $true; Exit = $code; Text = $text } }
  Write-Host "CASE-BAD $Name (step $Step, plant '$Plant'): $($bad -join '; ')"
  return [pscustomobject]@{ Ok = $false; Exit = $code; Text = $text }
}

# What no lease step's masked output holds: a printed ID or alias would show as these placeholders.
$script:DNoLease = @('<uuid>', '<lease-alias>')
