# Stand-in for localswim.exe in E0.12-T05-S02's dry runs (drafter-owned). It answers `comment list --limit 200 <board> 267` and refuses any other call,
# and a call whose token is not in its environment or is on its command line.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
$token = $env:LOCALSWIM_TOKEN
if ([string]::IsNullOrEmpty($token) -or [string]::IsNullOrEmpty($env:LOCALSWIM_URL)) { Stop-SiRefused 'localswim' 'the credential is not in the environment' }
if (@($a | Where-Object { ([string]$_).Contains($token) }).Count -gt 0) { Stop-SiRefused 'localswim' 'the token is on the command line' }
if ($a.Count -eq 6 -and $a[0] -ceq 'comment' -and $a[1] -ceq 'list' -and $a[2] -ceq '--limit' -and $a[3] -ceq '200' -and $a[4] -cmatch '^[0-9a-f-]{36}$' -and $a[5] -ceq '267') {
  Write-SiCall 'localswim comment list board 267 (token in the environment)'
  Send-SiAnswer 'localswim' 'comment-list'
}
Stop-SiRefused 'localswim' ($a -join ' ')
