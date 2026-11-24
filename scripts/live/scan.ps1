# The live kit's scan of a file for anything the mask would change. Process tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage: pwsh -NoProfile -File scripts\live\scan.ps1 -File <file>
#
# It prints, for each line the mask would change, the line's number and its masked text only, never
# the line itself, then a count. It exits 1 on any hit and 0 on none, and 2 when the file is missing.
# A clean pass MUST be believed only after the scan has exited 1 on a planted file (mask-proof.ps1
# runs it so, on a planted file and on a clean one).
#
# The file's name is held in $ScanTarget, a name mask.ps1 never sets: v0.08-T06's scan held it in
# $Path, which dot-sourcing the mask reset, and the scan passed every file.
param(
  [Parameter(Mandatory)][string]$File
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$ScanTarget = $File
. (Join-Path $PSScriptRoot 'mask.ps1')

if (-not (Test-Path -LiteralPath $ScanTarget)) {
  'Scan: the file is missing.'
  exit 2
}
$scanLines = [IO.File]::ReadAllLines((Resolve-Path -LiteralPath $ScanTarget).Path)
$scanHits = 0
for ($n = 0; $n -lt $scanLines.Count; $n++) {
  $masked = ConvertTo-LiveMasked -Text $scanLines[$n]
  if ($masked -cne $scanLines[$n]) {
    $scanHits++
    "Line $($n + 1): $masked"
  }
}
"Scan: $($scanLines.Count) lines, $scanHits hits."
if ($scanHits) { exit 1 }
exit 0
