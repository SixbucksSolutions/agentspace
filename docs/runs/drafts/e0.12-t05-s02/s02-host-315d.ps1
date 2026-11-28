# Host-side probe of measure (d) (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself): whether
# winget answers in a key-based ssh session, and what the host holds of winget's own logs and of each tool, so the bootstrap's choice
# between The tools' two columns can be read. Prints `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function U([DateTime]$T) { return $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }

$cmd = Get-Command -Name 'winget' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
P 'winget_on_path' ($null -ne $cmd)
$code = 'not-run'
$first = ''
try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = 'winget'
    $psi.Arguments = '--version'
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $err = $p.StandardError.ReadToEndAsync()
    $out = $p.StandardOutput.ReadToEnd()
    if (-not $p.WaitForExit(60000)) { try { $p.Kill() } catch { }; $code = 'timeout' } else { $code = [string]$p.ExitCode }
    $first = @($out -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' } | Select-Object -First 1)[0]
} catch {
    $code = 'error:' + $_.Exception.GetType().Name
}
P 'winget_version_exit' $code
P 'winget_version' $first

$logDir = Join-Path $env:LOCALAPPDATA 'Packages\Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\LocalState\DiagOutputDir'
if (Test-Path -LiteralPath $logDir -PathType Container) {
    $files = @(Get-ChildItem -LiteralPath $logDir -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc)
    P 'winget_log_files' $files.Count
    if ($files.Count -gt 0) { P 'winget_log_first_utc' (U $files[0].LastWriteTimeUtc); P 'winget_log_last_utc' (U $files[-1].LastWriteTimeUtc) }
    foreach ($id in 'Microsoft.PowerShell', 'Git.Git', 'OpenJS.NodeJS.LTS', 'astral-sh.uv', 'GitHub.cli') {
        $n = 0
        foreach ($f in $files) { if (Select-String -LiteralPath $f.FullName -Pattern $id -SimpleMatch -Quiet -ErrorAction SilentlyContinue) { $n++ } }
        P 'winget_log_mentions' ("$id|$n")
    }
} else {
    P 'winget_log_files' 'no log folder'
}

$tools = [ordered]@{
    'PowerShell 7' = 'C:\Program Files\PowerShell\7\pwsh.exe'
    'Git for Windows' = 'C:\Program Files\Git\bin\bash.exe'
    'Node' = 'C:\Program Files\nodejs\node.exe'
    'uv' = (Join-Path $env:USERPROFILE '.local\bin\uv.exe')
    'Claude Code' = (Join-Path $env:USERPROFILE '.local\bin\claude.exe')
    'gh' = 'C:\Program Files\GitHub CLI\gh.exe'
}
foreach ($k in $tools.Keys) {
    $f = $tools[$k]
    if (Test-Path -LiteralPath $f -PathType Leaf) { P 'tool_file' ("$k|True|" + (U (Get-Item -LiteralPath $f -Force).CreationTimeUtc)) } else { P 'tool_file' "$k|False|" }
}
$cli = 'C:\ProgramData\agentspace\patch.log'
$noWinget = @()
if (Test-Path -LiteralPath $cli -PathType Leaf) {
    $w = @([IO.File]::ReadAllLines($cli) | Where-Object { $_ -match 'Tools: winget upgrade' })
    P 'patch_log_winget_upgrade_lines' $w.Count
    foreach ($l in ($w | Select-Object -First 10)) { P 'patch_log_winget_upgrade' $l }
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
