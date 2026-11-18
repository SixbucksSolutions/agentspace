#Requires -Version 7
<#
.SYNOPSIS
Registers the logon task "agentspace board holder", which runs the board holder keeper
(scripts\board-holder.ps1 -Watch) unattended; -DryRun shows the task without registering it, and
-Uninstall removes it.

.DESCRIPTION
RFC 2119 keywords; the capitals are load-bearing.

Registering the task is persistent configuration on Terry's laptop, so it MUST only be run with his
yes (D30; story E0.02-T03-S01). -DryRun changes nothing and builds nothing.

After the watcher's install-autostart.ps1: a logon trigger for the current user, a second trigger
that repeats every minute indefinitely from registration (a crashed keeper is started again within
about a minute, and a firing while it runs is ignored), standard rights, an interactive logon, no
time limit, a second start ignored, a missed start run when available, and started and kept running
on battery. RestartCount 3 and RestartInterval a minute are also set, but they did not restart a
keeper whose action exited non-zero (E0.02-T03-S01); the repeating trigger is what does (D30). The
action is a no-window launcher, built from scripts\BoardHolderLauncher.cs by csc
at install time into %USERPROFILE%\.agentspace (never into the repo), that runs
`pwsh -NoProfile -NonInteractive -File <repo>\scripts\board-holder.ps1 -Watch` in <repo>, and exits
with pwsh's exit code. A real install starts the task as soon as it is registered.

New-BoardHolderTask builds the task object, and both the install and -DryRun use it: the install
registers that object with Register-ScheduledTask -InputObject, and -DryRun prints its fields, one
`Label: value` line each. So the dry run shows what would really be registered.

<repo> is the folder this script is in, one level up. The install MUST be run from the main
checkout: a git worktree is deleted after its branch merges, which would leave the task pointing at
nothing. Without -DryRun, an install from a worktree is refused with exit 2; -DryRun prints a
Warning: line in that case and still exits 0.

