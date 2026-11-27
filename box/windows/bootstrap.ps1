# bootstrap.ps1: the toolchain bootstrap of a Windows host (D46, D96, D97; N20), the counterpart of
# box/bootstrap.sh. agentspace-operator bootstrap copies it to <home>\.agentspace-bootstrap.ps1,
# runs it as agent under Windows PowerShell 5.1 with -File, and removes it (docs/operator.md,
# Windows hosts, bootstrap on a Windows host; The tools). It is written in Windows PowerShell 5.1's
# syntax, never asks for input, and is safe to run again: each tool is installed only when it is
# missing.
#
# agent-windows' tools (D97), each by winget where a package exists and winget answers in this
# session, and by the tool's own installer otherwise: PowerShell 7, Git for Windows with Git Bash,
# Node's newest LTS, uv, Claude Code and gh. Not installed: Codex and Rust. After the tools, it
# applies DefaultShell's rule, Git Bash now that Git is in place, and Windows Update's policy, as
# the install does. It exits 1 when any tool could not be installed, after trying every one.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$pwsh7Path = 'C:\Program Files\PowerShell\7\pwsh.exe'
$powershell51Path = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
$gitBashPath = 'C:\Program Files\Git\bin\bash.exe'
$profileDir = [Environment]::GetFolderPath('UserProfile')
$userAgent = @{ 'User-Agent' = 'agentspace-bootstrap' }
$failures = @()

function Write-AgentspaceOut([string]$Text) {
    [Console]::Out.WriteLine($Text)
}

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

# Runs a program to its end with its output passed through, and returns its exit code.
function Invoke-AgentspaceProgram([string]$FilePath, [string[]]$ArgumentList) {
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = (@($ArgumentList | ForEach-Object { ConvertTo-AgentspaceArgument $_ }) -join ' ')
    $startInfo.UseShellExecute = $false
    $process = [System.Diagnostics.Process]::Start($startInfo)
    $process.WaitForExit()
    return $process.ExitCode
}

# This session's PATH, read again from the machine's and the user's, so a tool just installed is found.
function Update-AgentspaceSessionPath {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $extra = @((Join-Path $profileDir '.local\bin'), (Join-Path $profileDir 'AppData\Roaming\npm'))
    $env:Path = (@($machinePath, $userPath) + $extra | Where-Object { -not [string]::IsNullOrEmpty($_) }) -join ';'
}

function Test-AgentspaceCommand([string]$Name) {
    Update-AgentspaceSessionPath
    return ($null -ne (Get-Command -Name $Name -CommandType Application -ErrorAction SilentlyContinue))
}

# The newest release's asset whose name matches, from a GitHub repository's releases.
function Get-AgentspaceReleaseAsset([string]$Repository, [string]$Pattern) {
    $release = Invoke-RestMethod -UseBasicParsing -Uri ('https://api.github.com/repos/' + $Repository + '/releases/latest') -Headers $userAgent
    $asset = @($release.assets | Where-Object { $_.name -match $Pattern }) | Select-Object -First 1
    if ($null -eq $asset) { throw ('the newest release of ' + $Repository + ' has no asset matching ' + $Pattern) }
    return $asset
}

function Save-AgentspaceDownload([string]$Url, [string]$Name) {
    $target = Join-Path ([IO.Path]::GetTempPath()) $Name
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $target -Headers $userAgent
    return $target
}

function Install-AgentspaceMsi([string]$MsiPath, [string[]]$Properties) {
    $code = Invoke-AgentspaceProgram 'C:\Windows\System32\msiexec.exe' (@('/i', $MsiPath, '/quiet', '/norestart') + $Properties)
    Remove-Item -LiteralPath $MsiPath -Force -ErrorAction SilentlyContinue
    if ($code -ne 0 -and $code -ne 3010) { throw ('msiexec exited ' + $code) }
}

# Runs a PowerShell installer script from its URL in a Windows PowerShell of its own.
function Invoke-AgentspaceScriptInstaller([string]$Url, [string]$Name) {
    $scriptPath = Save-AgentspaceDownload $Url $Name
    try {
        $code = Invoke-AgentspaceProgram $powershell51Path @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $scriptPath)
    } finally {
        Remove-Item -LiteralPath $scriptPath -Force -ErrorAction SilentlyContinue
    }
    if ($code -ne 0) { throw ('its installer exited ' + $code) }
}

function Install-AgentspaceWinget([string]$PackageId) {
    $code = Invoke-AgentspaceProgram 'winget' @('install', '--id', $PackageId, '--exact', '--silent', '--disable-interactivity', '--accept-package-agreements', '--accept-source-agreements')
    if ($code -ne 0) { throw ('winget install ' + $PackageId + ' exited ' + $code) }
}

# Installs one tool when it is missing, by winget or by its own installer.
function Install-AgentspaceTool([string]$Title, [scriptblock]$IsPresent, [string]$WingetId, [scriptblock]$OwnInstaller) {
    Write-AgentspaceOut ('== ' + $Title)
    if (& $IsPresent) {
        Write-AgentspaceOut ('Already installed: ' + $Title)
        return
    }
    try {
        if ($script:wingetWorks -and -not [string]::IsNullOrEmpty($WingetId)) {
            Install-AgentspaceWinget $WingetId
            $how = 'winget ' + $WingetId
        } else {
            & $OwnInstaller
            $how = 'its own installer'
        }
        if (-not (& $IsPresent)) { throw 'it is still not found after its install' }
        Write-AgentspaceOut ('Installed: ' + $Title + ', by ' + $how)
    } catch {
        Write-AgentspaceError ('bootstrap.ps1: ' + $Title + ' could not be installed: ' + $_.Exception.Message)
        $script:failures += $Title
    }
}

