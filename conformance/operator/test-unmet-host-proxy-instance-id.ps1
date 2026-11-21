#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of what the ProxyCommand writes into the host registry (docs/operator.md, "The
# host registry, as the operator uses it", the bullet "The ProxyCommand's `up` writes only that: a
# missing `instance_id`"; docs/state.md, "The instance ID"; "Which checks show it", the row for the
# ProxyCommand writing a missing `instance_id`), run through the test seam. Story E0.07-T04-S02 (#0156).
#
#   - A host whose record has no instance ID: the stand-in `aws` is asked for that host's own stack
#     (`agentspace-host-<project>`, or `agentbox` for the box) and the tunnel goes to the ID it
#     answers; the ID is then written into that record, and nothing else in the file changes (every
#     other member, every other record, the order), for a host alias, for a lease alias of a ready
#     host and for no argument.
#   - A record that already holds an ID is never changed: the file's bytes stay as they were (a file
#     with a byte order mark and whitespace, so a rewrite shows), no stack is asked for, and the
#     tunnel goes to the ID the record holds, though the stack would answer another.
#   - A missing registry reads as the box's record alone, and the ProxyCommand's write then creates
#     hosts.json holding that record with its ID, as the spec's seed document gives it (compact JSON,
#     no byte order mark, one LF). That holds for no argument and for agentspace-host.swimcoach.
#   - It never adds a record: a host alias for a project with no record fails and writes nothing, and
#     does not create a missing file.
#   - A stack with no InstanceId output, and a stack that does not exist, fail the connection with the
#     caller's line, exit 1, no tunnel, and write nothing.
#   - A write that fails does not fail the connection, which uses the ID it read (the file is made
#     read-only, so the replace of it cannot succeed).
#
# AGENTSPACE_NESTED is set, so `up` pushes nothing, and ssh and scp are tripwires that no step may call.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential, and the check confirms first that `aws sts get-caller-identity` fails
# under that environment for every AWS CLI the operator can run (SKIPPED if not). Every ID is made up.
# A failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_SSH_PROXY_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-proxy-instance-id.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$regSec = 'operator.md#the-host-registry-as-the-operator-uses-it'
$leaseOther = '1111111a-1111-4111-8111-11111111111a'
$aliasOther = 'agentspace-other-1a2b3c4d'
$attempt = '2026-01-01T00:00:00Z'

# True when the registry text is what a writer writes: compact JSON, no byte order mark, one final LF.
function Test-HpCompactRegistry([string]$Text) {
  if ($null -eq $Text) { return $false }
  $Text -cmatch '\A[^\s﻿]+\n\z'
}
# The members of a registry's records, as a canonical text, with one record's instance_id dropped
# (or all of the file's, when $Project is empty), so two registries compare apart from that one member.
function Get-HpRegistryCanon([string]$Text, [string]$DropIdOf) {
  $doc = ConvertFrom-ConformanceJson $Text
  if ($null -eq $doc) { return 'unparseable' }
  $out = foreach ($r in $doc['hosts']) {
    $members = foreach ($k in ($r.Keys | Sort-Object)) {
      if ($k -ceq 'instance_id' -and $r['project'] -ceq $DropIdOf) { continue }
      "$k=$($r[$k])"
    }
    ($members -join ',')
  }
  "v=$($doc['version'])|" + ($out -join '|')
}
function Get-HpRecordId([string]$Text, [string]$Project) {
  $doc = ConvertFrom-ConformanceJson $Text
  if ($null -eq $doc) { return $null }
  foreach ($r in $doc['hosts']) { if ($r['project'] -ceq $Project) { return $r['instance_id'] } }
  $null
}
function Get-HpRecordCount([string]$Text) {
  $doc = ConvertFrom-ConformanceJson $Text
  if ($null -eq $doc) { return -1 }
  @($doc['hosts']).Count
}

