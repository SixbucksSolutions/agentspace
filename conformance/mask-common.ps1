#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The conformance suite's one mask (card #0243; docs/conformance.md, "The shared mask"). A failure's reason
# that quotes captured text, which is a program's stdout or stderr, an exception's message, or a value read
# from a document or a file a program wrote, MUST pass that text through ConvertTo-ConformanceMasked first
# (AGENTS.md, "Terry's real files in a review"). The harness (lease-cli\harness.ps1) dot-sources this file,
# and every check of the lease CLI, the operator and the keeper dot-sources the harness, so the function is
# defined wherever a reason is written.
#
# It follows scripts\live\mask.ps1's rules 1 and 2, reimplemented here because a conformance file MUST NOT
# depend on a process tool (scripts\):
# - Rule 1: a path's prefix as the environment gives it and in its long and its 8.3 short form (TEMP is often
#   given short, so its long form is derived), longest first, in either slash direction, with doubled
#   backslashes and in Git Bash's /c/... form, without regard to case. The profile becomes %USERPROFILE%, as
#   in mask.ps1, and the temp folder %TEMP%.
# - Rule 2: the computer name, the user name and the profile's last folder, long and short, as <literal>.
# Every value is read from the environment of the case's own process when the harness loads this file (the
# last line below builds the rules then), and none is written into a file. So the harness MUST be loaded
# before a check moves its own process's USERPROFILE, HOME or TEMP, as keeper-common.ps1's New-CaseHome does
# through Set-CaseEnvironment: rules built after such a move would mask the throwaway home and not the real
# profile (architect, 2026-11-26, for v0.11-T05, F1). A text that is null or empty gives ''.
#
# A text that is cut to a length MUST be masked before it is cut, so that a cut never leaves part of a name
# unmasked.

$script:ConformanceMaskRules = $null

# The 8.3 short form of a folder, or the folder itself when the volume keeps no short names.
function Get-ConformanceShortPath([string]$LongPath) {
  try { [string](New-Object -ComObject Scripting.FileSystemObject).GetFolder($LongPath).ShortPath } catch { $LongPath }
}

# The long form of a folder written in its 8.3 short form (kernel32's GetLongPathName), or the folder itself. An
# environment's TEMP is often written short, so its long form is a spelling a program's own error text can use.
function Get-ConformanceLongPath([string]$ShortPath) {
  try {
    if (-not ('ConformanceMask.PathNative' -as [type])) {
      Add-Type -Namespace ConformanceMask -Name PathNative -MemberDefinition '[System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode)] public static extern uint GetLongPathName(string shortPath, System.Text.StringBuilder longPath, uint size);'
    }
    $sb = [Text.StringBuilder]::new(32768)
    $n = [ConformanceMask.PathNative]::GetLongPathName($ShortPath, $sb, 32768)
    if ($n -gt 0 -and $n -lt 32768) { return $sb.ToString() }
  } catch { }
  $ShortPath
}

# A regex for a path's prefix: its parts joined by any separator, and Git Bash's form.
function Get-ConformancePathPattern([string]$FullPath) {
  $parts = @($FullPath -split '[\\/]+' | Where-Object { $_ -ne '' })
  if ($parts.Count -lt 2) { return $null }
  $sep = '(?:\\\\|\\|/)'
  $escaped = @($parts | ForEach-Object { [regex]::Escape($_) })
  $windows = $escaped -join $sep
  if ($parts[0] -match '^[A-Za-z]:$') { return '(?:' + $windows + '|/' + [regex]::Escape($parts[0].TrimEnd(':')) + '/' + (($escaped | Select-Object -Skip 1) -join '/') + ')' }
  $windows
}

# The masking rules of this process: a regex and its token each, paths first and longest first, then the literals.
function Get-ConformanceMaskRules {
  if ($null -ne $script:ConformanceMaskRules) { return $script:ConformanceMaskRules }
  $rules = [System.Collections.Generic.List[object]]::new()
  $paths = [System.Collections.Generic.List[object]]::new()
  foreach ($pair in @(@($env:TEMP, '%TEMP%'), @($env:USERPROFILE, '%USERPROFILE%'))) {
    if (-not $pair[0]) { continue }
    foreach ($form in @($pair[0], (Get-ConformanceShortPath $pair[0]), (Get-ConformanceLongPath $pair[0]))) {
      if ($form) { $paths.Add([pscustomobject]@{ Path = $form; Token = $pair[1] }) }
    }
  }
  $seen = @{}
  foreach ($e in @($paths | Sort-Object -Property @{ Expression = { $_.Path.Length }; Descending = $true })) {
    $pat = Get-ConformancePathPattern $e.Path
    if (-not $pat -or $seen.ContainsKey($pat)) { continue }
    $seen[$pat] = $true
    $rules.Add([pscustomobject]@{ Regex = [regex]::new($pat, 'IgnoreCase'); Token = $e.Token })
  }
  $literals = [System.Collections.Generic.List[string]]::new()
  foreach ($v in @($env:COMPUTERNAME, $env:USERNAME)) { if ($v) { $literals.Add($v) } }
  if ($env:USERPROFILE) { foreach ($p in @($env:USERPROFILE, (Get-ConformanceShortPath $env:USERPROFILE), (Get-ConformanceLongPath $env:USERPROFILE))) { if ($p) { $leaf = Split-Path -Leaf $p; if ($leaf) { $literals.Add($leaf) } } } }
  foreach ($l in @($literals | Select-Object -Unique | Sort-Object -Property Length -Descending)) {
    $rules.Add([pscustomobject]@{ Regex = [regex]::new([regex]::Escape($l), 'IgnoreCase'); Token = '<literal>' })
  }
  $script:ConformanceMaskRules = $rules.ToArray()
  $script:ConformanceMaskRules
}

# Text with the temp folder, the profile, the computer name and the user name replaced by their tokens.
function ConvertTo-ConformanceMasked([AllowNull()][AllowEmptyString()][string]$Text) {
  if ([string]::IsNullOrEmpty($Text)) { return '' }
  $t = $Text
  foreach ($r in (Get-ConformanceMaskRules)) { $t = $r.Regex.Replace($t, $r.Token) }
  $t
}

# The rules are built now, while the process's environment is still the one the check started with.
$null = Get-ConformanceMaskRules