# winget, when it answers in this session; otherwise every tool takes its own installer.
$script:wingetWorks = $false
try {
    $wingetCode = Invoke-AgentspaceProgram 'winget' @('--version')
    $script:wingetWorks = ($wingetCode -eq 0)
} catch {
    $script:wingetWorks = $false
}
if ($script:wingetWorks) { Write-AgentspaceOut 'winget answers; tools with a winget package are installed by it' } else { Write-AgentspaceOut 'winget does not answer; every tool is installed by its own installer' }

Install-AgentspaceTool 'PowerShell 7' { Test-Path -LiteralPath $pwsh7Path -PathType Leaf } 'Microsoft.PowerShell' {
    $asset = Get-AgentspaceReleaseAsset 'PowerShell/PowerShell' '^PowerShell-[0-9.]+-win-x64\.msi$'
    Install-AgentspaceMsi (Save-AgentspaceDownload $asset.browser_download_url $asset.name) @('ALLUSERS=1', 'ADD_PATH=1', 'USE_MU=0', 'ENABLE_MU=0')
}

Install-AgentspaceTool 'Git for Windows, Git Bash included' { Test-Path -LiteralPath $gitBashPath -PathType Leaf } 'Git.Git' {
    $asset = Get-AgentspaceReleaseAsset 'git-for-windows/git' '^Git-[0-9.]+-64-bit\.exe$'
    $installerPath = Save-AgentspaceDownload $asset.browser_download_url $asset.name
    try {
        $code = Invoke-AgentspaceProgram $installerPath @('/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES', '/ALLUSERS')
    } finally {
        Remove-Item -LiteralPath $installerPath -Force -ErrorAction SilentlyContinue
    }
    if ($code -ne 0) { throw ('its installer exited ' + $code) }
}

Install-AgentspaceTool 'Node, the newest LTS' { Test-AgentspaceCommand 'node' } 'OpenJS.NodeJS.LTS' {
    $index = Invoke-RestMethod -UseBasicParsing -Uri 'https://nodejs.org/dist/index.json' -Headers $userAgent
    $lts = @($index | Where-Object { $_.lts -and $_.lts -ne $false }) | Select-Object -First 1
    if ($null -eq $lts) { throw 'Node''s release index names no LTS' }
    $msiName = 'node-' + $lts.version + '-x64.msi'
    Install-AgentspaceMsi (Save-AgentspaceDownload ('https://nodejs.org/dist/' + $lts.version + '/' + $msiName) $msiName) @('ALLUSERS=1')
}

Install-AgentspaceTool 'uv' { Test-AgentspaceCommand 'uv' } 'astral-sh.uv' {
    Invoke-AgentspaceScriptInstaller 'https://astral.sh/uv/install.ps1' 'agentspace-uv-install.ps1'
}

# Claude Code has no winget package here: Anthropic's native installer, which installs into agent's
# profile and updates itself.
Install-AgentspaceTool 'Claude Code' { Test-AgentspaceCommand 'claude' } '' {
    Invoke-AgentspaceScriptInstaller 'https://claude.ai/install.ps1' 'agentspace-claude-install.ps1'
}

Install-AgentspaceTool 'gh' { Test-AgentspaceCommand 'gh' } 'GitHub.cli' {
    $asset = Get-AgentspaceReleaseAsset 'cli/cli' '^gh_[0-9.]+_windows_amd64\.msi$'
    Install-AgentspaceMsi (Save-AgentspaceDownload $asset.browser_download_url $asset.name) @('ALLUSERS=1')
}

# DefaultShell's rule: Git Bash once Git is in place, Windows PowerShell 5.1 otherwise (pin P15).
Write-AgentspaceOut '== The login shell and Windows Update''s policy'
if (Test-Path -LiteralPath $gitBashPath -PathType Leaf) { $shellPath = $gitBashPath } else { $shellPath = $powershell51Path }
$openSshKey = 'HKLM:\SOFTWARE\OpenSSH'
if (-not (Test-Path -LiteralPath $openSshKey)) { New-Item -Path $openSshKey -Force | Out-Null }
New-ItemProperty -LiteralPath $openSshKey -Name 'DefaultShell' -PropertyType String -Value $shellPath -Force | Out-Null
Write-AgentspaceOut ('Installed: the login shell is ' + $shellPath)

# Windows Update installs nothing and schedules no restart by itself (N10).
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
if (-not (Test-Path -LiteralPath $policyKey)) { New-Item -Path $policyKey -Force | Out-Null }
New-ItemProperty -LiteralPath $policyKey -Name 'NoAutoUpdate' -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -LiteralPath $policyKey -Name 'NoAutoRebootWithLoggedOnUsers' -PropertyType DWord -Value 1 -Force | Out-Null
Write-AgentspaceOut 'Installed: Windows Update installs nothing and schedules no restart by itself'

if ($failures.Count -gt 0) {
    Write-AgentspaceError ('bootstrap.ps1: not installed: ' + ($failures -join ', '))
    exit 1
}
exit 0
