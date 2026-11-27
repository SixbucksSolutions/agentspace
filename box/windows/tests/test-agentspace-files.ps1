#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests the rules of docs/operator.md, "The host agent's files", "The host agent's tasks", "The board's
# wrapper" and "What a Windows host is" that no seam of "The tests' seams" reaches (E0.12-T03-S07,
# #0260; D96, D101; pins P9, P15, P16, P18), written from operator.md and not from the scripts.
# - The file set, and no first-boot file.
# - Windows PowerShell 5.1: bootstrap.ps1, localswim.ps1 and install.ps1 (whose first step runs under
#   it, so the whole file is parsed by it) MUST parse under it and carry no #Requires for PowerShell 7.
#   The scripts are parsed, never run, under Windows PowerShell 5.1.
# - idle.conf's three settings; localswim's line endings and its launch of PowerShell 7 by full path.
# - localswim.ps1's first refusal, run under PowerShell 7 and under Windows PowerShell 5.1. Its other
#   four steps need the CLI at its fixed path under C:\ProgramData, and no seam moves it, so they are
#   not tested here (reported to the architect as a missing seam).
# - Text checks of what only a host shows: the three tasks' full-path PowerShell 7 command, folder,
#   account, limits and instances rule; Windows Update's policy values; DefaultShell's rule. A text check
#   shows that the rule is written, never that it works, and the live check (E0.12-T05-S02) shows it work.
#
# The script's folder is -ScriptDir (default: box/windows beside this folder).
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$resolvedDir = Resolve-WtScriptDir $ScriptDir
$testHome = New-WtHome
$winPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

function Read-WtScriptText { param([string]$Name) return Read-WtText -Path (Join-Path $resolvedDir $Name) }

# The script's text without its comment lines, so that a rule stated only in a comment shows as missing.
function Read-WtScriptCode {
  param([string]$Name)
  $lines = (Read-WtScriptText $Name) -split "`n" | Where-Object { -not $_.TrimStart().StartsWith('#') }
  return ($lines -join "`n")
}

# Parses a script with the parser of the PowerShell given and returns its error messages, one a line.
function Get-WtParseErrors {
  param([Parameter(Mandatory)][string]$Shell, [Parameter(Mandatory)][string]$Path)
  # The path goes into the snippet itself, since a -Command run takes no $args.
  $quoted = "'" + $Path.Replace("'", "''") + "'"
  $snippet = '$e = $null; $t = $null; [void][System.Management.Automation.Language.Parser]::ParseFile(' + $quoted + ', [ref]$t, [ref]$e); foreach ($x in $e) { "line " + $x.Extent.StartLineNumber + ": " + $x.Message }; exit 0'
  return Invoke-WtScriptBlock -Shell $Shell -Snippet $snippet
}

# Runs a -Command snippet in a shell, in the throwaway home's environment.
function Invoke-WtScriptBlock {
  param([Parameter(Mandatory)][string]$Shell, [Parameter(Mandatory)][string]$Snippet, [string[]]$Arguments = @())
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $testHome -Override @{}
  $argList = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', $Snippet) + $Arguments
  return Start-ConformanceProcess -FilePath $Shell -Arguments $argList -ChildEnvironment $childEnv -TimeoutSeconds 120 -WorkingDirectory $testHome.UserProfile
}

try {
  $realBefore = Get-WtRealMark

  # --- The file set -----------------------------------------------------------------------------------------------
  Test-WtCase 'Files: box/windows holds exactly the files operator.md lists and the tests folder' {
    $want = @('agentspace-boot.ps1', 'agentspace-idle-check.ps1', 'agentspace-patch-check.ps1', 'agentspace-patch.ps1', 'agentspace-tools.ps1', 'bootstrap.ps1', 'idle.conf', 'install.ps1', 'localswim', 'localswim.ps1', 'sshd_config.agentspace')
    $got = @(Get-ChildItem -LiteralPath $resolvedDir -File -Force -Name | Sort-Object)
    $wantSorted = @($want | Sort-Object)
    Assert-WtEqual ($got -join ',') ($wantSorted -join ',') 'The files of box/windows'
  }
  Test-WtCase 'Files: box/windows holds no first-boot file (pin P15 withdrew it)' {
    $hits = @(Get-ChildItem -LiteralPath $resolvedDir -Recurse -Force -Name | Where-Object { $_ -match '(?i)first[-_ ]?boot|userdata|user-data' })
    Assert-Wt ($hits.Count -eq 0) ("A first-boot file is not part of box/windows: {0}" -f ($hits -join ', '))
  }

  # --- idle.conf -----------------------------------------------------------------------------------------------------
  Test-WtCase 'Idle.conf: it sets IDLE_MINUTES=60, BUSY_CPU_SECONDS=6 and HORIZON_GRACE_MINUTES=15, and nothing else' {
    $lines = @((Read-WtScriptText 'idle.conf') -split "`n" | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_.Trim() -ne '' -and -not $_.TrimStart().StartsWith('#') })
    Assert-WtEqual (($lines | Sort-Object) -join ',') (@('BUSY_CPU_SECONDS=6', 'HORIZON_GRACE_MINUTES=15', 'IDLE_MINUTES=60') -join ',') 'idle.conf settings'
  }

  # --- localswim, the Git Bash shim ------------------------------------------------------------------------------------
  Test-WtCase 'Localswim: The shim holds LF line endings alone' {
    $bytes = Read-WtBytes -Path (Join-Path $resolvedDir 'localswim')
    Assert-Wt ($bytes.Length -gt 0) 'localswim MUST NOT be empty'
    Assert-Wt (-not ($bytes -contains [byte]13)) 'localswim MUST hold no CR (bash needs LF alone)'
  }
  Test-WtCase 'Localswim: The shim sets MSYS2_ARG_CONV_EXCL to * so Git Bash converts no argument' {
    $t = Read-WtScriptText 'localswim'
    Assert-Wt ($t -match "(?m)^\s*(export\s+)?MSYS2_ARG_CONV_EXCL=(['""]?)\*\2\s*$") 'The shim MUST set MSYS2_ARG_CONV_EXCL to *'
  }
  Test-WtCase 'Localswim: The shim runs PowerShell 7 by its full path with -NoProfile -NonInteractive -File and the wrapper, never pwsh by name' {
    $t = Read-WtScriptText 'localswim'
    Assert-Wt ($t -match '(?i)C:[\\/]+Program Files[\\/]+PowerShell[\\/]+7[\\/]+pwsh\.exe') 'The shim MUST name C:\Program Files\PowerShell\7\pwsh.exe'
    Assert-Wt ($t -match '-NoProfile\s+-NonInteractive\s+-File\s+C:/ProgramData/agentspace/bin/localswim\.ps1') 'The shim MUST pass -NoProfile -NonInteractive -File C:/ProgramData/agentspace/bin/localswim.ps1'
    Assert-Wt ($t -match '"\$@"') 'The shim MUST pass its own arguments as given ("$@")'
    $bare = @(($t -split "`n") | Where-Object { $_ -match '^\s*(exec\s+)?pwsh(\.exe)?\s' })
    Assert-Wt ($bare.Count -eq 0) 'The shim MUST NOT run pwsh by name'
  }
  Test-WtCase 'Localswim: The shim exits with the status of the wrapper (exec, or exit $?)' {
    $t = Read-WtScriptText 'localswim'
    Assert-Wt ($t -match '(?m)^\s*exec\s+.*localswim\.ps1' -or $t -match '(?m)exit\s+\$\?' -or $t -match '(?m)exit\s+\$rc') 'The shim MUST exit with the wrapper''s status'
  }

  # --- Windows PowerShell 5.1 -------------------------------------------------------------------------------------------
  Assert-Wt (Test-Path -LiteralPath $winPowerShell -PathType Leaf) 'Windows PowerShell 5.1 is not on this laptop'
  foreach ($name in @('bootstrap.ps1', 'localswim.ps1', 'install.ps1')) {
    $nameCopy = $name
    Test-WtCase "Windows PowerShell 5.1: $nameCopy parses under it with no error" {
      $p = Join-Path $resolvedDir $nameCopy
      Assert-Wt (Test-Path -LiteralPath $p -PathType Leaf) "$nameCopy is missing"
      $r = Get-WtParseErrors -Shell $winPowerShell -Path $p
      Assert-WtEqual $r.ExitCode 0 'Exit code of the parse'
      Assert-Wt ($r.Stdout.Trim() -eq '') ("Windows PowerShell 5.1 parse errors: {0}" -f $r.Stdout.Trim())
    }
    Test-WtCase "Windows PowerShell 5.1: $nameCopy carries no #Requires for PowerShell 6 or 7" {
      $t = Read-WtScriptText $nameCopy
      Assert-Wt ($t -notmatch '(?im)^\s*#requires\s+-version\s+[67]') "$nameCopy MUST run under Windows PowerShell 5.1"
    }
    Test-WtCase "Windows PowerShell 5.1: $nameCopy uses no PowerShell 7 operator or parameter" {
      $t = Read-WtScriptText $nameCopy
      $code = (($t -split "`n") | Where-Object { -not $_.TrimStart().StartsWith('#') }) -join "`n"
      Assert-Wt ($code -notmatch '\?\?') 'The null-coalescing operator is PowerShell 7'
      Assert-Wt ($code -notmatch '-Parallel\b') 'ForEach-Object -Parallel is PowerShell 7'
      Assert-Wt ($code -notmatch '-AsArray\b|-SchemaFile\b|-ErrorAction\s+Break') 'A PowerShell 7 parameter'
    }
  }
  foreach ($name in @('agentspace-boot.ps1', 'agentspace-patch.ps1', 'agentspace-patch-check.ps1', 'agentspace-idle-check.ps1', 'agentspace-tools.ps1', 'bootstrap.ps1', 'install.ps1', 'localswim.ps1')) {
    $nameCopy = $name
    Test-WtCase "Parse: $nameCopy parses under PowerShell 7 with no error" {
      $p = Join-Path $resolvedDir $nameCopy
      Assert-Wt (Test-Path -LiteralPath $p -PathType Leaf) "$nameCopy is missing"
      $r = Get-WtParseErrors -Shell $script:WtPwsh -Path $p
      Assert-Wt ($r.Stdout.Trim() -eq '') ("PowerShell 7 parse errors: {0}" -f $r.Stdout.Trim())
    }
  }

  # --- localswim.ps1's first refusal ---------------------------------------------------------------------------------------
  $cliPath = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'agentspace\localswim\localswim.exe'
  foreach ($shellLabel in @('PowerShell 7', 'Windows PowerShell 5.1')) {
    $labelCopy = $shellLabel
    Test-WtCase "Localswim: Under $labelCopy, with the CLI not installed, it refuses with its message and exit 1" {
      if (Test-Path -LiteralPath $cliPath) { throw 'The machine has the localswim CLI installed at its fixed path, so rule 1 cannot be shown here' }
      # A credential file is there, so the refusal shown is the CLI's and not the credentials'.
      Write-WtText -TestHome $testHome -Path (Join-Path $testHome.UserProfile '.config\localswim\bot.env') -Text "LOCALSWIM_URL=http://127.0.0.1:1`nLOCALSWIM_TOKEN=test-only`n"
      $shell = if ($labelCopy -eq 'PowerShell 7') { $script:WtPwsh } else { $winPowerShell }
      $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $testHome -Override @{}
      $r = Start-ConformanceProcess -FilePath $shell -Arguments @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $resolvedDir 'localswim.ps1'), 'card', 'get', '1') -ChildEnvironment $childEnv -TimeoutSeconds 60 -WorkingDirectory $testHome.UserProfile
      Assert-WtEqual $r.ExitCode 1 'Exit code'
      Assert-Wt ($r.Stderr.Contains('localswim: The CLI is not installed; run agentspace-operator sync for this host on the laptop.')) ("stderr MUST hold the refusal. {0}" -f (Format-WtRun $r))
      Assert-WtEqual $r.Stdout.Trim() '' 'stdout'
    }
  }

  # --- What only a host shows: the rule is written ---------------------------------------------------------------------------
  Test-WtCase 'Tasks (text): Each task names PowerShell 7 by its full path, never pwsh by name' {
    $t = Read-WtScriptCode 'install.ps1'
    Assert-Wt ($t -match '(?i)C:\\Program Files\\PowerShell\\7\\pwsh\.exe') 'install.ps1 MUST name C:\Program Files\PowerShell\7\pwsh.exe'
    Assert-Wt ($t -notmatch "(?i)-Execute\s+['""]?pwsh(\.exe)?['""]?(\s|\))") 'A task action MUST NOT run pwsh by name'
  }
  Test-WtCase 'Tasks (text): The three tasks are in the \agentspace\ folder, as SYSTEM, highest privileges' {
    $t = Read-WtScriptCode 'install.ps1'
    foreach ($n in @('agentspace-boot', 'agentspace-patch', 'agentspace-idle')) { Assert-Wt ($t.Contains($n)) "install.ps1 MUST register the task $n" }
    Assert-Wt ($t -match '(?i)\\agentspace\\') 'The tasks MUST be in Task Scheduler''s folder \agentspace\'
    Assert-Wt ($t -match '(?i)SYSTEM') 'The tasks MUST run as SYSTEM'
    Assert-Wt ($t -match '(?i)Highest') 'The tasks MUST run with the highest privileges'
  }
  Test-WtCase 'Tasks (text): The tasks start no second instance while one runs, with limits of 10, 20 and 4 minutes' {
    $t = Read-WtScriptCode 'install.ps1'
    Assert-Wt ($t -match '(?i)IgnoreNew') 'The tasks MUST NOT start a second instance (MultipleInstances IgnoreNew)'
    Assert-Wt ($t -match '(?i)ExecutionTimeLimit') 'The tasks MUST carry a time limit (ExecutionTimeLimit)'
    # Each task's registration, within 600 characters of its name, holds its limit in minutes.
    foreach ($pair in @(@('agentspace-boot', '10'), @('agentspace-patch', '20'), @('agentspace-idle', '4'))) {
      $i = $t.IndexOf("'" + $pair[0] + "'")
      Assert-Wt ($i -ge 0) ("install.ps1 MUST register the task '{0}'" -f $pair[0])
      $window = $t.Substring($i, [Math]::Min(600, $t.Length - $i))
      Assert-Wt ($window -match ('(?i)\b' + $pair[1] + '\b')) ("The limit of {0} is {1} minutes" -f $pair[0], $pair[1])
    }
  }
  Test-WtCase 'Policy (text): The install sets Windows Update''s NoAutoUpdate and NoAutoRebootWithLoggedOnUsers to 1 under the AU policy key' {
    $t = Read-WtScriptCode 'install.ps1'
    Assert-Wt ($t -match 'SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU') 'The AU policy key'
    Assert-Wt ($t.Contains('NoAutoUpdate') -and $t.Contains('NoAutoRebootWithLoggedOnUsers')) 'Both policy values'
  }
  Test-WtCase 'Policy (text): The bootstrap sets the same Windows Update policy' {
    $t = Read-WtScriptCode 'bootstrap.ps1'
    Assert-Wt ($t -match 'SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU') 'The AU policy key'
    Assert-Wt ($t.Contains('NoAutoUpdate') -and $t.Contains('NoAutoRebootWithLoggedOnUsers')) 'Both policy values'
  }
  Test-WtCase 'DefaultShell (text): The install and the bootstrap write DefaultShell under HKLM:\SOFTWARE\OpenSSH, and no DefaultShellCommandOption' {
    foreach ($n in @('install.ps1', 'bootstrap.ps1')) {
      $t = Read-WtScriptCode $n
      Assert-Wt ($t -match '[''"]DefaultShell[''"]' -and $t -match 'SOFTWARE\\OpenSSH') "$n MUST write DefaultShell under HKLM:\SOFTWARE\OpenSSH"
      Assert-Wt ($t -notmatch 'DefaultShellCommandOption') "$n MUST NOT write DefaultShellCommandOption"
      Assert-Wt ($t -match '(?i)C:\\Program Files\\Git\\bin\\bash\.exe') "$n MUST name Git Bash's launcher"
      Assert-Wt ($t -match '(?i)C:\\Windows\\System32\\WindowsPowerShell\\v1\.0\\powershell\.exe') "$n MUST name Windows PowerShell 5.1 as the fallback"
    }
  }
  Test-WtCase 'Tools (text): agentspace-tools.ps1 does not update PowerShell 7' {
    $t = Read-WtScriptCode 'agentspace-tools.ps1'
    Assert-Wt ($t -notmatch '(?i)winget\s+upgrade[^\n]*Microsoft\.PowerShell') 'The tools script MUST NOT winget-upgrade PowerShell 7'
  }
  Test-WtCase 'Patch (text): The patch run searches with the criteria IsInstalled=0 and IsHidden=0 and Type=''Software''' {
    $t = Read-WtScriptCode 'agentspace-patch.ps1'
    Assert-Wt ($t.Contains("IsInstalled=0 and IsHidden=0 and Type='Software'")) 'The search criteria'
    Assert-Wt ($t.Contains('Microsoft.Update.Session')) 'The Windows Update Agent API'
  }
  Test-WtCase 'Patch (text): The patch run restarts with shutdown.exe /r /t 0' {
    $t = Read-WtScriptCode 'agentspace-patch.ps1'
    Assert-Wt ($t -match '(?i)\bshutdown(\.exe)?[''"]?\s+/r\s+/t\s+0') 'shutdown.exe /r /t 0'
  }
  Test-WtCase 'Idle (text): The idle check powers off with shutdown.exe /s /t 0' {
    $t = Read-WtScriptCode 'agentspace-idle-check.ps1'
    Assert-Wt ($t -match '(?i)\bshutdown(\.exe)?[''"]?\s+/s\s+/t\s+0') 'shutdown.exe /s /t 0'
  }

  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun
