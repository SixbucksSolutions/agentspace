# Conformance check: D1 holds at connect time, for the ProxyCommand's failure path.
#
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# THIS CHECK IS EXPECTED TO FAIL AGAINST THE PROTOTYPE. The prototype's ProxyCommand prints backend
# text (instance IDs, EC2 states, the AWS CLI's text, a runtime's error record) instead of
# agentspace's own message, which is card #0087. docs/operator.md lists the requirement under
# "Requirements the prototype does not meet yet", and a rewrite MUST pass this check.
#
# Spec: docs/operator.md, "What a caller sees (D1, card #0087)", and "The ProxyCommand".
#
# What it does: runs the ProxyCommand, with no alias, with no AWS sign-in reachable, so that its
# `up` fails. It asserts what the caller's ssh would see on that failure:
#   1. A non-zero exit, and nothing on stdout (stdout is the SSH connection).
#   2. A last message on stderr that starts "agentspace: ".
#   3. No backend text on stderr (an instance or volume ID, a region name, a machine type, a cloud
#      state name, or the words EC2, AWS, SSM, Session Manager, CloudFormation, the stack's name),
#      except #0034's sign-in message, exactly as operator.md gives it (operator.md Q29).
#   4. No runtime error record on stderr.
# It never reports the offending text, only which rule it broke, because the text may carry an ID.
#
# The program under test is named by AGENTSPACE_SSH_PROXY_CMD. When it is unset, the default is the
# shim conformance/prototype/ssh-proxy.cmd (docs/conformance.md, "Naming the program under test");
# no other path to the prototype appears in this file.
#
# SAFETY. The ProxyCommand's `up` calls the real AWS CLI. So the process runs with USERPROFILE,
# AGENTSPACE_STATE_DIR, AGENTSPACE_SSH_DIR and LOCALAPPDATA in a throwaway folder under TEMP;
# AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE naming files that do not exist; and every other
# AWS credential variable removed. Before it runs the proxy, it confirms under that same
# environment that `aws sts get-caller-identity` fails, and reports SKIPPED, never running the
# proxy, if it succeeds or cannot be confirmed. The throwaway folder is removed in a finally.
#
# Result lines: PASS <name>, FAIL <name>: <reason>, SKIPPED <name>: <reason>. The last line is
# "<n> passed, <n> failed, <n> skipped". Exit 0 when all passed, 1 when any failed, 2 when none
# failed but any was skipped. These are the lease CLI harness's lines (docs/conformance.md); this
# check carries its own copy of them so that it needs no other file.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Pass = 0; $script:Fail = 0; $script:Skip = 0
function Write-Result([string]$Kind, [string]$Name, [string]$Reason) {
  switch ($Kind) {
    'PASS'    { $script:Pass++; Write-Output "PASS $Name" }
    'FAIL'    { $script:Fail++; Write-Output "FAIL ${Name}: $Reason" }
    'SKIPPED' { $script:Skip++; Write-Output "SKIPPED ${Name}: $Reason" }
  }
}
function Exit-Run {
  Write-Output "$script:Pass passed, $script:Fail failed, $script:Skip skipped"
  if ($script:Fail -gt 0) { exit 1 }
  if ($script:Skip -gt 0) { exit 2 }
  exit 0
}

$section = 'operator.md#what-a-caller-sees'
Write-Output 'NOTE This check is expected to fail against the prototype: its ProxyCommand names the backend on failure (card #0087, operator.md "Requirements the prototype does not meet yet").'

