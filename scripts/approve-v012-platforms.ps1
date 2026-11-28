# v0.12's one write to Terry's project list (docs/state.md, "The project list", "Who writes it": at
# v0.12's install, agentspace's entry gains `platforms`; Q21). Terry runs this himself, since auto mode
# refused the write in v0.12-T05. It prints counts and yes/no answers only, never another project's
# name. Process tool, not product (D28). Tested on a throwaway home by the architect, 2026-11-27: the
# first run gave state.md's example bytes, and a second run refused with exit 4.
#
# Run: pwsh -NoProfile -File scripts\approve-v012-platforms.ps1
#
# What it does, and nothing else:
#   1. Reads %USERPROFILE%\.agentspace\projects.json and refuses (exit 4, nothing written) unless it
#      is version 0 with exactly one `agentspace` entry that has no `platforms` member.
#   2. Copies the file to projects.json.before-v0.12 beside it, once (refuses if that copy exists).
#   3. Gives agentspace's entry `platforms`: linux/arm64 on the entry's own `approved`, then
#      linux/amd64 and windows/amd64 on 2026-11-26. Every other entry and member is kept as it is.
#   4. Writes compact JSON plus one LF, UTF-8 without a BOM, to a temp file in the same folder, then
#      moves it over projects.json.
#   5. Reads it back and checks every other entry is unchanged and agentspace's is exactly as meant.
#      If a check fails it puts the saved copy back and exits 5.
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:USERPROFILE '.agentspace'
$file = Join-Path $dir 'projects.json'
$copy = Join-Path $dir 'projects.json.before-v0.12'

$doc = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json -AsHashtable -DateKind String
$mine = @($doc.projects | Where-Object { $_.name -ceq 'agentspace' })
if ($doc.version -ne 0 -or $mine.Count -ne 1 -or $mine[0].ContainsKey('platforms')) {
  Write-Output "Refused: version $($doc.version), agentspace entries $($mine.Count), already has platforms: $(if ($mine.Count -eq 1) { $mine[0].ContainsKey('platforms') } else { 'n/a' }). Nothing written."
  exit 4
}
if (Test-Path -LiteralPath $copy) { Write-Output 'Refused: projects.json.before-v0.12 already exists. Nothing written.'; exit 4 }

$before = @($doc.projects | ForEach-Object { if ($_.name -cne 'agentspace') { $_ | ConvertTo-Json -Compress -Depth 5 } })
Copy-Item -LiteralPath $file -Destination $copy

$approved = $mine[0].approved
$entries = foreach ($p in $doc.projects) {
  if ($p.name -ceq 'agentspace') {
    [ordered]@{
      name = $p.name
      approved = $approved
      platforms = @(
        [ordered]@{ platform = 'linux/arm64'; approved = $approved },
        [ordered]@{ platform = 'linux/amd64'; approved = '2026-11-26' },
        [ordered]@{ platform = 'windows/amd64'; approved = '2026-11-26' }
      )
    }
  }
  else { $p }
}
$json = ([ordered]@{ version = 0; projects = @($entries) } | ConvertTo-Json -Compress -Depth 6) + "`n"
$temp = Join-Path $dir ('projects.json.tmp-' + [guid]::NewGuid().ToString('N'))
[IO.File]::WriteAllText($temp, $json, [Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $temp -Destination $file -Force

$check = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json -AsHashtable -DateKind String
$after = @($check.projects | ForEach-Object { if ($_.name -cne 'agentspace') { $_ | ConvertTo-Json -Compress -Depth 5 } })
$a = @($check.projects | Where-Object { $_.name -ceq 'agentspace' })
$othersSame = ($before -join "`n") -ceq ($after -join "`n")
$mineRight = $a.Count -eq 1 -and $a[0].approved -ceq $approved -and (($a[0].platforms | ForEach-Object { "$($_.platform)@$($_.approved)" }) -join ',') -ceq "linux/arm64@$approved,linux/amd64@2026-11-26,windows/amd64@2026-11-26"
$endsLf = [IO.File]::ReadAllBytes($file)[-1] -eq 10
if (-not ($othersSame -and $mineRight -and $endsLf)) {
  Copy-Item -LiteralPath $copy -Destination $file -Force
  Write-Output "Check failed (others unchanged: $othersSame, agentspace right: $mineRight, ends in LF: $endsLf). The saved copy was put back."
  exit 5
}
Write-Output "Done. Entries: $($check.projects.Count). Other entries unchanged: $othersSame. agentspace platforms: linux/arm64, linux/amd64, windows/amd64. Saved copy: projects.json.before-v0.12."
exit 0
