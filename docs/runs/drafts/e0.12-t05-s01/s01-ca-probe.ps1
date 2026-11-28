$root = Join-Path (Resolve-Path 'target\t05\s01-tmp').Path 'ca-probe-home'
New-Item -ItemType Directory -Force -Path (Join-Path $root 'home\.agentspace'), (Join-Path $root 'home\.ssh'), (Join-Path $root 'home\AppData\Local'), (Join-Path $root 'home\AppData\Roaming') | Out-Null
$env:USERPROFILE = Join-Path $root 'home'; $env:HOME = $env:USERPROFILE; $env:LOCALAPPDATA = Join-Path $env:USERPROFILE 'AppData\Local'; $env:APPDATA = Join-Path $env:USERPROFILE 'AppData\Roaming'
$op = Join-Path (Resolve-Path 'target\dist').Path 'agentspace-operator.exe'
& $op ca 2>&1 | ForEach-Object { "$_" }
"exit: $LASTEXITCODE"
Remove-Item -LiteralPath $root -Recurse -Force
