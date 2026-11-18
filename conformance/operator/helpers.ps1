# Shared code for the operator and ProxyCommand usage-error checks (conformance/operator/).
# This file holds code only. It loads no prototype code. It runs the program under test only
# through the environment variable that names that program, and every run gets a throwaway
# folder and the AWS variables described in docs/operator.md (Environment variables).

$script:Passed = 0
$script:Failed = 0
$script:Skipped = 0

function Write-CheckResult {
    param(
        [Parameter(Mandatory)][ValidateSet('PASS', 'FAIL', 'SKIPPED')][string]$Status,
        [Parameter(Mandatory)][string]$Name,
        [string]$Reason = ''
    )
    switch ($Status) {
        'PASS' { $script:Passed++; Write-Output "PASS $Name" }
        'FAIL' { $script:Failed++; Write-Output "FAIL ${Name}: $Reason" }
        'SKIPPED' { $script:Skipped++; Write-Output "SKIPPED ${Name}: $Reason" }
    }
}

function Exit-CheckRun {
    Write-Output ('{0} passed, {1} failed, {2} skipped' -f $script:Passed, $script:Failed, $script:Skipped)
    if ($script:Failed -gt 0) { exit 1 }
    if ($script:Skipped -gt 0) { exit 2 }
    exit 0
}

