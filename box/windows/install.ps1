# install.ps1: sync's installer for a Windows host, the counterpart of install.sh (D96; N1, pin
# P15), run by sync's row 4 as agent, elevated, from the staging folder <home>\.agentspace-system
# (docs/operator.md, Windows hosts, What the install does). Safe to run again: every sync runs it.
#
# It starts under Windows PowerShell 5.1, puts PowerShell 7 in place when it is missing, and runs
# the rest of itself under PowerShell 7, so this whole file is written in Windows PowerShell 5.1's
# syntax. In order:
#   1. Under Windows PowerShell 5.1: installs PowerShell 7 when C:\Program Files\PowerShell\7\pwsh.exe
#      is missing, then runs itself again under PowerShell 7, by that full path, with the same
#      arguments, and exits with that run's status.
#   2. Fails at once if agentspace_ca.pub is missing or empty beside itself.
#   3. Makes C:\ProgramData\agentspace with its own access rules.
#   4. Installs the host agent's files from beside itself into that folder.
#   5. Turns off Windows Update's own installs and restarts.
#   6. Puts the tool folders on the machine PATH.
#   7. Applies DefaultShell's rule.
#   8. Registers the scheduled tasks.
#   9. Installs the CA's public key and the agentspace lines of sshd_config as one transaction,
#      and checks sshd's effective settings.
#  10. Starts this boot's patch run when it has not finished.
# The Installed: and Patch run: lines go to stdout, and every install.ps1: line to stderr. It never
# asks for input and never restarts the host.
#
# The test seam (docs/operator.md, Windows hosts, The tests' seams; pins P17 and P18):
# AGENTSPACE_INSTALL_ONLY set to sshd-config, AGENTSPACE_SSHD_CONFIG and
# AGENTSPACE_SSHD_TEST_PROGRAM, set together or not at all. Set, steps 1 to 8 and 10 are skipped
# and step 9's edit runs alone, on the file AGENTSPACE_SSHD_CONFIG names, beside which the CA file
# is agentspace_ca.pub. sshd.exe -t -f <file> is the named program given the temporary file's full
# path as its one argument, and sshd -T for the PerSourcePenalties probe is the same program given
# the one argument -T. In place of restarting sshd and of step 9's sshd -T check, it writes
# sshd-restarted beside the file. No task, script or file the install puts on a host sets any of
# them.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$scriptName = 'install'
$seamNames = @('AGENTSPACE_INSTALL_ONLY', 'AGENTSPACE_SSHD_CONFIG', 'AGENTSPACE_SSHD_TEST_PROGRAM')
$pwsh7Path = 'C:\Program Files\PowerShell\7\pwsh.exe'
$powershell51Path = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
$gitBashPath = 'C:\Program Files\Git\bin\bash.exe'
# sshd by its full path, never by name (pin P13). Under the seam it is never started: the stand-in
# AGENTSPACE_SSHD_TEST_PROGRAM names takes its place.
$sshdExe = 'C:\Windows\System32\OpenSSH\sshd.exe'
$agentspaceDir = 'C:\ProgramData\agentspace'
$hostSshdConfig = 'C:\ProgramData\ssh\sshd_config'
$penaltyLine = 'PerSourcePenaltyExemptList 127.0.0.1,::1'

function Write-AgentspaceOut([string]$Text) {
    [Console]::Out.WriteLine($Text)
}

function Write-AgentspaceError([string]$Text) {
    [Console]::Error.WriteLine($Text)
}

function Get-AgentspaceUnixNow {
    return [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
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

# The seam: every variable set, or none.
$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })
if ($seamSet.Count -gt 0 -and $seamSet.Count -lt $seamNames.Count) {
    Write-AgentspaceError ($scriptName + ': ' + ($seamNames -join ', ') + ' are set together or not at all; nothing was done')
    exit 1
}
$underSeam = ($seamSet.Count -eq $seamNames.Count)
if ($underSeam -and $env:AGENTSPACE_INSTALL_ONLY -cne 'sshd-config') {
    Write-AgentspaceError 'install.ps1: AGENTSPACE_INSTALL_ONLY names no step it can run alone; the one it takes is sshd-config'
    exit 1
}

