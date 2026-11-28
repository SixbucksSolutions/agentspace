$h = (& (Join-Path (Get-Location).Path 'target\dist\agentspace.exe') lease create --help 2>&1 | Out-String)
"length: $($h.Length)"
"has --platform: $($h.Contains('--platform'))"
"has windows/amd64: $($h.Contains('windows/amd64'))"
"has linux/amd64: $($h.Contains('linux/amd64'))"
"has --board: $($h.Contains('--board'))"
