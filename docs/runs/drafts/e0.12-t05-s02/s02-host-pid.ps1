# Host-side probe (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself): sshd's service
# state, its process ID and the process's start time, for measure (b). Prints `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
$svc = Get-CimInstance -ClassName Win32_Service -Filter "Name='sshd'" -ErrorAction SilentlyContinue
if ($null -eq $svc) { P 'sshd_state' 'missing' } else {
    P 'sshd_state' $svc.State
    P 'sshd_pid' $svc.ProcessId
    try { P 'sshd_start_utc' ((Get-Process -Id $svc.ProcessId -ErrorAction Stop).StartTime.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")) } catch { P 'sshd_start_utc' 'unknown' }
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
