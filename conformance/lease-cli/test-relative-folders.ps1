#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 4): a relative state or
# ssh folder is made a full path before it is used or written into a file. It enforces:
#   - docs/state.md, "The home folder": a relative value of `AGENTSPACE_STATE_DIR` or
#     `AGENTSPACE_SSH_DIR` is resolved against the process's working folder, and an implementation
#     MUST make it a full path before it writes it into a file, because the program that later reads
#     that file runs in another working folder.
#   - docs/state.md, "The alias file": the key's and the certificate's paths are full paths, in the
#     form the spec gives, whatever form the state and ssh folders were given in.
#   - docs/state.md, "Requirements the prototype does not meet yet" (every path written into a file
#     is a full path).
# The run is started in a working folder inside the throwaway home, with both variables given as
# relative names, so that they resolve under the home and the harness's rule that a state or ssh
# folder is under the home holds. Then a second working folder, also in the home, is used to show
# that the same relative names mean another folder there, and that the full paths of the first folders
# find the lease from anywhere.
#
# - NOT SHOWN, and why. A relative `USERPROFILE` cannot be given: the harness refuses an override of
#   USERPROFILE, HOME, LOCALAPPDATA and APPDATA (docs/conformance.md, "The harness"), and this file
#   does not work around that. So the home folder written into the alias file, the one place the
#   lease CLI writes a home-relative path, is seen only in its full form. A holder's launch request,
#   the other file the spec names, is the keeper's, and the keeper's checks are in conformance/keeper/.
# - The working folder is set with [Environment]::CurrentDirectory and put back in a `finally`, before
#   the home is removed. The children inherit it. Every write calls Assert-ConformancePath at the
#   call, with no program run between the call and the write. The check deletes nothing. A lease ID or
#   alias read from the program's output names a file only after the document passed its schema
#   (Get-CcLeaseDir, Get-CcAliasPath). A failure reason never quotes the program's output.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process.
#   It reads no real folder, loads no prototype code and names no prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-relative-folders.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#the-home-folder'
$savedCwd = [Environment]::CurrentDirectory
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: A relative state folder and ssh folder are resolved against the working folder and written as full paths" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $cwdA = Join-Path $lab.Root 'cwd-a'
    $cwdB = Join-Path $lab.Root 'cwd-b'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $cwdA) | Out-Null
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $cwdB) | Out-Null
    $relEnv = @{ AGENTSPACE_STATE_DIR = 'rel-state'; AGENTSPACE_SSH_DIR = 'rel-ssh' }
    $fullA = @{ AGENTSPACE_STATE_DIR = (Join-Path $cwdA 'rel-state'); AGENTSPACE_SSH_DIR = (Join-Path $cwdA 'rel-ssh') }

    [Environment]::CurrentDirectory = $cwdA
    $l = New-CcReadyLease -Lab $lab -Project 'relfold' -EnvVars $relEnv
    if (-not $l.Ok) {
      Write-ConformanceResult -Name "$sec`: Setup, a ready create with relative state and ssh folders" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates"
    } else {
      $id = [string]$l.Doc['id']
      $alias = [string]$l.Doc['ssh']['host_alias']
      $stateA = Join-Path $cwdA 'rel-state\leases.json'
      $aliasFile = Join-Path $cwdA "rel-ssh\agentspace.d\$alias"
      $aliasFile = Assert-ConformancePath -ConformanceHome $lab -Path $aliasFile
      [void](Get-CcLeaseDir $lab $id)

      $p = [System.Collections.Generic.List[string]]::new()
      if (-not (Test-Path -LiteralPath $stateA -PathType Leaf)) { $p.Add('There is no state file in the state folder resolved against the working folder') }
      if (-not (Test-Path -LiteralPath $aliasFile -PathType Leaf)) { $p.Add('There is no alias file in the ssh folder resolved against the working folder') }
      if (-not (Test-Path -LiteralPath (Join-Path $cwdA 'rel-ssh\config') -PathType Leaf)) { $p.Add('There is no config in the ssh folder resolved against the working folder') }
      if (Test-Path -LiteralPath (Join-Path $lab.StateDir 'leases.json')) { $p.Add('A state file was written to the harness''s own state folder') }
      Complete-ConformanceCheck -Name "$sec`: A relative AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR are resolved against the working folder: the state file, the alias file and the config are there" -Problems $p

      # The alias file holds the key's and the certificate's full paths, written as state.md says.
      $p = [System.Collections.Generic.List[string]]::new()
      if (Test-Path -LiteralPath $aliasFile -PathType Leaf) {
        $text = [IO.File]::ReadAllText($aliasFile)
        $homeText = ($lab.UserProfile.TrimEnd('\', '/') -replace '\\', '/') -replace '%', '%%'
        foreach ($f in @(@('IdentityFile', 'id_ed25519'), @('CertificateFile', 'id_ed25519-cert.pub'))) {
          $want = "  $($f[0]) `"$homeText/.agentspace/leases/$id/$($f[1])`""
          if (-not $text.Contains("`n$want`n") -and -not $text.Contains("`n$want`r`n")) { $p.Add("The $($f[0]) line is not the full path in the form state.md gives") }
        }
      } else { $p.Add('There is no alias file to read') }
      Complete-ConformanceCheck -Name "$sec`: The alias file's IdentityFile and CertificateFile are full paths in the form state.md gives" -Problems $p

      # From another working folder, the full paths of the first folders find the lease, and release
      # removes the alias from the folder named, with no relative path left to resolve wrongly.
      [Environment]::CurrentDirectory = $cwdB
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', $id) -EnvVars $fullA
      Complete-ConformanceCheck -Name "$sec`: From another working folder, get with the full path of the state folder finds the lease" -Problems $(if ((Test-CcSuccess $run) -and $run.Json['id'] -ceq $id) { $null } else { [System.Collections.Generic.List[string]]@('get did not print the lease') })

      # The same relative names mean a folder of this working folder, which holds no lease.
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', $id) -EnvVars $relEnv
      Complete-ConformanceCheck -Name "$sec`: From another working folder, the same relative state folder is that folder's own, and holds no lease: get is not_found" -Problems (Get-HardeningErrorProblems -Run $run -Code 'not_found' -LeaseId $id)

      $run = Invoke-CcLease -Lab $lab -Arguments @('release', $id) -EnvVars $fullA
      $p = [System.Collections.Generic.List[string]]::new()
      if (-not ((Test-CcSuccess $run) -and $run.Json['state'] -ceq 'released')) { $p.Add('release did not print the lease released') }
      if (Test-Path -LiteralPath $aliasFile) { $p.Add('The alias file is still in the ssh folder named by its full path') }
      Complete-ConformanceCheck -Name "$sec`: From another working folder, release with the full path of the ssh folder removes the alias file there" -Problems $p
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-relative-folders.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  [Environment]::CurrentDirectory = $savedCwd
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun
