# Stand-in for C:\Projects\claude-localswim-watcher\lib\env.ps1 in step 330's dry runs (drafter-owned): the same two functions, setting a made-up
# URL and token for the process and clearing them, and logging each call (S02_STANDIN_CALLS) so a run can count the pairs. It reads no credential.
function Set-LocalswimEnv {
  [Environment]::SetEnvironmentVariable('LOCALSWIM_URL', 'http://127.0.0.1:1', 'Process')
  [Environment]::SetEnvironmentVariable('LOCALSWIM_TOKEN', 'dryrun-token-0123456789abcdefghijklmnopqrstuv', 'Process')
  if ($env:S02_STANDIN_CALLS) { New-Item -ItemType Directory -Force -Path $env:S02_STANDIN_CALLS | Out-Null; [IO.File]::AppendAllText((Join-Path $env:S02_STANDIN_CALLS 'calls.log'), "env Set-LocalswimEnv`n") }
}
function Clear-LocalswimEnv {
  [Environment]::SetEnvironmentVariable('LOCALSWIM_TOKEN', $null, 'Process')
  [Environment]::SetEnvironmentVariable('LOCALSWIM_URL', $null, 'Process')
  if ($env:S02_STANDIN_CALLS) { New-Item -ItemType Directory -Force -Path $env:S02_STANDIN_CALLS | Out-Null; [IO.File]::AppendAllText((Join-Path $env:S02_STANDIN_CALLS 'calls.log'), "env Clear-LocalswimEnv`n") }
}
