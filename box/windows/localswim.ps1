# localswim.ps1: the board's wrapper on a Windows host (D12, D96; N13), installed in
# C:\ProgramData\agentspace\bin, which is on the machine PATH (docs/operator.md, Windows hosts, The
# board's wrapper). It runs alike under Windows PowerShell 5.1 and PowerShell 7, since it runs in
# whichever PowerShell an agent calls it from, so it is written in Windows PowerShell 5.1's syntax.
# Each run:
#   1. Refuses when the localswim CLI is not installed.
#   2. Refuses when the running user's .config\localswim\bot.env is missing or cannot be read.
#   3. Reads its LOCALSWIM_URL= and LOCALSWIM_TOKEN= lines as data, never running any part of it.
#   4. Refuses when the board does not answer one TCP connection within 3 seconds.
#   5. Sets the two values in its own process's environment, for the one run of the CLI, runs the
#      CLI with every argument exactly as given, and exits with its exit code. It never prints
#      either value.

$ErrorActionPreference = 'Stop'
$cliPath = 'C:\ProgramData\agentspace\localswim\localswim.exe'

function Write-AgentspaceError([string]$Text) {
    [Console]::Error.WriteLine($Text)
}

# Quotes one argument for a Windows command line, as the C runtime reads it back.
function ConvertTo-AgentspaceArgument([string]$Text) {
    if ($Text.Length -gt 0 -and $Text -notmatch '[\s"]') { return $Text }
    $built = New-Object System.Text.StringBuilder
    [void]$built.Append('"')
    $slashes = 0
    foreach ($ch in $Text.ToCharArray()) {
        if ($ch -eq [char]92) {
            $slashes++
            continue
        }
        if ($ch -eq [char]34) {
            [void]$built.Append([char]92, (2 * $slashes + 1))
        } elseif ($slashes -gt 0) {
            [void]$built.Append([char]92, $slashes)
        }
        [void]$built.Append($ch)
        $slashes = 0
    }
    if ($slashes -gt 0) { [void]$built.Append([char]92, (2 * $slashes)) }
    [void]$built.Append('"')
    return $built.ToString()
}

# 1. The CLI.
if (-not (Test-Path -LiteralPath $cliPath -PathType Leaf)) {
    Write-AgentspaceError 'localswim: The CLI is not installed; run agentspace-operator sync for this host on the laptop.'
    exit 1
}

# 2. The Bot credentials, which the token push writes into the running user's profile.
$noCredentials = 'localswim: No Bot credentials yet; they arrive when the laptop connects to this host.'
$profileDir = [Environment]::GetFolderPath('UserProfile')
$envFile = Join-Path $profileDir '.config\localswim\bot.env'
$envText = $null
try {
    if (Test-Path -LiteralPath $envFile -PathType Leaf) { $envText = [IO.File]::ReadAllText($envFile) }
} catch {
    $envText = $null
}
if ($null -eq $envText) {
    Write-AgentspaceError $noCredentials
    exit 1
}

# 3. Its two lines, as data: the value is what follows the first =, with a CR at its end removed.
$boardUrl = $null
$boardToken = $null
foreach ($envLine in ($envText -split "`n")) {
    $line = $envLine
    if ($line.EndsWith("`r")) { $line = $line.Substring(0, $line.Length - 1) }
    if ($line.StartsWith('LOCALSWIM_URL=', [StringComparison]::Ordinal)) { $boardUrl = $line.Substring($line.IndexOf('=') + 1) }
    elseif ($line.StartsWith('LOCALSWIM_TOKEN=', [StringComparison]::Ordinal)) { $boardToken = $line.Substring($line.IndexOf('=') + 1) }
}
if ([string]::IsNullOrEmpty($boardUrl) -or [string]::IsNullOrEmpty($boardToken)) {
    Write-AgentspaceError $noCredentials
    exit 1
}

# 4. One TCP connection to the board's address and port, given up after 3 seconds.
$reachable = $false
$client = $null
try {
    $uri = New-Object System.Uri($boardUrl)
    $client = New-Object System.Net.Sockets.TcpClient
    $connect = $client.ConnectAsync($uri.Host, $uri.Port)
    if ($connect.Wait(3000) -and $client.Connected) { $reachable = $true }
} catch {
    $reachable = $false
} finally {
    if ($null -ne $client) { $client.Close() }
}
if (-not $reachable) {
    Write-AgentspaceError ('localswim: The board at ' + $boardUrl + ' is unreachable. It is forwarded from the laptop only while a connection holds it. Stop and report; do not work without a card.')
    exit 1
}

# 5. The CLI, with both values in this process's environment alone, and every argument as given.
[Environment]::SetEnvironmentVariable('LOCALSWIM_URL', $boardUrl, 'Process')
[Environment]::SetEnvironmentVariable('LOCALSWIM_TOKEN', $boardToken, 'Process')
$startInfo = New-Object System.Diagnostics.ProcessStartInfo
$startInfo.FileName = $cliPath
$startInfo.Arguments = (@($args | ForEach-Object { ConvertTo-AgentspaceArgument ([string]$_) }) -join ' ')
$startInfo.UseShellExecute = $false
$cli = [System.Diagnostics.Process]::Start($startInfo)
$cli.WaitForExit()
exit $cli.ExitCode
