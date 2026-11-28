# Host-side probe of step 314 (runs ON the Windows host under PowerShell 7, as agent, sent by scp and removed by itself).
# It prints `S02 key=value` lines and nothing else: sshd's DefaultShell, the first-boot log's lines, and the horizon file's
# bytes (a count, whether it holds 0x0D, and its form), never the horizon's digits. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }

$shell = $null
try { $shell = [string](Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\OpenSSH' -Name 'DefaultShell' -ErrorAction Stop).DefaultShell } catch { }
P 'defaultshell' $(if ($shell) { $shell } else { '<none>' })
P 'defaultshell_file_exists' $(if ($shell) { [bool](Test-Path -LiteralPath $shell -PathType Leaf) } else { $false })
P 'gitbash_expected' 'C:\Program Files\Git\bin\bash.exe'

$log = 'C:\ProgramData\agentspace\first-boot.log'
if (Test-Path -LiteralPath $log -PathType Leaf) {
    $lines = @([IO.File]::ReadAllLines($log) | Where-Object { $_ -ne '' })
    P 'firstboot_lines' $lines.Count
    foreach ($l in ($lines | Select-Object -First 60)) { P 'firstboot' $l }
} else {
    P 'firstboot_lines' 'missing'
}

$horizon = 'C:\ProgramData\agentspace\lease-horizon'
if (Test-Path -LiteralPath $horizon -PathType Leaf) {
    $b = [IO.File]::ReadAllBytes($horizon)
    $lf = [Array]::IndexOf($b, [byte]10)
    $firstLength = if ($lf -lt 0) { $b.Length } else { $lf }
    $digits = ($firstLength -ge 1 -and $firstLength -le 15)
    for ($i = 0; $digits -and $i -lt $firstLength; $i++) { if ($b[$i] -lt 48 -or $b[$i] -gt 57) { $digits = $false } }
    $form = 'other'
    if ($digits -and $lf -lt 0) { $form = 'digits-alone' }
    elseif ($digits -and $lf -eq $b.Length - 1) { $form = 'digits-then-one-LF' }
    P 'horizon_bytes' $b.Length
    P 'horizon_has_cr' ($b -contains [byte]13)
    P 'horizon_form' $form
    P 'horizon_written_utc' ((Get-Item -LiteralPath $horizon -Force).LastWriteTimeUtc.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'"))
} else {
    P 'horizon_bytes' 'missing'
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
