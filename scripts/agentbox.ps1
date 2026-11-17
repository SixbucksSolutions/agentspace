<#
.SYNOPSIS
  Manages the agentbox EC2 instance from the laptop.

.DESCRIPTION
  deploy  Creates or updates the CloudFormation stack, then runs sync.
  up      Starts the instance if needed and waits until it has finished patching.
  sync    Installs box/system (idle check, patch at boot) on the instance, and copies
          ~/.claude/CLAUDE.md there.
  down    Stops the instance.
  status  Shows state, type, disk size and Session Manager reachability.
  grow    Grows the root volume by -GiB (default 2) and the filesystem with it.

  Every message goes to stderr, so `up` is safe inside an SSH ProxyCommand.
#>
param(
  [Parameter(Mandatory, Position = 0)]
  [ValidateSet('deploy', 'up', 'sync', 'down', 'status', 'grow')]
  [string]$Command,
  [int]$GiB = 2,
  [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
$Region = 'us-west-2'
$Stack = 'agentbox'
$Root = Split-Path $PSScriptRoot -Parent
$IdFile = Join-Path $Root '.instance-id'
$Aws = 'C:\Program Files\Amazon\AWSCLIV2\aws.exe'
if (-not (Test-Path $Aws)) { $Aws = 'aws' }
# The script's own ssh and scp calls skip the RemoteForwards in the agentbox SSH entry, so
# they never collide with an interactive session that already holds those ports.
$NoForward = @('-o', 'ClearAllForwardings=yes')

# Inside ssh's ProxyCommand (stdin a pipe, stderr the console), Windows PowerShell 5.1 sets
# the console's output mode to VT processing alone as it starts, dropping
# ENABLE_PROCESSED_OUTPUT and ENABLE_WRAP_AT_EOL_OUTPUT. Messages then print CR and LF as
# glyphs, and ssh later saves that mode and builds the session's raw mode on it, so the whole
# session shows glyphs and ignores escape sequences (clear, reset). Repair turns both back on
# and leaves them on; it runs as the script starts, before each message and as it ends.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AgentboxConsole {
  [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int n);
  [DllImport("kernel32.dll")] static extern bool GetConsoleMode(IntPtr h, out uint m);
  [DllImport("kernel32.dll")] static extern bool SetConsoleMode(IntPtr h, uint m);
  public static void Repair() {
    IntPtr h = GetStdHandle(-12);
    uint mode;
    // ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT on, DISABLE_NEWLINE_AUTO_RETURN off.
    if (GetConsoleMode(h, out mode)) SetConsoleMode(h, (mode | 0x3u) & ~0x8u);
  }
}
'@
[AgentboxConsole]::Repair()

function Write-Message([string]$Text) {
  [AgentboxConsole]::Repair()
  [Console]::Error.WriteLine($Text)
}

function Say([string]$Text) { if (-not $Quiet) { Write-Message $Text } }
function Fail([string]$Text) { Write-Message "agentbox: $Text"; exit 1 }

function Invoke-Aws {
  $out = & $Aws @args --region $Region --output json 2>&1
  if ($LASTEXITCODE -ne 0) {
    $msg = ($out | Out-String).Trim()
    if ($msg -match 'expired|aws login|Unable to locate credentials') {
      Fail "AWS sign-in has expired. Run 'aws login' in a fresh Incognito window, then retry."
    }
    Fail $msg
  }
  $text = ($out | Out-String).Trim()
  if ($text) { $text | ConvertFrom-Json } else { $null }
}

function Get-InstanceId {
  if (Test-Path $IdFile) { return (Get-Content $IdFile -Raw).Trim() }
  $s = Invoke-Aws cloudformation describe-stacks --stack-name $Stack
  $id = ($s.Stacks[0].Outputs | Where-Object OutputKey -eq 'InstanceId').OutputValue
  Set-Content -Path $IdFile -Value $id -NoNewline
  $id
}

function Get-Instance([string]$Id) {
  (Invoke-Aws ec2 describe-instances --instance-ids $Id).Reservations[0].Instances[0]
}

function Test-SsmOnline([string]$Id) {
  $r = Invoke-Aws ssm describe-instance-information --filters "Key=InstanceIds,Values=$Id"
  ($r.InstanceInformationList | Select-Object -First 1).PingStatus -eq 'Online'
}

# Asks the box, through Session Manager Run Command, whether the boot-time patch run
# has finished. Returns 'patched', 'pending' or 'unknown'.
function Get-PatchState([string]$Id) {
  # No commas, brackets or equals signs: the CLI's shorthand syntax would split on them.
  $cmd = 'test -f /run/agentbox/patched && echo patched || (systemctl is-enabled --quiet agentbox-patch.service && echo pending || echo unknown)'
  $sent = Invoke-Aws ssm send-command --instance-ids $Id --document-name AWS-RunShellScript `
    --comment 'agentbox up: patch check' --parameters "commands=$cmd"
  $cid = $sent.Command.CommandId
  for ($n = 0; $n -lt 20; $n++) {
    Start-Sleep -Seconds 1
    $inv = & $Aws ssm get-command-invocation --region $Region --output json `
      --command-id $cid --instance-id $Id 2>$null | Out-String
    if (-not $inv) { continue }
    $inv = $inv | ConvertFrom-Json
    if ($inv.Status -in 'Pending', 'InProgress', 'Delayed') { continue }
    return $inv.StandardOutputContent.Trim()
  }
  'pending'
}

