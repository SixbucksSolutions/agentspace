# Host-side probe of step 320 (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself): whether Claude
# Code's .credentials.json and gh's hosts.yml exist, and the accounts in each one's access list as icacls prints them (README, Signing in on
# a new Windows host, step 5). It never prints a file's content, only its size and the account names. Prints `S02 key=value` lines.
# RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
$files = [ordered]@{
    'claude' = (Join-Path $env:USERPROFILE '.claude\.credentials.json')
    'gh' = (Join-Path $env:APPDATA 'GitHub CLI\hosts.yml')
}
foreach ($k in $files.Keys) {
    $f = $files[$k]
    $exists = Test-Path -LiteralPath $f -PathType Leaf
    P 'exists' "$k|$exists"
    if (-not $exists) { continue }
    P 'size' ("$k|" + (Get-Item -LiteralPath $f -Force).Length)
    $out = @(& icacls.exe $f 2>&1 | ForEach-Object { [string]$_ })
    foreach ($line in $out) {
        $t = $line.Trim()
        if ($t.StartsWith($f, [StringComparison]::OrdinalIgnoreCase)) { $t = $t.Substring($f.Length).Trim() }
        $m = [regex]::Match($t, '^(?<acct>.+?):\((?<flags>[^:]+)$')
        if ($m.Success) { P 'acl' ("$k|" + $m.Groups['acct'].Value + '|' + $m.Groups['flags'].Value) }
    }
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
