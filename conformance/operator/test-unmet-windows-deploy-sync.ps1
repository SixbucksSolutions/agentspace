#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `deploy`, `bootstrap`, `sync`, `up` and `grow` on `windows/amd64`: W23 to
# W27 of docs/conformance.md, "The operator's Windows cases" (docs/operator.md, "deploy", "bootstrap", "sync",
# "The patch wait", "What up waits for", "grow", "The platform seam", "Windows hosts", "The remote command on
# Windows"; D44, D94, D96, D97). Card #0262 (E0.12-T04-S02), from #0253's plan.
#
#   W23  `deploy` for a stack that does not exist reads the image parameter once, then passes the stack
#        `agentspace-windows-amd64-host-<project>`, `HostName`, `ImageId`, `RootVolumeGiB=30`, `HostOs=windows`
#        and `InstanceType=m8a.xlarge` last (the given type with `-InstanceType`), each one argument; for a
#        stack that exists, no read and no parameter; a `stack-parameters.json` naming `HostOs` is refused; no
#        line the operator prints holds the image ID.
#   W24  `bootstrap` copies `bootstrap.ps1` by scp and runs it under Windows PowerShell 5.1 with `-File`, in one
#        encoded remote command of at most 8,191 characters holding no line of the script; its failures.
#   W25  `sync` builds the localswim CLI for windows/amd64, sends `box\windows` and the CA's public key to
#        `.agentspace-system`, runs `install.ps1` with `-File`, waits for the patch, copies `CLAUDE.md` and the CLI,
#        and pushes: nine rows in order. `sync -All` builds once for each platform present and sends each host
#        its own platform's files and build.
#   W26  `up`'s patch check and `sync`'s patch wait send the PowerShell document, its parameters one JSON
#        argument, the command naming PowerShell 7 by its full path; the answers and the phase message; a Linux
#        host is sent the shell document.
#   W27  `grow` on a Windows host: the same AWS calls with the host's own volume (30 GiB at its start), its
#        online step an encoded remote command that names no `growpart` or `resize2fs`, and the shared refusals.
#
# EXPECTED TO FAIL AGAINST v0.11's OPERATOR, which has no -Platform and refuses a registry record holding
# `platform`: every case reports a FAIL there, each by its own assertion. A v0.12 build MUST pass every result.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh`, `scp` and `go` that log
# every call (conformance/operator/host-commands-common.ps1 and windows-common.ps1), a host registry that the case
# writes, and the home's throwaway CA. No stand-in is a Windows host: a case reads the arguments the operator
# passes and the files it writes, and never what a host does with them. `sync`'s build runs only when
# C:\Projects\localswim exists (docs/operator.md, sync, step 7), so a case that shows the build reports SKIPPED,
# naming the folder, when it is absent. That is one Test-Path -PathType Container on it; no case lists, reads or
# writes anything in it, and the `go` stand-in does not build.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes every
# AWS credential. Before any program that can reach AWS starts, the check confirms under the same environment that
# `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports SKIPPED, running nothing,
# if it does not. Every instance and image ID is made up, the token is a made-up one that the ssh stand-in looks for
# and never prints, and a failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-windows-deploy-sync.ps1
. (Join-Path $PSScriptRoot 'windows-common.ps1')

$ErrorActionPreference = 'Stop'

