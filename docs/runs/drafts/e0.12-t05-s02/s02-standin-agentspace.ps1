# Stand-in for agentspace.exe in E0.12-T05-S02's dry runs (drafter-owned). It answers `lease create`, `get`, `list` and `release` from its
# answers folder (s02-standin-lib.ps1), and refuses a `create` that is not --platform windows/amd64 --project agentspace --ttl <d> alone.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
$uuid = '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
if ($a.Count -ge 2 -and $a[0] -ceq 'lease') {
  switch ($a[1]) {
    'create' {
      $rest = @($a | Select-Object -Skip 2)
      $pairs = @{}
      for ($i = 0; $i -lt $rest.Count; $i += 2) {
        if ($rest[$i] -notin @('--platform', '--project', '--ttl') -or $i + 1 -ge $rest.Count) { Stop-SiRefused 'agentspace' "create with an argument it must not send: $($rest[$i])" }
        $pairs[$rest[$i]] = $rest[$i + 1]
      }
      if ($pairs['--platform'] -cne 'windows/amd64' -or $pairs['--project'] -cne 'agentspace' -or -not $pairs.ContainsKey('--ttl')) { Stop-SiRefused 'agentspace' 'create without --platform windows/amd64 --project agentspace --ttl' }
      Write-SiCall "agentspace lease create platform=$($pairs['--platform']) project=$($pairs['--project']) ttl=$($pairs['--ttl'])"
      Send-SiAnswer 'agentspace' 'lease-create'
    }
    'get' {
      if ($a.Count -ne 3 -or $a[2] -cnotmatch $uuid) { Stop-SiRefused 'agentspace' 'get without one lowercase lease ID' }
      Write-SiCall 'agentspace lease get <id>'
      Send-SiAnswer 'agentspace' 'lease-get'
    }
    'list' {
      if ($a.Count -ne 2) { Stop-SiRefused 'agentspace' 'list with arguments' }
      Write-SiCall 'agentspace lease list'
      Send-SiAnswer 'agentspace' 'lease-list'
    }
    'release' {
      if ($a.Count -ne 3 -or $a[2] -cnotmatch $uuid) { Stop-SiRefused 'agentspace' 'release without one lowercase lease ID' }
      Write-SiCall 'agentspace lease release <id>'
      Send-SiAnswer 'agentspace' 'lease-release'
    }
  }
}
Stop-SiUnknown 'agentspace' ($a -join ' ')
