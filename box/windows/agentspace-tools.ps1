# agentspace-tools.ps1: moves agent-windows' tools forward at each start, best effort (D96, D97),
# run as SYSTEM under PowerShell 7 by the patch run, which waits for it (docs/operator.md, Windows
# hosts, The tools). Node to the newest LTS, uv by uv self update, gh, and Git for Windows, each by
# winget upgrade where winget answers, and otherwise by its own installer when the installed version
# is older than the newest. It never updates PowerShell 7, under which it runs: the bootstrap, run
# again by a person, does that. Claude Code updates itself. Each failure is logged and the next tool
# is tried, and it writes one Tools: line to C:\ProgramData\agentspace\patch.log with each tool's
# version. It has no test seam: what it does is a live check's (docs/conformance.md). So it runs only
# as SYSTEM, as the patch run starts it, and refuses anyone else before it does anything (pins P13
# and P17 of v0.12-T03).

$ErrorActionPreference = 'Stop'
$scriptName = 'agentspace-tools'

function Write-AgentspaceError([string]$Text) {
    [Console]::Error.WriteLine($Text)
}

# The account guard (pins P13 and P17): it has no seam, and it upgrades the packages of the machine
# it runs on, so it runs only as SYSTEM (S-1-5-18), as the patch run starts it, and a run as anyone
# else, a test or a person on the laptop among them, changes nothing. It MUST stay the first
# statement after $ErrorActionPreference, with nothing before it that writes, makes, starts or logs
# anything.
if ([System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne 'S-1-5-18') {
    Write-AgentspaceError ($scriptName + ': without its test seam it runs only as SYSTEM; nothing was done')
    exit 1
}

$ProgressPreference = 'SilentlyContinue'
$logPath = 'C:\ProgramData\agentspace\patch.log'
$userAgent = @{ 'User-Agent' = 'agentspace-tools' }

function Write-AgentspaceLog([string]$Text) {
    try {
        $logItem = Get-Item -LiteralPath $logPath -Force -ErrorAction SilentlyContinue
        if ($null -ne $logItem -and -not $logItem.PSIsContainer -and $logItem.Length -gt 1048576) {
            Move-Item -LiteralPath $logPath -Destination ($logPath + '.1') -Force
        }
        $stamp = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
        [IO.File]::AppendAllText($logPath, ($stamp + ' ' + $Text + "`n"))
    } catch {
    }
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

# Runs a program to its end with its output discarded, and returns its exit code.
function Invoke-AgentspaceProgram([string]$FilePath, [string[]]$ArgumentList, [hashtable]$Environment) {
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = (@($ArgumentList | ForEach-Object { ConvertTo-AgentspaceArgument $_ }) -join ' ')
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    if ($null -ne $Environment) {
        foreach ($name in $Environment.Keys) { $startInfo.Environment[$name] = $Environment[$name] }
    }
    $process = [System.Diagnostics.Process]::Start($startInfo)
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    [void]$outTask.Result
    [void]$errTask.Result
    return $process.ExitCode
}

# A program's first line of output for the arguments given, or $null.
function Get-AgentspaceOutput([string]$FilePath, [string[]]$ArgumentList) {
    try {
        if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) { return $null }
        $startInfo = New-Object System.Diagnostics.ProcessStartInfo
        $startInfo.FileName = $FilePath
        $startInfo.Arguments = (@($ArgumentList | ForEach-Object { ConvertTo-AgentspaceArgument $_ }) -join ' ')
        $startInfo.UseShellExecute = $false
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $process = [System.Diagnostics.Process]::Start($startInfo)
        $errTask = $process.StandardError.ReadToEndAsync()
        $text = $process.StandardOutput.ReadToEnd()
        $process.WaitForExit()
        [void]$errTask.Result
        $first = @($text -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' }) | Select-Object -First 1
        return $first
    } catch {
        return $null
    }
}

# The numeric part of a version as [version], or $null.
function ConvertTo-AgentspaceVersion([string]$Text) {
    if ($null -eq $Text) { return $null }
    $found = [regex]::Match($Text, '[0-9]+(\.[0-9]+){1,3}')
    if (-not $found.Success) { return $null }
    try { return [version]$found.Value } catch { return $null }
}

function Get-AgentspaceRelease([string]$Repository) {
    return Invoke-RestMethod -Uri ('https://api.github.com/repos/' + $Repository + '/releases/latest') -Headers $userAgent
}

function Save-AgentspaceDownload([string]$Url, [string]$Name) {
    $target = Join-Path 'C:\Windows\Temp' ('agentspace-tools-' + $Name)
    Invoke-WebRequest -Uri $Url -OutFile $target -Headers $userAgent
    return $target
}

function Install-AgentspaceMsi([string]$MsiPath) {
    try {
        $code = Invoke-AgentspaceProgram 'C:\Windows\System32\msiexec.exe' @('/i', $MsiPath, '/quiet', '/norestart', 'ALLUSERS=1')
    } finally {
        Remove-Item -LiteralPath $MsiPath -Force -ErrorAction SilentlyContinue
    }
    if ($code -ne 0 -and $code -ne 3010) { throw ('msiexec exited ' + $code) }
}

# agent's profile folder, from Windows' profile list.
$agentProfile = $null
try {
    $agentSid = (New-Object System.Security.Principal.NTAccount('agent')).Translate([System.Security.Principal.SecurityIdentifier]).Value
    $agentProfile = (Get-ItemProperty -LiteralPath ('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $agentSid) -Name 'ProfileImagePath').ProfileImagePath
} catch {
    $agentProfile = $null
}

$nodeExe = 'C:\Program Files\nodejs\node.exe'
$gitExe = 'C:\Program Files\Git\cmd\git.exe'
$ghExe = 'C:\Program Files\GitHub CLI\gh.exe'
$pwshExe = 'C:\Program Files\PowerShell\7\pwsh.exe'
$uvExe = $null
$claudeExe = $null
if (-not [string]::IsNullOrEmpty($agentProfile)) {
    $uvExe = Join-Path $agentProfile '.local\bin\uv.exe'
    $claudeExe = Join-Path $agentProfile '.local\bin\claude.exe'
}

# winget, when it answers as SYSTEM.
$wingetWorks = $false
try {
    $wingetWorks = ((Invoke-AgentspaceProgram 'winget' @('--version') $null) -eq 0)
} catch {
    $wingetWorks = $false
}

function Update-AgentspaceByWinget([string]$Title, [string]$PackageId) {
    $code = Invoke-AgentspaceProgram 'winget' @('upgrade', '--id', $PackageId, '--exact', '--silent', '--disable-interactivity', '--accept-package-agreements', '--accept-source-agreements') $null
    # winget answers a package with no newer version with a non-zero code of its own; it is logged.
    if ($code -ne 0) { Write-AgentspaceLog ('Tools: winget upgrade ' + $PackageId + ' exited ' + $code) }
}

# Node: the newest LTS.
try {
    if (Test-Path -LiteralPath $nodeExe -PathType Leaf) {
        if ($wingetWorks) {
            Update-AgentspaceByWinget 'Node' 'OpenJS.NodeJS.LTS'
        } else {
            $index = Invoke-RestMethod -Uri 'https://nodejs.org/dist/index.json' -Headers $userAgent
            $lts = @($index | Where-Object { $_.lts }) | Select-Object -First 1
            $installed = ConvertTo-AgentspaceVersion (Get-AgentspaceOutput $nodeExe @('--version'))
            $newest = ConvertTo-AgentspaceVersion ([string]$lts.version)
            if ($null -ne $newest -and ($null -eq $installed -or $installed -lt $newest)) {
                $msiName = 'node-' + $lts.version + '-x64.msi'
                Install-AgentspaceMsi (Save-AgentspaceDownload ('https://nodejs.org/dist/' + $lts.version + '/' + $msiName) $msiName)
            }
        }
    } else {
        Write-AgentspaceLog 'Tools: Node is not installed yet; run the bootstrap'
    }
} catch {
    Write-AgentspaceLog ('Tools: Node could not be updated: ' + $_.Exception.Message)
}

# uv: its own self update, run with agent's profile as its home, where its install receipt is.
try {
    if ($null -ne $uvExe -and (Test-Path -LiteralPath $uvExe -PathType Leaf)) {
        $uvHome = @{
            USERPROFILE  = $agentProfile
            HOME         = $agentProfile
            APPDATA      = (Join-Path $agentProfile 'AppData\Roaming')
            LOCALAPPDATA = (Join-Path $agentProfile 'AppData\Local')
        }
        $code = Invoke-AgentspaceProgram $uvExe @('self', 'update', '--quiet') $uvHome
        if ($code -ne 0) { Write-AgentspaceLog ('Tools: uv self update exited ' + $code) }
    } else {
        Write-AgentspaceLog 'Tools: uv is not installed yet; run the bootstrap'
    }
} catch {
    Write-AgentspaceLog ('Tools: uv could not be updated: ' + $_.Exception.Message)
}

# gh: the newest x64 MSI from the GitHub CLI's releases.
try {
    if (Test-Path -LiteralPath $ghExe -PathType Leaf) {
        if ($wingetWorks) {
            Update-AgentspaceByWinget 'gh' 'GitHub.cli'
        } else {
            $release = Get-AgentspaceRelease 'cli/cli'
            $installed = ConvertTo-AgentspaceVersion (Get-AgentspaceOutput $ghExe @('--version'))
            $newest = ConvertTo-AgentspaceVersion ([string]$release.tag_name)
            if ($null -ne $newest -and ($null -eq $installed -or $installed -lt $newest)) {
                $asset = @($release.assets | Where-Object { $_.name -match '^gh_[0-9.]+_windows_amd64\.msi$' }) | Select-Object -First 1
                if ($null -eq $asset) { throw 'the newest release has no x64 MSI' }
                Install-AgentspaceMsi (Save-AgentspaceDownload $asset.browser_download_url $asset.name)
            }
        }
    } else {
        Write-AgentspaceLog 'Tools: gh is not installed yet; run the bootstrap'
    }
} catch {
    Write-AgentspaceLog ('Tools: gh could not be updated: ' + $_.Exception.Message)
}

# Git for Windows: the newest 64-bit installer, run silently.
try {
    if (Test-Path -LiteralPath $gitExe -PathType Leaf) {
        if ($wingetWorks) {
            Update-AgentspaceByWinget 'Git' 'Git.Git'
        } else {
            $release = Get-AgentspaceRelease 'git-for-windows/git'
            $installed = ConvertTo-AgentspaceVersion (Get-AgentspaceOutput $gitExe @('--version'))
            $newest = ConvertTo-AgentspaceVersion ([string]$release.tag_name)
            if ($null -ne $newest -and ($null -eq $installed -or $installed -lt $newest)) {
                $asset = @($release.assets | Where-Object { $_.name -match '^Git-[0-9.]+-64-bit\.exe$' }) | Select-Object -First 1
                if ($null -eq $asset) { throw 'the newest release has no 64-bit installer' }
                $installerPath = Save-AgentspaceDownload $asset.browser_download_url $asset.name
                try {
                    $code = Invoke-AgentspaceProgram $installerPath @('/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES', '/ALLUSERS') $null
                } finally {
                    Remove-Item -LiteralPath $installerPath -Force -ErrorAction SilentlyContinue
                }
                if ($code -ne 0) { throw ('its installer exited ' + $code) }
            }
        }
    } else {
        Write-AgentspaceLog 'Tools: Git is not installed yet; run the bootstrap'
    }
} catch {
    Write-AgentspaceLog ('Tools: Git could not be updated: ' + $_.Exception.Message)
}

# One Tools: line with each tool's version.
function Get-AgentspaceVersionText([string]$FilePath, [string[]]$ArgumentList) {
    if ([string]::IsNullOrEmpty($FilePath)) { return 'missing' }
    $text = Get-AgentspaceOutput $FilePath $ArgumentList
    if ([string]::IsNullOrEmpty($text)) { return 'missing' }
    return $text
}
$versions = @(
    ('Node ' + (Get-AgentspaceVersionText $nodeExe @('--version'))),
    ('uv ' + (Get-AgentspaceVersionText $uvExe @('--version'))),
    ('gh ' + (Get-AgentspaceVersionText $ghExe @('--version'))),
    ('Git ' + (Get-AgentspaceVersionText $gitExe @('--version'))),
    ('Claude Code ' + (Get-AgentspaceVersionText $claudeExe @('--version'))),
    ('PowerShell ' + (Get-AgentspaceVersionText $pwshExe @('-NoProfile', '-NonInteractive', '-Command', '$PSVersionTable.PSVersion.ToString()')))
)
Write-AgentspaceLog ('Tools: ' + ($versions -join ', '))
exit 0
