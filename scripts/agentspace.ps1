<#
.SYNOPSIS
  Manages the agentspace EC2 instance from the laptop.

.DESCRIPTION
  deploy  Creates or updates the CloudFormation stack, then runs sync.
  up      Starts the instance if needed, waits until it has finished patching, and sends
          the localswim Bot token if it changed.
  sync    Checks the agentspace SSH CA, then installs box/system (idle check, patch at boot,
          localswim wrapper, the sshd drop-in that trusts the CA and closes root login) on the
          instance with the CA's public key, copies ~/.claude/CLAUDE.md there, builds and
          installs the localswim CLI, and sends the current localswim Bot token.
  down    Stops the instance.
  status  Shows state, type, disk size and Session Manager reachability.
  grow    Grows the root volume by -GiB (default 2) and the filesystem with it.
  ca      Creates the agentspace SSH CA (ed25519) in %LOCALAPPDATA%\agentspace\ca the first
          time, locked to the current user (decision D17). Later runs change nothing: they check
          the key pair and its access rules, and print its paths and fingerprint. A half-made or
          loosened CA fails with what is wrong; ca never repairs or overwrites one.

  Every message goes to stderr, so `up` is safe inside an SSH ProxyCommand.
#>
param(
  [Parameter(Mandatory, Position = 0)]
  [ValidateSet('deploy', 'up', 'sync', 'down', 'status', 'grow', 'ca')]
  [string]$Command,
  [int]$GiB = 2,
  [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
$Region = 'us-west-2'
# The CloudFormation stack keeps the project's first name: renaming a stack replaces the instance.
$Stack = 'agentbox'
$Root = Split-Path $PSScriptRoot -Parent
$IdFile = Join-Path $Root '.instance-id'
$Aws = 'C:\Program Files\Amazon\AWSCLIV2\aws.exe'
if (-not (Test-Path $Aws)) { $Aws = 'aws' }
# The script's own ssh and scp calls skip the RemoteForwards in the agentspace SSH entry, so
# they never collide with an interactive session that already holds those ports.
$NoForward = @('-o', 'ClearAllForwardings=yes')
# Every ssh this script starts runs the agentspace ProxyCommand, and so this script again. The
# flag tells those nested runs not to push the localswim token a second time.
$Nested = [bool]$env:AGENTSPACE_NESTED
$env:AGENTSPACE_NESTED = '1'
$LocalswimRepo = 'C:\Projects\localswim'
$LocalswimReceipt = Join-Path $env:LOCALAPPDATA 'localswim-v2\service.json'
$PushedFile = Join-Path $env:LOCALAPPDATA 'agentspace\localswim-pushed.sha256'
# The agentspace SSH CA (decisions D7 and D17). The private key MUST stay in this directory, which
# is neither in the repo nor synced; sync sends the public key from here.
$CaDir = Join-Path $env:LOCALAPPDATA 'agentspace\ca'
$CaKey = Join-Path $CaDir 'agentspace_ca'
$CaPub = "$CaKey.pub"
$SshKeygen = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
# Windows OpenSSH by full path: from Git Bash, a bare scp finds MSYS's, which mangles the
# ProxyCommand (card #0027 moves the older bare calls over).
$Scp = Join-Path $env:SystemRoot 'System32\OpenSSH\scp.exe'
$Icacls = Join-Path $env:SystemRoot 'System32\icacls.exe'
# Windows PowerShell 5.1 drops an empty argument on its way to a native command, so an empty
# passphrase is written as a pair of quotes there; PowerShell 7.3 and later pass '' as is.
$NoPassphrase = if ($PSNativeCommandArgumentPassing -and $PSNativeCommandArgumentPassing -ne 'Legacy') { '' } else { '""' }

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
function Fail([string]$Text) { Write-Message "agentspace: $Text"; exit 1 }

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
  $cmd = 'test -f /run/agentspace/patched && echo patched || (systemctl is-enabled --quiet agentspace-patch.service && echo pending || echo unknown)'
  $sent = Invoke-Aws ssm send-command --instance-ids $Id --document-name AWS-RunShellScript `
    --comment 'agentspace up: patch check' --parameters "commands=$cmd"
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
    if ((Get-Date) -gt $deadline) { Fail "$Id was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch" }
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

# Writes the laptop localswim service's URL and current Bot token to ~/.config/localswim/bot.env
# on the box (mode 0600), for the box's localswim wrapper. The service makes a new token at
# every cold start, so this runs at each connection but sends only when the instance or the
# token has changed since the last send. The token goes over SSH on stdin: never on a command
# line, never printed, never stored on the laptop outside localswim's own DPAPI file. A failure
# warns and never blocks the connection.
function Push-Localswim([string]$Id, [switch]$Force) {
  if (-not (Test-Path $LocalswimReceipt)) { return }
  $token = $null
  $content = $null
  try {
    $receipt = Get-Content $LocalswimReceipt -Raw | ConvertFrom-Json
    $token = (Import-Clixml (Join-Path $receipt.credential_directory 'bot.xml')).GetNetworkCredential().Password
    $content = "LOCALSWIM_URL=$($receipt.base)`nLOCALSWIM_TOKEN=$token`n"
    $sha = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes("$Id`n$content"))
    $sha = [BitConverter]::ToString($sha).Replace('-', '')
    if (-not $Force -and (Test-Path $PushedFile) -and (Get-Content $PushedFile -Raw).Trim() -eq $sha) { return }
    Say 'Sending the localswim Bot token to the instance...'
    # Windows PowerShell pipes text to native commands with a UTF-8 BOM unless told otherwise;
    # a BOM turns the first assignment into a command name. Send none, and strip one anyway.
    $OutputEncoding = [Text.UTF8Encoding]::new($false)
    $content | & ssh @NoForward agentspace 'umask 077 && mkdir -p ~/.config/localswim && tr -d ''\r'' | sed ''1s/^\xEF\xBB\xBF//; /^$/d'' > ~/.config/localswim/bot.env' | Out-Null
    if ($LASTEXITCODE -ne 0) { Say 'agentspace: Could not send the localswim token; the board will not work on the box this session.'; return }
    New-Item -ItemType Directory -Force (Split-Path $PushedFile) | Out-Null
    Set-Content -Path $PushedFile -Value $sha -NoNewline
  } catch {
    Say 'agentspace: Could not read the localswim Bot token; the board will not work on the box this session.'
  } finally {
    $token = $null
    $content = $null
  }
}