function Wait-Ready([string]$Id) {
  $deadline = (Get-Date).AddMinutes(15)
  $said = ''
  while ($true) {
    if ((Get-Date) -gt $deadline) { Fail "$Id was not ready within 15 minutes; check: ssh agentbox journalctl -u agentbox-patch" }
    if (-not (Test-SsmOnline $Id)) { $msg = 'Waiting for Session Manager...' }
    else {
      $p = Get-PatchState $Id
      if ($p -ne 'pending') { return }
      $msg = 'Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...'
    }
    if ($msg -ne $said) { Say $msg; $said = $msg }
    Start-Sleep -Seconds 5
  }
}

# Starts the instance if it is stopped, then waits until it has finished patching.
function Start-Box([string]$Id) {
  $state = (Get-Instance $Id).State.Name
  switch ($state) {
    'running' { }
    'pending' { }
    'stopped' { Say "Starting $Id..."; $null = Invoke-Aws ec2 start-instances --instance-ids $Id }
    'stopping' {
      Say "Waiting for $Id to finish stopping..."
      & $Aws ec2 wait instance-stopped --region $Region --instance-ids $Id
      Say "Starting $Id..."
      $null = Invoke-Aws ec2 start-instances --instance-ids $Id
    }
    default { Fail "Instance $Id is $state." }
  }
  # A box that has been up for 20 minutes finished its boot-time patch long ago.
  $launched = [datetime](Get-Instance $Id).LaunchTime
  if ($state -ne 'running' -or ((Get-Date).ToUniversalTime() - $launched.ToUniversalTime()).TotalMinutes -lt 20) {
    Wait-Ready $Id
  }
  Say "Ready: $Id"
}

switch ($Command) {
  'deploy' {
    $key = (Get-Content (Join-Path $HOME '.ssh\id_ed25519.pub') -Raw).Trim()
    $tpl = Join-Path $Root 'infra\agentbox.yaml'
    Say "Deploying stack '$Stack' in $Region..."
    & $Aws cloudformation deploy --region $Region --stack-name $Stack --template-file $tpl `
      --capabilities CAPABILITY_IAM --no-fail-on-empty-changeset `
      --tags project=agentbox --parameter-overrides "SshPublicKey=$key"
    if ($LASTEXITCODE -ne 0) { Fail 'Stack deploy failed; see the stack events in the console.' }
    Remove-Item $IdFile -ErrorAction SilentlyContinue
    Say "Instance: $(Get-InstanceId)"
    & $PSCommandPath sync
  }

  'sync' {
    Start-Box (Get-InstanceId)
    $src = Join-Path $Root 'box\system'
    Say 'Copying box/system to the instance...'
    & ssh @NoForward agentbox 'rm -rf /tmp/agentbox-system'
    & scp @NoForward -q -r $src 'agentbox:/tmp/agentbox-system'
    if ($LASTEXITCODE -ne 0) { Fail 'Copy to the instance failed.' }
    & ssh @NoForward agentbox 'sudo bash /tmp/agentbox-system/install.sh && rm -rf /tmp/agentbox-system'
    if ($LASTEXITCODE -ne 0) { Fail 'Install on the instance failed.' }
    # Terry's global instructions for Claude Code, so agents on the box follow the same
    # standing orders as on the laptop. The laptop copy is the source; the box copy is replaced.
    $claudeMd = Join-Path $HOME '.claude\CLAUDE.md'
    if (Test-Path $claudeMd) {
      Say 'Copying ~/.claude/CLAUDE.md to the instance...'
      & ssh @NoForward agentbox 'mkdir -p ~/.claude'
      & scp @NoForward -q $claudeMd 'agentbox:.claude/CLAUDE.md'
      if ($LASTEXITCODE -ne 0) { Fail 'Copying CLAUDE.md to the instance failed.' }
    }
  }

  'up' {
    Start-Box (Get-InstanceId)
  }

  'down' {
    $id = Get-InstanceId
    $null = Invoke-Aws ec2 stop-instances --instance-ids $id
    Say "Stopping $id."
  }

  'status' {
    $id = Get-InstanceId
    $i = Get-Instance $id
    $volId = ($i.BlockDeviceMappings | Where-Object DeviceName -eq $i.RootDeviceName).Ebs.VolumeId
    $vol = (Invoke-Aws ec2 describe-volumes --volume-ids $volId).Volumes[0]
    $ssm = if ($i.State.Name -eq 'running') { if (Test-SsmOnline $id) { 'online' } else { 'not yet' } } else { '-' }
    Write-Message (("Instance  {0}`nState     {1}`nType      {2}`nDisk      {3} GiB gp3 ({4})`nSSM       {5}" -f
      $id, $i.State.Name, $i.InstanceType, $vol.Size, $volId, $ssm))
  }

  'grow' {
    $id = Get-InstanceId
    $i = Get-Instance $id
    $volId = ($i.BlockDeviceMappings | Where-Object DeviceName -eq $i.RootDeviceName).Ebs.VolumeId
    $size = (Invoke-Aws ec2 describe-volumes --volume-ids $volId).Volumes[0].Size
    $new = $size + $GiB
    Say "Growing $volId from $size to $new GiB (EBS allows one change per volume every 6 hours)..."
    $null = Invoke-Aws ec2 modify-volume --volume-id $volId --size $new
    if ($i.State.Name -eq 'running') {
      Say 'Growing the filesystem online...'
      Start-Sleep -Seconds 10
      & ssh @NoForward agentbox 'sudo growpart /dev/nvme0n1 1; sudo resize2fs /dev/nvme0n1p1 && df -h /'
    } else {
      Say 'The filesystem grows on its own at next boot.'
    }
  }
}
[AgentboxConsole]::Repair()
