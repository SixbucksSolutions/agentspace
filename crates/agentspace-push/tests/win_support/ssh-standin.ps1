# The stand-in ssh of the push's Windows tests (docs/operator.md, The test seam). A .cmd starts it
# with Windows PowerShell 5.1, so that it sees the arguments as a program would. It records every
# call beside itself, acts as the box does (What the box does) as the scenario file says, and
# touches nothing outside its own folder.
$ErrorActionPreference = 'Stop'
$dir = $PSScriptRoot
function Read-Setting([string]$Name) {
  ([IO.File]::ReadAllText((Join-Path $dir $Name))).Trim()
}
$scenario = Read-Setting 'scenario.txt'
$zone = Read-Setting 'zone.txt'
$n = @(Get-ChildItem -LiteralPath $dir -Filter 'args-*.json').Count + 1

[IO.File]::WriteAllText((Join-Path $dir "args-$n.json"), (ConvertTo-Json -InputObject @($args) -Compress))
$envLines = foreach ($e in (Get-ChildItem Env:)) { $e.Name + '=' + $e.Value }
[IO.File]::WriteAllText((Join-Path $dir "env-$n.txt"), ($envLines -join "`n"))
[IO.File]::WriteAllText((Join-Path $dir "cwd-$n.txt"), (Get-Location).Path)
# Each process is recorded by its ID and its creation time, as a FILETIME, so that a clean-up ends it
# only while that ID is still the same process, never another one that the ID was given to later.
$created = [Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTimeUtc()
[IO.File]::AppendAllText((Join-Path $dir 'pids.txt'), "ssh $n $PID $created`n")

function Start-Sleeper([string]$Tag) {
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe')
  $psi.Arguments = '-NoProfile -NonInteractive -Command Start-Sleep -Seconds 90'
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $p = [Diagnostics.Process]::Start($psi)
  [IO.File]::AppendAllText((Join-Path $dir 'pids.txt'), "$Tag $n $($p.Id) $($p.StartTime.ToFileTimeUtc())`n")
}

function Write-Raw([string]$Text) {
  [Console]::Out.Write($Text)
  [Console]::Out.Flush()
}

# A scenario that never reads its standard input: the push's write must be cut off by its limit.
if ($scenario -eq 'noread') { Start-Sleep -Seconds 90; exit 0 }

$m = New-Object IO.MemoryStream
[Console]::OpenStandardInput().CopyTo($m)
$bytes = $m.ToArray()
[IO.File]::WriteAllBytes((Join-Path $dir "stdin-$n.bin"), $bytes)

if ($scenario -eq 'hang') { Start-Sleep -Seconds 90; exit 0 }
if ($scenario -eq 'hangtree') { Start-Sleeper 'child'; Start-Sleep -Seconds 90; exit 0 }
# Exits 0 at once, with a child that holds the stdout pipe open and prints nothing.
if ($scenario -eq 'holdstdout') { Start-Sleeper 'holder'; exit 0 }
if ($scenario -eq 'exit255') { exit 255 }
if ($scenario -eq 'slow') { Start-Sleep -Seconds 4 }

$text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
$lines = @($text -split "`n" | Where-Object { $_ -ne '' })
$norm = (($lines -join "`n") + "`n")
$sha = [Security.Cryptography.SHA256]::Create()
$hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
$zeros = '0' * 64

switch ($scenario) {
  'exit1hash' { Write-Raw ($hash + "  -`n"); exit 1 }
  'nohash' { Write-Raw "Welcome to the box`nNothing else`n"; exit 0 }
  'wronghash' { Write-Raw ($zeros + "  -`n"); if ($zone -ne '') { Write-Raw "zone $zone`n" }; exit 0 }
  'upperhash' { Write-Raw ($hash.ToUpperInvariant() + "  -`n"); if ($zone -ne '') { Write-Raw "zone $zone`n" }; exit 0 }
  'banner' {
    Write-Raw "Welcome to the box`r`n"
    Write-Raw ($hash.Substring(0, 63) + "  -`r`n")
    Write-Raw ($zeros + "x  -`r`n")
    Write-Raw ($hash + "  -`r`n")
    if ($zone -ne '') { Write-Raw "zone $zone`r`n" }
    exit 0
  }
  'nolf' { Write-Raw ($hash + '  -'); exit 0 }
  'nolfzone' { Write-Raw ($hash + "  -`n"); Write-Raw "zone $zone"; exit 0 }
  'zonefailed' { Write-Raw ($hash + "  -`n"); Write-Raw "zone-failed`n"; exit 0 }
  'nozone' { Write-Raw ($hash + "  -`n"); exit 0 }
  'zonecase' { Write-Raw ($hash + "  -`n"); Write-Raw ('zone ' + $zone.ToLowerInvariant() + "`n"); exit 0 }
  'zoneother' { Write-Raw ($hash + "  -`n"); Write-Raw "zone Etc/Other`n"; exit 0 }
  default {
    Write-Raw ($hash + "  -`n")
    if ($zone -ne '') { Write-Raw "zone $zone`n" }
    exit 0
  }
}
