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

After the watcher's install-autostart.ps1: a logon trigger for the current user, standard rights,
an interactive logon, no time limit, a second start ignored, a missed start run when available,
started and kept running on battery, and a process that exits other than 0 restarted up to 3 times,
a minute apart. The action is a no-window launcher, built from scripts\BoardHolderLauncher.cs by csc
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
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    New-ScheduledTask -Action $action -Trigger $trigger -Principal $principal -Settings $settings `
        -Description 'Keeps the agentspace board holder running while a lease is ready (D30).'
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
    $t = @($task.Triggers)[0]
    $a = @($task.Actions)[0]
    $kind = if ($t.CimClass.CimClassName -ceq 'MSFT_TaskLogonTrigger') { 'AtLogOn' } else { $t.CimClass.CimClassName }
    $limit = if ($s.ExecutionTimeLimit -ceq 'PT0S') { 'PT0S (none)' } else { $s.ExecutionTimeLimit }
    if ($isWorktree) {
        Write-Output "Warning: $($script:Repo) is a git worktree, not the main checkout. A real install from here is refused, because the folder is deleted after its branch merges."
    }
    Write-Output "Name: $($script:TaskName)"
    Write-Output "Description: $($task.Description)"
    Write-Output "Trigger: $kind user=$($t.UserId)"
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
& $script:Csc /nologo /target:winexe "/out:$($script:Launcher)" $script:LauncherSource
if ($LASTEXITCODE -ne 0) { throw "csc exited $LASTEXITCODE." }

Register-ScheduledTask -TaskName $script:TaskName -InputObject $task -Force | Out-Null
Start-ScheduledTask -TaskName $script:TaskName
Write-Output "Registered and started '$($script:TaskName)'. Log: $(Join-Path $env:USERPROFILE '.agentspace\board-holder.log')"
