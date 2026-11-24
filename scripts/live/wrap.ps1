# The live kit's wrapper (docs/architect.md, "Live checks on Terry's laptop and the box"). Process
# tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage:
#   pwsh -NoProfile -File scripts\live\wrap.ps1 -Out <folder> -Name <nnn-what> -Command '<PowerShell>'
#   pwsh -NoProfile -File scripts\live\wrap.ps1 -Out <folder> -Name <nnn-what> -Script <file.ps1>
#
# It runs one command in a child pwsh with its streams to files under the numbered prefix, in this
# order. A name that already has any of these files is refused; a run picks the next number, and a
# letter suffix (221a, 221b) or a counter (401-status-0001) keeps a group's names apart:
#   <Name>.start   First: the start time, ISO 8601 UTC and the laptop's local time.
#   <Name>.cmd.ps1 The command as run (-Command only). It MAY hold a lease ID, so it stays raw.
#   <Name>.out     The command's stdout, raw.
#   <Name>.err     The command's stderr, raw.
#   <Name>.masked.txt  stdout then stderr, through scripts\live\mask.ps1, the one file a reader prints.
#   <Name>.exit    Last: `exit=<code>`, `end_utc=<ISO 8601>`, `end_local=<h:mm:ssam>`.
#
# The raw files MUST stay inside the run's folder, which MUST be under the worktree's target\, never
# %TEMP%. A reader MUST print only .exit and .masked.txt files. A waiter MUST wait on .exit, which is
# written last, never on the command: a subagent's shell moves a command of about 3 minutes or more to
# the background (v0.08-T06).
#
# The exit code is the command's: a native program's nonzero $LASTEXITCODE wins, then a failed last
# statement gives 1, else 0. The wrapper's own exit code is the same.
param(
  [Parameter(Mandatory)][string]$Out,
  [Parameter(Mandatory)][string]$Name,
  [string]$Command,
  [string]$Script
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrEmpty($Command) -eq [string]::IsNullOrEmpty($Script)) {
  throw 'Give exactly one of -Command and -Script.'
}
if ($Name -notmatch '^\d{3}[A-Za-z0-9._-]*$') {
  throw 'The name MUST start with three digits, then letters, digits, dots, underscores or dashes.'
}
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$base = Join-Path (Resolve-Path -LiteralPath $Out).Path $Name
foreach ($suffix in '.start', '.out', '.err', '.masked.txt', '.exit') {
  if (Test-Path -LiteralPath ($base + $suffix)) {
    throw "Wrapper $Name already has a $suffix file; pick another name."
  }
}

function Get-WrapLocalTime {
  [TimeZoneInfo]::ClearCachedData()
  $now = Get-Date
  return $now.ToString('h:mm:ss', [Globalization.CultureInfo]::InvariantCulture) + $now.ToString('tt', [Globalization.CultureInfo]::InvariantCulture).ToLowerInvariant()
}

$start = [DateTime]::UtcNow
Set-Content -LiteralPath ($base + '.start') -Encoding utf8 -Value @(
  "start_utc=$($start.ToString('yyyy-MM-ddTHH:mm:ssZ'))",
  "start_local=$(Get-WrapLocalTime)"
)

if ($Command) {
  $runFile = $base + '.cmd.ps1'
  Set-Content -LiteralPath $runFile -Encoding utf8 -Value @(
    '$ErrorActionPreference = ''Continue''',
    '$global:LASTEXITCODE = 0',
    $Command,
    '$wrapOk = $?',
    'if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }',
    'if (-not $wrapOk) { exit 1 }',
    'exit 0'
  )
}
else {
  $runFile = (Resolve-Path -LiteralPath $Script).Path
}

$code = 1
try {
  & pwsh -NoProfile -NonInteractive -File $runFile 1> ($base + '.out') 2> ($base + '.err')
  $code = $LASTEXITCODE
}
catch {
  Add-Content -LiteralPath ($base + '.err') -Value ('Wrapper: ' + $_.Exception.Message)
  $code = 1
}

try {
  . (Join-Path $PSScriptRoot 'mask.ps1')
  $outText = if (Test-Path -LiteralPath ($base + '.out')) { [IO.File]::ReadAllText($base + '.out') } else { '' }
  $errText = if (Test-Path -LiteralPath ($base + '.err')) { [IO.File]::ReadAllText($base + '.err') } else { '' }
  $masked = (ConvertTo-LiveMasked -Text $outText)
  if ($errText) { $masked += "`n--- stderr ---`n" + (ConvertTo-LiveMasked -Text $errText) }
  Set-Content -LiteralPath ($base + '.masked.txt') -Encoding utf8 -Value $masked
}
catch {
  Set-Content -LiteralPath ($base + '.masked.txt') -Encoding utf8 -Value 'Masking failed; read nothing from this wrapper but its .exit file.'
}

$end = [DateTime]::UtcNow
Set-Content -LiteralPath ($base + '.exit') -Encoding utf8 -Value @(
  "exit=$code",
  "end_utc=$($end.ToString('yyyy-MM-ddTHH:mm:ssZ'))",
  "end_local=$(Get-WrapLocalTime)"
)
exit $code
