#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's certificate for a host (docs/operator.md, "How the operator
# reaches a host", steps 2 and 3; docs/state.md, "The operator's key and certificate"; "Which checks
# show it", the row for the operator's certificate's fields), run through the test seam. Story
# E0.07-T04-S02 (#0156). The command is `bootstrap -Host other` with a stand-in ssh that exits 0.
# The real ssh-keygen reads the certificate (-L) against the throwaway CA, which the harness made in
# the home. It checks that the certificate:
#   - is a user certificate for the operator's key (`hosts\other\id_ed25519`), signed by the CA (the
#     fingerprints are compared), with the key ID `operator-other`, exactly the principal `ubuntu`,
#     and exactly the two extensions `permit-port-forwarding` and `permit-pty`;
#   - is valid before its issue time plus one hour, exactly (the issue time lies between the check's
#     own readings of the clock around the command), and not valid from a time later than the issue time;
# and that the operator's key is an ed25519 key made once and never replaced by a later command (its
# bytes stay), that a later command signs a certificate again that has the same fields, and that no
# output of the command holds the private key.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA, APPDATA and the ssh folder into the
# throwaway home and removes every AWS credential, and the check confirms first that `aws sts
# get-caller-identity` fails under that environment for every AWS CLI the operator can run (SKIPPED if
# not). Only the throwaway CA signs, and ssh-keygen runs under the harness's environment. A failure
# reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-alias-certificate.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'state.md#the-operators-key-and-certificate'
$project = 'other'

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the operator's certificate for a host"
  $operator = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  $keygen = Get-Command ssh-keygen -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $operator) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
  } elseif (-not $keygen) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason 'No ssh-keygen is on PATH'
  } elseif (-not $h.CaReady) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $h.CaReason
  } else {
    $S = New-HpStandIns $h
    $S.Ssh = New-OperatorStandIn -ConformanceHome $h -Name 'ssh-ok' -Body @('findstr "^" >nul', 'exit /b 0')
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-HpHostRecord $project 'ready' $HpIdOther))
    $hostDir = Get-HpHostDir $h $project
    $keyPath = Join-Path $hostDir 'id_ed25519'
    $pubPath = Join-Path $hostDir 'id_ed25519.pub'
    $certPath = Join-Path $hostDir 'id_ed25519-cert.pub'
    $caPub = Join-Path $h.UserProfile '.agentspace\ca\agentspace_ca.pub'
    $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $h

    function Invoke-Keygen([string[]]$Arguments) { Start-ConformanceProcess -FilePath $keygen.Source -Arguments $Arguments -ChildEnvironment $childEnv -TimeoutSeconds 60 }
    function Get-Fingerprint([string]$File) {
      $r = Invoke-Keygen @('-l', '-f', $File)
      if ($r.ExitCode -ne 0 -or $r.Stdout -notmatch '(SHA256:\S+)') { return $null }
      $Matches[1]
    }
    # The fields of `ssh-keygen -L` for a certificate: Type, Key ID, Valid from and to (in the
    # laptop's zone), Principals, Extensions, and the two fingerprints.
    function Read-Certificate([string]$File) {
      $r = Invoke-Keygen @('-L', '-f', $File)
      if ($r.ExitCode -ne 0) { return $null }
      $lines = @($r.Stdout -split "`r?`n")
      $c = @{ Principals = @(); Extensions = @(); Type = ''; KeyId = $null; From = $null; To = $null; PublicKey = ''; CA = '' }
      $section = ''
      foreach ($l in $lines) {
        if ($l -match '^\s*Type:\s*(.+?)\s*$') { $c.Type = $Matches[1]; $section = ''; continue }
        if ($l -match '^\s*Public key:\s*\S+\s+(SHA256:\S+)') { $c.PublicKey = $Matches[1]; $section = ''; continue }
        if ($l -match '^\s*Signing CA:\s*\S+\s+(SHA256:\S+)') { $c.CA = $Matches[1]; $section = ''; continue }
        if ($l -match '^\s*Key ID:\s*"(.*)"\s*$') { $c.KeyId = $Matches[1]; $section = ''; continue }
        if ($l -match '^\s*Valid:\s*from\s+(\S+)\s+to\s+(\S+)\s*$') { $c.From = $Matches[1]; $c.To = $Matches[2]; $section = ''; continue }
        if ($l -match '^\s*Principals:\s*$') { $section = 'P'; continue }
        if ($l -match '^\s*Extensions:\s*$') { $section = 'E'; continue }
        if ($l -match '^\s*(Serial|Critical Options):') { $section = ''; continue }
        if ($section -eq 'P' -and $l.Trim() -ne '') { $c.Principals += $l.Trim() }
        if ($section -eq 'E' -and $l.Trim() -ne '') { $c.Extensions += $l.Trim() }
      }
      $c
    }
    # ssh-keygen prints the validity in local time with no zone: read it as the laptop's local time.
    function ConvertFrom-KeygenTime([string]$Text) {
      $dt = [DateTime]::ParseExact($Text, 'yyyy-MM-ddTHH:mm:ss', [Globalization.CultureInfo]::InvariantCulture)
      [TimeZoneInfo]::ConvertTimeToUtc([DateTime]::SpecifyKind($dt, [DateTimeKind]::Unspecified), [TimeZoneInfo]::Local)
    }
    function Get-CertProblems($Cert, [DateTime]$T0, [DateTime]$T1) {
      $p = New-HpProblems
      if ($null -eq $Cert) { $p.Add('ssh-keygen -L could not read the certificate'); return , $p }
      if ($Cert.Type -notmatch 'user certificate') { $p.Add('It is not a user certificate') }
      if ($Cert.KeyId -cne "operator-$project") { $p.Add('The key ID is not operator-<project>') }
      if (@($Cert.Principals).Count -ne 1 -or $Cert.Principals[0] -cne 'ubuntu') { $p.Add('The principals are not exactly ubuntu') }
      $ext = @($Cert.Extensions | Sort-Object)
      if ($ext.Count -ne 2 -or $ext[0] -cne 'permit-port-forwarding' -or $ext[1] -cne 'permit-pty') { $p.Add('The extensions are not exactly permit-port-forwarding and permit-pty') }
      if ($null -eq $Cert.To) { $p.Add('No validity window was read'); return , $p }
      $to = ConvertFrom-KeygenTime $Cert.To
      $from = ConvertFrom-KeygenTime $Cert.From
      $issue = $to.AddSeconds(-3600)
      $lo = $T0.AddSeconds(-1)
      $hi = $T1.AddSeconds(1)
      if ($issue -lt $lo -or $issue -gt $hi) { $p.Add('Valid-before is not the issue time plus one hour, 3600 seconds, exactly (the issue time is not within the command''s run)') }
      if ($from -gt $issue) { $p.Add('Valid-after is later than the issue time') }
      , $p
    }

    function Invoke-Sign { Invoke-HpOperator $h $S @('bootstrap', '-Host', $project) }
    $t0 = [DateTime]::UtcNow
    $r = Invoke-Sign
    $t1 = [DateTime]::UtcNow
    $p = New-HpProblems
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0 (the command that signs failed)") }
    foreach ($f in $keyPath, $pubPath, $certPath) { if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { $p.Add('A file of the operator''s key and certificate is missing') ; break } }
    Write-HpProblems "${sec}: A command given -Host makes the operator's key and certificate for the host in hosts\<project>\" $p

    $cert = if (Test-Path -LiteralPath $certPath -PathType Leaf) { Read-Certificate $certPath } else { $null }
    Write-HpProblems "${sec}: The certificate's key ID is operator-<project>, its one principal is ubuntu, its extensions are permit-port-forwarding and permit-pty, valid before its issue time plus one hour exactly and not valid from a later time" (Get-CertProblems $cert $t0 $t1)

    $p = New-HpProblems
    if ($null -eq $cert) { $p.Add('The certificate could not be read') }
    else {
      $keyFp = Get-Fingerprint $pubPath
      $caFp = Get-Fingerprint $caPub
      if (-not $keyFp -or $cert.PublicKey -cne $keyFp) { $p.Add('The certificate is not for the operator''s public key') }
      if (-not $caFp -or $cert.CA -cne $caFp) { $p.Add('The certificate is not signed by the CA') }
    }
    Write-HpProblems "${sec}: The certificate is for the operator's key and is signed by the CA" $p

    $p = New-HpProblems
    $pub = Get-HpText $pubPath
    if ($null -eq $pub -or $pub -cnotmatch '\Assh-ed25519 \S+') { $p.Add('The operator''s public key is not an ed25519 key in OpenSSH''s form') }
    $priv = Get-HpText $keyPath
    if ($null -eq $priv -or $priv -cnotmatch '-----BEGIN OPENSSH PRIVATE KEY-----') { $p.Add('The operator''s private key is not in OpenSSH''s form') }
    else {
      $body = @($priv -split "`r?`n" | Where-Object { $_ -ne '' -and $_ -notmatch '^-----' })
      foreach ($b in $body) { if ($r.Stdout.Contains($b) -or $r.Stderr.Contains($b)) { $p.Add('The command printed a line of the private key'); break } }
      $load = Invoke-Keygen @('-y', '-P', '', '-f', $keyPath)
      if ($load.ExitCode -ne 0) { $p.Add('The private key does not load with an empty passphrase') }
    }
    Write-HpProblems "${sec}: The operator's key is an ed25519 key with no passphrase, and the private key is printed nowhere" $p

    # ---- A later command signs again and never replaces the key ----
    $keyHash = Get-HpBytesHash $keyPath
    $pubHash = Get-HpBytesHash $pubPath
    $t0 = [DateTime]::UtcNow
    $r2 = Invoke-Sign
    $t1 = [DateTime]::UtcNow
    $p = New-HpProblems
    if ($r2.ExitCode -ne 0) { $p.Add("Exit $($r2.ExitCode), not 0") }
    if ((Get-HpBytesHash $keyPath) -cne $keyHash -or (Get-HpBytesHash $pubPath) -cne $pubHash) { $p.Add('The operator''s key was replaced') }
    foreach ($x in (Get-CertProblems (Read-Certificate $certPath) $t0 $t1)) { $p.Add($x) }
    $leftovers = @(Get-ChildItem -LiteralPath $hostDir -Filter 'sign-*' -Force -ErrorAction SilentlyContinue)
    if ($leftovers.Count -gt 0) { $p.Add('A sign-<hex> file was left in the host''s folder') }
    Write-HpProblems "${sec}: A later command keeps the key as it was and signs a certificate with the same fields, leaving no sign-<hex> file" $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-alias-certificate.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun
