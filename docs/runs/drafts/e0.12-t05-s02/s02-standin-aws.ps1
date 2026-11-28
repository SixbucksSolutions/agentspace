# Stand-in for aws in E0.12-T05-S02's dry runs (drafter-owned). It answers `sts get-caller-identity` (with a document that holds an account
# number and a user ID, which a step MUST NOT print) and the one read-only `ec2 describe-instances` of step 341, and refuses any other call.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
if ($a.Count -eq 2 -and $a[0] -ceq 'sts' -and $a[1] -ceq 'get-caller-identity') { Write-SiCall 'aws sts get-caller-identity'; Send-SiAnswer 'aws' 'sts' }
if ($a.Count -ge 2 -and $a[0] -ceq 'ec2' -and $a[1] -ceq 'describe-instances') {
  $joined = $a -join ' '
  if ($joined -cnotmatch 'Name=tag:aws:cloudformation:stack-name,Values=agentspace-windows-amd64-host-agentspace') { Stop-SiRefused 'aws' 'describe-instances not filtered by the Windows stack''s name' }
  if ($joined -cnotmatch '--query Reservations\[\]\.Instances\[\]\.\[State\.Name,StateReason\.Code\]') { Stop-SiRefused 'aws' 'describe-instances with a --query that returns more than State.Name and StateReason.Code' }
  Write-SiCall 'aws ec2 describe-instances (stack-filtered, State.Name and StateReason.Code)'
  Send-SiAnswer 'aws' 'describe'
}
Stop-SiRefused 'aws' ($a -join ' ')
