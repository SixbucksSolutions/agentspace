# Step 330 of E0.12-T05-S02, after Terry's desktop-app session (pin P2: the agent there adds one comment to #0267 with fixed text): read
# #0267's comments back from the laptop as Bot and print only the count of comments and whether one holds the fixed text. Run through
# scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# - The credential is loaded the way C:\Projects\claude-localswim-watcher\lib\env.ps1's Set-LocalswimEnv does it, for this one run, then
#   Clear-LocalswimEnv in a finally. The token is never printed and never an argument; the CLI reads it from the process environment.
# - The CLI is C:\Projects\localswim\localswim.exe by its full path: `localswim comment list --limit 200 <board id> 267`.
# - The fixed text is the one line of target\t05\s02-p2-fixed-text.txt, which the dev lead writes when pin P2 settles. The step STOPs
#   without it. The text is compared, never printed. S02_ENVLIB names a stand-in loader in a dry run.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '330' 'after the desktop-app session: #0267''s comments read back as Bot, count and fixed text only')) { exit 5 }
$fixedFile = if ($env:S02_P2_TEXT_FILE) { [IO.Path]::GetFullPath($env:S02_P2_TEXT_FILE) } else { Join-Path $PSScriptRoot 's02-p2-fixed-text.txt' }
if (-not (Test-Path -LiteralPath $fixedFile -PathType Leaf)) { Stop-S02Step 'target\t05\s02-p2-fixed-text.txt is missing; pin P2 has not settled the fixed text.'; exit 5 }
$fixed = ([IO.File]::ReadAllText($fixedFile, [Text.UTF8Encoding]::new($false)) -replace '\r?\n$', '')
if ([string]::IsNullOrWhiteSpace($fixed)) { Stop-S02Step 'target\t05\s02-p2-fixed-text.txt is empty.'; exit 5 }

function Get-S02Leaves($Node) {
  if ($null -eq $Node) { return }
  if ($Node -is [string]) { $Node; return }
  if ($Node -is [System.Collections.IDictionary]) { foreach ($k in $Node.Keys) { Get-S02Leaves $Node[$k] }; return }
  if ($Node -is [System.Collections.IEnumerable]) { foreach ($i in $Node) { Get-S02Leaves $i }; return }
}

exit (Invoke-S02Main {
    $board = $null
    $agents = [IO.File]::ReadAllText((Join-Path $script:S02Repo 'AGENTS.md'))
    $i = $agents.IndexOf("`n## The board")
    if ($i -ge 0) { $m = [regex]::Match($agents.Substring($i), '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'); if ($m.Success) { $board = $m.Value } }
    if ($null -eq $board) { Add-S02Finding 'AGENTS.md, "The board", names no board ID.'; return }
    $lib = if ($env:S02_ENVLIB) { [IO.Path]::GetFullPath($env:S02_ENVLIB) } else { 'C:\Projects\claude-localswim-watcher\lib\env.ps1' }
    $loaded = $false
    $run = $null
    try {
      . $lib
      Set-LocalswimEnv
      $loaded = $true
      $run = Invoke-S02Tool -Tool 'localswim' -Arguments @('comment', 'list', '--limit', '200', $board, '267') -TimeoutSeconds 120
    } finally {
      if ($loaded -or (Get-Command Clear-LocalswimEnv -ErrorAction SilentlyContinue)) { Clear-LocalswimEnv }
    }
    Assert-S02 $loaded 'The Bot credential was loaded for this one run.' 'The Bot credential could not be loaded.'
    if ($null -eq $run) { return }
    Write-S02Line "localswim comment list exit: $($run.ExitCode); the output is $($run.Stdout.Length) character(s); the token left in this process: $(-not [string]::IsNullOrEmpty($env:LOCALSWIM_TOKEN))"
    Assert-S02 ($run.ExitCode -eq 0 -and -not $run.TimedOut) 'The read exited 0.' "The read exited $($run.ExitCode)."
    Assert-S02 ([string]::IsNullOrEmpty($env:LOCALSWIM_TOKEN)) 'LOCALSWIM_TOKEN was cleared after the read.' 'LOCALSWIM_TOKEN is still set after the read.'
    $doc = ConvertFrom-S02Json $run.Stdout
    if ($null -eq $doc) { Add-S02Finding 'The CLI''s output is not JSON, so the comments cannot be counted.'; return }
    $list = $null
    if ($doc -is [System.Collections.IDictionary]) {
      foreach ($name in 'comments', 'items', 'results') { if ($doc.ContainsKey($name) -and $doc[$name] -is [System.Collections.IEnumerable] -and $doc[$name] -isnot [string]) { $list = @($doc[$name]); break } }
      if ($null -eq $list) { foreach ($k in $doc.Keys) { if ($doc[$k] -is [System.Collections.IEnumerable] -and $doc[$k] -isnot [string] -and $doc[$k] -isnot [System.Collections.IDictionary]) { $list = @($doc[$k]); break } } }
    } elseif ($doc -is [System.Collections.IEnumerable]) { $list = @($doc) }
    if ($null -eq $list) { Add-S02Finding 'No list of comments was found in the CLI''s output.'; return }
    $withText = 0
    foreach ($c in $list) { if (@(Get-S02Leaves $c | Where-Object { $_.Contains($fixed) }).Count -gt 0) { $withText++ } }
    Write-S02Line "Comments on #0267: $($list.Count)"
    Write-S02Line "Comments holding the fixed text: $withText"
    Assert-S02 ($list.Count -ge 1) '#0267 has at least one comment.' '#0267 has no comment.'
    Assert-S02 ($withText -ge 1) 'A comment holds the fixed text.' 'No comment holds the fixed text.'
    if ($withText -gt 1) { Write-S02Line "NOTE: $withText comments hold the fixed text; one was expected." }
  })