# The account guard (pin P13): without its seam it changes the machine it runs on, so it runs only
# as the host's user agent, as sync runs it, and a run as anyone else, a test that left its seam
# unset among them, changes nothing. It MUST stay the first statement after the seam's detection,
# with nothing before it that writes, makes, starts or logs anything.
if (-not $underSeam -and -not [System.Security.Principal.WindowsIdentity]::GetCurrent().Name.EndsWith('\agent', [StringComparison]::OrdinalIgnoreCase)) {
    Write-AgentspaceError ($scriptName + ': without its test seam it runs only as agent; nothing was done')
    exit 1
}

# 1. PowerShell 7, then the rest of this file under it.
function Install-AgentspacePowerShell7 {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $release = Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/PowerShell/PowerShell/releases/latest' -Headers @{ 'User-Agent' = 'agentspace-install' }
    $asset = @($release.assets | Where-Object { $_.name -match '^PowerShell-[0-9.]+-win-x64\.msi$' }) | Select-Object -First 1
    if ($null -eq $asset) { throw 'the newest release has no x64 MSI' }
    $version = ([string]$release.tag_name) -replace '^v', ''
    $msiPath = Join-Path ([IO.Path]::GetTempPath()) $asset.name
    Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $msiPath
    $code = Invoke-AgentspaceProgram 'C:\Windows\System32\msiexec.exe' @('/i', $msiPath, '/quiet', '/norestart', 'ALLUSERS=1', 'ADD_PATH=1', 'USE_MU=0', 'ENABLE_MU=0')
    Remove-Item -LiteralPath $msiPath -Force -ErrorAction SilentlyContinue
    if ($code -ne 0 -and $code -ne 3010) { throw ('msiexec exited ' + $code) }
    if (-not (Test-Path -LiteralPath $pwsh7Path -PathType Leaf)) { throw ($pwsh7Path + ' is still missing after its install') }
    return $version
}

if (-not $underSeam -and $PSVersionTable.PSVersion.Major -lt 7) {
    if (-not (Test-Path -LiteralPath $pwsh7Path -PathType Leaf)) {
        try {
            $installedVersion = Install-AgentspacePowerShell7
        } catch {
            Write-AgentspaceError ('install.ps1: PowerShell 7 could not be installed: ' + $_.Exception.Message)
            exit 1
        }
        Write-AgentspaceOut ('Installed: PowerShell 7 (' + $installedVersion + ')')
    }
    # By its full path: sshd, started before this install, keeps its old PATH for this session.
    & $pwsh7Path -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $PSCommandPath @args
    exit $LASTEXITCODE
}

# The rest runs under PowerShell 7.

# This boot's start, as Unix seconds.
function Get-AgentspaceBootSeconds {
    $bootTime = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
    return [DateTimeOffset]::new($bootTime.ToUniversalTime()).ToUnixTimeSeconds()
}

# Writes a file whole to a temporary name in its folder, then renames it over the old one.
function Copy-AgentspaceFile([string]$Source, [string]$Destination) {
    $tempPath = $Destination + '.agentspace-new'
    [IO.File]::Copy($Source, $tempPath, $true)
    Move-Item -LiteralPath $tempPath -Destination $Destination -Force
}

function Test-AgentspaceSameBytes([byte[]]$Left, [byte[]]$Right) {
    if ($null -eq $Left -or $null -eq $Right) { return $false }
    if ($Left.Length -ne $Right.Length) { return $false }
    for ($i = 0; $i -lt $Left.Length; $i++) {
        if ($Left[$i] -ne $Right[$i]) { return $false }
    }
    return $true
}

# Inheritance off, full control for SYSTEM and Administrators, read and execute for Users.
function Set-AgentspaceFolderRules([string]$FolderPath) {
    $acl = Get-Acl -LiteralPath $FolderPath
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.GetAccessRules($true, $false, [System.Security.Principal.SecurityIdentifier]))) {
        [void]$acl.RemoveAccessRuleSpecific($rule)
    }
    $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    $propagate = [System.Security.AccessControl.PropagationFlags]::None
    $allow = [System.Security.AccessControl.AccessControlType]::Allow
    foreach ($grant in @(@('S-1-5-18', 'FullControl'), @('S-1-5-32-544', 'FullControl'), @('S-1-5-32-545', 'ReadAndExecute'))) {
        $sid = New-Object System.Security.Principal.SecurityIdentifier($grant[0])
        $rights = [System.Security.AccessControl.FileSystemRights]$grant[1]
        $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($sid, $rights, $inherit, $propagate, $allow)))
    }
    Set-Acl -LiteralPath $FolderPath -AclObject $acl
}

# Gives a file the access rules another file has: its DACL alone, never its owner.
function Copy-AgentspaceAccessRules([string]$FromPath, [string]$ToPath) {
    $sddl = (Get-Acl -LiteralPath $FromPath).GetSecurityDescriptorSddlForm('Access')
    $toAcl = New-Object System.Security.AccessControl.FileSecurity
    $toAcl.SetSecurityDescriptorSddlForm($sddl, 'Access')
    $toFile = New-Object System.IO.FileInfo($ToPath)
    if ('System.IO.FileSystemAclExtensions' -as [type]) {
        [System.IO.FileSystemAclExtensions]::SetAccessControl($toFile, $toAcl)
    } else {
        $toFile.SetAccessControl($toAcl)
    }
}

# Windows Update installs nothing and schedules no restart by itself (N10).
function Set-AgentspaceUpdatePolicy {
    $policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
    if (-not (Test-Path -LiteralPath $policyKey)) { New-Item -Path $policyKey -Force | Out-Null }
    New-ItemProperty -LiteralPath $policyKey -Name 'NoAutoUpdate' -PropertyType DWord -Value 1 -Force | Out-Null
    New-ItemProperty -LiteralPath $policyKey -Name 'NoAutoRebootWithLoggedOnUsers' -PropertyType DWord -Value 1 -Force | Out-Null
}

# DefaultShell: Git Bash when it is installed, Windows PowerShell 5.1 otherwise (N12, pin P15).
function Set-AgentspaceDefaultShell {
    if (Test-Path -LiteralPath $gitBashPath -PathType Leaf) { $shellPath = $gitBashPath } else { $shellPath = $powershell51Path }
    $openSshKey = 'HKLM:\SOFTWARE\OpenSSH'
    if (-not (Test-Path -LiteralPath $openSshKey)) { New-Item -Path $openSshKey -Force | Out-Null }
    New-ItemProperty -LiteralPath $openSshKey -Name 'DefaultShell' -PropertyType String -Value $shellPath -Force | Out-Null
    return $shellPath
}

# Runs the seam's stand-in or sshd.exe with the arguments given, returning its stdout lines and its
# exit code. A .ps1 stand-in runs in a PowerShell of its own, so its exit code and output are a
# program's.
function Invoke-AgentspaceSshd([string]$SeamProgram, [string[]]$SshdArguments) {
    $ErrorActionPreference = 'Continue'
    if ([string]::IsNullOrEmpty($SeamProgram)) {
        $lines = @(& $sshdExe @SshdArguments)
    } elseif ($SeamProgram -match '\.ps1$') {
        $selfPath = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
        $lines = @(& $selfPath -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $SeamProgram @SshdArguments)
    } else {
        $lines = @(& $SeamProgram @SshdArguments)
    }
    return @{ Lines = @($lines | ForEach-Object { [string]$_ }); ExitCode = $LASTEXITCODE }
}

# The CA's fingerprint as ssh-keygen -l prints it: SHA256: and the unpadded base64 of the key's hash.
function Get-AgentspaceFingerprint([byte[]]$KeyBytes) {
    try {
        $keyText = [Text.Encoding]::ASCII.GetString($KeyBytes)
        $firstLine = @($keyText -split "`n" | Where-Object { $_.Trim() -ne '' })[0]
        $blob = [Convert]::FromBase64String((($firstLine.Trim()) -split '\s+')[1])
        $sha = [System.Security.Cryptography.SHA256]::Create()
        $hash = $sha.ComputeHash($blob)
        return 'SHA256:' + ([Convert]::ToBase64String($hash)).TrimEnd('=')
    } catch {
        return 'unreadable key'
    }
}

# Step 9: the CA's public key and the agentspace lines of sshd_config, as one transaction
# (docs/operator.md, Windows hosts, sshd's settings on a Windows host; #0193's twin). Exits 1 on
# any failure, with the files as they were.
function Install-AgentspaceSshdConfig([string]$ConfigPath, [string]$SeamProgram) {
    $sourceCa = Join-Path $PSScriptRoot 'agentspace_ca.pub'
    $sourceLines = Join-Path $PSScriptRoot 'sshd_config.agentspace'
    $sshDir = Split-Path -Parent $ConfigPath
    $destCa = Join-Path $sshDir 'agentspace_ca.pub'
    $latin1 = [Text.Encoding]::GetEncoding(28591)

    if (-not (Test-Path -LiteralPath $sourceCa -PathType Leaf) -or (Get-Item -LiteralPath $sourceCa).Length -eq 0) {
        Write-AgentspaceError 'install.ps1: agentspace_ca.pub is missing; run agentspace-operator sync, which sends it'
        exit 1
    }
    if (-not (Test-Path -LiteralPath $sourceLines -PathType Leaf)) {
        Write-AgentspaceError 'install.ps1: sshd_config.agentspace is missing beside install.ps1; run agentspace-operator sync, which sends it'
        exit 1
    }
    if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
        Write-AgentspaceError ('install.ps1: ' + $ConfigPath + ' is missing; the host''s first boot makes it, so check C:\ProgramData\agentspace\first-boot.log')
        exit 1
    }

    # 1. sshd_config as bytes, split into lines at each LF, each line keeping its own ending.
    $oldBytes = [IO.File]::ReadAllBytes($ConfigPath)
    $lines = New-Object System.Collections.Generic.List[byte[]]
    $start = 0
    for ($i = 0; $i -lt $oldBytes.Length; $i++) {
        if ($oldBytes[$i] -eq 10) {
            $segment = New-Object byte[] ($i - $start + 1)
            [Array]::Copy($oldBytes, $start, $segment, 0, $segment.Length)
            $lines.Add($segment)
            $start = $i + 1
        }
    }
    if ($start -lt $oldBytes.Length) {
        $segment = New-Object byte[] ($oldBytes.Length - $start)
        [Array]::Copy($oldBytes, $start, $segment, 0, $segment.Length)
        $lines.Add($segment)
    }
    $texts = New-Object System.Collections.Generic.List[string]
    foreach ($segment in $lines) {
        $text = $latin1.GetString($segment)
        if ($text.EndsWith("`n")) { $text = $text.Substring(0, $text.Length - 1) }
        if ($text.EndsWith("`r")) { $text = $text.Substring(0, $text.Length - 1) }
        $texts.Add($text)
    }

    # 2. Every block of agentspace lines removed; a begin line with no end line after it refused.
    $kept = New-Object System.Collections.Generic.List[byte[]]
    $index = 0
    while ($index -lt $lines.Count) {
        if ($texts[$index].StartsWith('# agentspace begin:', [StringComparison]::Ordinal)) {
            $endIndex = -1
            for ($j = $index + 1; $j -lt $lines.Count; $j++) {
                if ($texts[$j] -ceq '# agentspace end') {
                    $endIndex = $j
                    break
                }
            }
            if ($endIndex -lt 0) {
                Write-AgentspaceError ('install.ps1: line ' + ($index + 1) + ' of ' + $ConfigPath + ' starts agentspace''s lines with no end line after it; the file is left as it was, so fix it by hand and run agentspace-operator sync again')
                exit 1
            }
            $index = $endIndex + 1
            continue
        }
        $kept.Add($lines[$index])
        $index++
    }

    # The PerSourcePenalties probe: sshd -T printing a persourcepenaltyexemptlist line.
    $probe = Invoke-AgentspaceSshd $SeamProgram @('-T')
    $knowsPenalties = $false
    foreach ($probeLine in $probe.Lines) {
        if ($probeLine -imatch '^persourcepenaltyexemptlist(\s|$)') { $knowsPenalties = $true }
    }

    # 3. The new file: the agentspace lines, each ending in CR LF, then every remaining line.
    $agentspaceLines = New-Object System.Collections.Generic.List[string]
    foreach ($sourceLine in ([IO.File]::ReadAllText($sourceLines) -split "`n")) {
        $line = $sourceLine
        if ($line.EndsWith("`r")) { $line = $line.Substring(0, $line.Length - 1) }
        $agentspaceLines.Add($line)
    }
    if ($agentspaceLines.Count -gt 0 -and $agentspaceLines[$agentspaceLines.Count - 1] -eq '') {
        $agentspaceLines.RemoveAt($agentspaceLines.Count - 1)
    }
    if ($knowsPenalties) {
        $endAt = $agentspaceLines.IndexOf('# agentspace end')
        if ($endAt -lt 0) { $endAt = $agentspaceLines.Count }
        $agentspaceLines.Insert($endAt, $penaltyLine)
    }
    $newStream = New-Object System.IO.MemoryStream
    foreach ($line in $agentspaceLines) {
        $lineBytes = [Text.Encoding]::ASCII.GetBytes($line + "`r`n")
        $newStream.Write($lineBytes, 0, $lineBytes.Length)
    }
    foreach ($segment in $kept) {
        $newStream.Write($segment, 0, $segment.Length)
    }
    $newBytes = $newStream.ToArray()

    # 6. The CA's public key is replaced whatever step 4 finds, never added to; the old bytes are
    # kept for step 5's failure.
    $oldCa = $null
    if (Test-Path -LiteralPath $destCa -PathType Leaf) { $oldCa = [IO.File]::ReadAllBytes($destCa) }
    $caBytes = [IO.File]::ReadAllBytes($sourceCa)
    [IO.File]::WriteAllBytes($destCa, $caBytes)

    if (Test-AgentspaceSameBytes $oldBytes $newBytes) {
        # 4. The same bytes: nothing written, sshd not restarted.
        Write-AgentspaceOut 'Installed: sshd_config holds the agentspace lines (already there)'
    } else {
        # 5. The new file in a temporary file beside sshd_config, with its access rules, tested by
        # sshd -t before it replaces sshd_config.
        $suffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
        $tempConfig = Join-Path $sshDir ('sshd_config.agentspace-' + $suffix + '.tmp')
        $testPassed = $false
        try {
            [IO.File]::WriteAllBytes($tempConfig, $newBytes)
            Copy-AgentspaceAccessRules $ConfigPath $tempConfig
        } catch {
            Remove-Item -LiteralPath $tempConfig -Force -ErrorAction SilentlyContinue
            if ($null -ne $oldCa) { [IO.File]::WriteAllBytes($destCa, $oldCa) } else { Remove-Item -LiteralPath $destCa -Force -ErrorAction SilentlyContinue }
            Write-AgentspaceError ('install.ps1: the new sshd_config could not be written beside the old one (' + $_.Exception.Message + '); the old files were kept and sshd was not restarted')
            exit 1
        }
        try {
            if ([string]::IsNullOrEmpty($SeamProgram)) {
                $test = Invoke-AgentspaceSshd '' @('-t', '-f', $tempConfig)
            } else {
                $test = Invoke-AgentspaceSshd $SeamProgram @($tempConfig)
            }
            $testPassed = ($test.ExitCode -eq 0)
        } catch {
            $testPassed = $false
        }
        if (-not $testPassed) {
            Remove-Item -LiteralPath $tempConfig -Force -ErrorAction SilentlyContinue
            if ($null -ne $oldCa) { [IO.File]::WriteAllBytes($destCa, $oldCa) } else { Remove-Item -LiteralPath $destCa -Force -ErrorAction SilentlyContinue }
            Write-AgentspaceError 'install.ps1: sshd -t rejected the new sshd_config; the old files were kept and sshd was not restarted'
            exit 1
        }
        Move-Item -LiteralPath $tempConfig -Destination $ConfigPath -Force
        if ([string]::IsNullOrEmpty($SeamProgram)) {
            Restart-Service -Name 'sshd' -Force
        } else {
            [IO.File]::WriteAllText((Join-Path $sshDir 'sshd-restarted'), ([string](Get-AgentspaceUnixNow) + "`n"))
        }
        Write-AgentspaceOut 'Installed: sshd_config holds the agentspace lines (added)'
    }

    # 7. sshd's effective settings; under the seam, sshd-restarted stands in for this check.
    if ([string]::IsNullOrEmpty($SeamProgram)) {
        $effective = Invoke-AgentspaceSshd '' @('-T')
        $wanted = [ordered]@{
            'pubkeyauthentication'         = 'yes'
            'passwordauthentication'       = 'no'
            'kbdinteractiveauthentication' = 'no'
            'allowusers'                   = 'agent'
            'logingracetime'               = '30'
            'clientaliveinterval'          = '15'
            'clientalivecountmax'          = '5'
        }
        if ($knowsPenalties) { $wanted['persourcepenaltyexemptlist'] = '127.0.0.1,::1' }
        $seen = @{}
        $reported = New-Object System.Collections.Generic.List[string]
        foreach ($effectiveLine in $effective.Lines) {
            $parts = $effectiveLine.Trim() -split '\s+', 2
            $keyword = $parts[0].ToLowerInvariant()
            if ($keyword -eq 'trustedusercakeys' -or $wanted.Contains($keyword)) {
                $value = ''
                if ($parts.Count -gt 1) { $value = $parts[1] }
                if (-not $seen.ContainsKey($keyword)) { $seen[$keyword] = $value }
                $reported.Add($keyword + ' ' + $value)
            }
        }
        $good = ($effective.ExitCode -eq 0)
        if (-not $seen.ContainsKey('trustedusercakeys') -or $seen['trustedusercakeys'] -notmatch '[\\/]ssh[\\/]agentspace_ca\.pub$') { $good = $false }
        foreach ($keyword in $wanted.Keys) {
            if (-not $seen.ContainsKey($keyword) -or $seen[$keyword] -ne $wanted[$keyword]) { $good = $false }
        }
        if (-not $good) {
            $sorted = @($reported | Sort-Object)
            Write-AgentspaceError ('install.ps1: sshd reports ''' + ($sorted -join ' ') + '''; another setting overrides the agentspace lines of sshd_config')
            exit 1
        }
    }

    # 8. The closing lines.
    if ($knowsPenalties) { $penalties = '127.0.0.1 and ::1 exempt from sshd penalties' } else { $penalties = 'this sshd has no PerSourcePenalties' }
    Write-AgentspaceOut ('Installed: SSH trusts the agentspace CA (' + (Get-AgentspaceFingerprint $caBytes) + '), password login closed, agent the only login')
    Write-AgentspaceOut ('Installed: login grace time 30 seconds; ' + $penalties)
    Write-AgentspaceOut 'Installed: a session whose client has gone is dropped about 90 seconds after it goes quiet (probe every 15 seconds, 5 unanswered)'
}

# The seam's one step: nothing before or after it runs.
if ($underSeam) {
    Install-AgentspaceSshdConfig $env:AGENTSPACE_SSHD_CONFIG $env:AGENTSPACE_SSHD_TEST_PROGRAM
    exit 0
}

# 2. sync sends the CA's public key beside these files; it is never in the repository (D17).
$caSource = Join-Path $PSScriptRoot 'agentspace_ca.pub'
if (-not (Test-Path -LiteralPath $caSource -PathType Leaf) -or (Get-Item -LiteralPath $caSource).Length -eq 0) {
    Write-AgentspaceError 'install.ps1: agentspace_ca.pub is missing; run agentspace-operator sync, which sends it'
    exit 1
}

# 3. C:\ProgramData\agentspace, so no account but an administrator can change a script SYSTEM runs.
foreach ($folder in @($agentspaceDir, (Join-Path $agentspaceDir 'bin'), (Join-Path $agentspaceDir 'localswim'))) {
    if (-not (Test-Path -LiteralPath $folder -PathType Container)) { New-Item -ItemType Directory -Path $folder -Force | Out-Null }
}
Set-AgentspaceFolderRules $agentspaceDir

# 4. The host agent's files, each replaced; idle.conf only when it differs.
$installs = @(
    @('agentspace-boot.ps1', $agentspaceDir),
    @('agentspace-patch.ps1', $agentspaceDir),
    @('agentspace-patch-check.ps1', $agentspaceDir),
    @('agentspace-idle-check.ps1', $agentspaceDir),
    @('agentspace-tools.ps1', $agentspaceDir),
    @('localswim.ps1', (Join-Path $agentspaceDir 'bin')),
    @('localswim', (Join-Path $agentspaceDir 'bin')),
    @('idle.conf', $agentspaceDir)
)
foreach ($entry in $installs) {
    $source = Join-Path $PSScriptRoot $entry[0]
    $destination = Join-Path $entry[1] $entry[0]
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        Write-AgentspaceError ('install.ps1: ' + $entry[0] + ' is missing beside install.ps1; run agentspace-operator sync, which sends it')
        exit 1
    }
    if ($entry[0] -eq 'idle.conf' -and (Test-Path -LiteralPath $destination -PathType Leaf)) {
        if (Test-AgentspaceSameBytes ([IO.File]::ReadAllBytes($source)) ([IO.File]::ReadAllBytes($destination))) { continue }
    }
    Copy-AgentspaceFile $source $destination
}

# 5. Windows Update's own installs and restarts off again, for a policy changed by hand.
Set-AgentspaceUpdatePolicy

# 6. The machine PATH: agentspace's bin, then agent's .local\bin and AppData\Roaming\npm, each
# appended when it is not already an entry (N15, #0166's twin).
$agentProfile = $null
try {
    $agentSid = (New-Object System.Security.Principal.NTAccount('agent')).Translate([System.Security.Principal.SecurityIdentifier]).Value
    $profileKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $agentSid
    $agentProfile = (Get-ItemProperty -LiteralPath $profileKey -Name 'ProfileImagePath').ProfileImagePath
} catch {
    $agentProfile = $null
}
if ([string]::IsNullOrEmpty($agentProfile)) { $agentProfile = $env:USERPROFILE }
$toolFolders = @((Join-Path $agentspaceDir 'bin'), (Join-Path $agentProfile '.local\bin'), (Join-Path $agentProfile 'AppData\Roaming\npm'))
$envKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment', $true)
$machinePath = [string]$envKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$entries = @($machinePath -split ';')
$added = @()
foreach ($folder in $toolFolders) {
    $want = $folder.TrimEnd('\')
    $present = $false
    foreach ($entry in ($entries + $added)) {
        foreach ($form in @($entry, [Environment]::ExpandEnvironmentVariables($entry))) {
            if ([string]::Equals($form.TrimEnd('\'), $want, [StringComparison]::OrdinalIgnoreCase)) { $present = $true }
        }
    }
    if (-not $present) { $added += $folder }
}
if ($added.Count -gt 0) {
    if ($machinePath -eq '') { $newPath = ($added -join ';') }
    elseif ($machinePath.EndsWith(';')) { $newPath = $machinePath + ($added -join ';') }
    else { $newPath = $machinePath + ';' + ($added -join ';') }
    $envKey.SetValue('Path', $newPath, [Microsoft.Win32.RegistryValueKind]::ExpandString)
    Write-AgentspaceOut 'Installed: the machine PATH holds agentspace''s and agent''s tool folders (added)'
} else {
    Write-AgentspaceOut 'Installed: the machine PATH holds agentspace''s and agent''s tool folders (already there)'
}
$envKey.Close()

# 7. DefaultShell's rule.
$loginShell = Set-AgentspaceDefaultShell
Write-AgentspaceOut ('Installed: the login shell is ' + $loginShell)

# 8. The scheduled tasks, each in \agentspace\, as SYSTEM with the highest privileges, starting
# PowerShell 7 by its full path, and never a second instance while one runs (N21).
$taskPath = '\agentspace\'
$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
function New-AgentspaceTaskAction([string]$Script) {
    $argument = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File ' + (ConvertTo-AgentspaceArgument (Join-Path $agentspaceDir $Script))
    return New-ScheduledTaskAction -Execute $pwsh7Path -Argument $argument
}
function New-AgentspaceTaskSettings([int]$LimitMinutes) {
    return New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes $LimitMinutes) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
}
$bootTrigger = New-ScheduledTaskTrigger -AtStartup
Register-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-boot' -Action (New-AgentspaceTaskAction 'agentspace-boot.ps1') -Trigger $bootTrigger -Principal $principal -Settings (New-AgentspaceTaskSettings 10) -Force | Out-Null
Register-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-patch' -Action (New-AgentspaceTaskAction 'agentspace-patch.ps1') -Principal $principal -Settings (New-AgentspaceTaskSettings 20) -Force | Out-Null
$idleTrigger = New-ScheduledTaskTrigger -AtStartup
$idleTrigger.Delay = 'PT5M'
$idleTrigger.Repetition = (New-ScheduledTaskTrigger -Once -At ([DateTime]::Now) -RepetitionInterval (New-TimeSpan -Minutes 5)).Repetition
Register-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-idle' -Action (New-AgentspaceTaskAction 'agentspace-idle-check.ps1') -Trigger $idleTrigger -Principal $principal -Settings (New-AgentspaceTaskSettings 4) -Force | Out-Null
$idleState = [string](Get-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-idle').State
$patchState = [string](Get-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-patch').State
Write-AgentspaceOut ('Installed: idle check (task ' + $idleState + '), patch at boot (task ' + $patchState + ')')

# 9. The CA's public key and sshd_config's agentspace lines.
Install-AgentspaceSshdConfig $hostSshdConfig ''

# 10. Last, this boot's patch run, when it has not finished (#0163's twin).
$patchedPath = Join-Path $agentspaceDir 'patched'
$patchedThisBoot = $false
if (Test-Path -LiteralPath $patchedPath -PathType Leaf) {
    $patchedWrite = [DateTimeOffset]::new((Get-Item -LiteralPath $patchedPath -Force).LastWriteTimeUtc).ToUnixTimeSeconds()
    $patchedThisBoot = ($patchedWrite -ge (Get-AgentspaceBootSeconds))
}
if ($patchedThisBoot) {
    Write-AgentspaceOut 'Patch run: finished this boot; not started again'
} else {
    $patchBefore = [string](Get-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-patch').State
    try {
        Start-ScheduledTask -TaskPath $taskPath -TaskName 'agentspace-patch' -ErrorAction Stop
    } catch {
        Write-AgentspaceError 'install.ps1: Task Scheduler refused to start agentspace-patch; this boot''s patch run has not started'
        exit 1
    }
    if ($patchBefore -eq 'Running' -or $patchBefore -eq 'Queued') {
        Write-AgentspaceOut 'Patch run: already running this boot; not started again'
    } else {
        Write-AgentspaceOut 'Patch run: started for this boot, as C:\ProgramData\agentspace\patched is missing'
    }
}
exit 0
