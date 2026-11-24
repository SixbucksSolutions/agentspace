# The live kit's proof of the mask and the scan. Process tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage: pwsh -NoProfile -File scripts\live\mask-proof.ps1 [-ProofDir <folder>]
#
# A live run MUST run it before its first masked print, and MUST NOT trust the mask when it exits
# nonzero. It asserts, for one made-up value of every shape the mask covers, three things: the masked
# value equals its expected text, the planted value is gone, and the placeholder is present. It asserts
# the near misses unchanged, among them v0.08-T05's three (a 10-digit Unix-seconds value, the `Host`
# line and the `Guard` line) and v0.09's sizes, types and messages. It then proves scan.ps1 exits 1 on
# a planted file and 0 on a clean one, in -ProofDir, which MUST be under the worktree's target\.
#
# For the real environment it prints only True or False, never a value:
#   COMPUTERNAME removed, USERNAME removed, USERPROFILE removed, USERPROFILE short form removed.
#
# Every planted value is made up, and every one shaped like an AWS identifier, an address or a key is
# assembled from pieces at run time, so this file passes scripts\scan-identifiers.ps1.
param(
  [string]$ProofDir = (Join-Path (Get-Location) 'target\live-proof')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'mask.ps1')

$proofPassed = 0
$proofFailed = 0
function Assert-ProofCase([string]$CaseName, [bool]$Ok) {
  if ($Ok) { $script:proofPassed++ } else { $script:proofFailed++; "FAILED: $CaseName" }
}

# Made-up names for rules 1 and 2.
$fakeUser = 'planted' + 'user'
$fakePc = 'PLANTED' + '-PC7'
$fakeProfile = 'C:\Users\' + $fakeUser
$fakeShortLeaf = 'PLANTE' + '~1'
$fakeShort = 'C:\Users\' + $fakeShortLeaf
$fakeProfiles = @($fakeProfile, $fakeShort)
$fakeLiterals = @($fakePc, $fakeUser, $fakeShortLeaf)
function Get-ProofMasked([string]$Value) {
  ConvertTo-LiveMasked -Text $Value -Profiles $fakeProfiles -Literals $fakeLiterals
}

$hex17 = '0a1b2c3d' + '4e5f60718'
$hex8 = '0a1b' + '2c3d'
$acct = '1234' + '5678' + '9012'
$b64 = 'AAAAC3Nza' + 'C1lZDI1NTE5AAAAIPlantedKeyMaterialOnly0123456789abcd'
$arnText = 'ar' + 'n:aws:iam::' + $acct + ':role/planted-role'
$stackArn = 'ar' + 'n:aws:cloudformation:us-west-2:' + $acct + ':stack/planted/' + $hex8 + '-1a2b-4c3d-8e9f-' + '0a1b2c3d4e5f'
$pemBegin = '-----BEGIN OPENSSH ' + 'PRIVATE KEY-----'
$pemEnd = '-----END OPENSSH ' + 'PRIVATE KEY-----'
$tokenValue = 'Pl4ntedTokenValue' + 'OnlyAbcdefGhij0123456789'

# Each case: name, planted text, the masked text expected, the part that MUST be gone, the placeholder.
$cases = @(
  @('instance id, long', ('i-' + $hex17), '<instance-id>', $hex17, '<instance-id>'),
  @('instance id, short', ('i-' + $hex8), '<instance-id>', $hex8, '<instance-id>'),
  @('account id', $acct, '<account-id>', $acct, '<account-id>'),
  @('uuid', ($hex8 + '-4e5f-4a6b-8c7d-' + '9e0f1a2b3c4d'), '<uuid>', '9e0f1a2b3c4d', '<uuid>'),
  @('lease alias, swimcoach', ('agentspace-swimcoach-' + $hex8), '<lease-alias>', $hex8, '<lease-alias>'),
  @('lease alias, agentspace', ('agentspace-agentspace-' + '9f8e' + '7d6c'), '<lease-alias>', '9f8e7d6c', '<lease-alias>'),
  @('lease alias, hyphenated project', ('agentspace-my-proj-' + '1234' + 'abcd'), '<lease-alias>', '1234abcd', '<lease-alias>'),
  @('private ip, 10 low', ('10.' + '0.0.0'), '<private-ip>', ('10.' + '0.0.0'), '<private-ip>'),
  @('private ip, 10 high', ('10.' + '255.255.255'), '<private-ip>', ('10.' + '255.255.255'), '<private-ip>'),
  @('private ip, 172.16', ('172.' + '16.0.1'), '<private-ip>', ('172.' + '16.0.1'), '<private-ip>'),
  @('private ip, 172.31', ('172.' + '31.255.254'), '<private-ip>', ('172.' + '31.255.254'), '<private-ip>'),
  @('private ip, 192.168 low', ('192.' + '168.0.1'), '<private-ip>', ('192.' + '168.0.1'), '<private-ip>'),
  @('private ip, 192.168 high', ('192.' + '168.255.255'), '<private-ip>', ('192.' + '168.255.255'), '<private-ip>'),
  @('public ip', ('54.' + '12.34.56'), '<public-ip>', ('54.' + '12.34.56'), '<public-ip>'),
  @('documentation ip', ('203.' + '0.113.7'), '<public-ip>', ('203.' + '0.113.7'), '<public-ip>'),
  @('172.15 is public', ('172.' + '15.0.1'), '<public-ip>', ('172.' + '15.0.1'), '<public-ip>'),
  @('private host, compute.internal', ('ip-' + '10-1-2-3' + '.us-west-2.compute.internal'), '<private-host>', '10-1-2-3', '<private-host>'),
  @('private host, ec2.internal', ('ip-' + '10-1-2-3' + '.ec2.internal'), '<private-host>', '10-1-2-3', '<private-host>'),
  @('private host, bare', ('ip-' + '172-16-0-9'), '<private-host>', '172-16-0-9', '<private-host>'),
  @('public host', ('ec2-' + '54-12-34-56' + '.us-west-2.compute.amazonaws.com'), '<public-host>', '54-12-34-56', '<public-host>'),
  @('vpc id', ('vpc' + '-' + $hex17), '<vpc-id>', $hex17, '<vpc-id>'),
  @('subnet id', ('subnet' + '-' + $hex8), '<subnet-id>', $hex8, '<subnet-id>'),
  @('vol id', ('vol' + '-' + $hex17), '<vol-id>', $hex17, '<vol-id>'),
  @('sg id', ('sg' + '-' + $hex8), '<sg-id>', $hex8, '<sg-id>'),
  @('ami id', ('ami' + '-' + $hex17), '<ami-id>', $hex17, '<ami-id>'),
  @('eni id', ('eni' + '-' + $hex17), '<eni-id>', $hex17, '<eni-id>'),
  @('igw id', ('igw' + '-' + $hex8), '<igw-id>', $hex8, '<igw-id>'),
  @('rtb id', ('rtb' + '-' + $hex8), '<rtb-id>', $hex8, '<rtb-id>'),
  @('snap id', ('snap' + '-' + $hex17), '<snap-id>', $hex17, '<snap-id>'),
  @('reservation id', ('r' + '-' + $hex17), '<r-id>', $hex17, '<r-id>'),
  @('arn', $arnText, '<arn>', $acct, '<arn>'),
  @('StackId', ('"StackId": "' + $stackArn + '"'), '"StackId": "<stack-id>"', $acct, '<stack-id>'),
  @('ChangeSetId', ('ChangeSetId=' + $stackArn), 'ChangeSetId=<stack-id>', $acct, '<stack-id>'),
  @('ssh-ed25519 key', ('ssh-' + 'ed25519 ' + $b64), '<ssh-key>', $b64, '<ssh-key>'),
  @('ssh-ed25519 key with comment', ('ssh-' + 'ed25519 ' + $b64 + ' planted@laptop'), '<ssh-key>', 'planted@laptop', '<ssh-key>'),
  @('ssh-rsa key', ('ssh-' + 'rsa ' + $b64), '<ssh-key>', $b64, '<ssh-key>'),
  @('ssh-dss key', ('ssh-' + 'dss ' + $b64), '<ssh-key>', $b64, '<ssh-key>'),
  @('ecdsa key', ('ecdsa-sha2-' + 'nistp256 ' + $b64 + ' comment-here'), '<ssh-key>', $b64, '<ssh-key>'),
  @('ssh certificate', ('ssh-' + 'ed25519-cert-v01@openssh.com ' + $b64 + ' planted-cert'), '<ssh-cert>', $b64, '<ssh-cert>'),
  @('pem block', ($pemBegin + "`n" + $b64 + "`n" + $pemEnd), '<private-key>', $b64, '<private-key>'),
  @('pem marker alone', $pemBegin, '<private-key>', 'OPENSSH', '<private-key>'),
  @('fingerprint', ('SHA' + '256:' + 'Pl4ntedFingerprintAbcdefGhijklmnop0123456'), '<fingerprint>', 'Pl4ntedFingerprint', '<fingerprint>'),
  @('token assignment', ('LOCALSWIM_' + 'TOKEN=' + $tokenValue), 'LOCALSWIM_TOKEN=<secret>', $tokenValue, '<secret>'),
  @('password assignment', ('pass' + 'word: "' + 'hunter2planted' + '"'), 'password: "<secret>"', 'hunter2planted', '<secret>'),
  @('json token member', ('"to' + 'ken": "' + 'plantedvalue77' + '"'), '"token": "<secret>"', 'plantedvalue77', '<secret>'),
  @('secret assignment', ('client_' + 'secret = ' + 'plantedsecret99'), 'client_secret = <secret>', 'plantedsecret99', '<secret>'),
  @('bare long token', $tokenValue, '<token>', $tokenValue, '<token>'),
  @('email', ('planted.person' + '@' + 'example.com'), '<email>', 'planted.person', '<email>'),
  @('computer name, upper', $fakePc, '<literal>', $fakePc, '<literal>'),
  @('computer name, lower', $fakePc.ToLowerInvariant(), '<literal>', $fakePc.ToLowerInvariant(), '<literal>'),
  @('user name, lower', $fakeUser, '<literal>', $fakeUser, '<literal>'),
  @('user name, upper', $fakeUser.ToUpperInvariant(), '<literal>', $fakeUser.ToUpperInvariant(), '<literal>'),
  @('profile, backslashes', ($fakeProfile + '\.agentspace\bin'), '%USERPROFILE%\.agentspace\bin', $fakeUser, '%USERPROFILE%'),
  @('profile, forward slashes', ($fakeProfile.Replace('\', '/') + '/.ssh/config'), '%USERPROFILE%/.ssh/config', $fakeUser, '%USERPROFILE%'),
  @('profile, doubled backslashes', ($fakeProfile.Replace('\', '\\') + '\\.agentspace'), '%USERPROFILE%\\.agentspace', $fakeUser, '%USERPROFILE%'),
  @('profile, git bash form', ('/c/Users/' + $fakeUser + '/.agentspace'), '%USERPROFILE%/.agentspace', $fakeUser, '%USERPROFILE%'),
  @('profile, 8.3 short form', ($fakeShort + '\AppData\Local\Temp'), '%USERPROFILE%\AppData\Local\Temp', $fakeShortLeaf, '%USERPROFILE%'),
  @('profile folder, 8.3 short form alone', ('in ' + $fakeShortLeaf + ' here'), 'in <literal> here', $fakeShortLeaf, '<literal>'),
  @('a line of several', ('Instance  i-' + $hex17 + ' at ' + ('10.' + '1.2.3') + ' for ' + $fakeUser),
    'Instance  <instance-id> at <private-ip> for <literal>', $hex17, '<instance-id>')
)
foreach ($c in $cases) {
  $masked = Get-ProofMasked $c[1]
  Assert-ProofCase "$($c[0]): equals" ($masked -ceq $c[2])
  Assert-ProofCase "$($c[0]): planted gone" (-not $masked.Contains($c[3]))
  Assert-ProofCase "$($c[0]): placeholder present" ($masked.Contains($c[4]))
}

# Near misses: unchanged under the made-up names and under the real environment's.
$nearMisses = @(
  '20261124091500',
  '1795452833000',
  '1795452833',
  'expires_at 1795452833 (Unix seconds)',
  'codex-cli 0.171.0',
  'v0.09 and 2.1.325',
  'm9g.large', 'm9g.xlarge', 'm9g.2xlarge', 'm9g.4xlarge',
  'Type      m9g.2xlarge',
  '"instance_type": "m9g.xlarge"',
  '"size": "l"',
  '"state": "ready"',
  'ttl_seconds 28800',
  '8',
  '4',
  'Disk      20 GiB gp3',
  '2026-11-24T16:04:08Z',
  'Host      agentspace (agentspace-host-agentspace), ready',
  'Host      swimcoach (agentbox), ready',
  'Guard     unexplained; stops at 10:06am',
  'Leases    1 live',
  'SSM       online',
  'State     running', 'State     stopped', 'State     stopping', 'State     pending',
  'c5e118446a9afeba894c15baa6987266fd7bd02c',
  '127.0.0.1',
  'agentspace-host-agentspace-idle-backstop',
  'agentspace-operator.exe status -All',
  'agentspace-keeper',
  'us-west-2',
  'Client.UserInitiatedShutdown',
  "No capacity can be found for project 'agentspace' at size 'xl': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.",
  "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.",
  'unsatisfiable no_capacity unsupported_size',
  'The token rule counts clients, and the password prompt is Terry''s.',
  'Plain words, and a reader named agentspace-v0.09-t05.'
)
foreach ($n in $nearMisses) {
  Assert-ProofCase "near miss unchanged (made-up names): $n" ((Get-ProofMasked $n) -ceq $n)
  Assert-ProofCase "near miss unchanged (real environment): $n" ((ConvertTo-LiveMasked -Text $n) -ceq $n)
}

# The scan: exit 1 on a planted file, 0 on a clean one.
New-Item -ItemType Directory -Force -Path $ProofDir | Out-Null
$plantedFile = Join-Path $ProofDir 'planted-scan.txt'
$cleanFile = Join-Path $ProofDir 'clean-scan.txt'
Set-Content -LiteralPath $plantedFile -Encoding utf8 -Value @('A clean line.', ('Instance  i-' + $hex17), ('Lease ' + 'agentspace-swimcoach-' + $hex8))
Set-Content -LiteralPath $cleanFile -Encoding utf8 -Value @('A clean line.', 'Type      m9g.2xlarge', 'Host      swimcoach (agentbox), ready')
$scanScript = Join-Path $PSScriptRoot 'scan.ps1'
$plantedOut = @(& pwsh -NoProfile -File $scanScript -File $plantedFile)
$plantedCode = $LASTEXITCODE
$cleanOut = @(& pwsh -NoProfile -File $scanScript -File $cleanFile)
$cleanCode = $LASTEXITCODE
Assert-ProofCase 'scan exits 1 on the planted file' ($plantedCode -eq 1)
Assert-ProofCase 'scan counts 2 hits on the planted file' (($plantedOut -join "`n").Contains('3 lines, 2 hits'))
Assert-ProofCase 'scan prints no planted value' (-not ($plantedOut -join "`n").Contains($hex17))
Assert-ProofCase 'scan exits 0 on the clean file' ($cleanCode -eq 0)
Assert-ProofCase 'scan counts 0 hits on the clean file' (($cleanOut -join "`n").Contains('3 lines, 0 hits'))

"Mask proof: $proofPassed passed, $proofFailed failed."

# The real environment: True or False only.
$realProfileShort = Get-LiveMaskShortPath $env:USERPROFILE
$realLine = "pc $env:COMPUTERNAME user $env:USERNAME at $env:USERPROFILE\x and $realProfileShort\y"
$realMasked = ConvertTo-LiveMasked -Text $realLine
"COMPUTERNAME removed: $(-not $realMasked.ToLowerInvariant().Contains($env:COMPUTERNAME.ToLowerInvariant()))"
"USERNAME removed: $(-not $realMasked.ToLowerInvariant().Contains($env:USERNAME.ToLowerInvariant()))"
"USERPROFILE removed: $(-not $realMasked.ToLowerInvariant().Contains($env:USERPROFILE.ToLowerInvariant()))"
"USERPROFILE short form removed: $(-not $realMasked.ToLowerInvariant().Contains($realProfileShort.ToLowerInvariant()))"

if ($proofFailed) { exit 1 }
exit 0
