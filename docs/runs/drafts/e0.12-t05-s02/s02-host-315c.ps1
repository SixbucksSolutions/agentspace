# Host-side probe of measure (c) (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself): the facts that
# tell which login shell the provisioning run's token push ran under and whether it succeeded: bot.env's creation and write times and
# its line shapes (never a value), when Git Bash appeared, and the DefaultShell now. Prints `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function U($T) { return ([DateTime]$T).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
$envFile = Join-Path $env:USERPROFILE '.config\localswim\bot.env'
if (Test-Path -LiteralPath $envFile -PathType Leaf) {
    $item = Get-Item -LiteralPath $envFile -Force
    P 'botenv_created_utc' (U $item.CreationTimeUtc)
    P 'botenv_written_utc' (U $item.LastWriteTimeUtc)
    $lines = @([IO.File]::ReadAllText($envFile) -split "`n" | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ -ne '' })
    P 'botenv_lines' $lines.Count
    P 'botenv_url_line' (@($lines | Where-Object { $_ -cmatch '^LOCALSWIM_URL=.+' }).Count)
    P 'botenv_token_line' (@($lines | Where-Object { $_ -cmatch '^LOCALSWIM_TOKEN=.+' }).Count)
} else {
    P 'botenv_lines' 'missing'
}
$bash = 'C:\Program Files\Git\bin\bash.exe'
if (Test-Path -LiteralPath $bash -PathType Leaf) { P 'gitbash_created_utc' (U (Get-Item -LiteralPath $bash -Force).CreationTimeUtc) } else { P 'gitbash_created_utc' 'missing' }
P 'defaultshell' ([string](Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\OpenSSH' -Name 'DefaultShell' -ErrorAction SilentlyContinue).DefaultShell)
P 'timezone' ((Get-TimeZone).Id)
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
