# The architect's identifier scan over a pull request (docs/architect.md, "Reviewing a pull
# request", step 6). Process tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage: pwsh -NoProfile -File scripts\scan-identifiers.ps1 -Head <ref> [-Base main] [-Pr <n>]
#   -Head   The reviewed head, such as refs/review/pr<n> after `git fetch origin pull/<n>/head:refs/review/pr<n>`.
#   -Base   The branch the PR merges into, `main` by default. Pass the previous reviewed head to scan
#           only the commits added since.
#   -Pr     The PR's number, to scan its body as well.
#
# It scans the added lines of the diff, every commit message and the PR body. Each pattern is first
# shown to match a planted value, and the run stops if one does not. It prints only the pattern's name
# and the number of distinct hits, never a value, so its output MAY be pasted anywhere. Every hit MUST
# then be looked at, and each SHOULD be a planted or documentation value the PR names. AWS's example
# instance IDs and an all-zero ID are not counted.
param(
  [Parameter(Mandatory)][string]$Head,
  [string]$Base = 'main',
  [int]$Pr
)
$ErrorActionPreference = 'Stop'

$patterns = [ordered]@{
  'instance id'   = 'i-[0-9a-f]{8,17}\b'
  '12 digits'     = '(?<![0-9a-f-])\d{12}(?![0-9a-f-])'
  'private ip'    = '\b(10\.\d+\.\d+\.\d+|172\.(1[6-9]|2\d|3[01])\.\d+\.\d+|192\.168\.\d+\.\d+)\b'
  'ip host name'  = '\bip-\d+-\d+-\d+-\d+\b'
  'vpc'           = '\bvpc-[0-9a-f]{8,}'
  'subnet'        = '\bsubnet-[0-9a-f]{8,}'
  'vol'           = '\bvol-[0-9a-f]{8,}'
  'sg'            = '\bsg-[0-9a-f]{8,}'
  'arn'           = 'arn:aws'
  'token assign'  = '(?i)(token|secret|password)\s*[:=]\s*[''"]?[A-Za-z0-9_\-]{16,}'
  'key text'      = '-----BEGIN [A-Z ]*PRIVATE KEY-----|ssh-(ed25519|rsa) AAAA[0-9A-Za-z+/]{20,}'
  'uuid'          = '\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b'
  'lease alias'   = '\bagentspace-[a-z0-9-]+-[0-9a-f]{8}\b'
  'computer name' = '(?i)\b' + [regex]::Escape($env:COMPUTERNAME) + '\b'
  'user name'     = '(?i)\b' + [regex]::Escape($env:USERNAME) + '\b'
}
$planted = @{
  'instance id' = 'i-0abc1234def567890'; '12 digits' = 'acct 123456789012 here'; 'private ip' = '10.1.2.3'
  'ip host name' = 'ip-10-1-2-3'; 'vpc' = 'vpc-0abc1234'; 'subnet' = 'subnet-0abc1234'; 'vol' = 'vol-0abc1234'
  'sg' = 'sg-0abc1234'; 'arn' = 'arn:aws:iam::x'; 'token assign' = 'token = "abcdefghijklmnopqrstu"'
  'key text' = '-----BEGIN OPENSSH PRIVATE KEY-----'; 'uuid' = '3a9c5e21-6b7d-4c8e-9f01-2d3e4f5a6b7c'
  'lease alias' = 'agentspace-swimcoach-3a9c5e21'
  'computer name' = "x $env:COMPUTERNAME x"; 'user name' = "x $env:USERNAME x"
}
foreach ($k in $patterns.Keys) {
  if ($planted[$k] -notmatch $patterns[$k]) { throw "Pattern '$k' misses its planted value." }
}
'Every pattern matched its planted value.'

$texts = [ordered]@{}
$texts['diff'] = (git diff "$Base...$Head" | Where-Object { $_ -like '+*' -and $_ -notlike '+++*' }) -join "`n"
$texts['commits'] = (git log --format='%B' "$Base..$Head") -join "`n"
if ($Pr) { $texts['pr body'] = (gh pr view $Pr --json body -q .body) }
$allowed = '^(i-0123456789abcdef0|i-0fedcba9876543210|i-0+)$'
$total = 0
foreach ($t in $texts.Keys) {
  foreach ($k in $patterns.Keys) {
    $hits = @([regex]::Matches([string]$texts[$t], $patterns[$k]) | ForEach-Object Value |
      Where-Object { $_ -notmatch $allowed } | Sort-Object -Unique)
    if ($hits.Count) { "${t}: $k, $($hits.Count) distinct"; $total += $hits.Count }
  }
}
"Scan done: $total distinct hits to look at."
