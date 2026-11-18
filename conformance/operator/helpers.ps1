#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code for the operator and ProxyCommand checks in conformance/operator/. It dot-sources the
# harness (conformance/lease-cli/harness.ps1), which starts every program and makes every home, so
# every default comes from conformance/prototype/defaults.ps1 and every child gets the harness's
# isolation set. This file loads no prototype code, and no check names a shim path.
#
# Three things the harness does not cover are here: the AWS gate, a runner for a value that cmd.exe
# needs quoted, and the code points a value arrives with.

. (Join-Path $PSScriptRoot '..\lease-cli\harness.ps1')

# The AWS CLI programs the operator can run: the fixed path of docs/operator.md ("Programs it
# starts"), and the `aws` that PATH finds, which the operator falls back to when the fixed file is
# missing. Returns every one that exists, so the gate probes every program a mistaken run could reach.
function Get-OperatorAwsPrograms {
  $found = [System.Collections.Generic.List[string]]::new()
  $fixed = 'C:\Program Files\Amazon\AWSCLIV2\aws.exe'
  if (Test-Path -LiteralPath $fixed -PathType Leaf) { $found.Add($fixed) }
  $onPath = Get-Command aws -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($onPath -and -not ($found | Where-Object { $_ -ieq $onPath.Source })) { $found.Add($onPath.Source) }
  return , $found.ToArray()
}

# The gate every check that can reach `aws` MUST pass first (docs/conformance.md, "The harness").
# Under the harness's environment for the home, `aws sts get-caller-identity` MUST fail for every
# AWS CLI the operator can run. Returns Unreachable (true only then) and a Reason. A missing CLI is
# reported as not confirmed, never as refused, so the check is SKIPPED.
function Test-AwsUnreachable {
  param([Parameter(Mandatory)]$ConformanceHome, [string[]]$Program = (Get-OperatorAwsPrograms))
  if (-not $Program -or $Program.Count -eq 0) {
    return [pscustomobject]@{ Unreachable = $false; Reason = 'The AWS CLI was not found, so the check cannot confirm that no sign-in is reachable' }
  }
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $ConformanceHome
  foreach ($p in $Program) {
    $r = Start-ConformanceProcess -FilePath $p -Arguments @('sts', 'get-caller-identity', '--region', 'us-west-2', '--output', 'json') -ChildEnvironment $childEnv -TimeoutSeconds 60
    if ($r.TimedOut -or $r.ExitCode -le 0) {
      return [pscustomobject]@{ Unreachable = $false; Reason = 'aws sts get-caller-identity succeeded, timed out or could not be run under the isolated environment, so nothing was run' }
    }
  }
  return [pscustomobject]@{ Unreachable = $true; Reason = '' }
}

# Reports one refusal: the exit code, an empty stdout, the expected text on stderr and no runtime
# error record. $Result is what Start-ConformanceProcess or Invoke-ConformanceProgram returned.
function Write-RefusalResult {
  param(
    [Parameter(Mandatory)]$Result,
    [Parameter(Mandatory)][int]$ExpectCode,
    [Parameter(Mandatory)][string]$ExpectText,
    [Parameter(Mandatory)][string]$Name
  )
  $problems = [System.Collections.Generic.List[string]]::new()
  if ($Result.TimedOut) { $problems.Add('timed out') }
  elseif ($Result.ExitCode -ne $ExpectCode) { $problems.Add("exit $($Result.ExitCode), expected $ExpectCode") }
  $outLength = $Result.Stdout.Trim().Length
  if ($outLength -gt 0) { $problems.Add("stdout had $outLength characters, expected none") }
  if (-not $Result.Stderr.Contains($ExpectText)) { $problems.Add("stderr lacks '$ExpectText'") }
  if ($Result.Stderr -match 'CategoryInfo|At line:|Exception') { $problems.Add('stderr shows a runtime error record') }
  if ($problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($problems -join '; ') }
}

# Writes a .cmd under the home that runs $Program with $Lead and then one quoted argument, taken
# from the variable CONFORMANCE_CASE_ARG. The harness passes arguments by .NET's rules, which leave
# `&` and `|` bare, and cmd.exe would read them as operators. A caller that quotes (as an ssh
# ProxyCommand line does) keeps each value whole, and so does this wrapper. Returns the file's path.
function New-QuotedWrapper {
  param(
    [Parameter(Mandatory)]$ConformanceHome,
    [Parameter(Mandatory)][string]$Program,
    [string]$Lead = '',
    [string]$Name = 'quoted-case'
  )
  $dir = Join-Path $ConformanceHome.Root 'wrappers'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $path = Join-Path $dir "$Name.cmd"
  $line = ('"{0}" {1} "%CONFORMANCE_CASE_ARG%"' -f $Program, $Lead)
  Set-Content -LiteralPath $path -Encoding ascii -Value @('@echo off', $line, 'exit /b %ERRORLEVEL%')
  return $path
}

# Runs a wrapper from New-QuotedWrapper with $Value as its one quoted argument, under the harness's
# environment. An embedded double quote is escaped with a backslash, as a caller's quoting would.
function Invoke-QuotedCase {
  param(
    [Parameter(Mandatory)]$ConformanceHome,
    [Parameter(Mandatory)][string]$Wrapper,
    [Parameter(Mandatory)][AllowEmptyString()][string]$Value,
    [hashtable]$Environment = @{},
    [int]$TimeoutSeconds = 60
  )
  $override = $Environment.Clone()
  $override['CONFORMANCE_CASE_ARG'] = ($Value -replace '"', '\"')
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $ConformanceHome -Override $override
  Start-ConformanceProcess -FilePath $Wrapper -ChildEnvironment $childEnv -TimeoutSeconds $TimeoutSeconds
}

# Passes $Value through the same cmd.exe, .cmd and powershell.exe -File path that the operator is
# run through, and returns the code points it arrives as, joined by commas (or $null when the probe
# did not run). This answers docs/conformance.md, open question 2, for this machine.
function Get-ArgumentCodePoint {
  param([Parameter(Mandatory)]$ConformanceHome, [Parameter(Mandatory)][string]$Value)
  $dir = Join-Path $ConformanceHome.Root 'probe'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $probePs1 = Join-Path $dir 'probe-args.ps1'
  $probeCmd = Join-Path $dir 'probe-args.cmd'
  $probeOut = Join-Path $dir 'probe-args.txt'
  Set-Content -LiteralPath $probePs1 -Encoding ascii -Value '[IO.File]::WriteAllText($env:CONFORMANCE_PROBE_OUT, (($args[0].ToCharArray() | ForEach-Object { [int]$_ }) -join ","))'
  Set-Content -LiteralPath $probeCmd -Encoding ascii -Value @(
    '@echo off',
    ('powershell.exe -NoProfile -ExecutionPolicy Bypass -File "' + $probePs1 + '" %*'),
    'exit /b %ERRORLEVEL%'
  )
  $wrapper = New-QuotedWrapper -ConformanceHome $ConformanceHome -Program $probeCmd -Name 'probe'
  $r = Invoke-QuotedCase -ConformanceHome $ConformanceHome -Wrapper $wrapper -Value $Value -Environment @{ CONFORMANCE_PROBE_OUT = $probeOut }
  if ($r.TimedOut -or -not (Test-Path -LiteralPath $probeOut)) { return $null }
  return (Get-Content -LiteralPath $probeOut -Raw).Trim()
}