# Builds the localswim CLI for the box from the laptop's localswim checkout, the same source as
# the service it talks to, and installs it where the box's localswim wrapper runs it.
function Install-LocalswimCli {
  if (-not (Get-Command go -ErrorAction SilentlyContinue) -or -not (Test-Path $LocalswimRepo)) {
    Say 'Skipping the localswim CLI: Go or C:\Projects\localswim is missing on the laptop.'
    return
  }
  $out = Join-Path $Root '.build\localswim-linux-arm64'
  Say 'Building the localswim CLI for linux/arm64...'
  $env:GOOS = 'linux'; $env:GOARCH = 'arm64'; $env:CGO_ENABLED = '0'
  Push-Location $LocalswimRepo
  try { & go build -trimpath -o $out ./cmd/localswim }
  finally { Pop-Location; Remove-Item Env:GOOS, Env:GOARCH, Env:CGO_ENABLED }
  if ($LASTEXITCODE -ne 0) { Fail 'Building the localswim CLI failed.' }
  Say 'Copying the localswim CLI to the instance...'
  & scp @NoForward -q $out 'agentspace:/tmp/localswim'
  if ($LASTEXITCODE -ne 0) { Fail 'Copying the localswim CLI failed.' }
  & ssh @NoForward agentspace 'sudo install -D -m 0755 /tmp/localswim /usr/local/lib/localswim/localswim && rm /tmp/localswim'
  if ($LASTEXITCODE -ne 0) { Fail 'Installing the localswim CLI failed.' }
}