function New-IsolatedHome {
    $root = Join-Path $env:TEMP ('agentspace-operator-conf-' + [guid]::NewGuid().ToString('N'))
    $iso = @{
        Root    = $root
        Profile = (Join-Path $root 'profile')
        State   = (Join-Path $root 'state')
        Ssh     = (Join-Path $root 'ssh')
    }
    foreach ($dir in @($iso.Profile, $iso.State, $iso.Ssh)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    return $iso
}

function Remove-IsolatedHome {
    param([Parameter(Mandatory)][hashtable]$Iso)
    $leaf = Split-Path -Leaf $Iso.Root
    if ($leaf.StartsWith('agentspace-operator-conf-') -and $Iso.Root.StartsWith($env:TEMP, [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $Iso.Root -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Get-IsolatedEnvironment {
    param([Parameter(Mandatory)][hashtable]$Iso)
    return @{
        USERPROFILE                            = $Iso.Profile
        LOCALAPPDATA                           = $Iso.Profile
        AGENTSPACE_STATE_DIR                   = $Iso.State
        AGENTSPACE_SSH_DIR                     = $Iso.Ssh
        AGENTSPACE_NESTED                      = $null
        AWS_CONFIG_FILE                        = (Join-Path $Iso.Root 'no-aws-config')
        AWS_SHARED_CREDENTIALS_FILE            = (Join-Path $Iso.Root 'no-aws-credentials')
        AWS_EC2_METADATA_DISABLED              = 'true'
        AWS_PROFILE                            = $null
        AWS_DEFAULT_PROFILE                    = $null
        AWS_ACCESS_KEY_ID                      = $null
        AWS_SECRET_ACCESS_KEY                  = $null
        AWS_SESSION_TOKEN                      = $null
        AWS_CONTAINER_CREDENTIALS_FULL_URI     = $null
        AWS_CONTAINER_CREDENTIALS_RELATIVE_URI = $null
    }
}

function Invoke-Captured {
    param(
        [Parameter(Mandatory)][string]$FileName,
        [Parameter(Mandatory)][string]$Arguments,
        [Parameter(Mandatory)][hashtable]$Iso,
        [hashtable]$Extra = @{},
        [int]$TimeoutMs = 60000
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FileName
    $psi.Arguments = $Arguments
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = $Iso.Root

    $vars = Get-IsolatedEnvironment -Iso $Iso
    foreach ($key in $Extra.Keys) { $vars[$key] = $Extra[$key] }
    foreach ($key in $vars.Keys) {
        foreach ($existing in @($psi.Environment.Keys)) {
            if ($existing -ieq $key) { [void]$psi.Environment.Remove($existing) }
        }
        if ($null -ne $vars[$key]) { $psi.Environment[$key] = [string]$vars[$key] }
    }

    $proc = [System.Diagnostics.Process]::new()
    $proc.StartInfo = $psi
    [void]$proc.Start()
    # The child MUST NOT read this process's stdin, so it gets a closed pipe.
    $proc.StandardInput.Close()
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    $timedOut = -not $proc.WaitForExit($TimeoutMs)
    if ($timedOut) {
        try { $proc.Kill() } catch { }
        $proc.WaitForExit()
    }
    $result = [pscustomobject]@{
        Code     = $(if ($timedOut) { -1 } else { $proc.ExitCode })
        Out      = $outTask.Result
        Err      = $errTask.Result
        TimedOut = $timedOut
    }
    $proc.Dispose()
    return $result
}

function Test-AwsRefused {
    # Every program-running check calls this first, under the isolated environment. If AWS
    # answers, nothing may run.
    param([Parameter(Mandatory)][hashtable]$Iso)
    try {
        $cmdExe = Join-Path $env:SystemRoot 'System32\cmd.exe'
        $r = Invoke-Captured -FileName $cmdExe -Arguments '/d /s /c "aws sts get-caller-identity"' -Iso $Iso
    }
    catch {
        return $false
    }
    return ((-not $r.TimedOut) -and ($r.Code -ne 0))
}

function ConvertTo-CmdArgument {
    # Wraps one argument in double quotes for PowerShell's command-line parser, escaping any
    # embedded double quote with a backslash.
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Value)
    return '"' + ($Value -replace '"', '\"') + '"'
}

function Invoke-RefusalCase {
    # Runs a .cmd program through cmd.exe, as the operator and the ProxyCommand are run, and
    # checks the exit code, that stdout is empty, and that stderr carries the expected message
    # without a runtime's error record.
    param(
        [Parameter(Mandatory)][hashtable]$Iso,
        [Parameter(Mandatory)][string]$Program,
        [Parameter(Mandatory)][string]$Arguments,
        [Parameter(Mandatory)][int]$ExpectCode,
        [Parameter(Mandatory)][string]$ExpectText,
        [Parameter(Mandatory)][string]$Name
    )
    $cmdExe = Join-Path $env:SystemRoot 'System32\cmd.exe'
    $r = Invoke-Captured -FileName $cmdExe -Arguments ('/d /s /c ""' + $Program + '" ' + $Arguments + '"') -Iso $Iso
    $problems = New-Object System.Collections.Generic.List[string]
    if ($r.TimedOut) { $problems.Add('timed out') }
    elseif ($r.Code -ne $ExpectCode) { $problems.Add("exit $($r.Code), expected $ExpectCode") }
    $outLength = $r.Out.Trim().Length
    if ($outLength -gt 0) { $problems.Add("stdout had $outLength characters, expected none") }
    if (-not $r.Err.Contains($ExpectText)) { $problems.Add("stderr lacks '$ExpectText'") }
    if ($r.Err -match 'CategoryInfo|At line:|Exception') { $problems.Add('stderr shows a runtime error record') }
    if ($problems.Count -eq 0) {
        Write-CheckResult -Status PASS -Name $Name
    }
    else {
        Write-CheckResult -Status FAIL -Name $Name -Reason ($problems -join '; ')
    }
}

function Get-ArgumentCodePoints {
    # Passes values through the same cmd.exe, .cmd and powershell.exe -File path that the
    # operator uses, and returns the code points each one arrives as. This answers
    # docs/conformance.md open question 2 for this machine.
    param(
        [Parameter(Mandatory)][hashtable]$Iso,
        [Parameter(Mandatory)][string[]]$Values
    )
    $probePs1 = Join-Path $Iso.Root 'probe-args.ps1'
    $probeCmd = Join-Path $Iso.Root 'probe-args.cmd'
    $probeOut = Join-Path $Iso.Root 'probe-args.txt'
    Set-Content -LiteralPath $probePs1 -Encoding ASCII -Value @(
        '$codes = @()',
        'foreach ($a in $args) { $codes += (($a.ToCharArray() | ForEach-Object { [int]$_ }) -join ",") }',
        '[IO.File]::WriteAllLines($env:AGENTSPACE_PROBE_OUT, [string[]]$codes)'
    )
    Set-Content -LiteralPath $probeCmd -Encoding ASCII -Value @(
        '@echo off',
        ('powershell.exe -NoProfile -ExecutionPolicy Bypass -File "' + $probePs1 + '" %*'),
        'exit /b %ERRORLEVEL%'
    )
    $argText = (($Values | ForEach-Object { ConvertTo-CmdArgument -Value $_ }) -join ' ')
    $cmdExe = Join-Path $env:SystemRoot 'System32\cmd.exe'
    $r = Invoke-Captured -FileName $cmdExe -Arguments ('/d /s /c ""' + $probeCmd + '" ' + $argText + '"') -Iso $Iso -Extra @{ AGENTSPACE_PROBE_OUT = $probeOut }
    if ($r.TimedOut -or -not (Test-Path -LiteralPath $probeOut)) { return @() }
    return @(Get-Content -LiteralPath $probeOut)
}
