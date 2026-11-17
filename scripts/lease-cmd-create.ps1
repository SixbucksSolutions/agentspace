#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease create (docs/lease-api.md, "agentspace lease create"). Story E1.02 wrote this
# stub; story E1.04 fills it in, and MUST NOT need to change scripts\lease-cli.ps1 to do so.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseCreate -Project <string> -Platform <string> -Profile <string> -Size <string> -TtlSeconds <int>
# - lease-cli.ps1 has already parsed the command line, applied the defaults, and normalized and
#   checked every value with scripts\lease-values.ps1, in the order of the contract's table. Every
#   usage, invalid_value and out_of_range error has therefore been reported before this runs.
# - Every argument is normalized and has its default applied: -Platform linux/arm64, -Profile
#   agent-linux, -Size m and -TtlSeconds 28800 (8 hours, D24) when the flag was not given.
#   -TtlSeconds is from 60 to 86400; the driver takes [TimeSpan]::FromSeconds($TtlSeconds).
# - It returns the document to print: the lease document, as ConvertTo-LeaseOutput returns it.
#   lease-cli.ps1 writes it with ConvertTo-Json -Compress -Depth 20, plus one line feed, and exits 0.
# - It fails by throwing. A lease error (New-LeaseError in scripts\lease-values.ps1) is printed as
#   it is, with the exit code of its code. Any other exception becomes internal (exit 1) with the
#   exception's message, which therefore MUST NOT name a backend, a location or a machine (D1).
# - lease-cli.ps1 dot-sources scripts\lease-values.ps1 and scripts\lease-state.ps1 before this file,
#   so their functions are available: the state file (Read-LeaseState, Update-LeaseState,
#   Find-LeaseRecord, ConvertTo-LeaseOutput) and the times (Get-LeaseNow, ConvertTo-LeaseTime).
#   This file MUST NOT normalize values itself; anything it needs MUST come from lease-values.ps1.
#   It MAY dot-source scripts\lease-driver.ps1 (story E1.03) for the driver interface.
#
# How it works (story E1.04):
# - The lease ID, the driver's place and the record's write all happen inside one Update-LeaseState
#   script block, under the state file's lock. A place that fails throws out of the block, so the
#   state file is left untouched; the driver removes its own credential and alias before it throws.
# - The ID is a new lowercase UUIDv4, generated again while a kept lease already has its host alias
#   (agentspace-<project>-<first 8 hex of the id>).
# - Test hooks, read here and nowhere else: AGENTSPACE_SSH_DIR, when set and not empty, is passed to
#   the driver as -SshDir, and AGENTSPACE_PROXY_COMMAND as -ProxyCommand. Unset, the driver uses the
#   real ~/.ssh and its default ProxyCommand.

. (Join-Path $PSScriptRoot 'lease-driver.ps1')

function Invoke-LeaseCreate {
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$Platform,
    [Parameter(Mandatory)][string]$Profile,
    [Parameter(Mandatory)][string]$Size,
    [Parameter(Mandatory)][int]$TtlSeconds
  )
  # The script block sees Update-LeaseState's locals (D8), so these names are specific to this file.
  $__createProject = $Project
  $__createPlatform = $Platform
  $__createProfile = $Profile
  $__createSize = $Size
  $__createTtl = $TtlSeconds
  Update-LeaseState -Script {
    param($__createRecords)
    do {
      $__createId = [guid]::NewGuid().ToString()
      $__createAlias = "agentspace-$__createProject-$($__createId.Substring(0, 8))"
      $__createTaken = $false
      foreach ($__createOther in $__createRecords) {
        if ("agentspace-$($__createOther['project'])-$(([string]$__createOther['id']).Substring(0, 8))" -ceq $__createAlias) { $__createTaken = $true }
      }
    } while ($__createTaken)

    $__createArgs = @{
      LeaseId = $__createId; Project = $__createProject; Platform = $__createPlatform
      Profile = $__createProfile; Size = $__createSize; Ttl = [TimeSpan]::FromSeconds($__createTtl)
    }
    if ($env:AGENTSPACE_SSH_DIR) { $__createArgs['SshDir'] = $env:AGENTSPACE_SSH_DIR }
    if ($env:AGENTSPACE_PROXY_COMMAND) { $__createArgs['ProxyCommand'] = $env:AGENTSPACE_PROXY_COMMAND }
    $__createPlaced = Invoke-LeaseDriverPlace @__createArgs

    $__createRecord = [ordered]@{
      id          = $__createId
      project     = $__createProject
      state       = [string]$__createPlaced.State
      platform    = $__createPlatform
      profile     = $__createProfile
      size        = $__createSize
      ttl_seconds = $__createTtl
      created_at  = ConvertTo-LeaseTime (Get-LeaseNow)
    }
    if ($__createPlaced.State -ceq 'ready') {
      $__createRecord['expires_at'] = ConvertTo-LeaseTime $__createPlaced.ExpiresAt
      $__createRecord['ssh'] = [ordered]@{ host_alias = [string]$__createPlaced.HostAlias; user = [string]$__createPlaced.User }
    } elseif ($__createPlaced.State -ceq 'unsatisfiable') {
      $__createRecord['reasons'] = @($__createPlaced.Reasons | ForEach-Object { [ordered]@{ code = [string]$_.code; message = [string]$_.message } })
    }
    $__createRecord['driver'] = [string]$__createPlaced.Driver
    $__createRecords.Add($__createRecord)
    ConvertTo-LeaseOutput -Record $__createRecord
  }
}