$h = New-ConformanceHome
try {
  $name0 = "${regSec}: the ProxyCommand writes a missing instance_id"
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program'
  } else {
    $S = New-HpStandIns $h
    $regPath = Get-HpRegistryPath $h
    $lockPath = Join-Path $h.UserProfile '.agentspace\hosts.lock'
    $noIds = @(
      (New-HpHostRecord 'swimcoach' 'ready'),
      (New-HpHostRecord 'other' 'creating' '' $attempt),
      (New-HpHostRecord 'third' 'ready' $HpIdThird))
    function Reset-Case([string[]]$Records) {
      Remove-HpPath $h $regPath
      Clear-HpStacks $h $S
      Clear-HpLogs $h $S
      Set-HpRegistry $h $Records
      Set-HpLeases $h @((New-HpLeaseRecord $leaseOther 'other' $aliasOther))
    }

    # ---- A missing instance_id is read from the host's own stack and written into its record alone ----
    $ways = @(
      @{ N = 'a host alias'; A = @('agentspace-host.other') },
      @{ N = 'a lease alias of a ready host'; A = @($aliasOther) }
    )
    foreach ($w in $ways) {
      # The lease alias needs a ready host record; the host alias works on a creating one.
      $recs = if ($w.A[0] -eq $aliasOther) { @($noIds[0], (New-HpHostRecord 'other' 'ready' '' $attempt), $noIds[2]) } else { $noIds }
      Reset-Case $recs
      Set-HpStack $h $S 'agentspace-host-other' $HpIdOther
      Set-HpStack $h $S 'agentbox' $HpIdBox
      Set-HpStack $h $S 'agentspace-host-third' $HpIdSpare
      $before = Get-HpText $regPath
      $r = Invoke-HpProxy $h $S $w.A
      $p = Get-HpReachedProblems $r $S $HpIdOther -StacksAllowed
      $a = Get-HpAsked $S
      if (@($a.Stacks).Count -ne 1 -or $a.Stacks[0] -cne 'agentspace-host-other') { $p.Add('The stacks asked for are not agentspace-host-other alone') }
      $after = Get-HpText $regPath
      if ((Get-HpRecordId $after 'other') -cne $HpIdOther) { $p.Add('The ID was not written into the host''s record') }
      if ($null -ne (Get-HpRecordId $after 'swimcoach')) { $p.Add('The box''s record gained an instance ID') }
      if ((Get-HpRecordId $after 'third') -cne $HpIdThird) { $p.Add('Another host''s record changed its instance ID') }
      if ((Get-HpRegistryCanon $after 'other') -cne (Get-HpRegistryCanon $before 'other')) { $p.Add('Something in the registry changed besides the one instance_id') }
      if (-not (Test-HpCompactRegistry $after)) { $p.Add('The file is not compact JSON with no byte order mark and one final LF') }
      Write-HpProblems "${regSec}: Through $($w.N) the ProxyCommand reads the host's stack, tunnels to its ID and writes it into that record, and changes nothing else" $p
    }

    # The record of the host it reached keeps state and last_attempt_at exactly (the creating host).
    Reset-Case $noIds
    Set-HpStack $h $S 'agentspace-host-other' $HpIdOther
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $doc = ConvertFrom-ConformanceJson (Get-HpText $regPath)
    $rec = if ($doc) { @($doc['hosts'] | Where-Object { $_['project'] -ceq 'other' })[0] } else { $null }
    $p = New-HpProblems
    if ($null -eq $rec) { $p.Add('The host''s record is gone') }
    else {
      if ($rec['state'] -cne 'creating') { $p.Add('The state changed from creating') }
      if ($rec['last_attempt_at'] -cne $attempt) { $p.Add('last_attempt_at changed') }
      if ($rec['stack'] -cne 'agentspace-host-other') { $p.Add('The stack changed') }
    }
    Write-HpProblems "${regSec}: The ProxyCommand never changes a record's state, stack or last_attempt_at, and a creating host stays creating" $p

    # ---- No argument: the box's own stack ----
    Reset-Case $noIds
    Set-HpStack $h $S 'agentbox' $HpIdBox
    $before = Get-HpText $regPath
    $r = Invoke-HpProxy $h $S @()
    $p = Get-HpReachedProblems $r $S $HpIdBox -StacksAllowed
    $a = Get-HpAsked $S
    if (@($a.Stacks).Count -ne 1 -or $a.Stacks[0] -cne 'agentbox') { $p.Add('The stacks asked for are not agentbox alone') }
    $after = Get-HpText $regPath
    if ((Get-HpRecordId $after 'swimcoach') -cne $HpIdBox) { $p.Add('The ID was not written into the box''s record') }
    if ($null -ne (Get-HpRecordId $after 'other')) { $p.Add('Another host''s record gained an instance ID') }
    if ((Get-HpRegistryCanon $after 'swimcoach') -cne (Get-HpRegistryCanon $before 'swimcoach')) { $p.Add('Something in the registry changed besides the one instance_id') }
    Write-HpProblems "${regSec}: With no argument the ProxyCommand reads the agentbox stack and writes the ID into the box's record alone" $p

    # ---- A record that holds an ID is left as it is, byte for byte ----
    Reset-Case $noIds
    Remove-HpPath $h $regPath
    $spaced = [string][char]0xFEFF + '{ "version": 0, "hosts": [ ' + (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox) + ', ' + (New-HpHostRecord 'other' 'ready' $HpIdOther) + " ] }`r`n"
    Set-HpFile $h $regPath $spaced
    Set-HpStack $h $S 'agentspace-host-other' $HpIdSpare
    Set-HpStack $h $S 'agentbox' $HpIdSpare
    Remove-HpPath $h $lockPath   # An earlier case made the lock file, which agentspace never deletes.
    $hashBefore = Get-HpBytesHash $regPath
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $p = Get-HpReachedProblems $r $S $HpIdOther
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The registry''s bytes changed though its record held an ID') }
    if (Test-Path -LiteralPath $lockPath) { $p.Add('hosts.lock was taken though nothing was written') }
    Write-HpProblems "${regSec}: A record that already holds an instance ID is used as it is: no stack is asked for and the file's bytes are unchanged" $p

    $hashBefore = Get-HpBytesHash $regPath
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @()
    $p = Get-HpReachedProblems $r $S $HpIdBox
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The registry''s bytes changed though its record held an ID') }
    Write-HpProblems "${regSec}: The box's record that holds an ID is used as it is too, with no argument" $p

    # ---- A missing registry: the box's record, written with the ID ----
    $seedText = '{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready","instance_id":"' + $HpIdBox + '"}]}' + "`n"
    foreach ($w in @(@{ N = 'no argument'; A = @() }, @{ N = 'agentspace-host.swimcoach'; A = @('agentspace-host.swimcoach') })) {
      Remove-HpPath $h $regPath
      Clear-HpStacks $h $S
      Clear-HpLogs $h $S
      Set-HpStack $h $S 'agentbox' $HpIdBox
      $r = Invoke-HpProxy $h $S $w.A
      $p = Get-HpReachedProblems $r $S $HpIdBox -StacksAllowed
      $a = Get-HpAsked $S
      if (@($a.Stacks).Count -ne 1 -or $a.Stacks[0] -cne 'agentbox') { $p.Add('The stacks asked for are not agentbox alone') }
      $after = Get-HpText $regPath
      if ($null -eq $after) { $p.Add('hosts.json was not created') }
      elseif ($after -cne $seedText) { $p.Add('hosts.json is not the box''s record alone, with its ID, as one compact line and one LF') }
      Write-HpProblems "${regSec}: A missing registry read as the box's record: with $($w.N) the ProxyCommand's write creates hosts.json holding that record alone, with its ID" $p
    }

    # ---- It never adds a record, and never creates a file for a project that has none ----
    Remove-HpPath $h $regPath
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $p = Get-HpRefusedProblems $r $S
    if (Test-Path -LiteralPath $regPath) { $p.Add('hosts.json was created for a host alias with no record') }
    Write-HpProblems "${regSec}: A host alias for a project with no record fails, and a missing registry is not created" $p

    Reset-Case $noIds
    $hashBefore = Get-HpBytesHash $regPath
    $r = Invoke-HpProxy $h $S @('agentspace-host.nobody')
    $p = Get-HpRefusedProblems $r $S
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The registry changed (a record may have been added)') }
    Write-HpProblems "${regSec}: A host alias for a project with no record adds no record" $p

    # ---- The stack gives no ID ----
    Reset-Case $noIds
    Set-HpStack $h $S 'agentspace-host-other' ''
    $hashBefore = Get-HpBytesHash $regPath
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $p = Get-HpRefusedProblems $r $S
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The registry changed') }
    Write-HpProblems "${regSec}: A stack with no InstanceId output fails the connection with the caller's line and writes nothing" $p

    Reset-Case $noIds
    $hashBefore = Get-HpBytesHash $regPath
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $p = Get-HpRefusedProblems $r $S
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The registry changed') }
    Write-HpProblems "${regSec}: A stack that does not exist fails the connection with the caller's line and writes nothing" $p

    # ---- A write that fails does not fail the connection ----
    Reset-Case $noIds
    Set-HpStack $h $S 'agentspace-host-other' $HpIdOther
    [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $regPath), [IO.FileAttributes]::ReadOnly)
    $hashBefore = Get-HpBytesHash $regPath
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    $p = Get-HpReachedProblems $r $S $HpIdOther -StacksAllowed
    if ((Get-HpBytesHash $regPath) -cne $hashBefore) { $p.Add('The write did not fail, so the case did not reach a failed write') }
    Write-HpProblems "${regSec}: A write of the ID that fails does not fail the connection, which uses the ID it read" $p
    [IO.File]::SetAttributes((Assert-ConformancePath -ConformanceHome $h -Path $regPath), [IO.FileAttributes]::Normal)
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-proxy-instance-id.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun
