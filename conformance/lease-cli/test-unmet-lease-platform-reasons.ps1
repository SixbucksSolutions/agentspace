#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.11 programs, whose unsupported_platform and unsupported_profile messages
# say `v0 supports` and which answer unsatisfiable for a profile that is not the platform's own with other reasons.
# A v0.12 build MUST pass every result line (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md,
# docs/driver.md and docs/state.md document, with v0.07's recording stand-in for the provisioning run (a program
# outside agentspace). See docs/conformance.md. Cases of E0.12-T04-S01 (card #0261), the requests v0.12 still
# refuses, each named by its row in docs/driver.md, "Conformance cases for v0.12" (D93, D98; architect, 2026-11-26, for
# v0.12-T01, pins P2, P21 and P26):
#   M2   An unsupported platform: create --platform windows/arm64 answers unsatisfiable with exactly one reason,
#        unsupported_platform, with the v0.12 message for windows/arm64 byte for byte, and prints profile agent-linux;
#        with --profile agent-linux and with --profile agent-windows the one reason is the same; with --profile
#        agent-linux@1 --size xxl the reasons are unsupported_platform, unsupported_profile (whose message names
#        windows/arm64) and unsupported_size, in that order; --platform linux/amd64/v3 is unsupported_platform. None
#        needs a CA, reads hosts.json, writes the registry or starts anything.
#   M3   A profile that is not the platform's own: --platform windows/amd64 --profile agent-linux, --platform
#        linux/amd64 --profile agent-windows and --profile agent-windows with the default platform each answer
#        unsatisfiable with exactly one reason, unsupported_profile, whose message names the request's profile and
#        platform byte for byte; none reads hosts.json or starts anything. (A versioned profile on a supported
#        platform, lease-api.md "Profile", is the same one reason.)
# Each case runs in two homes: one with a CA, a project list and a host registry in which the project has a ready
# host on each of the three platforms, so that a build that judged the request by its hosts would answer something
# else; and one with no CA and a host registry and a project list that do not validate, so that a build that read
# any of them would fail. Both MUST give the same answers.
# A stand-in that MUST NOT start is watched once, after a home's last create, and its problems join the last check that home reports,
# so that no result line passes only because a build started nothing, as v0.11 starts nothing for a platform it refuses.
# Sections enforced: driver.md "What it supports" and "Where it places a lease" ("The host is looked up only for a
# supported request"); lease-api.md "Platform", "Profile" and "Fields" (the messages).
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts a
# real run: every create here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it
# is given and makes no host. Every write calls Assert-ConformancePath at the call. A failure reason never quotes a
# value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-lease-platform-reasons.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 25 seconds (a stand-in that MUST NOT start is watched for 3 seconds).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')
. (Join-Path $PSScriptRoot 'lease-platform-common.ps1')

$script:RsProject = 'platcase'

