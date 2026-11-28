# Host-side probe of measure (i) and of the evidence for install.ps1's steps 1 to 8 and 10, localswim.ps1's steps 1 to 5 and
# bootstrap.ps1's own steps (runs ON the Windows host under PowerShell 7, as agent; sent by scp and removed by itself). Each evidence line is
# `S02 evid=<id>|<True or False>|<detail>`; no file's content is printed, and bot.env is read in memory only to find the board's URL and to
# check the token is not echoed. With the argument system-winget it also runs `winget --version` once as SYSTEM, through a transient
# scheduled task that it removes, the way agentspace-tools.ps1 meets winget when the patch run starts it. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function E([string]$Id, [bool]$Ok, [string]$Detail) { P 'evid' ("$Id|$Ok|$Detail") }
function U($T) { return ([DateTime]$T).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
$dir = 'C:\ProgramData\agentspace'
$boot = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime

# install.ps1 step 1: PowerShell 7 in place.
$pwsh7 = 'C:\Program Files\PowerShell\7\pwsh.exe'
E 'install-1' (Test-Path -LiteralPath $pwsh7 -PathType Leaf) ('pwsh.exe present; this probe runs under PowerShell ' + $PSVersionTable.PSVersion)
# Step 2: agentspace_ca.pub beside sshd_config, not empty.
$ca = 'C:\ProgramData\ssh\agentspace_ca.pub'
$caLen = if (Test-Path -LiteralPath $ca -PathType Leaf) { (Get-Item -LiteralPath $ca -Force).Length } else { 0 }
E 'install-2' ($caLen -gt 0) "agentspace_ca.pub is $caLen byte(s)"
# Step 3: C:\ProgramData\agentspace with its own access rules: inheritance off, SYSTEM and Administrators full, Users read and execute.
$acl = Get-Acl -LiteralPath $dir
$who = @($acl.Access | ForEach-Object { $_.IdentityReference.Value } | Sort-Object -Unique)
$sysFull = @($acl.Access | Where-Object { $_.IdentityReference.Value -eq 'NT AUTHORITY\SYSTEM' -and $_.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::FullControl }).Count -gt 0
$admFull = @($acl.Access | Where-Object { $_.IdentityReference.Value -eq 'BUILTIN\Administrators' -and $_.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::FullControl }).Count -gt 0
$usersRx = @($acl.Access | Where-Object { $_.IdentityReference.Value -eq 'BUILTIN\Users' -and ($_.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::ReadAndExecute) -ne 0 -and -not ($_.FileSystemRights -band [System.Security.AccessControl.FileSystemRights]::Write) }).Count -gt 0
$onlyThree = (@($who | Where-Object { $_ -notin @('NT AUTHORITY\SYSTEM', 'BUILTIN\Administrators', 'BUILTIN\Users') }).Count -eq 0)
E 'install-3' ($acl.AreAccessRulesProtected -and $sysFull -and $admFull -and $usersRx -and $onlyThree) "inheritance off: $($acl.AreAccessRulesProtected); SYSTEM full: $sysFull; Administrators full: $admFull; Users read and execute: $usersRx; no other account: $onlyThree"
# Step 4: the host agent's files.
$files = 'agentspace-boot.ps1', 'agentspace-patch.ps1', 'agentspace-patch-check.ps1', 'agentspace-idle-check.ps1', 'agentspace-tools.ps1', 'idle.conf', 'bin\localswim.ps1', 'bin\localswim'
$missing = @($files | Where-Object { -not (Test-Path -LiteralPath (Join-Path $dir $_) -PathType Leaf) })
$conf = @()
if (Test-Path -LiteralPath (Join-Path $dir 'idle.conf')) { $conf = @([IO.File]::ReadAllLines((Join-Path $dir 'idle.conf')) | Where-Object { $_ -match '^(IDLE_MINUTES|BUSY_CPU_SECONDS|HORIZON_GRACE_MINUTES)=' }) }
E 'install-4' ($missing.Count -eq 0 -and $conf.Count -eq 3) ("missing: " + $(if ($missing.Count) { $missing -join ',' } else { 'none' }) + '; idle.conf settings: ' + ($conf -join ' '))
# Step 5: Windows Update's policy.
$au = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
$nau = (Get-ItemProperty -LiteralPath $au -Name 'NoAutoUpdate' -ErrorAction SilentlyContinue).NoAutoUpdate
$nar = (Get-ItemProperty -LiteralPath $au -Name 'NoAutoRebootWithLoggedOnUsers' -ErrorAction SilentlyContinue).NoAutoRebootWithLoggedOnUsers
E 'install-5' ($nau -eq 1 -and $nar -eq 1) "NoAutoUpdate=$nau; NoAutoRebootWithLoggedOnUsers=$nar"
# Step 6: the machine PATH holds agentspace's bin and agent's .local\bin and AppData\Roaming\npm.
$envKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment')
$machinePath = [string]$envKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$entries = @($machinePath -split ';' | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') })
$want = @{ 'agentspace bin' = (Join-Path $dir 'bin'); 'agent .local\bin' = (Join-Path $env:USERPROFILE '.local\bin'); 'agent npm' = (Join-Path $env:USERPROFILE 'AppData\Roaming\npm') }
$res = @(); $all = $true
foreach ($k in $want.Keys) { $has = @($entries | Where-Object { [string]::Equals($_, $want[$k].TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) }).Count -gt 0; if (-not $has) { $all = $false }; $res += "$k=$has" }
E 'install-6' $all ($res -join '; ')
# Step 7: DefaultShell's rule.
$ds = [string](Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\OpenSSH' -Name 'DefaultShell' -ErrorAction SilentlyContinue).DefaultShell
E 'install-7' ($ds -ieq 'C:\Program Files\Git\bin\bash.exe') "DefaultShell is $ds"
# Step 8: the three tasks, as SYSTEM, highest privileges, starting PowerShell 7 by its full path.
$tasks = @(Get-ScheduledTask -TaskPath '\agentspace\' -ErrorAction SilentlyContinue)
$okTasks = ($tasks.Count -eq 3)
$det = @()
foreach ($n in 'agentspace-boot', 'agentspace-patch', 'agentspace-idle') {
    $t = $tasks | Where-Object { $_.TaskName -eq $n } | Select-Object -First 1
    if ($null -eq $t) { $okTasks = $false; $det += "$n=missing"; continue }
    $good = ($t.Principal.UserId -eq 'SYSTEM' -and [string]$t.Principal.RunLevel -eq 'Highest' -and $t.Actions[0].Execute -ieq $pwsh7)
    if (-not $good) { $okTasks = $false }
    $det += "$n=$($t.State),SYSTEM-highest-pwsh7:$good"
}
E 'install-8' $okTasks ($det -join '; ')
# Step 10: this boot's patch run: `patched` written this boot, and the log's start line.
$patched = Join-Path $dir 'patched'
$wrote = if (Test-Path -LiteralPath $patched -PathType Leaf) { (Get-Item -LiteralPath $patched -Force).LastWriteTimeUtc } else { $null }
$started = $false
if (Test-Path -LiteralPath (Join-Path $dir 'patch.log')) { $started = @([IO.File]::ReadAllLines((Join-Path $dir 'patch.log')) | Where-Object { $_ -match 'Patch run started' }).Count -gt 0 }
E 'install-10' ($null -ne $wrote -and $wrote -ge $boot.ToUniversalTime() -and $started) ("patched written " + $(if ($null -ne $wrote) { U $wrote } else { 'never' }) + "; boot " + (U $boot) + "; patch.log has a start line: $started")

# localswim.ps1, steps 1 to 5.
$cli = Join-Path $dir 'localswim\localswim.exe'
E 'localswim-1' (Test-Path -LiteralPath $cli -PathType Leaf) 'the CLI is installed'
$envFile = Join-Path $env:USERPROFILE '.config\localswim\bot.env'
$envLen = if (Test-Path -LiteralPath $envFile -PathType Leaf) { (Get-Item -LiteralPath $envFile -Force).Length } else { 0 }
E 'localswim-2' ($envLen -gt 0) "bot.env present, $envLen byte(s)"
$wrapper = Join-Path $dir 'bin\localswim.ps1'
$text = if (Test-Path -LiteralPath $wrapper) { [IO.File]::ReadAllText($wrapper) } else { '' }
$runsIt = [regex]::Matches($text, '(?im)Invoke-Expression|\biex\b|^\s*\.\s+\$').Count
E 'localswim-3' ($text.Contains("StartsWith('LOCALSWIM_URL='") -and $runsIt -eq 0) "the wrapper reads the lines with StartsWith and has $runsIt Invoke-Expression or dot-source"
$url = ''; $token = ''
if ($envLen -gt 0) {
    foreach ($l in ([IO.File]::ReadAllText($envFile) -split "`n")) {
        $l = $l.TrimEnd("`r")
        if ($l.StartsWith('LOCALSWIM_URL=')) { $url = $l.Substring(14) } elseif ($l.StartsWith('LOCALSWIM_TOKEN=')) { $token = $l.Substring(16) }
    }
}
$reach = $false; $hostPort = 'unknown'
try {
    $u = New-Object System.Uri($url)
    $hostPort = $u.Host + ':' + $u.Port
    $c = New-Object System.Net.Sockets.TcpClient
    $w = $c.ConnectAsync($u.Host, $u.Port)
    if ($w.Wait(3000) -and $c.Connected) { $reach = $true }
    $c.Close()
} catch { }
E 'localswim-4' $reach "a TCP connection to the board at $hostPort from the host"
$exit = 'none'; $echo = $true
try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $pwsh7
    $psi.Arguments = '-NoProfile -NonInteractive -File "' + $wrapper + '" --help'
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $err = $p.StandardError.ReadToEndAsync()
    $o = $p.StandardOutput.ReadToEnd()
    [void]$p.WaitForExit(60000)
    $exit = [string]$p.ExitCode
    $echo = ($token -ne '' -and ($o.Contains($token) -or $err.Result.Contains($token)))
} catch { $exit = 'error' }
E 'localswim-5' ($exit -eq '0' -and -not $echo) "the wrapper ran the CLI with --help: exit $exit; the token echoed: $echo"

# bootstrap.ps1's own steps: each tool's file and when it appeared, then the login shell and Windows Update's policy.
$tools = [ordered]@{
    'PowerShell 7' = $pwsh7
    'Git for Windows' = 'C:\Program Files\Git\bin\bash.exe'
    'Node' = 'C:\Program Files\nodejs\node.exe'
    'uv' = (Join-Path $env:USERPROFILE '.local\bin\uv.exe')
    'Claude Code' = (Join-Path $env:USERPROFILE '.local\bin\claude.exe')
    'gh' = 'C:\Program Files\GitHub CLI\gh.exe'
}
foreach ($k in $tools.Keys) {
    $f = $tools[$k]
    if (Test-Path -LiteralPath $f -PathType Leaf) { E "bootstrap-$k" $true ('file present, created ' + (U (Get-Item -LiteralPath $f -Force).CreationTimeUtc)) } else { E "bootstrap-$k" $false 'file missing' }
}
E 'bootstrap-login-shell' ($ds -ieq 'C:\Program Files\Git\bin\bash.exe') "DefaultShell is $ds"
E 'bootstrap-update-policy' ($nau -eq 1 -and $nar -eq 1) "NoAutoUpdate=$nau; NoAutoRebootWithLoggedOnUsers=$nar"

# winget as SYSTEM, the way the patch run's agentspace-tools.ps1 meets it.
if ($args.Count -gt 0 -and $args[0] -eq 'system-winget') {
    $taskPath = '\agentspace-s02\'
    $taskName = 's02-winget'
    $script = Join-Path $dir 's02-winget-system.ps1'
    $result = Join-Path $dir 's02-winget-system.txt'
    Remove-Item -LiteralPath $result -Force -ErrorAction SilentlyContinue
    $body = @'
$r = 'C:\ProgramData\agentspace\s02-winget-system.txt'
try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = 'winget'
    $psi.Arguments = '--version'
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $o = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    $first = @($o -split "`n" | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1)[0]
    Set-Content -LiteralPath $r -Value @('who=' + [Security.Principal.WindowsIdentity]::GetCurrent().Name, 'exit=' + $p.ExitCode, 'out=' + $first)
} catch {
    Set-Content -LiteralPath $r -Value @('who=' + [Security.Principal.WindowsIdentity]::GetCurrent().Name, 'exit=error:' + $_.Exception.GetType().Name, 'out=')
}
'@
    Set-Content -LiteralPath $script -Value $body
    try {
        $action = New-ScheduledTaskAction -Execute $pwsh7 -Argument ('-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $script + '"')
        $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
        Register-ScheduledTask -TaskPath $taskPath -TaskName $taskName -Action $action -Principal $principal -Force | Out-Null
        Start-ScheduledTask -TaskPath $taskPath -TaskName $taskName
        for ($i = 0; $i -lt 45 -and -not (Test-Path -LiteralPath $result); $i++) { Start-Sleep -Seconds 2 }
        if (Test-Path -LiteralPath $result) {
            foreach ($l in [IO.File]::ReadAllLines($result)) { if ($l -match '^(who|exit|out)=') { P ('system_winget_' + $l.Substring(0, $l.IndexOf('='))) $l.Substring($l.IndexOf('=') + 1) } }
        } else { P 'system_winget_exit' 'no-answer' }
    } catch {
        P 'system_winget_exit' ('task-error:' + $_.Exception.GetType().Name)
    } finally {
        Unregister-ScheduledTask -TaskPath $taskPath -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $script, $result -Force -ErrorAction SilentlyContinue
        P 'system_winget_task_removed' ($null -eq (Get-ScheduledTask -TaskPath $taskPath -TaskName $taskName -ErrorAction SilentlyContinue))
    }
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0
