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
no time limit, a second start ignored. The action is a no-window launcher, built from
scripts\BoardHolderLauncher.cs by csc at install time into %USERPROFILE%\.agentspace (never into the
repo), that runs `pwsh -NoProfile -NonInteractive -File <repo>\scripts\board-holder.ps1 -Watch`.

-DryRun prints one `Label: value` line per field of the definition, from the same function the
install uses. -Uninstall stops and unregisters the task and deletes the launcher it built.
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

# The one definition of the task: the install and -DryRun both read it, so the dry run prints what
# would really be registered. Builds and registers nothing.
function Get-BoardHolderTaskDefinition {
    $repo = Split-Path -Parent $PSScriptRoot
    $stateDir = Join-Path $env:USERPROFILE '.agentspace'
    $pwsh = (Get-Command pwsh).Source
    $holderScript = Join-Path $repo 'scripts\board-holder.ps1'
    [pscustomobject]@{
        Name               = $script:TaskName
        User               = "$env:USERDOMAIN\$env:USERNAME"
        RunLevel           = 'Limited'
        MultipleInstances  = 'IgnoreNew'
        StartWhenAvailable = $true
        ExecutionTimeLimit = [TimeSpan]::Zero
        LauncherSource     = Join-Path $repo 'scripts\BoardHolderLauncher.cs'
        Launcher           = Join-Path $stateDir 'BoardHolderLauncher.exe'
        Arguments          = "`"$pwsh`" `"$holderScript`" -Watch"
        WorkingDirectory   = $repo
        Runs               = "pwsh -NoProfile -NonInteractive -File $holderScript -Watch"
        Window             = 'hidden'
        Description        = 'Keeps the agentspace board holder running while a lease is ready (D30).'
    }
}

$def = Get-BoardHolderTaskDefinition

if ($DryRun) {
    Write-Output "Name: $($def.Name)"
    Write-Output "Trigger: AtLogOn user=$($def.User)"
    Write-Output "RunLevel: $($def.RunLevel)"
    Write-Output 'ExecutionTimeLimit: none'
    Write-Output "MultipleInstances: $($def.MultipleInstances)"
    Write-Output "StartWhenAvailable: $($def.StartWhenAvailable.ToString().ToLowerInvariant())"
    Write-Output "Action: $($def.Launcher)"
    Write-Output "Arguments: $($def.Arguments)"
    Write-Output "Runs: $($def.Runs)"
    Write-Output "Window: $($def.Window)"
    Write-Output "LauncherSource: $($def.LauncherSource)"
    Write-Output 'Mode: dry run; nothing was built or registered.'
    return
}

if ($Uninstall) {
    if (Get-ScheduledTask -TaskName $def.Name -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $def.Name -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $def.Name -Confirm:$false
        Write-Output "Unregistered '$($def.Name)'."
    } else {
        Write-Output "No task '$($def.Name)' to remove."
    }
    if (Test-Path -LiteralPath $def.Launcher) {
        Remove-Item -LiteralPath $def.Launcher -Force
        Write-Output "Removed $($def.Launcher)."
    }
    return
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $def.Launcher) | Out-Null
& $script:Csc /nologo /target:winexe "/out:$($def.Launcher)" $def.LauncherSource
if ($LASTEXITCODE -ne 0) { throw "csc exited $LASTEXITCODE." }

$action = New-ScheduledTaskAction -Execute $def.Launcher -Argument $def.Arguments -WorkingDirectory $def.WorkingDirectory
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $def.User
$principal = New-ScheduledTaskPrincipal -UserId $def.User -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit $def.ExecutionTimeLimit -MultipleInstances $def.MultipleInstances -StartWhenAvailable
Register-ScheduledTask -TaskName $def.Name -Action $action -Trigger $trigger -Principal $principal `
    -Settings $settings -Description $def.Description -Force | Out-Null
Start-ScheduledTask -TaskName $def.Name
Write-Output "Registered and started '$($def.Name)'. Log: $(Join-Path $env:USERPROFILE '.agentspace\board-holder.log')"
