# Usage-error conformance for the operator's -Alias check (docs/operator.md, "Command line").
# Runs the program named by AGENTSPACE_OPERATOR_CMD (default: conformance/prototype/operator.cmd).
# Every case is refused before anything runs, so each one exits 1 with stdout empty.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'helpers.ps1')

$operator = if ($env:AGENTSPACE_OPERATOR_CMD) { $env:AGENTSPACE_OPERATOR_CMD } else { Join-Path $PSScriptRoot '..\prototype\operator.cmd' }
$operator = [IO.Path]::GetFullPath($operator)

$aliasText = 'agentspace: -Alias MUST be an SSH host alias'
$appliesText = 'agentspace: -Alias applies to up alone.'

function New-AliasCase {
    param(
        [Parameter(Mandatory)][string]$Shown,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
        [string]$Mode = 'run',
        [string]$Reason = ''
    )
    [pscustomobject]@{
        Name      = "operator.md#command-line: up -Alias $Shown is refused before anything runs"
        Arguments = ('up -Alias ' + (ConvertTo-CmdArgument -Value $Value))
        Text      = $aliasText
        Mode      = $Mode
        Reason    = $Reason
    }
}

$cases = @(
    New-AliasCase -Shown '"a;b"' -Value 'a;b'
    New-AliasCase -Shown '"a b"' -Value 'a b'
    New-AliasCase -Shown '"a&b"' -Value 'a&b'
    New-AliasCase -Shown '""' -Value ''
    New-AliasCase -Shown '"a|b"' -Value 'a|b'
    New-AliasCase -Shown '"a\"b"' -Value 'a"b'
    New-AliasCase -Shown '-Fx' -Value '-Fx'
    New-AliasCase -Shown '"a/b"' -Value 'a/b'
    New-AliasCase -Shown '"a%b"' -Value 'a%b'
    New-AliasCase -Shown 'U+212A (Kelvin sign)' -Value ([string][char]0x212A) -Mode 'probe'
    New-AliasCase -Shown 'U+0130' -Value ([string][char]0x0130) -Mode 'probe'
    New-AliasCase -Shown '"a<LF>"' -Value ("a`n") -Mode 'never' -Reason "a line feed cannot cross the cmd.exe command line: it cuts the value to 'a', a valid alias, so a run could reach AWS"
    [pscustomobject]@{
        Name      = 'operator.md#command-line: status -Alias x is refused, -Alias applies to up alone'
        Arguments = 'status -Alias x'
        Text      = $appliesText
        Mode      = 'run'
        Reason    = ''
    }
)

$iso = New-IsolatedHome
try {
    if (-not (Test-AwsRefused -Iso $iso)) {
        foreach ($case in $cases) {
            Write-CheckResult -Status SKIPPED -Name $case.Name -Reason 'aws sts get-caller-identity succeeded under the isolated environment, so nothing was run'
        }
    }
    else {
        $probe = @()
        if (@($cases | Where-Object { $_.Mode -eq 'probe' }).Count -gt 0) {
            $probe = @(Get-ArgumentCodePoints -Iso $iso -Values @([string][char]0x212A, [string][char]0x0130))
        }
        # Kelvin sign is 8490 (0x212A) and U+0130 is 304 (0x0130). Both MUST arrive intact
        # before their cases run; a mangled value could read as a valid alias.
        $intact = ($probe.Count -eq 2 -and $probe[0] -eq '8490' -and $probe[1] -eq '304')
        foreach ($case in $cases) {
            switch ($case.Mode) {
                'run' {
                    Invoke-RefusalCase -Iso $iso -Program $operator -Arguments $case.Arguments -ExpectCode 1 -ExpectText $case.Text -Name $case.Name
                }
                'probe' {
                    if ($intact) {
                        Invoke-RefusalCase -Iso $iso -Program $operator -Arguments $case.Arguments -ExpectCode 1 -ExpectText $case.Text -Name $case.Name
                    }
                    else {
                        Write-CheckResult -Status SKIPPED -Name $case.Name -Reason ("Windows PowerShell 5.1 did not pass the argument intact (code points received: " + ($probe -join ' ') + "). See docs/conformance.md, open question 2")
                    }
                }
                'never' {
                    Write-CheckResult -Status SKIPPED -Name $case.Name -Reason $case.Reason
                }
            }
        }
    }
}
finally {
    Remove-IsolatedHome -Iso $iso
}
Exit-CheckRun
