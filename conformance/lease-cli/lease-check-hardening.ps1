#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Helpers shared by the lease CLI's hardening checks (E0.06-T04-S03, #0133, carrying #0110 points 3
# and 4). A check dot-sources harness.ps1, lease-cli-common.ps1 and lease-check-support.ps1 first,
# and this file last. It starts no program, writes nothing and deletes nothing, and loads no
# prototype code. Every expectation is taken from docs/lease-api.md or docs/state.md.
# The program under test is untrusted (docs/conformance.md, "The program under test is untrusted"):
# a failure reason built here never quotes the program's output.

# The problems with a run that was meant to fail, as Get-ConformanceErrorProblems gives them, without
# the program's own text: a reason that would quote stderr, a code, a field, a lease ID or a state says
# only that the value differs.
function Get-HardeningErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string]$LeaseId = '', [string]$State = '')
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($q in (Get-ConformanceErrorProblems -Run $Run -Code $Code -Field $Field -LeaseId $LeaseId -State $State)) {
    if ($q -like 'Stderr is not a JSON document:*') { $p.Add('Stderr is not a JSON document') }
    elseif ($q -cmatch "^(Code|Field|lease_id|state) is '") { $p.Add(($q -creplace "^(\w+) is '[^']*'", '$1 is another value')) }
    elseif ($q -cmatch '^An unwanted field') { $p.Add('The error carries a field it MUST NOT') }
    else { $p.Add($q) }
  }
  return , $p
}

# The problems with a refusal of a lease ID that is not accepted: invalid_value for `id`, exit 2, no
# `lease_id` member, and a message that writes an escaped control character in lowercase hex only
# (lease-api.md, "Exit codes and errors": the schema already forbids a raw one).
function Get-HardeningIdRefusalProblems($Run) {
  $p = Get-HardeningErrorProblems -Run $Run -Code 'invalid_value' -Field 'id'
  $doc = $Run.ErrorJson
  if ($doc -is [hashtable]) {
    if ($doc.ContainsKey('lease_id')) { $p.Add('The error carries a lease_id, which only not_found and invalid_state carry') }
    if ([string]$doc['message'] -cmatch '\\u[0-9a-f]*[A-F]') { $p.Add('The message writes an escaped control character with uppercase hex digits') }
  }
  return , $p
}

# Every file and folder under the home's .agentspace and .ssh, as a relative path to a SHA-256 (a file)
# or to 'dir' (a folder), leaving out the CA (checked on its own) and leases.lock, an empty file that any
# lease command may make (docs/state.md, "The lease state lock"). Two maps from before and after a
# command show whether it changed anything on disk. A junction or a symbolic link is recorded as
# 'link' and is never hashed or descended into: what it points at may be outside the throwaway home,
# and the program under test made it.
function Get-HardeningFileMap {
  param([Parameter(Mandatory)]$Lab)
  $map = @{}
  $base = [IO.Path]::GetFullPath($Lab.UserProfile).TrimEnd('\') + '\'
  $walk = [System.Collections.Generic.Stack[string]]::new()
  foreach ($top in '.agentspace', '.ssh') {
    $dir = Join-Path $Lab.UserProfile $top
    if (Test-Path -LiteralPath $dir -PathType Container) { $walk.Push($dir) }
  }
  while ($walk.Count -gt 0) {
    $dir = $walk.Pop()
    foreach ($f in Get-ChildItem -LiteralPath $dir -Force) {
      $rel = $f.FullName.Substring($base.Length)
      if ($rel -like '.agentspace\ca\*' -or $rel -ieq '.agentspace\ca' -or $rel -ieq '.agentspace\state\leases.lock') { continue }
      if (($f.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { $map[$rel] = 'link'; continue }
      if ($f.PSIsContainer) { $map[$rel] = 'dir'; $walk.Push($f.FullName) }
      else { $map[$rel] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash }
    }
  }
  return $map
}

# What differs between two maps, as counts by category and never as a name the program made: a
# reason that quotes a path would quote the untrusted program (docs/conformance.md, "The program
# under test is untrusted"). The category is the home-relative folder the entry is under, when the
# spec names that folder or file, and 'another place' when it does not.
function Get-HardeningMapDifferences {
  param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After)
  $known = @('.agentspace\leases', '.agentspace\state', '.agentspace\instance-id.json', '.ssh\agentspace.d', '.ssh\config')
  $category = {
    param([string]$Rel)
    $parts = $Rel -split '\\'
    $two = if ($parts.Count -ge 2) { "$($parts[0])\$($parts[1])" } else { $Rel }
    if ($known -contains $two) { $two } else { 'another place' }
  }
  $counts = [ordered]@{}
  $add = { param([string]$What, [string]$Rel) $key = "$What under $(& $category $Rel)"; $counts[$key] = 1 + [int]$counts[$key] }
  foreach ($k in $Before.Keys) {
    if (-not $After.ContainsKey($k)) { & $add 'gone' $k } elseif ($After[$k] -ne $Before[$k]) { & $add 'changed' $k }
  }
  foreach ($k in $After.Keys) { if (-not $Before.ContainsKey($k)) { & $add 'new' $k } }
  $d = [System.Collections.Generic.List[string]]::new()
  foreach ($key in $counts.Keys) { $d.Add("Entries $key`: $($counts[$key])") }
  return , $d
}