$script:work = $null
function Invoke-Check {
  # The program under test.
  $proxy = $env:AGENTSPACE_SSH_PROXY_CMD
  if ([string]::IsNullOrWhiteSpace($proxy)) {
    $proxy = Join-Path $PSScriptRoot '..\prototype\ssh-proxy.cmd'
  }
  if (-not (Test-Path -LiteralPath $proxy -PathType Leaf)) {
    Write-Result SKIPPED "$section`: the ProxyCommand's failure names no backend" 'No program to test: AGENTSPACE_SSH_PROXY_CMD is unset and the default shim is missing.'
    return
  }
  $proxy = (Resolve-Path -LiteralPath $proxy).Path

  # The AWS CLI, only to prove it cannot sign in. Never to call anything else.
  $aws = $null
  $cmd = Get-Command aws -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($cmd) { $aws = $cmd.Source }
  elseif (Test-Path -LiteralPath 'C:\Program Files\Amazon\AWSCLIV2\aws.exe') { $aws = 'C:\Program Files\Amazon\AWSCLIV2\aws.exe' }

  # The throwaway folder, and the environment that cannot reach AWS or the real token.
  $script:work = Join-Path $env:TEMP ('agentspace-conformance-proxy-' + [guid]::NewGuid().ToString('N'))
  $home1 = Join-Path $work 'home'
  $dirs = @{
    USERPROFILE = $home1
    AGENTSPACE_STATE_DIR = Join-Path $work 'state'
    AGENTSPACE_SSH_DIR = Join-Path $work 'ssh'
    LOCALAPPDATA = Join-Path $work 'localappdata'
  }
  foreach ($d in $dirs.Values) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
  New-Item -ItemType Directory -Force -Path (Join-Path $home1 '.agentspace') | Out-Null
  # A made-up instance ID, built here so that no ID is written into this file, in the cache's
  # place under the home (operator.md, "Laptop files").
  [System.IO.File]::WriteAllText((Join-Path $home1 '.agentspace\instance-id'), ('i-' + ('0' * 17)))

  function Set-SafeEnvironment($Psi) {
    foreach ($k in $dirs.Keys) { $Psi.Environment[$k] = $dirs[$k] }
    $Psi.Environment['AWS_CONFIG_FILE'] = Join-Path $work 'no-such-aws-config'
    $Psi.Environment['AWS_SHARED_CREDENTIALS_FILE'] = Join-Path $work 'no-such-aws-credentials'
    $Psi.Environment['AWS_EC2_METADATA_DISABLED'] = 'true'
    foreach ($k in 'AWS_PROFILE', 'AWS_DEFAULT_PROFILE', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'AWS_SESSION_TOKEN', 'AGENTSPACE_NESTED', 'AGENTSPACE_DIAGNOSTIC') {
      [void]$Psi.Environment.Remove($k)
    }
  }
  function Invoke-Safe([string]$File, [string]$Arguments, [int]$Seconds) {
    $psi = [System.Diagnostics.ProcessStartInfo]::new($File, $Arguments)
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    Set-SafeEnvironment $psi
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEndAsync(); $err = $p.StandardError.ReadToEndAsync()
    $timedOut = -not $p.WaitForExit($Seconds * 1000)
    if ($timedOut) {
      # Only the process this function started, and its own children.
      & "$env:SystemRoot\System32\taskkill.exe" /T /F /PID $p.Id *> $null
      [void]$p.WaitForExit(5000)
    }
    [pscustomobject]@{ TimedOut = $timedOut; ExitCode = $(if ($timedOut) { -1 } else { $p.ExitCode }); Stdout = $out.Result; Stderr = $err.Result }
  }

  # The guard: under that environment the AWS CLI MUST fail to sign in, or nothing runs.
  if (-not $aws) {
    Write-Result SKIPPED "$section`: the ProxyCommand's failure names no backend" 'The AWS CLI was not found, so the check cannot confirm that no sign-in is reachable.'
    return
  }
  $probe = Invoke-Safe $aws 'sts get-caller-identity --region us-west-2 --output json' 60
  if ($probe.TimedOut -or $probe.ExitCode -eq 0) {
    Write-Result SKIPPED "$section`: the ProxyCommand's failure names no backend" 'An AWS sign-in is reachable or could not be ruled out under the throwaway environment, so the proxy was not run.'
    return
  }

  # The run: the ProxyCommand as ssh runs it, with no alias and an empty stdin.
  $r = Invoke-Safe "$env:SystemRoot\System32\cmd.exe" ('/d /c ""' + $proxy + '""') 180
  if ($r.TimedOut) {
    Write-Result FAIL "$section`: the ProxyCommand fails instead of hanging" 'It did not end within 180 seconds.'
    return
  }

  $lines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })

  # 1. A failure, with nothing on stdout.
  $name = "$section`: a failed connect exits non-zero and writes nothing to stdout"
  if ($r.ExitCode -ne 0 -and $r.Stdout.Length -eq 0) { Write-Result PASS $name }
  else { Write-Result FAIL $name ('Exit code ' + $r.ExitCode + ', ' + $r.Stdout.Length + ' bytes on stdout.') }

  # 2. agentspace's own message is the last one.
  $name = "$section`: a failed connect ends with agentspace's own message"
  if ($lines.Count -gt 0 -and $lines[-1].StartsWith('agentspace: ')) { Write-Result PASS $name }
  else { Write-Result FAIL $name 'The last line on stderr does not start with "agentspace: ".' }

  # 3. No backend text, bar #0034's message.
  $signIn = "agentspace: AWS sign-in has expired. Run 'aws login' in a fresh Incognito window, then retry."
  $rest = @($lines | Where-Object { $_.Trim() -ne $signIn })
  $backend = [ordered]@{
    'an instance or volume ID'  = '\b(i|vol)-[0-9a-f]{8,17}\b'
    'a region name'             = '\b[a-z]{2}(-[a-z]+)+-\d\b'
    'a machine type'            = '\b[a-z]\d+[a-z]*\.(nano|micro|small|medium|\d*xlarge)\b'
    'a cloud state name'        = '\b(pending|stopping|stopped|shutting-down|terminated)\b'
    'a cloud or tool name'      = '\b(EC2|AWS|SSM|CloudFormation|Session Manager|agentbox|Ubuntu)\b'
  }
  $hits = @()
  foreach ($k in $backend.Keys) { if ($rest | Where-Object { $_ -match $backend[$k] }) { $hits += $k } }
  $name = "$section`: a failed connect names no backend on stderr"
  if ($hits.Count -eq 0) { Write-Result PASS $name }
  else { Write-Result FAIL $name ('Stderr holds ' + ($hits -join ', ') + '.') }

  # 4. No runtime error record.
  $record = '(At line:\d+|CategoryInfo|FullyQualifiedErrorId|Traceback \(most recent|Unhandled exception|\bException\b)'
  $name = "$section`: a failed connect prints no runtime error record"
  if (-not ($lines | Where-Object { $_ -match $record })) { Write-Result PASS $name }
  else { Write-Result FAIL $name 'Stderr holds a runtime error record.' }

  if ($script:Fail -gt 0) {
    Write-Output 'NOTE The failures above are the expected result against the prototype (card #0087).'
  }
}

try { Invoke-Check }
finally {
  if ($script:work -and (Test-Path -LiteralPath $script:work)) {
    Remove-Item -LiteralPath $script:work -Recurse -Force -ErrorAction SilentlyContinue
  }
}
Exit-Run