A real install is refused with exit 2 while a keeper runs: while the task is Running, or while a
pwsh runs <repo>\scripts\board-holder.ps1 -Watch. It compiles the launcher over the exe the running
task uses, which fails while the task runs, and stopping the task ends only the launcher, so the
registration would start a second keeper beside the old one (D30, card #0067). The install stops
and kills nothing itself; the refusal names the keeper's PID and how to stop it. The holder keeps
running, and the keeper the install starts adopts it.

-Uninstall stops and unregisters the task and deletes the launcher it built.
#>
[CmdletBinding()]
param(
    [switch] $DryRun,
    [switch] $Uninstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:TaskName = 'agentspace board holder'
$script:Csc = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$script:Repo = Split-Path -Parent $PSScriptRoot
$script:Launcher = Join-Path (Join-Path $env:USERPROFILE '.agentspace') 'BoardHolderLauncher.exe'
$script:LauncherSource = Join-Path $script:Repo 'scripts\BoardHolderLauncher.cs'
$script:HolderScript = Join-Path $script:Repo 'scripts\board-holder.ps1'

# The one definition of the task, as the task object Register-ScheduledTask takes. The New-*
# cmdlets only build objects on this machine: this registers, builds and starts nothing.
function New-BoardHolderTask {
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $pwsh = (Get-Command pwsh).Source
    $action = New-ScheduledTaskAction -Execute $script:Launcher -Argument "`"$pwsh`" `"$($script:HolderScript)`" -Watch" `
        -WorkingDirectory $script:Repo
    $logon = New-ScheduledTaskTrigger -AtLogOn -User $user
    # The second trigger repeats every minute, indefinitely: with no duration set, Task Scheduler
    # repeats without end. With MultipleInstances IgnoreNew a firing while the keeper runs is
    # ignored, and one while it is not starts it again (D30).
    $repeat = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 1)
    $trigger = @($logon, $repeat)
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    New-ScheduledTask -Action $action -Trigger $trigger -Principal $principal -Settings $settings `
        -Description 'Keeps the agentspace board holder running while a lease is ready (D30).'
}

# The read-back of a task's triggers, one `Label: value` line each, for -DryRun and for the line
# printed after a real registration. A repetition with no duration repeats indefinitely.
function Get-TriggerLines($Triggers) {
    $n = 0
    foreach ($t in @($Triggers)) {
        $n++
        $name = $t.CimClass.CimClassName
        if ($name -ceq 'MSFT_TaskLogonTrigger') {
            "Trigger ${n}: AtLogOn user=$($t.UserId)"
        } elseif ($name -ceq 'MSFT_TaskTimeTrigger') {
            $end = if ($t.EndBoundary) { $t.EndBoundary } else { 'none' }
            $line = "Trigger ${n}: Once StartBoundary=$($t.StartBoundary) EndBoundary=$end"
            if ($null -ne $t.Repetition -and $t.Repetition.PSObject.Properties['StopAtDurationEnd']) {
                $line += " StopAtDurationEnd=$($t.Repetition.StopAtDurationEnd)"
            }
            $line
        } else {
            "Trigger ${n}: $name"
        }
        $r = $t.Repetition
        if ($r -and $r.Interval) {
            $duration = if ($r.Duration) { $r.Duration } else { 'indefinite' }
            "Repetition ${n}: Interval=$($r.Interval) Duration=$duration"
        }
    }
}

# True when the repo folder is a git worktree rather than the main checkout: its git dir differs
# from its common git dir, or its path is under .claude\worktrees.
function Test-RepoIsWorktree([string]$Repo) {
    if ($Repo -match '[\\/]\.claude[\\/]worktrees[\\/]') { return $true }
    $resolve = {
        param([string]$Path)
        if (-not $Path) { return $null }
        if (-not [IO.Path]::IsPathRooted($Path)) { $Path = Join-Path $Repo $Path }
        [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    }
    try {
        $gitDir = & $resolve ((& git -C $Repo rev-parse --git-dir 2>$null) | Select-Object -First 1)
        $common = & $resolve ((& git -C $Repo rev-parse --git-common-dir 2>$null) | Select-Object -First 1)
    } catch { return $false }
    [bool]($gitDir -and $common -and -not [string]::Equals($gitDir, $common, [StringComparison]::OrdinalIgnoreCase))
}

if ($Uninstall) {
    if (Get-ScheduledTask -TaskName $script:TaskName -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $script:TaskName -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $script:TaskName -Confirm:$false
        Write-Output "Unregistered '$($script:TaskName)'."
    } else {
        Write-Output "No task '$($script:TaskName)' to remove."
    }
    if (Test-Path -LiteralPath $script:Launcher) {
        Remove-Item -LiteralPath $script:Launcher -Force
        Write-Output "Removed $($script:Launcher)."
    }
    return
}

$task = New-BoardHolderTask
$isWorktree = Test-RepoIsWorktree $script:Repo

if ($DryRun) {
    $s = $task.Settings
    $a = @($task.Actions)[0]
    $limit = if ($s.ExecutionTimeLimit -ceq 'PT0S') { 'PT0S (none)' } else { $s.ExecutionTimeLimit }
    if ($isWorktree) {
        Write-Output "Warning: $($script:Repo) is a git worktree, not the main checkout. A real install from here is refused, because the folder is deleted after its branch merges."
    }
    Write-Output "Name: $($script:TaskName)"
    Write-Output "Description: $($task.Description)"
    Get-TriggerLines $task.Triggers | Write-Output
    Write-Output "Principal user: $($task.Principal.UserId)"
    Write-Output "RunLevel: $($task.Principal.RunLevel)"
    Write-Output "LogonType: $($task.Principal.LogonType)"
    Write-Output "ExecutionTimeLimit: $limit"
    Write-Output "MultipleInstances: $($s.MultipleInstances)"
    Write-Output "StartWhenAvailable: $($s.StartWhenAvailable)"
    Write-Output "DisallowStartIfOnBatteries: $($s.DisallowStartIfOnBatteries)"
    Write-Output "StopIfGoingOnBatteries: $($s.StopIfGoingOnBatteries)"
    Write-Output "RestartCount: $($s.RestartCount)"
    Write-Output "RestartInterval: $($s.RestartInterval)"
    Write-Output "Action Execute: $($a.Execute)"
    Write-Output "Arguments: $($a.Arguments)"
    Write-Output "WorkingDirectory: $($a.WorkingDirectory)"
    Write-Output "Runs: $((Get-Command pwsh).Source) -NoProfile -NonInteractive -File $($script:HolderScript) -Watch"
    Write-Output 'Window: hidden'
    Write-Output "LauncherSource: $($script:LauncherSource)"
    Write-Output 'Start: a real install starts the task immediately after registering it.'
    Write-Output 'Mode: dry run; nothing was built, registered or started.'
    return
}

if ($isWorktree) {
    [Console]::Error.WriteLine("install-board-holder.ps1: $($script:Repo) is a git worktree, not the main checkout. The task MUST point at the main checkout, because a worktree is deleted after its branch merges. Run this from the main checkout.")
    exit 2
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $script:Launcher) | Out-Null
# A real install MUST NOT run while a keeper does. The launcher exe cannot be overwritten while the
# task runs it, and stopping the task here would end only the launcher: the keeper's pwsh would keep
# running, and the registration below would start a second keeper beside it (D30, card #0067). So
# the install refuses, and names what to stop. It stops and kills nothing itself.
$existing = Get-ScheduledTask -TaskName $script:TaskName -ErrorAction SilentlyContinue
$keeperScript = Join-Path $script:Repo 'scripts\board-holder.ps1'
$keepers = @(Get-CimInstance Win32_Process -Filter "Name = 'pwsh.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains($keeperScript) -and $_.CommandLine -match '(?i)\s-Watch\b' })
if (($existing -and $existing.State -eq 'Running') -or $keepers.Count -gt 0) {
    $pids = if ($keepers.Count -gt 0) { ($keepers | ForEach-Object { $_.ProcessId }) -join ', ' } else { 'none found' }
    [Console]::Error.WriteLine("install-board-holder.ps1: A keeper is running (task state: $(if ($existing) { $existing.State } else { 'not registered' }); keeper pwsh PID: $pids). Stop it first: End the task in Task Scheduler (Stop-ScheduledTask), then stop each keeper pwsh by its PID after checking its command line runs $keeperScript -Watch. Its holder keeps running, and the keeper this install starts adopts it.")
    exit 2
}
& $script:Csc /nologo /target:winexe "/out:$($script:Launcher)" $script:LauncherSource
if ($LASTEXITCODE -ne 0) { throw "csc exited $LASTEXITCODE." }

Register-ScheduledTask -TaskName $script:TaskName -InputObject $task -Force | Out-Null
Start-ScheduledTask -TaskName $script:TaskName
Get-TriggerLines (Get-ScheduledTask -TaskName $script:TaskName).Triggers | Write-Output
Write-Output "Registered and started '$($script:TaskName)'. Log: $(Join-Path $env:USERPROFILE '.agentspace\board-holder.log')"
