$ok = $false
try {
  $j = & "$env:USERPROFILE\.agentspace\bin\agentspace.exe" lease list | ConvertFrom-Json
  $ok = ($LASTEXITCODE -eq 0 -and $null -ne $j)
} catch { $ok = $false }
"installed v0.11 lease list succeeded (so no record held plan_source): $ok"
if (-not $ok) { exit 2 }
"total leases: $(@($j.leases).Count)"
$j.leases | Group-Object state | Sort-Object Name | ForEach-Object { "state $($_.Name): $($_.Count)" }
$live = @($j.leases | Where-Object { $_.state -in 'ready','pending' }).Count
"ready or pending: $live"
if ($live -gt 0) { exit 3 }
