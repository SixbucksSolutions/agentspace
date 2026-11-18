#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Usage-error conformance for the operator's -Alias check (docs/operator.md, "Command line").
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/prototype/defaults.ps1 (docs/conformance.md, "Naming the program under test").
# Every case is refused before anything runs, so each one exits 1 with stdout empty.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-alias-refusals.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$aliasText = 'agentspace: -Alias MUST be an SSH host alias'
$appliesText = 'agentspace: -Alias applies to up alone.'

# A value is passed through a quoting wrapper (helpers.ps1), so that cmd.exe keeps `&` and `|` whole.
function New-AliasCase {
  param(
    [Parameter(Mandatory)][string]$Shown,
    [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
    [string]$Mode = 'run',
    [string]$Reason = ''
  )
  [pscustomobject]@{
    Name = "operator.md#command-line: up -Alias $Shown is refused before anything runs"
    Value = $Value; Mode = $Mode; Reason = $Reason
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
  # An alias holding a line feed is not run: cmd.exe cuts the value at it, to 'a', a valid alias, so a
  # run could get past the refusal and reach AWS. It is a gap in docs/conformance.md, not a SKIPPED.
)
$statusName = 'operator.md#command-line: status -Alias x is refused, -Alias applies to up alone'

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status SKIPPED -Reason $gate.Reason }
    Write-ConformanceResult -Name $statusName -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program' }
    Write-ConformanceResult -Name $statusName -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } else {
    $wrapper = New-QuotedWrapper -ConformanceHome $h -Program $program -Lead 'up -Alias' -Name 'operator-up-alias'
    foreach ($case in $cases) {
      switch ($case.Mode) {
        'run' {
          $r = Invoke-QuotedCase -ConformanceHome $h -Wrapper $wrapper -Value $case.Value
          Write-RefusalResult -Result $r -ExpectCode 1 -ExpectText $aliasText -Name $case.Name
        }
        'probe' {
          # Kelvin sign is 8490 and U+0130 is 304. A value MUST arrive intact before its case runs,
          # or a mangled value could read as a valid alias.
          $expected = (($case.Value.ToCharArray() | ForEach-Object { [int]$_ }) -join ',')
          $got = Get-ArgumentCodePoint -ConformanceHome $h -Value $case.Value
          if ($got -eq $expected) {
            $r = Invoke-QuotedCase -ConformanceHome $h -Wrapper $wrapper -Value $case.Value
            Write-RefusalResult -Result $r -ExpectCode 1 -ExpectText $aliasText -Name $case.Name
          } else {
            Write-ConformanceResult -Name $case.Name -Status SKIPPED -Reason "Windows PowerShell 5.1 did not pass the argument intact (code points received: $got). See docs/conformance.md, open question 2"
          }
        }
        'never' { Write-ConformanceResult -Name $case.Name -Status SKIPPED -Reason $case.Reason }
      }
    }
    $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $h -Arguments @('status', '-Alias', 'x') -TimeoutSeconds 60
    Write-RefusalResult -Result $r -ExpectCode 1 -ExpectText $appliesText -Name $statusName
  }
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun
