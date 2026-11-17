#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The architect's watchdog on a dev lead (decision D22). It waits until -Minutes have passed since
# the dev lead's last status update, prints one line and exits; the architect runs it in the
# background and is told when it exits.
#
#   pwsh -NoProfile -File scripts\watchdog.ps1 -Lead mvp-t2            # wait for the timer to fire
#   pwsh -NoProfile -File scripts\watchdog.ps1 -Lead mvp-t2 -Reset     # a status update arrived
#
# The timer is the modification time of a file under %USERPROFILE%\.agentspace\watchdog (D21), so a
# reset is one -Reset run and the waiting copy picks it up without a restart.
param(
  [Parameter(Mandatory)][string]$Lead,
  [int]$Minutes = 10,
  [switch]$Reset
)

$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:USERPROFILE '.agentspace\watchdog'
$file = Join-Path $dir "$Lead.last-status"
New-Item -ItemType Directory -Force $dir | Out-Null

if ($Reset -or -not (Test-Path $file)) {
  Set-Content -NoNewline -Path $file -Value ([DateTime]::UtcNow.ToString('o'))
  if ($Reset) { Write-Output "Watchdog for ${Lead}: reset, fires after $Minutes minutes of silence."; exit 0 }
}

while ($true) {
  $since = [DateTime]::UtcNow - (Get-Item $file).LastWriteTimeUtc
  if ($since.TotalMinutes -ge $Minutes) {
    Write-Output ("Watchdog for {0} fired: no status update for {1:N0} minutes." -f $Lead, $since.TotalMinutes)
    exit 0
  }
  Start-Sleep -Seconds 20
}