# Runs each case in a home of the kind given, and returns a dictionary of the checks, one for each case, and one for
# the stand-in that MUST NOT have started.
function Test-RsCases {
  param([Parameter(Mandatory)][string]$Heading, [Parameter(Mandatory)][bool]$Broken, [Parameter(Mandatory)][object[]]$Cases)
  $sub = @{ Heading = $Heading; Broken = $Broken; Cases = $Cases }
  Use-PvLab -NoCa:$Broken -Name $sub.Heading -Body {
    param($lab)
    if ($sub.Broken) {
      Remove-PvCa -Lab $lab
      Write-PvText -Lab $lab -Path (Get-PvHostsPath $lab) -Text "this is not json`n"
      Write-PvText -Lab $lab -Path (Get-PvProjectsPath $lab) -Text "this is not json`n"
    } else {
      $records = foreach ($plat in 'linux/arm64', 'linux/amd64', 'windows/amd64') {
        New-LpHostRecord -Project $script:RsProject -Platform $plat -InstanceType $script:LpPlatform[$plat].Type['m']
      }
      [void](Set-LpHosts -Lab $lab -Records @($records))
      Set-LpProjects -Lab $lab -Entries @((New-LpProjectEntry -Name $script:RsProject))
    }
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $results = [ordered]@{}
    $lastKey = $null
    $lastList = $null
    foreach ($c in $sub.Cases) {
      $before = Get-LpSnapshot $lab
      $run = Invoke-LpCreate -Lab $lab -Project $script:RsProject -Extra $c.Extra -StandIn $standIn
      $p = Get-LpUnsatProblems -Run $run -Project $script:RsProject -Codes $c.Codes -Messages $c.Messages -Platform $c.Platform -ProfileName $c.ProfileName
      if ($run.Json -is [hashtable] -and $c.ContainsKey('Size') -and $run.Json['size'] -cne $c.Size) { $p.Add('The lease''s size is not the size asked for') }
      foreach ($x in (Get-LpChangeProblems -Before $before -After (Get-LpSnapshot $lab) -NewRecords 1 -NewFiles 0)) { $p.Add($x) }
      if ($sub.Broken -and (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\ca'))) { $p.Add('create made a CA folder') }
      $results[$c.Key] = $p
      $lastKey = $c.Key
      $lastList = $p
    }
    # The stand-in that MUST NOT start is watched once, after the last create, and its problems join the last check's.
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $lastList.Add($x) }
    $results.Remove($lastKey)
    $results["$lastKey, and none of the creates starts the stand-in"] = $lastList
    return $results
  }
}

# ---- M2 -----------------------------------------------------------------------------------------------------
$wa = 'windows/arm64'
$m2 = @(
  @{ Key = 'create --platform windows/arm64 answers unsatisfiable with exactly one reason, unsupported_platform, its message byte for byte, and prints profile agent-linux'
    Extra = @('--platform', $wa); Codes = @('unsupported_platform'); Messages = @((Get-LpPlatformMessage $wa)); Platform = $wa; ProfileName = 'agent-linux' }
  @{ Key = 'with --profile agent-linux the one reason is the same'
    Extra = @('--platform', $wa, '--profile', 'agent-linux'); Codes = @('unsupported_platform'); Messages = @((Get-LpPlatformMessage $wa)); Platform = $wa; ProfileName = 'agent-linux' }
  @{ Key = 'with --profile agent-windows the one reason is the same'
    Extra = @('--platform', $wa, '--profile', 'agent-windows'); Codes = @('unsupported_platform'); Messages = @((Get-LpPlatformMessage $wa)); Platform = $wa; ProfileName = 'agent-windows' }
  @{ Key = 'with --profile agent-linux@1 --size xxl the reasons are unsupported_platform, unsupported_profile (naming windows/arm64) and unsupported_size, in that order, each message byte for byte'
    Extra = @('--platform', $wa, '--profile', 'agent-linux@1', '--size', 'xxl'); Codes = @('unsupported_platform', 'unsupported_profile', 'unsupported_size')
    Messages = @((Get-LpPlatformMessage $wa), (Get-LpProfileMessage 'agent-linux@1' $wa), (Get-LpSizeMessage 'xxl')); Platform = $wa; ProfileName = 'agent-linux@1'; Size = 'xxl' }
  @{ Key = 'the platform is normalized before it is quoted: --platform '' Windows/ARM64 '' gives the same message as windows/arm64'
    Extra = @('--platform', ' Windows/ARM64 '); Codes = @('unsupported_platform'); Messages = @((Get-LpPlatformMessage $wa)); Platform = $wa; ProfileName = 'agent-linux' }
  @{ Key = 'a platform with a variant is not the platform it varies: --platform linux/amd64/v3 is unsupported_platform, quoting linux/amd64/v3'
    Extra = @('--platform', 'linux/amd64/v3'); Codes = @('unsupported_platform'); Messages = @((Get-LpPlatformMessage 'linux/amd64/v3')); Platform = 'linux/amd64/v3'; ProfileName = 'agent-linux' }
)
Test-RsCases -Heading 'M2 (driver.md#what-it-supports, lease-api.md#fields): With a CA, a project list, and a ready host of the project on each supported platform,' -Broken $false -Cases $m2
Test-RsCases -Heading 'M2 (driver.md#what-it-supports, lease-api.md#fields): With no CA, and a host registry and a project list that do not validate,' -Broken $true -Cases $m2

# ---- M3 -----------------------------------------------------------------------------------------------------
$m3 = @(
  @{ Key = 'create --platform windows/amd64 --profile agent-linux answers unsatisfiable with exactly one reason, unsupported_profile, naming agent-linux and windows/amd64 byte for byte'
    Extra = @('--platform', 'windows/amd64', '--profile', 'agent-linux'); Codes = @('unsupported_profile'); Messages = @((Get-LpProfileMessage 'agent-linux' 'windows/amd64')); Platform = 'windows/amd64'; ProfileName = 'agent-linux' }
  @{ Key = 'create --platform linux/amd64 --profile agent-windows answers the same, naming agent-windows and linux/amd64'
    Extra = @('--platform', 'linux/amd64', '--profile', 'agent-windows'); Codes = @('unsupported_profile'); Messages = @((Get-LpProfileMessage 'agent-windows' 'linux/amd64')); Platform = 'linux/amd64'; ProfileName = 'agent-windows' }
  @{ Key = 'create --profile agent-windows with the default platform answers the same, naming agent-windows and linux/arm64'
    Extra = @('--profile', 'agent-windows'); Codes = @('unsupported_profile'); Messages = @((Get-LpProfileMessage 'agent-windows' 'linux/arm64')); Platform = 'linux/arm64'; ProfileName = 'agent-windows' }
  @{ Key = 'a versioned profile on a supported platform is the same one reason: --platform linux/amd64 --profile Agent-Linux@01 names agent-linux@1'
    Extra = @('--platform', 'linux/amd64', '--profile', 'Agent-Linux@01'); Codes = @('unsupported_profile'); Messages = @((Get-LpProfileMessage 'agent-linux@1' 'linux/amd64')); Platform = 'linux/amd64'; ProfileName = 'agent-linux@1' }
)
Test-RsCases -Heading 'M3 (driver.md#what-it-supports, lease-api.md#profile): With a CA, a project list, and a ready host of the project on each supported platform,' -Broken $false -Cases $m3
Test-RsCases -Heading 'M3 (driver.md#what-it-supports, lease-api.md#profile): With no CA, and a host registry and a project list that do not validate,' -Broken $true -Cases $m3

Exit-ConformanceRun