# Runs a native command with stderr folded into its output as text. Windows PowerShell 5.1 turns
# a redirected stderr line into a terminating error under $ErrorActionPreference = 'Stop', which
# would hide the exit code; stdin is closed, so a prompt fails instead of waiting unseen.
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
  $saved = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try { $out = $null | & $Exe @Arguments 2>&1 | ForEach-Object { "$_" } }
  finally { $ErrorActionPreference = $saved }
  [pscustomobject]@{ Code = $LASTEXITCODE; Out = (@($out) -join "`n").Trim() }
}

# The CA's directory and private key MUST be the current user's alone (D17): owned by the user,
# one rule giving the user full control, nothing inherited. Returns what breaks that, or nothing.
function Get-OwnerOnlyProblems([string]$Path) {
  $me = [Security.Principal.WindowsIdentity]::GetCurrent().User
  $full = [Security.AccessControl.FileSystemRights]::FullControl
  $acl = Get-Acl -LiteralPath $Path
  if ($acl.GetOwner([Security.Principal.SecurityIdentifier]) -ne $me) { "is owned by $($acl.Owner)" }
  if (-not $acl.AreAccessRulesProtected) { 'inherits access rules from its parent' }
  $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
  foreach ($rule in $rules) {
    if ($rule.IdentityReference -eq $me -and $rule.AccessControlType -eq 'Allow') {
      if (($rule.FileSystemRights -band $full) -ne $full) { "gives $env:USERNAME $($rule.FileSystemRights), not full control" }
      continue
    }
    $who = try { $rule.IdentityReference.Translate([Security.Principal.NTAccount]).Value } catch { $rule.IdentityReference.Value }
    if ($rule.AccessControlType -eq 'Allow') { "has a rule allowing $who" } else { "has a rule denying $who" }
  }
  if ($rules.Count -gt 1) { "has $($rules.Count) access rules instead of one" }
}

# Checks the CA (D17) without changing it: both files present, the directory and private key the
# current user's alone, the private key readable without a passphrase, and the public key its own
# ed25519 key. Fails with what is wrong, and returns the fingerprint. sync calls it before it sends
# the public key to a host.
function Assert-Ca {
  if (-not (Test-Path -LiteralPath $CaKey)) { Fail "$CaKey does not exist; run ca." }
  if (-not (Test-Path -LiteralPath $CaPub)) { Fail "$CaPub does not exist; run ca." }
  foreach ($path in $CaDir, $CaKey) {
    $problems = @(Get-OwnerOnlyProblems $path)
    if ($problems) {
      Fail ("$path MUST be $env:USERNAME's alone, but it $($problems -join '; '). ca does not change " +
        "access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). " +
        "Otherwise lock it by hand: icacls `"$CaDir`" /inheritance:r /grant:r `"${env:USERNAME}:(OI)(CI)F`", " +
        "then icacls `"$CaKey`" /reset, then icacls `"$CaKey`" /inheritance:d, then remove any other " +
        "rule icacls still lists, and run ca again.")
    }
  }
  # Reading the private key with an empty passphrase also proves it has none, as unattended
  # signing needs.
  $r = Invoke-Native $SshKeygen @('-y', '-P', $NoPassphrase, '-f', $CaKey)
  if ($r.Code -ne 0) { Fail "ssh-keygen could not read $CaKey without a passphrase; the lease CLI signs unattended (D17)." }
  $derived = $r.Out -split '\s+'
  $stored = (Get-Content -LiteralPath $CaPub -Raw).Trim() -split '\s+'
  if ($stored[0] -ne 'ssh-ed25519' -or $derived[0] -ne $stored[0] -or $derived[1] -ne $stored[1]) {
    Fail "$CaPub is not the ed25519 public key of $CaKey. ca does not repair a mismatched pair."
  }
  $r = Invoke-Native $SshKeygen @('-l', '-f', $CaPub)
  if ($r.Code -ne 0) { Fail "ssh-keygen could not read ${CaPub}: $($r.Out)" }
  ($r.Out -split '\s+')[1]
}

switch ($Command) {
  'ca' {
    if (-not (Test-Path -LiteralPath $SshKeygen)) { Fail "$SshKeygen is missing; install the Windows OpenSSH client." }
    foreach ($forbidden in @($env:OneDrive, $env:OneDriveConsumer, $env:OneDriveCommercial, $Root)) {
      if ($forbidden -and "$CaDir\".StartsWith($forbidden.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        Fail "$CaDir is inside $forbidden; the CA key MUST NOT live in a synced folder or in the repo."
      }
    }
    $hasKey = Test-Path -LiteralPath $CaKey
    $hasPub = Test-Path -LiteralPath $CaPub
    if ($hasKey -and -not $hasPub) {
      Fail "$CaKey exists but $CaPub does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again."
    }
    if ($hasPub -and -not $hasKey) {
      Fail "$CaPub exists but $CaKey does not: the CA's private key is lost, and nothing more can be signed with it. ca does not replace it on its own. To make a new CA, delete $CaPub and run ca again, then run sync for every host and re-issue every lease's certificate."
    }
    $created = $false
    if (-not $hasKey) {
      # An elevated token makes Administrators the owner of what it creates, which D17 forbids.
      $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
      if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Fail 'ca MUST NOT create the CA from an elevated shell; run it without "Run as administrator".'
      }
      Say "Creating the agentspace SSH CA in $CaDir..."
      New-Item -ItemType Directory -Force $CaDir | Out-Null
      # The directory stops inheriting from %LOCALAPPDATA%, which also grants other principals
      # (sandbox users among them), and keeps one rule for the user that its files inherit. It is
      # checked before the key exists, so a directory still open to others never gets one.
      $me = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
      $r = Invoke-Native $Icacls @($CaDir, '/inheritance:r', '/grant:r', "*${me}:(OI)(CI)F", '/q')
      if ($r.Code -ne 0) { Fail "icacls could not lock $CaDir to ${env:USERNAME}: $($r.Out)" }
      $problems = @(Get-OwnerOnlyProblems $CaDir)
      if ($problems) {
        Fail "$CaDir MUST be $env:USERNAME's alone before the CA is made in it, but it $($problems -join '; '). Remove the other rules, or delete the directory, and run ca again."
      }
      $comment = "agentspace-ca $env:COMPUTERNAME $(Get-Date -Format yyyy-MM-dd)"
      $r = Invoke-Native $SshKeygen @('-q', '-t', 'ed25519', '-N', $NoPassphrase, '-C', $comment, '-f', $CaKey)
      if ($r.Code -ne 0) { Fail "ssh-keygen could not create the CA: $($r.Out)" }
      # ssh-keygen writes its own rules on the key, granting SYSTEM and Administrators as well.
      # Resetting gives the key the directory's single rule; the second call stops inheriting it.
      $r = Invoke-Native $Icacls @($CaKey, '/reset', '/q')
      if ($r.Code -eq 0) { $r = Invoke-Native $Icacls @($CaKey, '/inheritance:d', '/q') }
      if ($r.Code -ne 0) { Fail "icacls could not lock the new $CaKey to ${env:USERNAME}: $($r.Out). Delete $CaKey and $CaPub, and run ca again." }
      $problems = @(Get-OwnerOnlyProblems $CaKey)
      if ($problems) {
        Fail "The new $CaKey MUST be $env:USERNAME's alone, but it $($problems -join '; '). Treat it as exposed: delete $CaKey and $CaPub, and run ca again."
      }
      $created = $true
    }
    $fingerprint = Assert-Ca
    if ($created) { Say 'Created the agentspace SSH CA.' } else { Say 'The agentspace SSH CA already exists; nothing changed.' }
    Say "Private key  $CaKey"
    Say "Public key   $CaPub"
    Say "Fingerprint  $fingerprint"
  }

  'deploy' {
    $key = (Get-Content (Join-Path $HOME '.ssh\id_ed25519.pub') -Raw).Trim()
    $tpl = Join-Path $Root 'infra\agentspace.yaml'
    # Account-specific parameters (VpcId, SubnetId) live in infra\local.parameters, which is not
    # committed; infra\local.parameters.example shows the format.
    $localParams = Join-Path $Root 'infra\local.parameters'
    if (-not (Test-Path $localParams)) { Fail "Missing $localParams; copy infra\local.parameters.example and fill it in." }
    $overrides = @("SshPublicKey=$key") + @(Get-Content $localParams | Where-Object { $_ -match '^\s*[A-Za-z]+=\S' } | ForEach-Object { $_.Trim() })
    Say "Deploying stack '$Stack' in $Region..."
    & $Aws cloudformation deploy --region $Region --stack-name $Stack --template-file $tpl `
      --capabilities CAPABILITY_IAM --no-fail-on-empty-changeset `
      --tags project=agentbox --parameter-overrides @overrides
    if ($LASTEXITCODE -ne 0) { Fail 'Stack deploy failed; see the stack events in the console.' }
    Remove-Item $IdFile -ErrorAction SilentlyContinue
    Say "Instance: $(Get-InstanceId)"
    & $PSCommandPath sync
  }

  'sync' {
    # The CA is checked before the box is touched, so a swapped or loosened pair never becomes a
    # host's trusted CA (D17). install.sh replaces the box's copy of the public key.
    $fingerprint = Assert-Ca
    Start-Box (Get-InstanceId)
    $src = Join-Path $Root 'box\system'
    Say 'Copying box/system to the instance...'
    & ssh @NoForward agentspace 'rm -rf /tmp/agentspace-system'
    & scp @NoForward -q -r $src 'agentspace:/tmp/agentspace-system'
    if ($LASTEXITCODE -ne 0) { Fail 'Copy to the instance failed.' }
    Say "Copying the agentspace SSH CA's public key ($fingerprint) to the instance..."
    & $Scp @NoForward -q $CaPub 'agentspace:/tmp/agentspace-system/agentspace_ca.pub'
    if ($LASTEXITCODE -ne 0) { Fail "Copying the CA's public key to the instance failed." }
    & ssh @NoForward agentspace 'sudo bash /tmp/agentspace-system/install.sh && rm -rf /tmp/agentspace-system'
    if ($LASTEXITCODE -ne 0) { Fail 'Install on the instance failed.' }
    # Terry's global instructions for Claude Code, so agents on the box follow the same
    # standing orders as on the laptop. The laptop copy is the source; the box copy is replaced.
    $claudeMd = Join-Path $HOME '.claude\CLAUDE.md'
    if (Test-Path $claudeMd) {
      Say 'Copying ~/.claude/CLAUDE.md to the instance...'
      & ssh @NoForward agentspace 'mkdir -p ~/.claude'
      & scp @NoForward -q $claudeMd 'agentspace:.claude/CLAUDE.md'
      if ($LASTEXITCODE -ne 0) { Fail 'Copying CLAUDE.md to the instance failed.' }
    }
    Install-LocalswimCli
    Push-Localswim (Get-InstanceId) -Force
  }

  'up' {
    $id = Get-InstanceId
    Start-Box $id
    if (-not $Nested) { Push-Localswim $id }
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
      & ssh @NoForward agentspace 'sudo growpart /dev/nvme0n1 1; sudo resize2fs /dev/nvme0n1p1 && df -h /'
    } else {
      Say 'The filesystem grows on its own at next boot.'
    }
  }
}
[AgentboxConsole]::Repair()