Invoke-HostFile -FileName 'test-unmet-windows-deploy-sync.ps1' -Push -Body {
  param($fx)
  $goDir = Initialize-WnStandIns
  Set-WnTokenNeedle $fx.Token
  $project = 'example'
  $wn = 'windows/amd64'
  $idW = $script:WnIdWin; $idA = $script:WnIdArm; $idL = $script:WnIdAmd
  $stackW = Get-WnStack $project $wn
  $aliasW = Get-WnAlias $project $wn
  $image = $script:WnImage
  $patchCheck = "& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -NonInteractive -File C:\ProgramData\agentspace\agentspace-patch-check.ps1"
  $logHint = "cat C:/ProgramData/agentspace/patch.log"
  $withGo = @{ AGENTSPACE_TIMEZONE = 'Mountain Standard Time' }
  $noPush = @{ AGENTSPACE_NESTED = '1' }
  $recentLaunch = { Format-WnAwsTime ((Get-WnNow).AddMinutes(-2)) }
  # One Windows host that is ready, running and launched long ago, and the box.
  $seedWin = {
    Remove-WnLeases
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn) -Size 30) (Get-HealthyAnswers -Id $script:IdBox))
  }

  # ============ W23: deploy passes the Windows image and type at creation alone ============
  $s23 = 'operator.md#deploy'
  $ca = ([IO.File]::ReadAllText((Join-Path $fx.CaDir 'agentspace_ca.pub')).Trim() -split '\s+')[0..1] -join ' '
  $key = $fx.PubKey
  $sub = "SubnetId=$($fx.SubnetValue)"; $vpc = "VpcId=$($fx.VpcValue)"
  $createOverrides = @("SshPublicKey=$key", $sub, $vpc, "HostName=$stackW", "CaPublicKey=$ca", "ImageId=$image", 'RootVolumeGiB=30', 'HostOs=windows')
  $deploying = "Deploying stack '$stackW' in us-west-2..."
  $noInstanceId = "agentspace: Stack '$stackW' has no InstanceId output."
  $creatingBytes = Get-HostsBytes @((New-BoxRecord), (New-WnRecord -Project $project -Platform $wn -State 'creating' -LastAttempt '2026-01-01T00:00:00Z'))
  # A Windows host whose record exists, and whose stack is answered by $Steps (its existence check, then step 5's read).
  function Set-WnStackCase($Steps, [hashtable]$More = @{}) {
    Set-HostsFile $creatingBytes
    Set-AwsScenario (Merge-Scenario @{ "cloudformation describe-stacks $stackW" = @($Steps) } $More)
  }
  $noOutput = New-Step (New-StackDoc -StackName $stackW)
  $noImageInOutput = { param($run) foreach ($t in @($run.Stdout, $run.Stderr)) { if ($t.Contains($image)) { 'A line the operator printed holds the image ID' } } }
  Test-HostCase "${s23}: W23 deploy -Host <project> -Platform windows/amd64 for a stack that does not exist reads the image parameter once, then passes the stack, HostName, ImageId, RootVolumeGiB=30, HostOs=windows and InstanceType=m8a.xlarge last, each one argument" @('deploy', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId))
    Calls = @((Get-CallStack $stackW), (Get-WnCallGetParameter $script:WnImageParameter), (Get-WnDeployCall $stackW ($createOverrides + 'InstanceType=m8a.xlarge')), (Get-CallStack $stackW))
    Check = { param($run, $started) & $noImageInOutput $run } } -Setup {
    Set-WnStackCase @((New-Step '' (Get-WnDoesNotExist $stackW) 254), $noOutput) @{ 'ssm get-parameter' = @(Get-WnImageAnswer $image) }
  }
  Test-HostCase "${s23}: W23 deploy -Host <project> -Platform windows/amd64 -InstanceType <type> of a new stack passes InstanceType=<type> last in place of the platform's type" @('deploy', '-Host', $project, '-Platform', $wn, '-InstanceType', 'm8a.2xlarge') @{
    Exit = 1
    Calls = @((Get-CallStack $stackW), (Get-WnCallGetParameter $script:WnImageParameter), (Get-WnDeployCall $stackW ($createOverrides + 'InstanceType=m8a.2xlarge')), (Get-CallStack $stackW))
    Check = { param($run, $started) & $noImageInOutput $run } } -Setup {
    Set-WnStackCase @((New-Step '' (Get-WnDoesNotExist $stackW) 254), $noOutput) @{ 'ssm get-parameter' = @(Get-WnImageAnswer $image) }
  }
  foreach ($status in 'CREATE_COMPLETE', 'UPDATE_COMPLETE') {
    $st = $status
    Test-HostCase "${s23}: W23 deploy -Host <project> -Platform windows/amd64 for a stack in $st reads no image parameter and passes no ImageId, RootVolumeGiB or HostOs: no parameter at all, every parameter's previous value" @('deploy', '-Host', $project, '-Platform', $wn) @{
      Exit = 1; Stderr = (Join-Lines @($deploying, $noInstanceId))
      Calls = @((Get-CallStack $stackW), (Get-WnDeployCall $stackW), (Get-CallStack $stackW))
      Check = { param($run, $started) & $noImageInOutput $run } } -Setup {
      Set-WnStackCase @((New-Step (New-StackDoc -StackName $stackW -Status $st)))
    }
  }
  Test-HostCase "${s23}: W23 a malformed image answer fails deploy before any call that changes anything, with the message that holds no part of the answer" @('deploy', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Stderr = "agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.`n"
    Calls = @((Get-CallStack $stackW), (Get-WnCallGetParameter $script:WnImageParameter))
    Check = { param($run, $started)
      & $noImageInOutput $run
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $creatingBytes)) { 'hosts.json was changed' } } } -Setup {
    Set-WnStackCase @((New-Step '' (Get-WnDoesNotExist $stackW) 254), $noOutput) @{ 'ssm get-parameter' = @(Get-WnImageAnswer 'not-an-image') }
  }
  $paramsWith = { param([string]$Members) '{"version":0,"parameters":{' + $Members + '}}' }
  $hostOsMembers = '"SubnetId":"' + $fx.SubnetValue + '","VpcId":"' + $fx.VpcValue + '","HostOs":"windows"'
  $badMember = {
    param($run, [string]$Member)
    $m = [regex]::Match($run.Stderr.TrimEnd("`n"), '^agentspace: (.+) is not a valid stack parameters file: (.+)\.$')
    if (-not $m.Success) { return 'Stderr is not agentspace: <path> is not a valid stack parameters file: <reason>.' }
    if (-not $m.Groups[2].Value.Contains($Member)) { "The reason does not name $Member" }
    if ($m.Groups[2].Value.Contains("`n")) { 'The reason holds a line break' }
  }
  Test-HostCase "${s23}: W23 a stack parameters file that names HostOs is refused for the box, with no AWS call at all" @('deploy') @{
    Exit = 1; Calls = @(); Check = { param($run, $started) & $badMember $run 'HostOs' } } -Setup { Set-ParamsFile (& $paramsWith $hostOsMembers) }
  Test-HostCase "${s23}: W23 a stack parameters file that names HostOs is refused for a new Windows host's stack, before any call that changes anything" @('deploy', '-Host', $project, '-Platform', $wn) @{
    Exit = 1; Calls = @(Get-CallStack $stackW)
    Check = { param($run, $started)
      & $badMember $run 'HostOs'
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $creatingBytes)) { 'hosts.json was changed' } } } -Setup {
    Set-WnStackCase @((New-Step '' (Get-WnDoesNotExist $stackW) 254))
    Set-ParamsFile (& $paramsWith $hostOsMembers)
  }

  # ============ W24: bootstrap sends the script and runs it under Windows PowerShell 5.1 ============
  $s24 = 'operator.md#bootstrap'
  $bootErr = 'agentspace: The toolchain bootstrap failed on the instance.'
  $bootstrap = @('bootstrap', '-Host', $project, '-Platform', $wn)
  Test-HostCase "${s24}: W24 bootstrap -Host <project> -Platform windows/amd64 copies bootstrap.ps1 by scp, then runs it with -File in one encoded remote command that holds no line of the script, with an empty stdin, and prints Bootstrapped: <id>" $bootstrap @{
    Exit = 0; StderrLines = @('Running the toolchain bootstrap on the instance...', "Bootstrapped: $idW"); Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $ssh = @(Get-SshCalls); $scp = @(Get-ScpCalls)
      if ($scp.Count -ne 1 -or $ssh.Count -ne 1) { return "There were $($scp.Count) scp and $($ssh.Count) ssh calls, not 1 and 1" }
      $sa = @($scp[0].a)
      if ($sa.Count -ne 5 -or $sa[0] -cne '-o' -or $sa[1] -cne 'ClearAllForwardings=yes' -or $sa[2] -cne '-q' -or $sa[4] -cne "${aliasW}:.agentspace-bootstrap.ps1") { 'The scp is not -o ClearAllForwardings=yes -q <bootstrap.ps1> <alias>:.agentspace-bootstrap.ps1' }
      $l = $scp[0].locals[0]
      if ($null -eq $l -or $l.leaf -cne 'bootstrap.ps1' -or -not $l.exists -or $l.isDir) { 'The scp does not send the file bootstrap.ps1' }
      elseif ($l.path -cnotmatch '\\\.agentspace\\operator\\[0-9a-f]{32}\\bootstrap\.ps1$') { 'The script is not in the command''s own operator\<hex>\ folder' }
      elseif ($l.sha -cne (Get-WnRepoFileSha 'box\windows\bootstrap.ps1')) { 'The file sent is not the repository''s box\windows\bootstrap.ps1' }
      $a = @($ssh[0].a)
      if ($a.Count -ne 4 -or $a[0] -cne '-o' -or $a[1] -cne 'ClearAllForwardings=yes' -or $a[2] -cne $aliasW) { 'The ssh is not -o ClearAllForwardings=yes <alias> <remote command>' }
      if ($a -contains 'BatchMode=yes') { 'The ssh is the push''s' }
      if (@($a | Where-Object { $_ -match '(?i)\bbash\b' }).Count -gt 0) { 'An ssh argument names bash' }
      $cmd = Get-WnRemote $ssh[0]
      foreach ($m in (Get-WnEncodedProblems $cmd 'The ssh')) { $m }
      foreach ($m in (Get-WnLeakProblems $cmd 'The ssh' -LineOf 'bootstrap.ps1')) { $m }
      $script1 = Get-WnScript $cmd
      if ($null -eq $script1 -or -not $script1.Contains('.agentspace-bootstrap.ps1') -or -not $script1.Contains('-File')) { 'The script the ssh carries does not run .agentspace-bootstrap.ps1 with -File' }
      $in = Get-FileBytesOrNull (Join-Path $fx.Standins 'ssh-stdin-1.bin')
      if ($null -eq $in -or $in.Length -ne 0) { 'The ssh''s stdin was not empty and closed' }
      if ([long]$scp[0].t -ge [long]$ssh[0].t) { 'The copy did not come before the run' }
      if ($ssh[0].tokenArg -or $ssh[0].tokenEnv -or $ssh[0].tokenFile) { 'The token was within reach of the bootstrap''s ssh' } } } -Setup $seedWin
  foreach ($code in 7, 255) {
    $c = $code
    Test-HostCase "${s24}: W24 an ssh that exits $c fails bootstrap with the toolchain bootstrap failure, exit 1, and no Bootstrapped line" $bootstrap @{
      Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; StderrNot = @('Bootstrapped:')
      Check = { param($run, $started) if (@($run.Stderr -split "`n" | Where-Object { $_ -ne '' } | Select-Object -Last 1)[0] -cne $bootErr) { 'The last line of stderr is not the toolchain bootstrap failure' } } } -Setup {
      & $seedWin
      Set-ProgramAnswer -Program ssh -Code $c
    }
  }
  Test-HostCase "${s24}: W24 a failed copy fails bootstrap with the toolchain bootstrap failure, exit 1" $bootstrap @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; StderrNot = @('Bootstrapped:')
    Check = { param($run, $started) if (@($run.Stderr -split "`n" | Where-Object { $_ -ne '' } | Select-Object -Last 1)[0] -cne $bootErr) { 'The last line of stderr is not the toolchain bootstrap failure' } } } -Setup {
    & $seedWin
    Set-ProgramAnswer -Program scp -Code 1
  }

  # ============ W25: sync builds the localswim CLI for windows/amd64 and sends box\windows ============
  $s25 = 'operator.md#sync'
  $localswimFolder = 'C:\Projects\localswim'
  $syncSetup = { & $seedWin; Set-WnClaudeMd }
  if (-not (Test-WnLocalswimFolder)) {
    Write-ConformanceResult -Name "${s25}: W25 sync -Host <project> -Platform windows/amd64 builds and sends in its nine rows" -Status SKIPPED -Reason "The folder $localswimFolder does not exist on this machine, so sync's step 7 builds nothing and a case that shows the build cannot run"
    Write-ConformanceResult -Name "${s25}: W25 sync -All over a Linux host and a Windows host builds once for each platform" -Status SKIPPED -Reason "The folder $localswimFolder does not exist on this machine, so sync's step 7 builds nothing and a case that shows the build cannot run"
  } else {
    $idBox = $script:IdBox
    Test-HostCase "${s25}: W25 sync -Host <project> -Platform windows/amd64 prints the Windows build line, runs go build with GOOS=windows, GOARCH=amd64 and CGO_ENABLED=0, and sends box\windows, the CA's key, CLAUDE.md and the build, in the table's nine rows" @('sync', '-Host', $project, '-Platform', $wn) @{
      Exit = 0
      StderrLines = @("Checking that $idW has finished this boot's patch run...", "Patched: $idW", 'Building the localswim CLI for windows/amd64...', 'Copying the localswim CLI to the instance...', 'Sending the localswim Bot token to the instance...')
      Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $tl = @(Get-WnTimeline)
        $kinds = @($tl | ForEach-Object { $_.K }) -join ','
        if ($kinds -cne 'ssh,scp,scp,ssh,ssh,scp,scp,ssh,ssh') { return "The ssh and scp calls are not rows 1 to 9 of the table, in order (ssh,scp,scp,ssh,ssh,scp,scp,ssh,ssh); there were $($tl.Count)" }
        $row = { param([int]$N) $tl[$N - 1].C }
        # Rows 1 and 4, 5 and 8 are remote commands of the encoded form; row 9 is the push's.
        foreach ($n in 1, 4, 5, 8, 9) {
          $call = & $row $n
          $a = @($call.a)
          $want = if ($n -eq 9) { @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $aliasW) } else { @('-o', 'ClearAllForwardings=yes', $aliasW) }
          if ($a.Count -ne $want.Count + 1 -or (($a | Select-Object -First $want.Count) -join [char]0) -cne ($want -join [char]0)) { "Row $n is not -o ClearAllForwardings=yes$(if ($n -eq 9) { ' -o BatchMode=yes' }) <alias> <remote command>" }
          foreach ($m in (Get-WnEncodedProblems (Get-WnRemote $call) "Row $n")) { $m }
        }
        $install = Get-WnScript (Get-WnRemote (& $row 4))
        if ($null -eq $install -or -not $install.Contains('install.ps1') -or -not $install.Contains('-File') -or -not $install.Contains('.agentspace-system')) { 'Row 4 does not run .agentspace-system\install.ps1 with -File' }
        $s2 = & $row 2
        $a = @($s2.a); $l = $s2.locals[0]
        if ($a.Count -ne 6 -or (($a | Select-Object -First 4) -join ' ') -cne '-o ClearAllForwardings=yes -q -r' -or $a[5] -cne "${aliasW}:.agentspace-system") { 'Row 2 is not -o ClearAllForwardings=yes -q -r <folder> <alias>:.agentspace-system' }
        elseif ($l.leaf -cne 'windows') { 'Row 2 does not send a folder named windows' }
        else {
          foreach ($m in (Get-WnFolderProblems $l 'box\windows' 'The folder row 2 sends' -Without 'tests')) { $m }
          if ($l.path -cnotmatch '\\\.agentspace\\operator\\[0-9a-f]{32}\\windows$') { 'The folder row 2 sends is not in the command''s own operator\<hex>\ folder' }
        }
        $a = @((& $row 3).a)
        if ($a.Count -ne 5 -or (($a | Select-Object -First 3) -join ' ') -cne '-o ClearAllForwardings=yes -q' -or $a[4] -cne "${aliasW}:.agentspace-system/agentspace_ca.pub" -or $a[3] -cnotmatch '\\\.agentspace\\ca\\agentspace_ca\.pub$') { 'Row 3 is not -o ClearAllForwardings=yes -q <the CA''s public key> <alias>:.agentspace-system/agentspace_ca.pub' }
        $a = @((& $row 6).a)
        if ($a.Count -ne 5 -or (($a | Select-Object -First 3) -join ' ') -cne '-o ClearAllForwardings=yes -q' -or $a[4] -cne "${aliasW}:.claude/CLAUDE.md" -or $a[3] -cnotmatch '\\\.claude\\CLAUDE\.md$') { 'Row 6 is not -o ClearAllForwardings=yes -q <CLAUDE.md> <alias>:.claude/CLAUDE.md' }
        $a = @((& $row 7).a)
        if ($a.Count -ne 5 -or (($a | Select-Object -First 3) -join ' ') -cne '-o ClearAllForwardings=yes -q' -or $a[4] -cne "${aliasW}:.agentspace-localswim.exe" -or $a[3] -cnotmatch '\\\.agentspace\\operator\\[0-9a-f]{32}\\localswim-windows-amd64\.exe$') { 'Row 7 is not -o ClearAllForwardings=yes -q <operator\<hex>\localswim-windows-amd64.exe> <alias>:.agentspace-localswim.exe' }
        $go = @(Get-WnGoCalls)
        if ($go.Count -ne 1) { return "go was started $($go.Count) times, not once" }
        $ga = @($go[0].a)
        $i = [array]::IndexOf($ga, '-o')
        if ($ga.Count -ne 5 -or $ga[0] -cne 'build' -or $ga[1] -cne '-trimpath' -or $i -ne 2 -or $ga[4] -cne './cmd/localswim') { 'The build is not exactly go build -trimpath -o <out> ./cmd/localswim' }
        elseif ($ga[3] -cnotmatch '\\\.agentspace\\operator\\[0-9a-f]{32}\\localswim-windows-amd64\.exe$') { 'The build''s output is not localswim-windows-amd64.exe in the command''s own folder' }
        elseif ([IO.Path]::GetFullPath($ga[3]) -ine [IO.Path]::GetFullPath(@((& $row 7).a)[3])) { 'Row 7 does not copy the file the build wrote' }
        if ($go[0].goenv.GOOS -cne 'windows' -or $go[0].goenv.GOARCH -cne 'amd64' -or $go[0].goenv.CGO_ENABLED -cne '0') { 'The build is not run with GOOS=windows, GOARCH=amd64 and CGO_ENABLED=0' }
        # The patch wait comes after row 4 and before row 5.
        $send = @(Get-WnSends)
        if ($send.Count -ne 1) { 'The patch wait did not send exactly one check' }
        elseif ([long]$send[0].t -lt [long]$tl[3].T -or [long]$send[0].t -gt [long]$tl[4].T) { 'The patch wait did not run between rows 4 and 5' }
        if ((Test-Path -LiteralPath $fx.OperatorDir) -and @(Get-ChildItem -LiteralPath $fx.OperatorDir -Force | Where-Object { $_.Name -cmatch '^[0-9a-f]{32}$' }).Count -gt 0) { 'A folder was left under operator\' } } } -Setup $syncSetup -Environment $withGo
    # sync -All over the box, a Linux host and a Windows host.
    $idLinux = $idA
    Test-HostCase "${s25}: W25 sync -All over a Linux host and a Windows host builds once for each platform present, sends each host the files of its own platform, and copies each host the build of its own" @('sync', '-All') @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      StderrLines = @('Building the localswim CLI for linux/arm64...', 'Building the localswim CLI for windows/amd64...')
      Check = { param($run, $started)
        $go = @(Get-WnGoCalls)
        $built = @($go | ForEach-Object { "$($_.goenv.GOOS)/$($_.goenv.GOARCH)" } | Sort-Object)
        if (($built -join ',') -cne 'linux/arm64,windows/amd64') { 'go was not run once for each platform present, linux/arm64 and windows/amd64' }
        foreach ($line in 'Building the localswim CLI for linux/arm64...', 'Building the localswim CLI for windows/amd64...') {
          if (@($run.Stderr -split "`n" | Where-Object { $_ -ceq $line }).Count -ne 1) { "The line '$line' was not printed once" }
        }
        $scp = @(Get-ScpCalls)
        $send = @($scp | Where-Object { $_.locals.Count -gt 0 -and $_.locals[0].isDir })
        $win = @($send | Where-Object { $_.locals[0].leaf -ceq 'windows' })
        $sys = @($send | Where-Object { $_.locals[0].leaf -ceq 'system' })
        if ($win.Count -ne 1 -or $sys.Count -ne 2) { return "The folders sent were $($win.Count) windows and $($sys.Count) system, not 1 and 2" }
        if (@($win[0].a)[-1] -cne "${aliasW}:.agentspace-system") { 'box\windows was sent to another alias than the Windows host''s' }
        foreach ($m in (Get-WnFolderProblems $win[0].locals[0] 'box\windows' 'The Windows host''s folder' -Without 'tests')) { $m }
        foreach ($s in $sys) {
          if (@($s.a)[-1] -cnotmatch '^agentspace(-host\.example)?:/tmp/agentspace-system$') { 'box\system was sent to something other than a Linux host''s staging folder' }
          foreach ($m in (Get-WnFolderProblems $s.locals[0] 'box\system' 'A Linux host''s folder')) { $m }
        }
        $cli = @($scp | Where-Object { $_.locals.Count -gt 0 -and -not $_.locals[0].isDir -and $_.locals[0].leaf -like 'localswim-*' })
        $winCli = @($cli | Where-Object { $_.locals[0].leaf -ceq 'localswim-windows-amd64.exe' })
        $armCli = @($cli | Where-Object { $_.locals[0].leaf -ceq 'localswim-linux-arm64' })
        if ($winCli.Count -ne 1 -or $armCli.Count -ne 2) { 'Each Linux host was not given the linux/arm64 build and the Windows host the windows/amd64 build' }
        elseif (@($winCli[0].a)[-1] -cne "${aliasW}:.agentspace-localswim.exe") { 'The Windows build was copied to another alias than the Windows host''s' }
        $hostLines = @($run.Stderr -split "`n" | Where-Object { $_ -cmatch '^Host      ' })
        if ($hostLines.Count -ne 3 -or $hostLines[2] -cne "Host      $(Get-WnHostName $project $wn) ($stackW), ready") { 'The Host lines are not the box, the arm64 host and the Windows host, in the file''s order' }
      } } -Setup {
      Remove-WnLeases
      Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -State 'ready' -InstanceId $script:WnIdArm), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin))
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) (Get-HealthyAnswers -Id $script:WnIdArm) (Get-HealthyAnswers -Id $script:IdBox))
      Set-WnClaudeMd
    } -Environment $withGo
  }

  # ============ W26: up's patch check and sync's patch wait use the PowerShell document ============
  $s26 = 'operator.md#what-up-waits-for'
  $phase = 'Patching Windows (Windows Update, restart if an update needs one)...'
  $answer = { param([string]$Word) New-Step ('{"Status":"Success","StandardOutputContent":"' + $Word + '\n"}') }
  $sendsProblems = {
    param([string]$Comment, [int]$Count)
    $sends = @(Get-WnSends)
    if ($sends.Count -ne $Count) { "There were $($sends.Count) patch checks, not $Count" }
    $i = 0
    foreach ($c in $sends) {
      $i++
      foreach ($m in (Get-WnSendProblems $c "Check $i" $Comment)) { $m }
      if ((Get-WnSendCommand $c) -cne $patchCheck) { "Check $i's command is not PowerShell 7 by its full path running agentspace-patch-check.ps1" }
    }
  }
  $recentWin = {
    Remove-WnLeases
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin))
  }
  Test-HostCase "${s26}: W26 up's patch check names AWS-RunPowerShellScript and passes the command as one JSON argument, {commands: [one string]}, naming pwsh.exe by its full path; a pending answer prints the Windows phase message, then patched ends the wait" @('up', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; StderrLines = @($phase, "Ready: $idW"); StderrNot = @('Patching Ubuntu'); Calls = 'any'
    Check = { param($run, $started) & $sendsProblems 'agentspace up: patch check' 2 } } -Setup {
    & $recentWin
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) @{
        "ec2 describe-instances $script:WnIdWin" = @(New-Step (New-InstanceDoc -Id $script:WnIdWin -Type (Get-WnType $wn) -Launch (& $recentLaunch)))
        'ssm get-command-invocation' = @((& $answer 'pending'), (& $answer 'patched'))
      })
  } -Environment $noPush
  foreach ($word in 'patched', 'failed', 'unknown') {
    $w = $word
    Test-HostCase "${s26}: W26 up reads a '$w' answer as ready: its wait ends with no phase message, and one check was sent" @('up', '-Host', $project, '-Platform', $wn) @{
      Exit = 0; Stderr = "Ready: $idW`n"; Calls = 'any'
      Check = { param($run, $started) & $sendsProblems 'agentspace up: patch check' 1 } } -Setup {
      & $recentWin
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) @{
          "ec2 describe-instances $script:WnIdWin" = @(New-Step (New-InstanceDoc -Id $script:WnIdWin -Type (Get-WnType $wn) -Launch (& $recentLaunch)))
          'ssm get-command-invocation' = @(& $answer $w)
        })
    } -Environment $noPush
  }
  Test-HostCase "${s26}: W26 up reads an invocation that ended with no output as ready" @('up', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; Stderr = "Ready: $idW`n"; Calls = 'any'
    Check = { param($run, $started) & $sendsProblems 'agentspace up: patch check' 1 } } -Setup {
    & $recentWin
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) @{
        "ec2 describe-instances $script:WnIdWin" = @(New-Step (New-InstanceDoc -Id $script:WnIdWin -Type (Get-WnType $wn) -Launch (& $recentLaunch)))
        'ssm get-command-invocation' = @(New-Step '{"Status":"Success"}')
      })
  } -Environment $noPush
  Test-HostCase "${s26}: W26 up of a linux/amd64 host sends the shell document, AWS-RunShellScript with commands=, and never the PowerShell one" @('up', '-Host', $project, '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = "Ready: $idL`n"; Calls = 'any'
    Check = { param($run, $started)
      $sends = @(Get-WnSends)
      if ($sends.Count -ne 1) { return "There were $($sends.Count) patch checks, not 1" }
      if ((Get-WnFlag $sends[0] '--document-name') -cne 'AWS-RunShellScript') { 'A Linux host was not sent AWS-RunShellScript' }
      if (-not (Get-WnFlag $sends[0] '--parameters').StartsWith('commands=')) { 'A Linux host''s check is not passed as commands=' }
      if (@($sends[0].a | Where-Object { $_ -match 'PowerShell' }).Count -gt 0) { 'A Linux host''s check names the PowerShell document or PowerShell' } } } -Setup {
    Remove-WnLeases
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdAmd))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdAmd -Type (Get-WnType 'linux/amd64')) @{
        "ec2 describe-instances $script:WnIdAmd" = @(New-Step (New-InstanceDoc -Id $script:WnIdAmd -Type (Get-WnType 'linux/amd64') -Launch (& $recentLaunch)))
      })
  } -Environment $noPush
  $s26b = 'operator.md#the-patch-wait'
  Test-HostCase "${s26b}: W26 sync's patch wait sends the same PowerShell document with the comment agentspace sync: patch check, prints the Windows phase message while pending, and Patched: <id> when done" @('sync', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; StderrLines = @("Checking that $idW has finished this boot's patch run...", $phase, "Patched: $idW"); StderrNot = @('Patching Ubuntu'); Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started) & $sendsProblems 'agentspace sync: patch check' 2 } } -Setup {
    & $syncSetup
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) @{ 'ssm get-command-invocation' = @((& $answer 'pending'), (& $answer 'patched')) })
  } -Environment $noPush
  Test-HostCase "${s26b}: W26 sync's patch wait ends at once on a failed answer with the warning that names cat C:/ProgramData/agentspace/patch.log, and sync goes on and exits 0" @('sync', '-Host', $project, '-Platform', $wn) @{
    Exit = 0; StderrLines = @("Checking that $idW has finished this boot's patch run...", "agentspace: The patch run on $idW failed; sync goes on without it. Check: ssh $aliasW $logHint"); StderrNot = @("Patched: $idW"); Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $sendsProblems 'agentspace sync: patch check' 1
      if (@(Get-SshCalls).Count -lt 3) { 'sync did not go on to its later rows' } } } -Setup {
    & $syncSetup
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:WnIdWin -Type (Get-WnType $wn)) @{ 'ssm get-command-invocation' = @(& $answer 'failed') })
  } -Environment $noPush

  # ============ W27: grow on a Windows host ============
  $s27 = 'operator.md#grow'
  $growMsg = { param([string]$Id, [long]$From, [long]$To) "Growing $(Get-VolumeOf $Id) from $From to $To GiB (EBS allows one change per volume every 6 hours)..." }
  $growCalls = @((Get-CallDescribe $idW), (Get-CallVolumes $idW), (Get-CallModify $idW 34))
  Test-HostCase "${s27}: W27 grow -Host <project> -Platform windows/amd64 -GiB 4 of a running host makes the host's own AWS calls on its 30 GiB volume, waits 10 seconds, then sends one encoded remote command through the alias that names neither growpart nor resize2fs" @('grow', '-Host', $project, '-Platform', $wn, '-GiB', '4') @{
    Exit = 0; StderrLines = @((& $growMsg $idW 30 34), 'Growing the filesystem online...'); Calls = $growCalls; SshCalls = 'any'
    Check = { param($run, $started)
      $ssh = @(Get-SshCalls)
      if ($ssh.Count -ne 1) { return "There were $($ssh.Count) ssh calls, not 1" }
      $a = @($ssh[0].a)
      if ($a.Count -ne 4 -or $a[0] -cne '-o' -or $a[1] -cne 'ClearAllForwardings=yes' -or $a[2] -cne $aliasW) { 'The ssh is not -o ClearAllForwardings=yes <alias> <remote command>' }
      $cmd = Get-WnRemote $ssh[0]
      foreach ($m in (Get-WnEncodedProblems $cmd 'The ssh')) { $m }
      foreach ($m in (Get-WnLeakProblems $cmd 'The ssh')) { $m }
      # operator.md, "grow on a Windows host", names no cmdlet or program for the extension (it says "Extend the `C:`
      # partition"), so the case checks the one thing the spec rules out for it, the Linux tools.
      $script1 = Get-WnScript $cmd
      if ($null -ne $script1) {
        if ($script1 -match '(?i)growpart|resize2fs') { 'The script grows the disk the Linux way: it names growpart or resize2fs' }
      }
      $aws = @(Get-AwsCalls)
      if ($aws.Count -eq 3 -and (([long]$ssh[0].t - [long]$aws[2].t) / 1e7) -lt 9) { 'The ssh started less than 10 seconds after the volume change' }
      foreach ($m in (Get-SignProblems (Get-WnHostName $project $wn))) { $m } } } -Setup {
    & $seedWin
  }
  Test-HostCase "${s27}: W27 grow of a stopped Windows host changes its volume, prints the next-boot line, and starts no ssh" @('grow', '-Host', $project, '-Platform', $wn, '-GiB', '4') @{
    Exit = 0; Stderr = (Join-Lines @((& $growMsg $idW 30 34), 'The filesystem grows on its own at next boot.')); Calls = $growCalls } -Setup {
    Remove-WnLeases
    Set-HostsRecords @((New-BoxRecord), (New-WnRecord -Project $project -Platform $wn -State 'ready' -InstanceId $script:WnIdWin))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:WnIdWin -State 'stopped' -Type (Get-WnType $wn) -Size 30)
  }
  Test-HostCase "${s27}: W27 an ssh that exits 255 fails grow with the online-growth failure, the volume change standing" @('grow', '-Host', $project, '-Platform', $wn, '-GiB', '4') @{
    Exit = 1; StderrLines = @((& $growMsg $idW 30 34), 'Growing the filesystem online...', 'agentspace: Growing the filesystem online failed; the volume is already 34 GiB, and the filesystem grows on its own at next boot.'); Calls = $growCalls; SshCalls = 'any' } -Setup {
    & $seedWin
    Set-ProgramAnswer -Program ssh -Code 255
  }
  foreach ($gib in '0', '-1') {
    $g = $gib
    Test-HostCase "${s27}: W27 grow -Host <project> -Platform windows/amd64 -GiB $g is refused before any call, as for every host, and writes nothing" @('grow', '-Host', $project, '-Platform', $wn, '-GiB', $g) @{
      Exit = 1; Stderr = "agentspace: -GiB MUST be 1 or more for grow, not '$g'.`n"; Calls = @()
      Check = { param($run, $started) foreach ($m in (Get-NothingSignedProblems)) { $m } } } -Setup { & $seedWin }
  }
}

Exit-ConformanceRun
