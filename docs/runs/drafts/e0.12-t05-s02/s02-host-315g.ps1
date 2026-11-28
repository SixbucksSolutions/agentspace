# Host-side probe of measure (g) (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself): the System
# event log's restart events since this boot (IDs 1074, 6005, 6006, 6008 and 41) and the WindowsUpdateClient source's events, as counts and
# times only, never a message. Prints `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function U($T) { return ([DateTime]$T).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
$boot = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
P 'boot_utc' (U $boot)
P 'now_utc' (U (Get-Date))
$providers = @{ 1074 = 'User32'; 6005 = 'EventLog'; 6006 = 'EventLog'; 6008 = 'EventLog'; 41 = 'Microsoft-Windows-Kernel-Power' }
foreach ($id in 1074, 6005, 6006, 6008, 41) {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = $providers[$id]; Id = $id; StartTime = $boot } -ErrorAction SilentlyContinue)
    $times = ($events | Select-Object -First 5 | ForEach-Object { U $_.TimeCreated }) -join ';'
    P 'sysevent' ("$id|$($events.Count)|$times")
}
$all = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'User32'; Id = 1074 } -ErrorAction SilentlyContinue)
P 'sysevent_1074_all_time' $all.Count
$wu = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WindowsUpdateClient'; StartTime = $boot } -ErrorAction SilentlyContinue)
P 'wuclient_events' $wu.Count
foreach ($g in ($wu | Group-Object Id | Sort-Object { [int]$_.Name })) {
    $first = ($g.Group | Sort-Object TimeCreated | Select-Object -First 1).TimeCreated
    $last = ($g.Group | Sort-Object TimeCreated | Select-Object -Last 1).TimeCreated
    P 'wuclient' ("$($g.Name)|$($g.Count)|$(U $first)|$(U $last)")
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
