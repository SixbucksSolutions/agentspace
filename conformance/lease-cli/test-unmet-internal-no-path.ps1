#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041, point 1), a MUST: an `internal` message MUST say what failed in
# agentspace's own words. It MUST NOT carry an absolute path on the caller's machine, the caller's
# user name, the output or error text of another program such as ssh-keygen, or a machine.
#   - docs/lease-api.md, "Exit codes and errors" (the rules for an `internal` message) and "Messages".
#   - docs/lease-api.md, `renew` (the key MUST NOT change, so with no key pair no certificate can be
#     issued and `renew` fails with `internal`) and docs/state.md, "Requirements the prototype does
#     not meet yet". The order in which a build reads the two key files is state.md's SHOULD, and is
#     not checked.
# Card: E0.04-T03-S03 (#0086), carrying #0041 point 1. What the prototype does instead: `renew` of a
# lease whose key pair is gone fails with an `internal` message that holds a path.
#
# How it gets there: a ready lease is made by create, its private key and public key are both deleted
# from the throwaway lease folder, and renew is run. The message is then read for what it MUST NOT
# hold. The check for another program's text is a heuristic: the words ssh-keygen and the system's
# own error phrases.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - The only deletes it makes are the throwaway lease's two key files, through Assert-ConformancePath, on a path
#   built from an ID and an alias that passed the schema and their patterns.
# - A failure reason never quotes the message, which is what holds the path or the name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-internal-no-path.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')

$script:UnmetProxy = 'agentspace-conformance-proxy --not-run'
$script:UnmetUuid4 = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'
$script:UnmetAlias = '\Aagentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}\z'

function Invoke-UnmetLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments)
  Invoke-AgentspaceCli -ConformanceHome $Lab -Arguments (@('lease') + $Arguments) -Environment @{ AGENTSPACE_PROXY_COMMAND = $script:UnmetProxy }
}

function Add-UnmetResult {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}

function Get-UnmetErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [Parameter(Mandatory)][int]$Exit)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -eq 0) { $p.Add("Exit 0, not $Exit`: the command succeeded"); return , $p }
  if ($Run.ExitCode -ne $Exit) { $p.Add("Exit $($Run.ExitCode), not $Exit") }
  if ($Run.StdoutBytes.Length -ne 0) { $p.Add("Stdout holds $($Run.StdoutBytes.Length) bytes") }
  $doc = $Run.ErrorJson
  if ($doc -isnot [hashtable]) { $p.Add("Stderr ($($Run.StderrBytes.Length) bytes) is not a JSON document"); return , $p }
  if (-not (Test-ConformanceSchema -Json $Run.Stderr.TrimEnd("`n") -Schema 'error.output.schema.json')) { $p.Add('The error document does not validate against error.output.schema.json') }
  if ($doc['code'] -cne $Code) { $p.Add("The error code is '$($doc['code'])', not '$Code'") }
  return , $p
}

function New-UnmetReadyLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project)
  $r = [pscustomobject]@{ Ok = $false; Id = ''; Alias = ''; Why = '' }
  $run = Invoke-UnmetLease $Lab @('create', '--project', $Project)
  if ($run.ExitCode -ne 0) { $r.Why = "create exited $($run.ExitCode), not 0"; return $r }
  $doc = $run.Json
  if ($doc -isnot [hashtable] -or -not (Test-ConformanceSchema -Json $run.Stdout.TrimEnd("`n") -Schema 'lease.output.schema.json')) { $r.Why = 'create did not print a lease document that validates'; return $r }
  if ($doc['state'] -cne 'ready') { $r.Why = "create gave state '$($doc['state'])', not ready"; return $r }
  $alias = [string]$doc['ssh']['host_alias']
  if ($doc['id'] -cnotmatch $script:UnmetUuid4 -or $alias -cnotmatch $script:UnmetAlias) { $r.Why = 'the ID or the alias does not match its pattern'; return $r }
  $r.Ok = $true; $r.Id = [string]$doc['id']; $r.Alias = $alias
  return $r
}

# True when $Name occurs in $Text as a word of its own, without regard to case. A name shorter than
# 3 characters is not looked for, since it would match inside other words.
function Test-UnmetNameIn {
  param([string]$Text, [string]$Name)
  if ([string]::IsNullOrEmpty($Name) -or $Name.Length -lt 3) { return $false }
  return [regex]::IsMatch($Text, '(?<![A-Za-z0-9])' + [regex]::Escape($Name) + '(?![A-Za-z0-9])', 'IgnoreCase')
}

$renew = 'lease-api.md#exit-codes-and-errors'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$renew`: An internal message from renew holds no absolute path" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $lease = New-UnmetReadyLease $lab 'nopath'
    if (-not $lease.Ok) {
      Write-ConformanceResult -Name "$renew`: Setup, a ready lease is made by create" -Status FAIL -Reason $lease.Why
    } else {
      $key = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ".agentspace\leases\$($lease.Id)\id_ed25519")
      if (-not (Test-Path -LiteralPath $key -PathType Leaf)) {
        Write-ConformanceResult -Name "$renew`: Setup, the ready lease has a key pair to delete" -Status FAIL -Reason 'The lease folder holds no id_ed25519'
      } else {
        # Both halves go, so that no certificate can be issued and the key MUST NOT change (lease-api.md, `renew`).
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $key) -Force
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path "$key.pub") -Force -ErrorAction SilentlyContinue
        $run = Invoke-UnmetLease $lab @('renew', $lease.Id)
        $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
        Add-UnmetResult -Name "$renew`: renew of a lease with no key pair fails with internal, exit 1, one error document and no stdout (lease-api.md#agentspace-lease-renew)" -Problems $p
        $message = if ($run.ErrorJson -is [hashtable] -and $run.ErrorJson['message'] -is [string]) { [string]$run.ErrorJson['message'] } else { $null }
        if ($null -eq $message -or $p.Count -gt 0) {
          foreach ($what in 'no drive-letter path', 'no UNC path', 'no home folder', 'no text of another program', 'no user name or computer name') {
            Write-ConformanceResult -Name "$renew`: An internal message holds $what (#0041 point 1)" -Status FAIL -Reason 'There is no internal error message to read'
          }
        } else {
          $p = [System.Collections.Generic.List[string]]::new()
          if ($message -match '(?<![A-Za-z0-9])[A-Za-z]:[\\/]') { $p.Add('The message holds a path that starts with a drive letter') }
          Add-UnmetResult -Name "$renew`: An internal message holds no drive-letter path (#0041 point 1)" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          if ($message.Contains('\\')) { $p.Add('The message holds a UNC path') }
          Add-UnmetResult -Name "$renew`: An internal message holds no UNC path (#0041 point 1)" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          foreach ($root in $lab.UserProfile, $lab.Root) {
            foreach ($form in $root, ($root -replace '\\', '/')) {
              if ($message.IndexOf($form, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $p.Add('The message holds the home folder or its root'); break }
            }
            if ($p.Count -gt 0) { break }
          }
          Add-UnmetResult -Name "$renew`: An internal message holds no home folder, though it MAY say ~/ (#0041 point 1)" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          $toolText = 'ssh-keygen', 'No such file or directory', 'Load key', 'Saving key', 'Permission denied', 'invalid format', 'incorrect passphrase', 'is not a public key file'
          foreach ($t in $toolText) { if ($message.IndexOf($t, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $p.Add('The message holds text that another program prints, such as ssh-keygen'); break } }
          Add-UnmetResult -Name "$renew`: An internal message holds no text of another program, such as ssh-keygen (#0041 point 1)" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          if (Test-UnmetNameIn -Text $message -Name $env:USERNAME) { $p.Add("The message holds the caller's user name") }
          if (Test-UnmetNameIn -Text $message -Name $env:COMPUTERNAME) { $p.Add("The message holds the caller's computer name") }
          Add-UnmetResult -Name "$renew`: An internal message holds no user name and no computer name (D1, #0041 point 1)" -Problems $p
        }
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-internal-no-path.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun
