# The dry runs' stand-in for aws: sts get-caller-identity (step 201) and ec2 describe-instances (step 240).
# RFC 2119 keywords; the capitals are load-bearing. S01_PLANT names ONE planted answer. The identifiers are made up.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 's01-standin-lib.ps1')
$plant = Get-SlPlant
$argv = @($args | ForEach-Object { [string]$_ })
if ($argv.Count -ge 2 -and $argv[0] -ceq 'sts' -and $argv[1] -ceq 'get-caller-identity') {
  if ($plant -ceq 'sts-expired') { [Console]::Error.WriteLine('Error when retrieving token from sso: Token has expired and refresh failed'); exit 255 }
  # The real one prints the caller's IDs: a planted shape the step MUST NOT print (its output is discarded).
  [Console]::Out.WriteLine('{"UserId":"' + 'AIDA' + 'PLANTEDUSERID01234","Account":"' + '123456' + '789012","Arn":"' + 'ar' + 'n:aws:iam::' + '123456' + '789012:user/planted"}')
  exit 0
}
if ($argv.Count -ge 2 -and $argv[0] -ceq 'ec2' -and $argv[1] -ceq 'describe-instances') {
  if ($plant -ceq 'ec2-fail') { [Console]::Error.WriteLine('An error occurred (ExpiredToken) when calling the DescribeInstances operation'); exit 255 }
  $filterText = ($argv -join ' ')
  $byStack = $filterText.Contains('aws:cloudformation:stack-name')
  $hasQuery = $filterText.Contains('--query') -and $filterText.Contains('State.Name') -and $filterText.Contains('StateReason.Code')
  if (-not $hasQuery) { [Console]::Error.WriteLine('stand-in: the query is not the read-only State.Name and StateReason.Code one'); exit 255 }
  if (-not $filterText.Contains('agentspace-linux-amd64-host-agentspace')) { [Console]::Error.WriteLine('stand-in: the filter does not name the stack'); exit 255 }
  switch ($plant) {
    'ec2-empty' { exit 0 }
    'ec2-second-form' { if ($byStack) { exit 0 } else { [Console]::Out.WriteLine("stopped`tClient.UserInitiatedShutdown"); exit 0 } }
    'ec2-host-own' { [Console]::Out.WriteLine("stopped`tClient.InstanceInitiatedShutdown"); exit 0 }
    'ec2-running' { [Console]::Out.WriteLine("running`tNone"); exit 0 }
    'ec2-two' { [Console]::Out.WriteLine("stopped`tClient.UserInitiatedShutdown"); [Console]::Out.WriteLine("stopped`tClient.UserInitiatedShutdown"); exit 0 }
    'ec2-terminated-old' { [Console]::Out.WriteLine("terminated`tClient.UserInitiatedShutdown"); [Console]::Out.WriteLine("stopped`tClient.UserInitiatedShutdown"); exit 0 }
    'ec2-other-code' { [Console]::Out.WriteLine("stopped`tServer.SpotInstanceTermination"); exit 0 }
    default { [Console]::Out.WriteLine("stopped`tClient.UserInitiatedShutdown"); exit 0 }
  }
}
[Console]::Error.WriteLine('stand-in: no answer for this aws command')
exit 255
