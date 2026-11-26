#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.11-T04-S01, #0239; D83 to D89): `agentspace lease create --board <id>
# --epic <ticket>`, the lease CLI's one read of a localswim board, shown against a STAND-IN board and a
# STAND-IN credential, never Terry's. It drives the lease CLI only as a separate process, through the harness,
# in throwaway homes, and builds every expectation from docs/lease-api.md ("The board read", "The plan block",
# "Plan", "Board", "Epic", "Turn", "TTL", "Exit codes and errors", "Messages") and docs/conformance.md ("The
# stand-in board and its credential"), never from a program's output.
#
# What it shows, by the item of conformance.md, "What the board cases MUST show":
#   1  Each refusal of the board read, with its error code, its `field` and its message byte for byte: a board
#      or an epic outside its form, an unknown board, an unknown ticket, an archived card, no block, two blocks,
#      a malformed fence, a block never closed, a document `agentspace-plan` refuses (one case for each check),
#      a turn the plan lacks, a board that answers 401, 403, 400, 405, 406, 418, 500 or 503, a bare 404, one that
#      hangs, one that cannot be reached, a body past the 64 MiB bound, and every case of the receipt and of the
#      credential, the `localhost` base among them.
#   2  The number of requests, from the stand-in's log: one GET for a good epic, two for a ticket or a board the
#      board does not know, one for a bare 404, one for every other answer, none for a refusal made before the
#      read, and two reads for a lease made twice.
#   3  No method but GET, in every case, with a control that shows the check can see one.
#   4  The token reached the stand-in (`auth=ok`) and appears nowhere else: not in a run's output, an error
#      document, a file of the home but the fixture and bot.json's blob, a log line, the lease state file or a
#      command line, searched as text and as UTF-16LE, with the search proven on a planted copy.
#   5  The client reads three members and nothing else: the stand-in's markers in the ignored members are in no
#      output, and a card whose ignored members differ gives the same lease.
#   6  A board lease and the same plan given through --plan give the same size, `sized_for` and TTL, for the
#      plan's busiest turn and for `--turn`, ready on a seeded host and unsatisfiable for a project with no host.
#   7  `--board` and `--epic` with `--plan` or `--size`, and the other usage cases of pin P4, are refused before
#      any request, each with a stand-in listening and a log that stays empty.
#
# SAFETY (conformance.md, "What a board case never reads"). Every path the file reads or writes is built from the
# throwaway home's own object, in a variable that is never named for PowerShell's own home variable. Every write
# and delete calls Assert-ConformancePath at the call. No case reads Terry's receipt, credential or board, starts
# his service, reads the value of a variable of the caller's that names a board, or connects to a port it did
# not open: the only addresses a case or the program connects to are the stand-in's, and the one port the
# credential-alone case names (the spec's own, where nothing listens). The port of a stand-in is picked by
# binding port 0 on the loopback address. Every board case removes the caller's board variables from the program's
# environment, found by listing the names of this process's variables (names only), except the one case that sets
# two of them. The token is drawn at random per run, lives in the fixture object and in the home's fixture file
# and bot.json, and is never printed, logged, passed as an argument or put in a reason: every name and reason
# this file prints passes through ConvertTo-BdRedacted (Write-BdResult), which hides the token's text and its
# UTF-16LE form and every drive path.
#
# DPAPI. The credential's token is protected for the Windows user the case runs as. A preflight protects a random
# string here and decrypts it in a child started with the harness's environment. When it fails, every case that
# needs a token is reported SKIPPED, and the run is incomplete, never passed.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-board-plan.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

# --- Constants -----------------------------------------------------------------------------------------------

$script:BdApi = '/api/2026-11-10'
$script:BdB1 = '0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f'
$script:BdBX = '1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a'
$script:BdRevision = 8675309
$script:BdProject = 'boardproj'
$script:BdNoHost = 'nohostproj'
$script:BdFence = ([string][char]96) * 3
$script:BdBomChar = [string][char]0xFEFF
$script:BdSentinel = 'Zdesc-prose'
$script:BdDpapiOk = $false
$script:BdDpapiWhy = 'The preflight has not run'
$script:BdUtf8 = [Text.UTF8Encoding]::new($false)

# --- The token, and the one function every printed text passes through ----------------------------------------

# 43 characters of A-Z, a-z, 0-9, '-' and '_': 32 random bytes in unpadded base64url, drawn once per run.
function New-BdToken {
  $bytes = [byte[]]::new(32)
  [Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
  return [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
}
$script:BdToken = New-BdToken
# The token as a UTF-16LE byte sequence seen one byte per character: each character followed by a NUL.
$script:BdTokenWide = -join ($script:BdToken.ToCharArray() | ForEach-Object { [string]$_ + [string][char]0 })

# Hides the run's token (its text and its UTF-16LE form) and every drive path, so that no reason, name or
# message this file prints can carry either, and then passes the text through the suite's mask
# (conformance\mask-common.ps1), which hides the rest of the temp folder's, the profile's, the user's and the
# computer's names. Applied to every name and reason (Write-BdResult), before Write-BdResult cuts a reason.
function ConvertTo-BdRedacted {
  param([AllowNull()][AllowEmptyString()][string]$Text)
  if ([string]::IsNullOrEmpty($Text)) { return '' }
  $x = $Text.Replace($script:BdToken, '<token>').Replace($script:BdTokenWide, '<token>')
  $x = [regex]::Replace($x, '[A-Za-z]:[\\/][^\s''"]*', '<path>')
  $x = ConvertTo-ConformanceMasked $x
  return $x
}

function Write-BdResult {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][ValidateSet('PASS', 'FAIL', 'SKIPPED')][string]$Status, [string]$Reason = '')
  $why = (ConvertTo-BdRedacted $Reason) -replace '\s+', ' '
  if ($why.Length -gt 600) { $why = $why.Substring(0, 600) + '...' }
  Write-ConformanceResult -Name (ConvertTo-BdRedacted $Name) -Status $Status -Reason $why
}

function Complete-BdCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-BdResult -Name $Name -Status PASS }
  else { Write-BdResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}

function New-BdProblems { return , [System.Collections.Generic.List[string]]::new() }

# --- Small helpers -------------------------------------------------------------------------------------------

# A value as a message quotes it (lease-api.md, Messages): each control character as \u and four lowercase hex digits.
function Format-BdQuoted([string]$Value) {
  $sb = [Text.StringBuilder]::new()
  foreach ($c in $Value.ToCharArray()) {
    $n = [int]$c
    if ($n -le 0x1F -or $n -eq 0x7F) { [void]$sb.Append('\u' + $n.ToString('x4')) } else { [void]$sb.Append($c) }
  }
  return $sb.ToString()
}

# Picks a port by binding port 0 on the loopback address, reading it and closing the socket. Never a fixed port.
function Get-BdFreePort {
  for ($i = 0; $i -lt 20; $i++) {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    if ($port -ne 8811) { return $port }
  }
  throw 'No usable port could be picked.'
}

# The names of the caller's board variables, each set to $null so that -Environment removes it. Names only: no
# value is ever read.
function Get-BdLocalswimNulls {
  $h = @{}
  foreach ($n in [Environment]::GetEnvironmentVariables().Keys) {
    if (([string]$n).StartsWith('LOCALSWIM_', [StringComparison]::OrdinalIgnoreCase)) { $h[[string]$n] = $null }
  }
  return $h
}

function Get-BdEnvironment {
  param([hashtable]$Set = @{})
  $e = Get-BdLocalswimNulls
  foreach ($k in $Set.Keys) { $e[$k] = $Set[$k] }
  return $e
}

# Writes bytes to a path in the home, the folder first. Both writes are checked at the call.
function Write-BdFile {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Bytes)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), $Bytes)
}
function Write-BdText {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyString()][string]$Text, [switch]$Bom)
  $bytes = $script:BdUtf8.GetBytes($Text)
  if ($Bom) { $bytes = [byte[]](@(0xEF, 0xBB, 0xBF) + $bytes) }
  Write-BdFile -Lab $Lab -Path $Path -Bytes $bytes
}

# --- The lab and its context ---------------------------------------------------------------------------------

function New-BdContext {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Name)
  [pscustomobject]@{
    Lab = $Lab; Name = $Name
    Runs = [System.Collections.Generic.List[object]]::new()
    Args = [System.Collections.Generic.List[string]]::new()
    StandIns = [System.Collections.Generic.List[object]]::new()
    Cur = $null; LogCount = 0; Markers = @()
  }
}

# --- The stand-in board --------------------------------------------------------------------------------------

function Start-BdProcessWithEnvironment {
  param([Parameter(Mandatory)][string]$FilePath, [string[]]$Arguments, [Parameter(Mandatory)][System.Collections.IDictionary]$ChildEnvironment, [string]$WorkingDirectory)
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $FilePath
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  if ($WorkingDirectory) { $psi.WorkingDirectory = $WorkingDirectory }
  foreach ($name in $ChildEnvironment.Keys) {
    if ($null -eq $ChildEnvironment[$name]) { [void]$psi.Environment.Remove($name) } else { $psi.Environment[$name] = [string]$ChildEnvironment[$name] }
  }
  return [Diagnostics.Process]::Start($psi)
}

# Starts one stand-in on a port the case picked, for one fixture, and waits for its ready file under a 30 second
# deadline. A start that exits 2 before it is ready (the port could not be bound) is tried again on another
# port, at most 3 times in all. Throws when it cannot be started. The fixture is written in the home first.
function Start-BdBoard {
  param([Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)]$Fixture, [Parameter(Mandatory)][string]$Key)
  $lab = $Ctx.Lab
  $standinScript = Join-Path $PSScriptRoot 'board-standin.ps1'
  $dir = Join-Path $lab.Root "board\$Key"
  $fixturePath = Join-Path $dir 'fixture.json'
  $logPath = Join-Path $dir 'log.txt'
  $readyPath = Join-Path $dir 'ready.txt'
  Write-BdText -Lab $lab -Path $fixturePath -Text (ConvertTo-Json -InputObject $Fixture -Depth 12 -Compress)
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $lab -Override (Get-BdLocalswimNulls)
  $last = 'It was not started'
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    foreach ($stale in @($logPath, $readyPath)) {
      if (Test-Path -LiteralPath $stale) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $stale) -Force }
    }
    $port = Get-BdFreePort
    $p = Start-BdProcessWithEnvironment -FilePath ([Environment]::ProcessPath) -ChildEnvironment $childEnv -WorkingDirectory $lab.Root -Arguments @(
      '-NoProfile', '-File', $standinScript, '-Port', [string]$port, '-Fixture', (Assert-ConformancePath -ConformanceHome $lab -Path $fixturePath),
      '-Log', (Assert-ConformancePath -ConformanceHome $lab -Path $logPath), '-Ready', (Assert-ConformancePath -ConformanceHome $lab -Path $readyPath))
    $s = [pscustomobject]@{
      Process = $p; Pid = $p.Id; StartTime = $p.StartTime; Port = $port; Log = $logPath; Ready = $readyPath; Fixture = $fixturePath
      Out = [IO.MemoryStream]::new(); Err = [IO.MemoryStream]::new(); Tasks = @(); OutBytes = [byte[]]@(); ErrBytes = [byte[]]@()
    }
    $s.Tasks = @($p.StandardOutput.BaseStream.CopyToAsync($s.Out), $p.StandardError.BaseStream.CopyToAsync($s.Err))
    try { $p.StandardInput.Close() } catch { }
    $Ctx.StandIns.Add($s)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    $ready = $false
    while ([DateTime]::UtcNow -lt $deadline) {
      if ($p.HasExited) { break }
      if (Test-Path -LiteralPath $readyPath -PathType Leaf) {
        $bytes = $null
        try { $bytes = [IO.File]::ReadAllBytes($readyPath) } catch { }
        if ($null -ne $bytes -and $bytes.Length -gt 0) {
          $text = $script:BdUtf8.GetString($bytes)
          if ($text -cmatch '\A[0-9]+\n\z' -and $bytes[0] -ne 0xEF) {
            if ([int]$text.Trim() -ne $s.Pid) { $Ctx.Cur = $s; Stop-BdBoard -Ctx $Ctx; throw 'The ready file holds a process ID other than the one the case started.' }
            $ready = $true
            break
          }
        }
      }
      Start-Sleep -Milliseconds 100
    }
    if ($ready) { $Ctx.Cur = $s; $Ctx.LogCount = 0; return $s }
    $code = $null
    if ($p.HasExited) { $code = $p.ExitCode } else { $Ctx.Cur = $s; Stop-BdBoard -Ctx $Ctx; throw 'The stand-in did not write its ready file within 30 seconds.' }
    $Ctx.Cur = $s
    Stop-BdBoard -Ctx $Ctx
    $last = "The stand-in ended with exit code $code before it was ready"
    if ($code -ne 2) { throw $last }
  }
  throw "$last, 3 times in a row"
}

# Ends the current stand-in by its own handle, after checking that its process ID and its creation time are the
# ones recorded at its start. Nothing else is touched.
function Stop-BdBoard {
  param([Parameter(Mandatory)]$Ctx)
  $s = $Ctx.Cur
  if ($null -eq $s) { return }
  try {
    if (-not $s.Process.HasExited) {
      $same = $false
      try { $same = ($s.Process.Id -eq $s.Pid -and $s.Process.StartTime -eq $s.StartTime -and $s.Process.ProcessName -like 'pwsh*') } catch { }
      if ($same) { try { $s.Process.Kill($true) } catch { } }
      [void]$s.Process.WaitForExit(10000)
    }
  } catch { }
  foreach ($t in $s.Tasks) { try { [void]$t.Wait(5000) } catch { } }
  $s.OutBytes = $s.Out.ToArray()
  $s.ErrBytes = $s.Err.ToArray()
  $Ctx.Cur = $null
}

function Read-BdLogLines {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return , [string[]]@() }
  $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  try { $text = [IO.StreamReader]::new($fs, $script:BdUtf8).ReadToEnd() } finally { $fs.Dispose() }
  if ($text.Length -eq 0) { return , [string[]]@() }
  $parts = @($text -split "`n")
  if ($parts[$parts.Count - 1] -eq '') { $parts = @($parts | Select-Object -First ($parts.Count - 1)) }
  return , [string[]]$parts
}

# The log lines the stand-in wrote since the last call. Read after the program has exited; counts lines, never times.
function Get-BdNewLog {
  param([Parameter(Mandatory)]$Ctx)
  if ($null -eq $Ctx.Cur) { return , [string[]]@() }
  $all = Read-BdLogLines -Path $Ctx.Cur.Log
  $new = @($all | Select-Object -Skip $Ctx.LogCount)
  $Ctx.LogCount = $all.Count
  return , [string[]]$new
}

function Get-BdTicketLine { param([string]$Board, [string]$Ticket, [string]$Auth = 'ok') "GET $script:BdApi/boards/${Board}?view=ticket&ticket=$Ticket&include_prose=true auth=$Auth" }
function Get-BdSummaryLine { param([string]$Board, [string]$Auth = 'ok') "GET $script:BdApi/boards/${Board}?view=summary auth=$Auth" }

# Item 3: every line of the log is a GET.
function Get-BdMethodProblems {
  param([AllowEmptyCollection()][string[]]$Lines)
  $p = New-BdProblems
  $i = 0
  foreach ($l in $Lines) {
    $i++
    if ($l -cnotmatch '\AGET ') { $p.Add("Log line $i is not a GET: the method is '$(($l -split ' ')[0])'") }
  }
  return , $p
}

# The problems with the lines the log gained: exactly the lines expected, in order, and no method but GET.
function Get-BdLogProblems {
  param([Parameter(Mandatory)]$Ctx, [AllowEmptyCollection()][string[]]$Expect = @())
  $p = New-BdProblems
  $new = Get-BdNewLog -Ctx $Ctx
  if ($new.Count -ne $Expect.Count) { $p.Add("The log gained $($new.Count) line(s), not $($Expect.Count)") }
  else {
    for ($i = 0; $i -lt $new.Count; $i++) {
      if ($new[$i] -cne $Expect[$i]) { $p.Add("Log line $($i + 1) is not the request the spec gives: '$($new[$i])'") }
    }
  }
  foreach ($x in (Get-BdMethodProblems -Lines $new)) { $p.Add($x) }
  return , $p
}

# --- The credential: the receipt and bot.json, in the home ---------------------------------------------------

function Get-BdLocalDir($Lab) { Join-Path $Lab.UserProfile 'AppData\Local\localswim-v2' }
function Get-BdReceiptPath($Lab) { Join-Path (Get-BdLocalDir $Lab) 'service.json' }
function Get-BdCredDir($Lab) { Join-Path (Get-BdLocalDir $Lab) 'credentials' }
function Get-BdBotPath($Lab) { Join-Path (Get-BdCredDir $Lab) 'bot.json' }

function Clear-BdReceipt {
  param([Parameter(Mandatory)]$Lab)
  $d = Get-BdLocalDir $Lab
  if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $d) -Recurse -Force }
}

# DPAPI in user scope with no entropy, the form bot.json holds: lowercase hex of the protected UTF-16LE text.
function New-BdBlobHex {
  param([Parameter(Mandatory)][string]$Plain)
  $secure = ConvertTo-SecureString -String $Plain -AsPlainText -Force
  return (ConvertFrom-SecureString -SecureString $secure).ToLowerInvariant()
}
$script:BdTokenHex = $null
function Get-BdTokenHex {
  if ($null -eq $script:BdTokenHex) { $script:BdTokenHex = New-BdBlobHex -Plain $script:BdToken }
  return $script:BdTokenHex
}

# A DPAPI header (version 1, the provider GUID, flags 0) followed by zero bytes, as localswim's own examples are.
$script:BdZeroBlobHex = '01000000d08c9ddf0115d1118c7a00c04fc297eb' + ('00' * 60)

function New-BdBotText {
  param([Parameter(Mandatory)][AllowEmptyString()][string]$Hex, [string]$FormatVersion = '1', [string]$Role = 'bot', [string]$Format = 'localswim-credential', [string]$Extra = '')
  return '{"format":"' + $Format + '","format_version":' + $FormatVersion + ',"role":"' + $Role + '","token_dpapi_hex":"' + $Hex + '"' + $Extra + '}'
}

function Get-BdReceiptObject {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Base)
  return [ordered]@{
    executable = (Join-Path $Lab.Root 'standin-service\service.bin'); data_directory = (Join-Path $Lab.Root 'standin-service\data')
    base = $Base; credential_directory = (Get-BdCredDir $Lab)
  }
}

function Set-BdReceiptText {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][AllowEmptyString()][string]$Text, [switch]$Bom)
  Write-BdText -Lab $Lab -Path (Get-BdReceiptPath $Lab) -Text $Text -Bom:$Bom
}
function Set-BdBotText {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][AllowEmptyString()][string]$Text)
  Write-BdText -Lab $Lab -Path (Get-BdBotPath $Lab) -Text $Text
}

# The good credential: the receipt naming the stand-in's port, and bot.json holding the fixture's token.
function Set-BdGoodCredential {
  param([Parameter(Mandatory)]$Ctx, [int]$Port = 0)
  $lab = $Ctx.Lab
  if ($Port -eq 0) { $Port = $Ctx.Cur.Port }
  Clear-BdReceipt -Lab $lab
  Set-BdReceiptText -Lab $lab -Text (ConvertTo-Json -InputObject (Get-BdReceiptObject -Lab $lab -Base "http://127.0.0.1:$Port") -Compress)
  if ($script:BdDpapiOk) { Set-BdBotText -Lab $lab -Text (New-BdBotText -Hex (Get-BdTokenHex)) }
}

# DPAPI preflight: protect a random string here, decrypt it in a pwsh child started with the harness's
# environment, and print nothing of either. The hex and the string travel on standard input, never as an argument.
function Test-BdDpapi {
  param([Parameter(Mandatory)]$Lab)
  try {
    $plain = New-BdToken
    $hex = New-BdBlobHex -Plain $plain
    $childScript = '$h = [Console]::In.ReadLine(); $want = [Console]::In.ReadLine(); $s = ConvertTo-SecureString $h; $b = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($s); try { $got = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($b) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($b) }; if ($got -ceq $want) { exit 0 } else { exit 3 }'
    $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $Lab -Override (Get-BdLocalswimNulls)
    $r = Start-ConformanceProcess -FilePath ([Environment]::ProcessPath) -Arguments @('-NoProfile', '-NonInteractive', '-Command', $childScript) -ChildEnvironment $childEnv -InputText ($hex + "`n" + $plain + "`n") -TimeoutSeconds 60 -WorkingDirectory $Lab.Root
    if ($r.TimedOut) { return @{ Ok = $false; Why = 'The decrypting child timed out' } }
    if ($r.ExitCode -ne 0) { return @{ Ok = $false; Why = "The decrypting child exited $($r.ExitCode)" } }
    return @{ Ok = $true; Why = '' }
  } catch {
    return @{ Ok = $false; Why = 'The preflight threw while it protected the string' }
  }
}

# --- Fixtures, plans and descriptions ------------------------------------------------------------------------

function New-BdCard {
  param([long]$Revision = $script:BdRevision, [bool]$Archived = $false, [AllowEmptyString()][string]$Description = '')
  return [ordered]@{ revision = $Revision; archived = $Archived; description = $Description }
}

function New-BdFixture {
  param([Parameter(Mandatory)][System.Collections.IDictionary]$Cards, $Answer, [switch]$HasAnswer)
  $f = [ordered]@{ token = $script:BdToken; boards = [ordered]@{} }
  $f.boards[$script:BdB1] = [ordered]@{ cards = $Cards }
  if ($HasAnswer) { $f['answer'] = $Answer }
  return $f
}

function New-BdSub { param([string]$Name, [string[]]$After) '{"name":"' + $Name + '","model":"Zmodel 4.2","effort":"zeffort9"' + $(if ($PSBoundParameters.ContainsKey('After')) { ',"after":[' + (($After | ForEach-Object { '"' + $_ + '"' }) -join ',') + ']' } else { '' }) + '}' }
function New-BdStory { param([string]$Name, [string[]]$Subs) '{"name":"' + $Name + '","subagents":[' + ($Subs -join ',') + ']}' }
function New-BdTurn { param([string]$Name, [string[]]$Stories, [string]$BudgetText) '{"name":"' + $Name + '","stories":[' + ($Stories -join ',') + ']' + $(if ($BudgetText) { ',"budget_minutes":' + $BudgetText } else { '' }) + '}' }
function New-BdPlan { param([string[]]$Turns, [string]$Name) '{' + $(if ($Name) { '"name":"' + $Name + '",' } else { '' }) + '"turns":[' + ($Turns -join ',') + ']}' }
# A turn of one story per peak, each of that many subagents none starting after another: its agents at once are
# the peaks' sum plus 1 for its dev lead (lease-api.md, Plan).
function New-BdPeaksTurn {
  param([string]$Name, [int[]]$Peaks, [string]$BudgetText)
  $i = 0
  $stories = foreach ($peak in $Peaks) { $i++; New-BdStory -Name "Zstory-$i" -Subs @(foreach ($j in 1..$peak) { New-BdSub -Name "zsub$j" }) }
  return New-BdTurn -Name $Name -Stories $stories -BudgetText $BudgetText
}
# One valid turn of a story of two subagents, for the plans that carry a single fault.
function New-BdOkTurn { param([string]$Name = 'T01') New-BdTurn -Name $Name -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a'), (New-BdSub -Name 'b')))) }

# The plans of the cases. PA: three turns, the busiest 'T02' (peaks 2 and 3, 6 agents), budgets on two of them.
$script:BdPA = New-BdPlan -Name 'Board plan A' -Turns @((New-BdPeaksTurn 'T01' @(2) '45'), (New-BdPeaksTurn 'T02' @(2, 3) '120'), (New-BdPeaksTurn 'T03 tail' @(1)))
$script:BdPlanSmall = New-BdPlan -Name 'Board small' -Turns @((New-BdPeaksTurn 'T01' @(1)))
$script:BdPB = New-BdPlan -Name 'Board plan B' -Turns @((New-BdPeaksTurn 'Wide' @(11) '200'))
$script:BdPC = New-BdPlan -Name 'Board plan C' -Turns @((New-BdPeaksTurn 'T01' @(1) '30'), (New-BdPeaksTurn 'T02 huge' @(32) '60'))

# A description: prose, the block built from the plan, prose. Lines end as $Nl says.
function New-BdBlock { param([string]$Plan, [string]$Nl = "`n") return $script:BdFence + 'agentspace-plan' + $Nl + $Plan + $Nl + $script:BdFence }
function New-BdDescription {
  param([Parameter(Mandatory)][string]$Plan, [string]$Nl = "`n", [string]$Prefix = '', [string]$BlockContent)
  $block = if ($PSBoundParameters.ContainsKey('BlockContent')) { $script:BdFence + 'agentspace-plan' + $Nl + $BlockContent + $script:BdFence } else { New-BdBlock -Plan $Plan -Nl $Nl }
  return "$script:BdSentinel before.$Nl$Nl$Prefix$block$Nl$Nl$script:BdSentinel after.$Nl"
}
# The plan, a content of exactly $Bytes bytes: spaces, then the one line feed the block's last line ends with.
function New-BdPaddedContent { param([string]$Plan, [int]$Bytes) return $Plan + (' ' * ($Bytes - $Plan.Length - 1)) + "`n" }
# A plan document nested exactly $Levels levels: the document is level 1, and the plan's name holds $Levels - 1 nested
# arrays, so the innermost array is level $Levels. The rest of it is a good turn, so only its nesting is at fault.
function New-BdNestedPlan { param([Parameter(Mandatory)][int]$Levels) return '{"name":' + ('[' * ($Levels - 1)) + (']' * ($Levels - 1)) + ',"turns":[' + (New-BdOkTurn) + ']}' }

# --- The home: manifest, token search, markers ---------------------------------------------------------------

# Every file and folder of the home but the stand-in's own folder, by relative path, with size and SHA-256.
function Get-BdManifest {
  param([Parameter(Mandatory)]$Lab)
  $root = Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.Root
  $skip = $root + '\board\'
  $m = @{}
  foreach ($i in (Get-ChildItem -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue)) {
    if (($i.FullName + '\').StartsWith($skip, [StringComparison]::OrdinalIgnoreCase) -or $i.FullName -ieq ($root + '\board')) { continue }
    $rel = $i.FullName.Substring($root.Length + 1)
    # The startup caches that the stand-in's own pwsh writes under the moved local folder, when it first runs. The
    # stand-in is the case's program, not the program under test, and it writes them at a time of its own.
    if (($rel + [string][char]92).StartsWith('home\AppData\Local\Microsoft\PowerShell\', [StringComparison]::OrdinalIgnoreCase)) { continue }
    if ($i.PSIsContainer) { $m[$rel] = 'folder'; continue }
    $hash = ''
    try { $hash = (Get-FileHash -LiteralPath $i.FullName -Algorithm SHA256).Hash } catch { $hash = 'unreadable' }
    $m[$rel] = "$($i.Length):$hash"
  }
  return $m
}

function Compare-BdManifest {
  param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After)
  $p = New-BdProblems
  $added = @($After.Keys | Where-Object { -not $Before.ContainsKey($_) }).Count
  $removed = @($Before.Keys | Where-Object { -not $After.ContainsKey($_) }).Count
  $changed = @($Before.Keys | Where-Object { $After.ContainsKey($_) -and $After[$_] -cne $Before[$_] }).Count
  if ($added + $removed + $changed -gt 0) {
    $named = @(@($After.Keys | Where-Object { -not $Before.ContainsKey($_) }) + @($Before.Keys | Where-Object { -not $After.ContainsKey($_) }) + @($Before.Keys | Where-Object { $After.ContainsKey($_) -and $After[$_] -cne $Before[$_] })) | Select-Object -First 4 | ForEach-Object { $_ -replace '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', '<id>' }
    $p.Add("The home changed: $added file(s) or folder(s) added, $removed removed, $changed changed, such as: $($named -join ' | ')")
  }
  return , $p
}

function ConvertTo-BdLatin1 { param([byte[]]$Bytes) return [Text.Encoding]::Latin1.GetString($Bytes) }

# The problems with the token being found in a set of byte arrays and files: as text, and as UTF-16LE.
function Find-BdTokenInBytes {
  param([byte[]]$Bytes, [string]$Token = $script:BdToken)
  $found = @()
  if ($null -eq $Bytes -or $Bytes.Length -eq 0) { return $found }
  $text = ConvertTo-BdLatin1 $Bytes
  if ($text.Contains($Token)) { $found += 'text' }
  $wide = -join ($Token.ToCharArray() | ForEach-Object { [string]$_ + [string][char]0 })
  if ($text.Contains($wide)) { $found += 'UTF-16LE' }
  return $found
}

# Item 4: the token is in no output of any run, no stand-in output, no argument given to the program, and in no
# file of the home but the fixture files (the log, the lease state file and bot.json's blob included).
function Get-BdTokenProblems {
  param([Parameter(Mandatory)]$Ctx)
  $p = New-BdProblems
  $i = 0
  foreach ($r in $Ctx.Runs) {
    $i++
    foreach ($where in (Find-BdTokenInBytes -Bytes ([byte[]]$r.StdoutBytes))) { $p.Add("The token appears as $where in the stdout of run $i") }
    foreach ($where in (Find-BdTokenInBytes -Bytes ([byte[]]$r.StderrBytes))) { $p.Add("The token appears as $where in the stderr of run $i") }
  }
  foreach ($s in $Ctx.StandIns) {
    foreach ($where in (Find-BdTokenInBytes -Bytes $s.OutBytes)) { $p.Add("The token appears as $where in a stand-in's stdout") }
    foreach ($where in (Find-BdTokenInBytes -Bytes $s.ErrBytes)) { $p.Add("The token appears as $where in a stand-in's stderr") }
  }
  foreach ($a in $Ctx.Args) { if ($a.Contains($script:BdToken)) { $p.Add('The token appears in an argument given to the program') } }
  foreach ($f in (Find-BdTokenFiles -Lab $Ctx.Lab)) { $p.Add($f) }
  return , $p
}

function Find-BdTokenFiles {
  param([Parameter(Mandatory)]$Lab)
  $out = @()
  $root = Assert-ConformancePath -ConformanceHome $Lab -Path $Lab.Root
  foreach ($f in (Get-ChildItem -LiteralPath $root -Recurse -Force -File -ErrorAction SilentlyContinue)) {
    if ($f.Name -ceq 'fixture.json' -and $f.FullName.StartsWith($root + '\board\', [StringComparison]::OrdinalIgnoreCase)) { continue }
    $bytes = $null
    try { $bytes = [IO.File]::ReadAllBytes($f.FullName) } catch { $out += "A file of the home could not be read for the search: $($f.FullName.Substring($root.Length + 1))"; continue }
    foreach ($where in (Find-BdTokenInBytes -Bytes $bytes)) { $out += "The token appears as $where in the home's file $($f.FullName.Substring($root.Length + 1))" }
  }
  return $out
}

function Get-BdStringLeaves {
  param($Value)
  if ($null -eq $Value) { return }
  if ($Value -is [string]) { $Value; return }
  if ($Value -is [System.Collections.IDictionary]) { foreach ($k in $Value.Keys) { Get-BdStringLeaves $Value[$k] }; return }
  if ($Value -is [System.Collections.IEnumerable]) { foreach ($i in $Value) { Get-BdStringLeaves $i } }
}

# One GET the case itself sends to the stand-in, with the fixture's token, to read what the stand-in answers for
# a card. Returns the parsed ticket view, or $null. In process, so the token is never an argument.
function Get-BdTicketView {
  param([Parameter(Mandatory)]$Ctx, [string]$Board = $script:BdB1, [Parameter(Mandatory)][string]$Ticket)
  $handler = [Net.Http.HttpClientHandler]::new()
  $handler.UseProxy = $false
  $handler.AllowAutoRedirect = $false
  $client = [Net.Http.HttpClient]::new($handler)
  $client.Timeout = [TimeSpan]::FromSeconds(30)
  try {
    $req = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, "http://127.0.0.1:$($Ctx.Cur.Port)$script:BdApi/boards/${Board}?view=ticket&ticket=$Ticket&include_prose=true")
    [void]$req.Headers.TryAddWithoutValidation('Authorization', 'Bearer ' + $script:BdToken)
    $resp = $client.SendAsync($req).GetAwaiter().GetResult()
    $text = $resp.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    if ([int]$resp.StatusCode -ne 200) { return $null }
    return @{ Text = $text; Doc = ($text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 50) }
  } catch { return $null } finally { $client.Dispose() }
}

# The marker strings the stand-in puts in the members the client has no use for: the string values of
# prose.comments, relationships, parent and children, 8 characters or more.
function Get-BdMarkersOf {
  param($View)
  $m = @()
  if ($null -eq $View -or $View.Doc -isnot [hashtable]) { return $m }
  $d = $View.Doc
  foreach ($part in @($d['relationships'], $d['parent'], $d['children'], $(if ($d['prose'] -is [hashtable]) { $d['prose']['comments'] }))) {
    foreach ($s in (Get-BdStringLeaves $part)) {
      if ($s.Length -lt 8 -or $s.Contains($script:BdB1) -or $s -cmatch '\A\d{4}-\d{2}-\d{2}T' -or $s -cmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-') { continue }
      $m += $s
    }
  }
  return @($m | Select-Object -Unique)
}

function Get-BdLeakProblems {
  param([Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)]$Run)
  $p = New-BdProblems
  $text = [string]$Run.Stdout + "`n" + [string]$Run.Stderr
  foreach ($mk in $Ctx.Markers) { if ($text.Contains($mk)) { $p.Add('An output holds a marker of a member the client has no use for') ; break } }
  foreach ($t in @($script:BdSentinel, 'Zstory-', 'zsub', 'Zmodel', 'zeffort9', [string]$script:BdRevision)) {
    if ($text.Contains($t)) { $p.Add("An output prints '$t', which no output prints") }
  }
  return , $p
}

# --- Running the program, and the checks that wrap a run -----------------------------------------------------

function Get-BdArgs {
  param([string]$Project = $script:BdProject, [string]$Board, [string]$Epic, [string]$Turn, [string[]]$More = @(), [switch]$NoProject)
  $a = @()
  if (-not $NoProject) { $a += @('--project', $Project) }
  if ($PSBoundParameters.ContainsKey('Board')) { $a += @('--board', $Board) }
  if ($PSBoundParameters.ContainsKey('Epic')) { $a += @('--epic', $Epic) }
  if ($PSBoundParameters.ContainsKey('Turn')) { $a += @('--turn', $Turn) }
  return @($a + $More)
}

function Invoke-BdLease {
  param([Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [hashtable]$Set = @{}, [int]$TimeoutSeconds = 0, [string]$Verb = 'create')
  $full = @('lease', $Verb) + $Arguments
  foreach ($a in $full) { $Ctx.Args.Add([string]$a) }
  $e = Get-BdEnvironment -Set $Set
  $sw = [Diagnostics.Stopwatch]::StartNew()
  if ($TimeoutSeconds -gt 0) { $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_CMD' -ConformanceHome $Ctx.Lab -Arguments $full -Environment $e -TimeoutSeconds $TimeoutSeconds }
  else { $r = Invoke-AgentspaceCli -ConformanceHome $Ctx.Lab -Arguments $full -Environment $e }
  $sw.Stop()
  Add-Member -InputObject $r -NotePropertyName Seconds -NotePropertyValue $sw.Elapsed.TotalSeconds -Force
  $Ctx.Runs.Add($r)
  return $r
}

function Skip-BdNeedsToken {
  param([Parameter(Mandatory)][string]$Name)
  Write-BdResult -Name $Name -Status SKIPPED -Reason "The DPAPI preflight failed, so a case that needs a token cannot run: $script:BdDpapiWhy"
}

# A refusal: the run, then the code, field and message byte for byte, the log lines expected (none by default),
# the home unchanged, and nothing of the card in any output.
function Test-BdRefusal {
  param(
    [Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Code, [string]$Field = '', [Parameter(Mandatory)][string]$Message, [string[]]$Log = @(), [hashtable]$Set = @{},
    [int]$TimeoutSeconds = 0, [switch]$NeedsToken, [switch]$NoManifest, [double]$MaxSeconds = 0
  )
  if ($NeedsToken -and -not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $Name; return }
  $before = if ($NoManifest) { $null } else { Get-BdManifest -Lab $Ctx.Lab }
  [void](Get-BdNewLog -Ctx $Ctx)
  $run = Invoke-BdLease -Ctx $Ctx -Arguments $Arguments -Set $Set -TimeoutSeconds $TimeoutSeconds
  $p = Get-ConformanceErrorProblems -Run $run -Code $Code -Field $Field
  if ($run.TimedOut) { $p.Add('The run did not end within its limit') }
  if ($MaxSeconds -gt 0 -and $run.Seconds -gt $MaxSeconds) { $p.Add("The run took $([int]$run.Seconds) seconds, more than the $([int]$MaxSeconds) the spec allows") }
  if ($run.ErrorJson -is [hashtable] -and $run.ErrorJson['message'] -cne $Message) { $p.Add("The message is not the one the spec gives: '$($run.ErrorJson['message'])'") }
  foreach ($x in (Get-BdLogProblems -Ctx $Ctx -Expect $Log)) { $p.Add($x) }
  if ($before) { foreach ($x in (Compare-BdManifest -Before $before -After (Get-BdManifest -Lab $Ctx.Lab))) { $p.Add($x) } }
  foreach ($x in (Get-BdLeakProblems -Ctx $Ctx -Run $run)) { $p.Add($x) }
  Complete-BdCheck -Name $Name -Problems $p
}

# The problems with a sized_for: an object of exactly turn and agents (and plan when the plan has a name).
function Get-BdSizedForProblems {
  param([Parameter(Mandatory)]$Doc, [Parameter(Mandatory)]$Expect)
  $p = New-BdProblems
  if (-not $Doc.ContainsKey('sized_for') -or $Doc['sized_for'] -isnot [hashtable]) { $p.Add('The lease has no sized_for object'); return , $p }
  $sf = $Doc['sized_for']
  $want = @('agents', 'turn') + $(if ($Expect.Plan) { 'plan' } else { @() })
  $got = @($sf.Keys | ForEach-Object { [string]$_ } | Sort-Object { $_ } -CaseSensitive)
  if (($got -join ',') -cne (@($want | Sort-Object { $_ } -CaseSensitive) -join ',')) { $p.Add("sized_for has the members [$($got -join ', ')], not [$((@($want | Sort-Object { $_ } -CaseSensitive)) -join ', ')]") }
  if ($sf['turn'] -cne $Expect.Turn) { $p.Add('sized_for.turn is not the turn the lease is for, as the plan writes it') }
  if ($Expect.Plan -and $sf['plan'] -cne $Expect.Plan) { $p.Add('sized_for.plan is not the plan''s own name') }
  if ($sf['agents'] -isnot [int] -and $sf['agents'] -isnot [long]) { $p.Add('sized_for.agents is not an integer') }
  elseif ($sf['agents'] -ne $Expect.Agents) { $p.Add("sized_for.agents is $($sf['agents']), not $($Expect.Agents)") }
  return , $p
}

# A good read: exit 0, the lease document, the sized_for of the plan, and exactly the log lines given.
function Test-BdGoodRead {
  param(
    [Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Arguments, [Parameter(Mandatory)]$Expect,
    [Parameter(Mandatory)][string[]]$Log, [hashtable]$Set = @{}, [string]$State = 'unsatisfiable', [switch]$NeedsToken, [switch]$CheckRecord, [string]$RecordTicket
  )
  if ($NeedsToken -and -not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $Name; return }
  [void](Get-BdNewLog -Ctx $Ctx)
  $run = Invoke-BdLease -Ctx $Ctx -Arguments $Arguments -Set $Set
  $p = Get-ConformanceSuccessProblems -Run $run
  $doc = $run.Json
  if ($doc -is [hashtable]) {
    if ($doc['state'] -cne $State) { $p.Add("The lease's state is '$($doc['state'])', not $State") }
    foreach ($x in (Get-BdSizedForProblems -Doc $doc -Expect $Expect)) { $p.Add($x) }
    foreach ($forbidden in @('plan', 'plan_source', 'board', 'epic', 'turn')) { if ($doc.ContainsKey($forbidden)) { $p.Add("The lease document has a member '$forbidden', which no output prints") } }
  }
  foreach ($x in (Get-BdLogProblems -Ctx $Ctx -Expect $Log)) { $p.Add($x) }
  foreach ($x in (Get-BdLeakProblems -Ctx $Ctx -Run $run)) { $p.Add($x) }
  if ($CheckRecord -and $doc -is [hashtable] -and $doc['id']) {
    $rec = Get-CcRecord $Ctx.Lab ([string]$doc['id'])
    $src = if ($rec) { $rec['plan_source'] } else { $null }
    if ($src -isnot [hashtable]) { $p.Add('The lease record has no plan_source') }
    else {
      if ($src['board'] -cne $script:BdB1) { $p.Add('plan_source.board is not the normalized board ID') }
      if ($src['ticket'] -ne [long]$RecordTicket) { $p.Add('plan_source.ticket is not the normalized ticket') }
      if ($src['revision'] -ne $script:BdRevision) { $p.Add('plan_source.revision is not the card''s revision as read') }
      if (@($src.Keys).Count -ne 3) { $p.Add('plan_source does not hold exactly three members') }
    }
    if ($rec -and -not $rec.ContainsKey('plan')) { $p.Add('The lease record does not keep the plan as read') }
  }
  Complete-BdCheck -Name $Name -Problems $p
}

# --- The lab -------------------------------------------------------------------------------------------------

# Makes a throwaway home, runs the body with a context, ends every stand-in the body started, searches the home
# and every output for the token (item 4), and removes the home. A body that throws is one FAIL.
function Use-BdLab {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body)
  $lab = $null
  $ctx = $null
  $savedCwd = [Environment]::CurrentDirectory
  try {
    $lab = New-ConformanceHome
    $ctx = New-BdContext -Lab $lab -Name $Name
    & $Body $ctx
    if ($ctx.Cur) { Stop-BdBoard -Ctx $ctx }
    Complete-BdCheck -Name "conformance.md#what-the-board-cases-must-show: The token reached the stand-in and appears nowhere else, in $Name" -Problems (Get-BdTokenProblems -Ctx $ctx)
  } catch {
    $why = ($_.Exception.Message -replace '\s+', ' ')
    Write-BdResult -Name "conformance.md#what-the-board-cases-must-show: $Name" -Status FAIL -Reason "It threw: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    [Environment]::CurrentDirectory = $savedCwd
    if ($ctx -and $ctx.Cur) { Stop-BdBoard -Ctx $ctx }
    if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
  }
}

# --- Group 0: the preflight, and the controls that show each checker can fail --------------------------------

$preLab = $null
try {
  $preLab = New-ConformanceHome
  $pre = Test-BdDpapi -Lab $preLab
  $script:BdDpapiOk = [bool]$pre.Ok
  $script:BdDpapiWhy = [string]$pre.Why
} catch {
  $script:BdDpapiOk = $false
  $script:BdDpapiWhy = 'The preflight home could not be made'
} finally {
  if ($preLab) { Remove-ConformanceHome -ConformanceHome $preLab }
}
if ($script:BdDpapiOk) { Write-BdResult -Name 'conformance.md#the-stand-in-credential: DPAPI protects a string here and decrypts it in a child under the moved home' -Status PASS }
else { Write-BdResult -Name 'conformance.md#the-stand-in-credential: DPAPI protects a string here and decrypts it in a child under the moved home' -Status SKIPPED -Reason "Every case that needs a token is skipped: $script:BdDpapiWhy" }

# The token is drawn at random per run, in the form the real reader checks.
$tokenProblems = New-BdProblems
if ($script:BdToken -cnotmatch '\A[A-Za-z0-9_-]{43}\z') { $tokenProblems.Add('The token is not 43 characters of A-Z, a-z, 0-9, - and _') }
if ((New-BdToken) -ceq (New-BdToken)) { $tokenProblems.Add('Two draws gave the same token') }
Complete-BdCheck -Name 'conformance.md#the-stand-in-board: The run''s token is drawn at random, in the form the real reader checks' -Problems $tokenProblems

# Redaction: a reason planted with the token, as text and as UTF-16LE seen byte by byte, comes out without it.
$redactProblems = New-BdProblems
$planted = "A reason holds $script:BdToken and $script:BdTokenWide and C:\Users\someone\file.txt here"
$out = ConvertTo-BdRedacted $planted
if ($out.Contains($script:BdToken)) { $redactProblems.Add('The token''s text is still in the redacted reason') }
if ($out.Contains($script:BdTokenWide)) { $redactProblems.Add('The token''s UTF-16LE form is still in the redacted reason') }
if ($out.Contains('someone')) { $redactProblems.Add('A drive path is still in the redacted reason') }
if (-not $out.Contains('<token>')) { $redactProblems.Add('The placeholder is not in the redacted reason') }
Complete-BdCheck -Name 'conformance.md#what-a-board-case-never-reads: Control: a reason planted with the token comes out of the redaction without it' -Problems $redactProblems

# The method check can see a method other than GET: in a planted log, and in the log of a real POST.
$methodProblems = New-BdProblems
if ((Get-BdMethodProblems -Lines @('GET /a auth=ok', 'GET /b auth=ok')).Count -ne 0) { $methodProblems.Add('The check flags a log of GETs alone') }
if ((Get-BdMethodProblems -Lines @('GET /a auth=ok', 'POST /b auth=ok')).Count -eq 0) { $methodProblems.Add('The check does not flag a planted POST line') }
if ((Get-BdMethodProblems -Lines @('GET /a auth=ok', 'INVALID - auth=none')).Count -eq 0) { $methodProblems.Add('The check does not flag an unparsed line') }
Complete-BdCheck -Name 'lease-api.md#the-board-read: Control: the method check fails on a planted line that is not a GET' -Problems $methodProblems

# The marker search and the manifest compare can fail too.
$markerCtx = [pscustomobject]@{ Markers = @('MARKER-planted-1') }
$markerRun = [pscustomobject]@{ Stdout = 'A document with MARKER-planted-1 inside'; Stderr = '' }
$ctlProblems = New-BdProblems
if ((Get-BdLeakProblems -Ctx $markerCtx -Run $markerRun).Count -eq 0) { $ctlProblems.Add('The marker search does not find a planted marker') }
if ((Compare-BdManifest -Before @{ 'a' = '1:X' } -After @{ 'a' = '1:X'; 'b' = '2:Y' }).Count -eq 0) { $ctlProblems.Add('The manifest compare does not see an added file') }
if ((Compare-BdManifest -Before @{ 'a' = '1:X' } -After @{ 'a' = '1:Z' }).Count -eq 0) { $ctlProblems.Add('The manifest compare does not see a changed file') }
if ((Compare-BdManifest -Before @{ 'a' = '1:X' } -After @{}).Count -eq 0) { $ctlProblems.Add('The manifest compare does not see a removed file') }
Complete-BdCheck -Name 'conformance.md#what-the-board-cases-must-show: Control: the marker search and the home compare fail on a planted marker and a planted change' -Problems $ctlProblems

# Item 4's search, proven on planted copies: a file holding the token as text, one holding it as UTF-16LE, and a
# clean one. Then item 3's second control and the stand-in's own answers, with a stand-in listening.
Use-BdLab -Name 'the controls' -Body {
  param($ctx)
  $lab = $ctx.Lab
  $planted = Join-Path $lab.Root 'planted'
  Write-BdText -Lab $lab -Path (Join-Path $planted 'text.txt') -Text ('A line, then ' + $script:BdToken + ', then more.')
  Write-BdFile -Lab $lab -Path (Join-Path $planted 'wide.bin') -Bytes ([Text.Encoding]::Unicode.GetBytes('Prefix ' + $script:BdToken + ' suffix'))
  Write-BdText -Lab $lab -Path (Join-Path $planted 'clean.txt') -Text 'Nothing secret here.'
  $hits = @(Find-BdTokenFiles -Lab $lab)
  $p = New-BdProblems
  if (-not (@($hits | Where-Object { $_ -match 'as text in the home''s file planted\\text\.txt' }).Count -eq 1)) { $p.Add('The search does not find the token as text in a planted file') }
  if (-not (@($hits | Where-Object { $_ -match 'as UTF-16LE in the home''s file planted\\wide\.bin' }).Count -eq 1)) { $p.Add('The search does not find the token as UTF-16LE in a planted file') }
  if (@($hits | Where-Object { $_ -match 'clean\.txt' }).Count -ne 0) { $p.Add('The search flags a file that holds no token') }
  Complete-BdCheck -Name 'conformance.md#what-the-board-cases-must-show: Control: the token search finds a planted copy as text and as UTF-16LE, and not a clean file' -Problems $p
  Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $planted) -Recurse -Force
  $p = New-BdProblems
  if (@(Find-BdTokenFiles -Lab $lab).Count -ne 0) { $p.Add('The search still finds the token after the planted files were removed') }
  Complete-BdCheck -Name 'conformance.md#what-the-board-cases-must-show: The token search of a home with no planted copy finds nothing' -Problems $p

  Start-BdBoard -Ctx $ctx -Key 'controls' -Fixture (New-BdFixture -Cards ([ordered]@{ '52' = (New-BdCard -Description (New-BdDescription -Plan $script:BdPA)) })) | Out-Null
  # A planted copy of the log, with a POST line.
  $logCopy = Join-Path $lab.Root 'board\controls\log-copy.txt'
  Write-BdText -Lab $lab -Path $logCopy -Text ((Get-BdTicketLine -Board $script:BdB1 -Ticket '52') + "`nPOST /api/2026-11-10/boards/x auth=ok`n")
  $p = New-BdProblems
  if ((Get-BdMethodProblems -Lines (Read-BdLogLines -Path $logCopy)).Count -eq 0) { $p.Add('The method check does not fail on a planted copy of the log') }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: Control: the method check fails on a copy of the log with a POST line planted in it' -Problems $p
  # A request the case itself sends with POST: the stand-in answers 405 and logs it, and the check fails on the log.
  $handler = [Net.Http.HttpClientHandler]::new(); $handler.UseProxy = $false; $handler.AllowAutoRedirect = $false
  $client = [Net.Http.HttpClient]::new($handler); $client.Timeout = [TimeSpan]::FromSeconds(30)
  $status = 0
  try { $status = [int]($client.PostAsync("http://127.0.0.1:$($ctx.Cur.Port)$script:BdApi/boards/$script:BdB1?view=summary", [Net.Http.StringContent]::new('')).GetAwaiter().GetResult().StatusCode) } catch { $status = -1 } finally { $client.Dispose() }
  $lines = Get-BdNewLog -Ctx $ctx
  $p = New-BdProblems
  if ($status -ne 405) { $p.Add("The stand-in answered a POST with status $status, not 405") }
  if ($lines.Count -ne 1) { $p.Add("The stand-in logged $($lines.Count) line(s) for the POST, not 1") }
  if ((Get-BdMethodProblems -Lines $lines).Count -eq 0) { $p.Add('The method check does not fail on the log of a real POST') }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: Control: a POST the case sends is answered 405 and logged, and the method check fails on it' -Problems $p

  # The stand-in's ticket view: whole, with every ignored part filled in, and the token not answered.
  $view = Get-BdTicketView -Ctx $ctx -Ticket '52'
  [void](Get-BdNewLog -Ctx $ctx)
  $p = New-BdProblems
  if ($null -eq $view -or $view.Doc -isnot [hashtable]) { $p.Add('The stand-in did not answer the ticket view with a JSON object') }
  else {
    $d = $view.Doc
    foreach ($m in 'board_id', 'board_revision', 'card', 'relationships', 'parent', 'children', 'prose') { if (-not $d.ContainsKey($m)) { $p.Add("The ticket view has no member '$m'") } }
    if ($d['prose'] -is [hashtable] -and @($d['prose']['comments']).Count -lt 2) { $p.Add('prose.comments holds fewer than two comments') }
    if (@($d['relationships']).Count -lt 1) { $p.Add('relationships holds no link') }
    if ($null -eq $d['parent']) { $p.Add('parent is not a card') }
    if (@($d['children']).Count -lt 1) { $p.Add('children holds no card') }
    if ($d['card'] -is [hashtable]) {
      if ($d['card']['revision'] -ne $script:BdRevision) { $p.Add('card.revision is not the fixture''s') }
      if ($d['card']['archived'] -isnot [bool] -or $d['card']['archived']) { $p.Add('card.archived is not false') }
    }
    if ($d['prose'] -is [hashtable] -and $d['prose']['description'] -cne (New-BdDescription -Plan $script:BdPA)) { $p.Add('prose.description is not the fixture''s') }
    if (@(Find-BdTokenInBytes -Bytes $script:BdUtf8.GetBytes($view.Text)).Count -ne 0) { $p.Add('The stand-in answers the token') }
    if ((Get-BdMarkersOf -View $view).Count -lt 4) { $p.Add('The ignored members hold fewer than four marker strings of 8 characters or more') }
  }
  Complete-BdCheck -Name 'conformance.md#the-stand-in-board: The stand-in answers a whole ticket view with every ignored part filled in and never answers the token' -Problems $p
}

# --- Group 1: refusals made before any request, and the reads that answer a card ------------------------------

$script:BdWhy = @{
  NoBlock = 'its description has no agentspace-plan block'
  Two = 'its description has more than one agentspace-plan block'
  Near = 'a line that starts an agentspace-plan block is not exactly the fence line'
  Open = 'its agentspace-plan block is never closed'
  Size = 'the file is larger than 262144 bytes'
  Json = 'the file is not valid JSON'
  Schema = 'the document does not match the plan schema'
  Big = 'the board''s answer for this card is larger than 64 MiB'
}
function Get-BdNoPlan([string]$Epic, [string]$Why) { "Epic '$Epic' carries no usable plan: $Why." }
function Get-BdNoAnswer([int]$Port) { "agentspace lease create failed: The board at 127.0.0.1:$Port did not answer. Nothing was recorded." }
$script:BdMsgRefused = "agentspace lease create failed: The board refused agentspace's credential. Nothing was recorded."
$script:BdMsgNoReceipt = "agentspace lease create failed: localswim's service receipt was not found; localswim may not be running. Nothing was recorded."
$script:BdMsgBadReceipt = "agentspace lease create failed: localswim's service receipt could not be read. Nothing was recorded."
$script:BdMsgBadCredential = "agentspace lease create failed: The board's credential could not be read. Nothing was recorded."
$script:BdCreateHelp = "Run 'agentspace lease create --help'."

# The cards of the first lab: the good epic, an archived one, and one card for each description the block rules
# refuse. Each is { Name, Ticket, Why } for its result.
function New-BdDescriptionCases {
  $f = $script:BdFence
  $pa = $script:BdPA
  $ok = New-BdPlan -Turns @((New-BdOkTurn))
  $cases = [System.Collections.Generic.List[object]]::new()
  $add = { param($Name, $Description, $Why) $cases.Add(@{ Name = $Name; Description = $Description; Why = $Why }) }
  # The line rules: no block, two, near misses, a block never closed.
  & $add 'A description with no block and no fence at all' "Only prose, $script:BdSentinel.`nNothing else.`n" $script:BdWhy.NoBlock
  & $add 'A description with fences of other kinds only, a json fence and a bare fence, is text for people' ("Prose.`n${f}json`n{}`n$f`n${f}`nplain`n$f`n") $script:BdWhy.NoBlock
  & $add 'A description that is empty' '' $script:BdWhy.NoBlock
  & $add 'Two blocks, each a plan' ((New-BdBlock -Plan $pa) + "`n`nBetween.`n`n" + (New-BdBlock -Plan $pa) + "`n") $script:BdWhy.Two
  & $add 'A block that is shown as an example inside a markdown fence, beside the real one, is a second block' ("${f}markdown`n" + (New-BdBlock -Plan $pa) + "`n$f`n`n" + (New-BdBlock -Plan $pa) + "`n") $script:BdWhy.Two
  & $add 'A first block whose content is not a plan, then a second block, is refused as two blocks and never with the content''s fault' ((New-BdBlock -Plan 'not json') + "`n" + (New-BdBlock -Plan $pa) + "`n") $script:BdWhy.Two
  & $add 'A near miss after a block that was found is refused before the first block''s content is looked at' ((New-BdBlock -Plan 'not json') + "`n${f}agentspace-plan json`n") $script:BdWhy.Near
  & $add 'An opening line followed by a space is a near miss' ("${f}agentspace-plan `n$pa`n$f`n") $script:BdWhy.Near
  & $add 'An opening line followed by a tab is a near miss' ("${f}agentspace-plan`t`n$pa`n$f`n") $script:BdWhy.Near
  & $add 'An opening line with four backticks is a near miss' ("${f}``agentspace-plan`n$pa`n$f`n") $script:BdWhy.Near
  & $add 'An opening line followed by another word is a near miss' ("${f}agentspace-plan json`n$pa`n$f`n") $script:BdWhy.Near
  & $add 'An opening line with a longer word, agentspace-planner, is a near miss' ("${f}agentspace-planner`n$pa`n$f`n") $script:BdWhy.Near
  & $add 'An opening line that ends in a carriage return that is not just before the line feed is a near miss' ("${f}agentspace-plan`r`r`n$pa`n$f`n") $script:BdWhy.Near
  & $add 'A block whose opening line holds a space after the name, in CRLF text, is a near miss' ("${f}agentspace-plan `r`n$pa`r`n$f`r`n") $script:BdWhy.Near
  & $add 'A block opened and never closed, to the end of the description' ("${f}agentspace-plan`n$pa`n") $script:BdWhy.Open
  & $add 'A block whose closing line has a space after it is never closed' ("${f}agentspace-plan`n$pa`n$f `n") $script:BdWhy.Open
  & $add 'A block whose closing line has four backticks is never closed' ("${f}agentspace-plan`n$pa`n" + $f + [string][char]96 + "`n") $script:BdWhy.Open
  & $add 'An opening line inside a block is refused as the block never closed' ("${f}agentspace-plan`n$pa`n${f}agentspace-plan`n$pa`n$f`n") $script:BdWhy.Open
  & $add 'A near miss inside a block is refused as the block never closed' ("${f}agentspace-plan`n$pa`n${f}agentspace-plan json`n$f`n") $script:BdWhy.Open
  # The content: every check of the plan, one case each, each with the why the table gives it.
  & $add 'A block closed straight after its opening, so its content is empty, is not valid JSON' ("${f}agentspace-plan`n$f`n") $script:BdWhy.Json
  & $add 'A block whose content is only whitespace is not valid JSON' ("${f}agentspace-plan`n   `n$f`n") $script:BdWhy.Json
  & $add 'A block whose content is cut off is not valid JSON' ("${f}agentspace-plan`n{`"turns`":`n$f`n") $script:BdWhy.Json
  & $add 'A block with text after its document is not valid JSON' ("${f}agentspace-plan`n$pa extra`n$f`n") $script:BdWhy.Json
  & $add 'A block that holds U+FEFF outside a string is not valid JSON' ("${f}agentspace-plan`n{$($script:BdBomChar)`"turns`":[$(New-BdOkTurn)]}`n$f`n") $script:BdWhy.Json
  & $add 'A block that holds a number the parser cannot hold, 1e400, is not valid JSON' ("${f}agentspace-plan`n{`"name`":1e400,`"turns`":[$(New-BdOkTurn)]}`n$f`n") $script:BdWhy.Json
  & $add 'A block whose content is one byte past 262144 is larger than the bound' ("${f}agentspace-plan`n" + (New-BdPaddedContent -Plan $pa -Bytes 262145) + $f + "`n") $script:BdWhy.Size
  & $add 'A block whose content is far past 262144 bytes is larger than the bound' ("${f}agentspace-plan`n" + (New-BdPaddedContent -Plan $pa -Bytes 300000) + $f + "`n") $script:BdWhy.Size
  & $add 'A plan with an object that gives a member twice' ("${f}agentspace-plan`n{`"name`":`"x`",`"name`":`"y`",`"turns`":[$(New-BdOkTurn)]}`n$f`n") 'an object has two members named ''name'''
  & $add 'A plan with a story that gives a member twice' ("${f}agentspace-plan`n{`"turns`":[{`"name`":`"T01`",`"stories`":[{`"name`":`"S1`",`"subagents`":[$(New-BdSub -Name 'a')],`"subagents`":[]}]}]}`n$f`n") 'an object has two members named ''subagents'''
  & $add 'A plan with no turns member, which the plan schema refuses' ("${f}agentspace-plan`n{`"name`":`"x`"}`n$f`n") $script:BdWhy.Schema
  & $add 'A plan with an empty turns array, which the plan schema refuses' ("${f}agentspace-plan`n{`"turns`":[]}`n$f`n") $script:BdWhy.Schema
  & $add 'A plan with a member that is not in the shape' ("${f}agentspace-plan`n{`"turns`":[$(New-BdOkTurn)],`"extra`":1}`n$f`n") $script:BdWhy.Schema
  & $add 'A plan that is not an object' ("${f}agentspace-plan`n[1,2]`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget written as 90.0, though the schema accepts it' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '90.0'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget written as 9e1, though the schema accepts it' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '9e1'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget written as -0' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '-0'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget of 961, past the largest' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '961'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget of 0' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '0'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget with a fraction, 90.5' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '90.5'))) + "`n$f`n") $script:BdWhy.Schema
  & $add 'A plan whose turn has a budget that is a string' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a')))) -BudgetText '"90"'))) + "`n$f`n") $script:BdWhy.Schema
  # The decided nesting case: the plan's name holds 63 nested arrays, so the document is level 1 and the innermost
  # array is level 64. A parser MUST allow at least 64 levels, and past level 9 a document is check 6, never check 4.
  & $add 'A document that nests exactly 64 levels, the plan''s name holding 63 nested arrays, is refused by the schema and not as invalid JSON' ("${f}agentspace-plan`n{`"name`":" + ('[' * 63) + (']' * 63) + ",`"turns`":[$(New-BdOkTurn)]}`n$f`n") $script:BdWhy.Schema
  # Pin P5 (architect, 2026-11-25, for v0.11-T04; lease-api.md, "Plan", check 4 and "Input tolerance"): nothing between 65
  # and 999 levels is asserted, since which check refuses a document there is the build's. The two bounds that the ruling
  # decides are asserted: 64 levels is check 6 (the case above, and the same document through --plan, in the refusals
  # lab), and 100,000 levels is check 4 (in the refusals lab, from a board and through --plan).
  & $add 'A plan with two turns of one name' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdOkTurn 'T01'), (New-BdOkTurn 'T01'))) + "`n$f`n") 'turn ''T01'' is named twice'
  & $add 'A plan with two stories of one name in a turn' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a'))), (New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a'))))))) + "`n$f`n") 'story ''S1'' is named twice in turn ''T01'''
  & $add 'A plan with two subagents of one name in a story' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a'), (New-BdSub -Name 'a'))))))) + "`n$f`n") 'subagent ''a'' is named twice in story ''S1'''
  & $add 'A plan with an after entry that names no subagent of its story' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a'), (New-BdSub -Name 'b' -After @('zz')))))))) + "`n$f`n") 'subagent ''b'' in story ''S1'' starts after ''zz'', which is no subagent of that story'
  & $add 'A plan whose subagents start after each other in a cycle' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a' -After @('b')), (New-BdSub -Name 'b' -After @('a')))))))) + "`n$f`n") 'the subagents of story ''S1'' start after each other in a cycle'
  & $add 'A plan with a subagent that starts after itself' ("${f}agentspace-plan`n" + (New-BdPlan -Turns @((New-BdTurn -Name 'T01' -Stories @((New-BdStory -Name 'S1' -Subs @((New-BdSub -Name 'a' -After @('a')))))))) + "`n$f`n") 'the subagents of story ''S1'' start after each other in a cycle'
  return $cases
}

Use-BdLab -Name 'the refusals' -Body {
  param($ctx)
  $lab = $ctx.Lab
  $bd = $script:BdB1
  $cards = [ordered]@{}
  $cards['52'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA)
  $cards['53'] = New-BdCard -Archived $true -Description (New-BdDescription -Plan $script:BdPA)
  # The exact bound of the block's content: 262144 bytes is a good plan.
  $cards['55'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPlanSmall -BlockContent (New-BdPaddedContent -Plan $script:BdPlanSmall -Bytes 262144))
  $descCases = New-BdDescriptionCases
  $ticket = 100
  foreach ($c in $descCases) { $ticket++; $c.Ticket = [string]$ticket; $cards[[string]$ticket] = New-BdCard -Description $c.Description }
  $cards['400'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPlanSmall -Nl "`r`n")
  # The nesting of the ruling of pin P5: 100,000 levels, about 200 KB, under the 262144-byte bound of the plan.
  $deepDeep = New-BdNestedPlan -Levels 100000
  $deepDeepBytes = $script:BdUtf8.GetByteCount($deepDeep)
  $cards['500'] = New-BdCard -Description (New-BdDescription -Plan $deepDeep)
  Start-BdBoard -Ctx $ctx -Key 'refusals' -Fixture (New-BdFixture -Cards $cards) | Out-Null
  Set-BdGoodCredential -Ctx $ctx
  $port = $ctx.Cur.Port
  # The markers of the members the client has no use for, found by one GET of the case's own.
  $ctx.Markers = Get-BdMarkersOf -View (Get-BdTicketView -Ctx $ctx -Ticket '52')
  [void](Get-BdNewLog -Ctx $ctx)
  $upper = $bd.ToUpperInvariant()
  $line52 = Get-BdTicketLine -Board $bd -Ticket '52'
  $expectA = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 }

  # Item 2, the good reads: one GET, normalized; a lease made twice reads twice, never more; the state file.
  Test-BdGoodRead -Ctx $ctx -Name 'lease-api.md#the-board-read: A good epic is read with one GET whose line is exactly the request the spec gives, auth=ok, and the lease is recorded with its plan_source' -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '52') -Expect $expectA -Log @($line52) -NeedsToken -CheckRecord -RecordTicket '52'
  Test-BdGoodRead -Ctx $ctx -Name 'lease-api.md#board: A board ID in capitals with whitespace round it, and an epic with leading zeros and whitespace, are normalized in the request' -Arguments (Get-BdArgs -Project $script:BdNoHost -Board "  $upper  " -Epic ' 0052 ') -Expect $expectA -Log @($line52) -NeedsToken -CheckRecord -RecordTicket '52'
  [void](Get-BdNewLog -Ctx $ctx)
  $first = Invoke-BdLease -Ctx $ctx -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '52')
  $second = Invoke-BdLease -Ctx $ctx -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '52')
  $p = New-BdProblems
  if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name 'lease-api.md#the-board-read: A lease made twice makes two good reads and not more' }
  else {
    foreach ($r in @($first, $second)) { if ($r.ExitCode -ne 0) { $p.Add("A create exited $($r.ExitCode), not 0") } }
    foreach ($x in (Get-BdLogProblems -Ctx $ctx -Expect @($line52, $line52))) { $p.Add($x) }
    Complete-BdCheck -Name 'lease-api.md#the-board-read: A lease made twice makes two good reads and not more' -Problems $p
  }
  Test-BdGoodRead -Ctx $ctx -Name 'lease-api.md#the-plan-block: A block whose content is exactly 262144 bytes is a plan' -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '55') -Expect @{ Plan = 'Board small'; Turn = 'T01'; Agents = 2 } -Log @(Get-BdTicketLine -Board $bd -Ticket '55') -NeedsToken
  Test-BdGoodRead -Ctx $ctx -Name 'lease-api.md#the-plan-block: A description in CRLF lines holds a plan' -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '400') -Expect @{ Plan = 'Board small'; Turn = 'T01'; Agents = 2 } -Log @(Get-BdTicketLine -Board $bd -Ticket '400') -NeedsToken

  # Item 1, the card's own refusals: unknown ticket and board (two requests), archived (one).
  $nocard = { param($v, $b) "No card of ticket '$v' on board '$b'. Give the epic's ticket number, as in '52'." }
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: A ticket the board has no card of is invalid_value for epic after the ticket view and the summary view, two GETs' -Arguments (Get-BdArgs -Board $bd -Epic '999') -Code invalid_value -Field epic -Message (& $nocard '999' $bd) -Log @((Get-BdTicketLine -Board $bd -Ticket '999'), (Get-BdSummaryLine -Board $bd)) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#epic: An unknown ticket is quoted as given, with its zeros and whitespace, and requested without the zeros' -Arguments (Get-BdArgs -Board $upper -Epic ' 0999 ') -Code invalid_value -Field epic -Message (& $nocard ' 0999 ' $bd) -Log @((Get-BdTicketLine -Board $bd -Ticket '999'), (Get-BdSummaryLine -Board $bd)) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#epic: The largest ticket, 9007199254740991, is in its form and, with no card, is an unknown ticket' -Arguments (Get-BdArgs -Board $bd -Epic '9007199254740991') -Code invalid_value -Field epic -Message (& $nocard '9007199254740991' $bd) -Log @((Get-BdTicketLine -Board $bd -Ticket '9007199254740991'), (Get-BdSummaryLine -Board $bd)) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: A board the stand-in does not know is invalid_value for board after two GETs, and the message quotes the value as given' -Arguments (Get-BdArgs -Board $script:BdBX -Epic '52') -Code invalid_value -Field board -Message "No board of ID '$script:BdBX' that agentspace can read." -Log @((Get-BdTicketLine -Board $script:BdBX -Ticket '52'), (Get-BdSummaryLine -Board $script:BdBX)) -NeedsToken
  $bxUpper = $script:BdBX.ToUpperInvariant()
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#board: An unknown board given in capitals is quoted as given and requested in lowercase' -Arguments (Get-BdArgs -Board $bxUpper -Epic '52') -Code invalid_value -Field board -Message "No board of ID '$bxUpper' that agentspace can read." -Log @((Get-BdTicketLine -Board $script:BdBX -Ticket '52'), (Get-BdSummaryLine -Board $script:BdBX)) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: An archived card is invalid_value for epic after one GET' -Arguments (Get-BdArgs -Board $bd -Epic '53') -Code invalid_value -Field epic -Message "Epic '53' is archived. Name an epic that is being worked." -Log @((Get-BdTicketLine -Board $bd -Ticket '53')) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#turn: A turn the plan lacks is invalid_value for turn, naming the plan''s turns in order, after one GET' -Arguments (Get-BdArgs -Board $bd -Epic '52' -Turn 'T04') -Code invalid_value -Field turn -Message "Turn 'T04' is not a turn of the plan; its turns are 'T01', 'T02', 'T03 tail'." -Log @($line52) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#turn: A turn compares exactly, so t01 names no turn of a plan whose turn is T01' -Arguments (Get-BdArgs -Board $bd -Epic '52' -Turn 't01') -Code invalid_value -Field turn -Message "Turn 't01' is not a turn of the plan; its turns are 'T01', 'T02', 'T03 tail'." -Log @($line52) -NeedsToken
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A turn the plan lacks is reported after the TTL is checked, so a bad TTL is first and no board is read' -Arguments (Get-BdArgs -Board $bd -Epic '52' -Turn 'T04' -More @('--ttl', '8')) -Code invalid_value -Field ttl -Message "TTL '8' has no unit. Give one, as in '8h'."

  # Item 1, the description: every refusal of the block rules and of the plan's own checks, one GET each.
  foreach ($c in $descCases) {
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#the-plan-block: $($c.Name), is invalid_value for epic with the spec's reason, after one GET" -Arguments (Get-BdArgs -Board $bd -Epic $c.Ticket) -Code invalid_value -Field epic -Message (Get-BdNoPlan $c.Ticket $c.Why) -Log @((Get-BdTicketLine -Board $bd -Ticket $c.Ticket)) -NeedsToken
  }

  # The nesting of pin P5, from a board and through --plan. 100,000 levels is check 4, within a bound of time, with no
  # lease recorded; 64 levels is check 6 through --plan as it is from a board above. Nothing between 65 and 999 levels
  # is asserted. A plan given through --plan reads no board, so the stand-in's log stays empty.
  $deepBytesProblems = New-BdProblems
  if ($deepDeepBytes -le 150000 -or $deepDeepBytes -ge 262144) { $deepBytesProblems.Add("The document of 100,000 levels is $deepDeepBytes bytes, not between 150000 and 262143") }
  Complete-BdCheck -Name 'lease-api.md#the-plan-block: The document nested 100,000 levels is under the 262144-byte bound, so it is refused for its nesting and never for its size' -Problems $deepBytesProblems
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-plan-block: A document that nests 100,000 levels, the plan''s name holding 99,999 nested arrays, is invalid_value for epic as not valid JSON, after one GET, within 60 seconds' -Arguments (Get-BdArgs -Board $bd -Epic '500') -Code invalid_value -Field epic -Message (Get-BdNoPlan '500' $script:BdWhy.Json) -Log @((Get-BdTicketLine -Board $bd -Ticket '500')) -NeedsToken -TimeoutSeconds 90 -MaxSeconds 60
  $nestDir = Join-Path $lab.Root 'plans'
  Write-BdText -Lab $lab -Path (Join-Path $nestDir 'deep100000.json') -Text $deepDeep
  Write-BdText -Lab $lab -Path (Join-Path $nestDir 'deep64.json') -Text (New-BdNestedPlan -Levels 64)
  $deepPath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $nestDir 'deep100000.json')
  $deep64Path = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $nestDir 'deep64.json')
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#plan: A plan file that nests 100,000 levels is invalid_value for plan as not valid JSON, with no request, within 60 seconds' -Arguments (Get-BdArgs -More @('--plan', $deepPath)) -Code invalid_value -Field plan -Message "Plan '$(Format-BdQuoted $deepPath)' is not a valid plan: $($script:BdWhy.Json)." -TimeoutSeconds 90 -MaxSeconds 60
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#plan: A plan file that nests exactly 64 levels is invalid_value for plan with the schema''s reason and not as invalid JSON, with no request' -Arguments (Get-BdArgs -More @('--plan', $deep64Path)) -Code invalid_value -Field plan -Message "Plan '$(Format-BdQuoted $deep64Path)' is not a valid plan: $($script:BdWhy.Schema)." -TimeoutSeconds 90 -MaxSeconds 60

  # Item 1 and item 7, the forms and the order of the checks: refused before any request, with the stand-in listening.
  $boardMsg = { param($v) "Board '$(Format-BdQuoted $v)' is not a board ID. Give the board's UUID, as in '0a1b2c3d-...'." }
  $boardForms = @(
    @('A name for a board', 'swimcoach'), @('A UUID of version 1', '0a1b2c3d-0a1b-1c2d-8e3f-0a1b2c3d4e5f'), @('A UUID of version 5', '0a1b2c3d-0a1b-5c2d-8e3f-0a1b2c3d4e5f'),
    @('A UUID of version 4 with a variant that is not 8, 9, a or b', '0a1b2c3d-0a1b-4c2d-0e3f-0a1b2c3d4e5f'), @('A UUID in braces', "{$bd}"), @('A UUID with a urn:uuid: prefix', "urn:uuid:$bd"),
    @('A UUID without its dashes', $bd.Replace('-', '')), @('A value of only whitespace', ' ')
  )
  foreach ($f in $boardForms) {
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#board: $($f[0]) is invalid_value for board, refused before any request" -Arguments (Get-BdArgs -Board $f[1] -Epic '52') -Code invalid_value -Field board -Message (& $boardMsg $f[1])
  }
  $epicMsg = { param($v) "Epic '$(Format-BdQuoted $v)' is not a ticket number. Give the epic's ticket as digits, as in '52'." }
  $epicForms = @(
    @('A ticket with a hash before it', '#0052'), @('A ticket with a fraction', '52.0'), @('A ticket with an exponent', '1e2'), @('A ticket with a plus sign', '+5'), @('A ticket with a minus sign', '-5'),
    @('A ticket of zero', '0'), @('A ticket of only zeros', '0000'), @('A ticket of 17 digits with leading zeros', '00000000000000052'), @('A ticket past the range', '9007199254740992'),
    @('A ticket that is a word', 'abc'), @('A ticket with a space inside it', '5 2'), @('A ticket with a line feed inside it', "5`n2"), @('A value of only whitespace', ' ')
  )
  foreach ($f in $epicForms) {
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#epic: $($f[0]) is invalid_value for epic, refused before any request" -Arguments (Get-BdArgs -Board $bd -Epic $f[1]) -Code invalid_value -Field epic -Message (& $epicMsg $f[1])
  }
  $turnMsg = { param($v) "Turn '$(Format-BdQuoted $v)' is not a turn name. Give the plan's turn name, as in 'T01'." }
  $turnForms = @(
    @('A turn with a space before it', ' T01'), @('A turn with a space after it', 'T01 '), @('A turn of 65 characters', ('T' * 65)), @('A turn with a character outside printable ASCII', "T$([char]0x00FC)"),
    @('A turn with U+FEFF in it', "T01$script:BdBomChar"), @('A turn of only whitespace', ' ')
  )
  foreach ($f in $turnForms) {
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#turn: $($f[0]) is invalid_value for turn, refused before any request and before any plan is matched" -Arguments (Get-BdArgs -Board $bd -Epic '52' -Turn $f[1]) -Code invalid_value -Field turn -Message (& $turnMsg $f[1])
  }
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad board form is reported before a bad epic and a bad turn, with no request' -Arguments (Get-BdArgs -Board 'nope' -Epic 'nope' -Turn ' T') -Code invalid_value -Field board -Message (& $boardMsg 'nope')
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad epic form is reported before a bad turn, with no request' -Arguments (Get-BdArgs -Board $bd -Epic 'nope' -Turn ' T') -Code invalid_value -Field epic -Message (& $epicMsg 'nope')
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad turn form is reported before a bad TTL, with no request' -Arguments (Get-BdArgs -Board $bd -Epic '52' -Turn ' T' -More @('--ttl', '8')) -Code invalid_value -Field turn -Message (& $turnMsg ' T')
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad board form is reported before a bad TTL, with no request' -Arguments (Get-BdArgs -Board 'nope' -Epic '52' -More @('--ttl', '8')) -Code invalid_value -Field board -Message (& $boardMsg 'nope')
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A TTL with no unit is invalid_value for ttl and reads no board' -Arguments (Get-BdArgs -Board $bd -Epic '52' -More @('--ttl', '8')) -Code invalid_value -Field ttl -Message "TTL '8' has no unit. Give one, as in '8h'."
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A TTL of 25 hours is out_of_range for ttl and reads no board' -Arguments (Get-BdArgs -Board $bd -Epic '52' -More @('--ttl', '25h')) -Code out_of_range -Field ttl -Message "TTL '25h' is outside the allowed range. Give from 1m to 24h; it is never clamped."
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad project is reported before the board values and reads no board' -Arguments (Get-BdArgs -Project 'bad_name' -Board 'nope' -Epic 'nope') -Code invalid_value -Field project -Message "Project 'bad_name' is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit."
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#exit-codes-and-errors: A bad platform is reported before the board values and reads no board' -Arguments (Get-BdArgs -Board 'nope' -Epic 'nope' -More @('--platform', 'bad!')) -Code invalid_value -Field platform -Message "Platform 'bad!' is not an os/arch pair. Give one as in 'linux/arm64'."

  # Item 7, the usage cases of pin P4: each refused before any request, each with the stand-in listening.
  $pair = { param($a, $b2, $why) "Options --$a and --$b2 cannot be given together; $why Run 'agentspace lease create --help'." }
  $carries = 'the epic carries the plan.'
  $picks = 'the epic''s plan picks the size.'
  $usage = @(
    @{ N = '--board and --epic with --plan, --plan last'; A = @('--board', $bd, '--epic', '52', '--plan', 'plan.json'); F = 'board'; M = (& $pair 'board' 'plan' $carries) }
    @{ N = '--board and --epic with --plan, --plan first'; A = @('--plan', 'plan.json', '--board', $bd, '--epic', '52'); F = 'board'; M = (& $pair 'board' 'plan' $carries) }
    @{ N = '--board and --epic with --size, --size last'; A = @('--board', $bd, '--epic', '52', '--size', 'm'); F = 'board'; M = (& $pair 'board' 'size' $picks) }
    @{ N = '--board and --epic with --size, --size first'; A = @('--size', 'm', '--board', $bd, '--epic', '52'); F = 'board'; M = (& $pair 'board' 'size' $picks) }
    @{ N = '--epic with --plan and no --board, --plan last'; A = @('--epic', '52', '--plan', 'plan.json'); F = 'epic'; M = (& $pair 'epic' 'plan' $carries) }
    @{ N = '--epic with --plan and no --board, --plan first'; A = @('--plan', 'plan.json', '--epic', '52'); F = 'epic'; M = (& $pair 'epic' 'plan' $carries) }
    @{ N = '--epic with --size and no --board, --size last'; A = @('--epic', '52', '--size', 'm'); F = 'epic'; M = (& $pair 'epic' 'size' $picks) }
    @{ N = '--epic with --size and no --board, --size first'; A = @('--size', 'm', '--epic', '52'); F = 'epic'; M = (& $pair 'epic' 'size' $picks) }
    @{ N = '--board, --epic and --plan with --board first, which names --board with --plan'; A = @('--board', $bd, '--epic', '7', '--plan', 'plan.json'); F = 'board'; M = (& $pair 'board' 'plan' $carries) }
    @{ N = '--epic, --plan and --board in that order, which names --epic with --plan at --plan and the field board'; A = @('--epic', '7', '--plan', 'plan.json', '--board', $bd); F = 'board'; M = (& $pair 'epic' 'plan' $carries) }
    @{ N = '--plan, --board and --size, which names --board with --plan at --board, before --plan with --size'; A = @('--plan', 'plan.json', '--board', $bd, '--size', 'm'); F = 'board'; M = (& $pair 'board' 'plan' $carries) }
    @{ N = '--plan with --size and no board flag still names plan'; A = @('--plan', 'plan.json', '--size', 'm'); F = 'plan'; M = "Options --plan and --size cannot be given together; the plan picks the size. $script:BdCreateHelp" }
    @{ N = '--board with no --epic, with --turn on the line'; A = @('--board', $bd, '--turn', 'T01'); F = 'epic'; M = "Option --board needs --epic, the epic's ticket number. $script:BdCreateHelp" }
    @{ N = '--board with no --epic'; A = @('--board', $bd); F = 'epic'; M = "Option --board needs --epic, the epic's ticket number. $script:BdCreateHelp" }
    @{ N = '--epic with no --board'; A = @('--epic', '52'); F = 'board'; M = "Option --epic needs --board, the board's ID. $script:BdCreateHelp" }
    @{ N = '--epic with no --board, with --turn on the line'; A = @('--epic', '52', '--turn', 'T01'); F = 'board'; M = "Option --epic needs --board, the board's ID. $script:BdCreateHelp" }
    @{ N = '--turn with no plan'; A = @('--turn', 'T01'); F = 'turn'; M = "Option --turn needs a plan, from --plan or --epic. $script:BdCreateHelp" }
    @{ N = '--turn with --size and no plan'; A = @('--turn', 'T01', '--size', 'm'); F = 'turn'; M = "Option --turn needs a plan, from --plan or --epic. $script:BdCreateHelp" }
    @{ N = '--board with no value'; A = @('--board'); F = 'board'; M = "Option --board needs a value. $script:BdCreateHelp" }
    @{ N = '--board followed by another flag'; A = @('--board', '--epic', '52'); F = 'board'; M = "Option --board needs a value. $script:BdCreateHelp" }
    @{ N = '--epic with no value'; A = @('--board', $bd, '--epic'); F = 'epic'; M = "Option --epic needs a value. $script:BdCreateHelp" }
    @{ N = '--turn with no value'; A = @('--board', $bd, '--epic', '52', '--turn'); F = 'turn'; M = "Option --turn needs a value. $script:BdCreateHelp" }
    @{ N = '--board with an empty value'; A = @('--board', '', '--epic', '52'); F = 'board'; M = "Option --board has an empty value. $script:BdCreateHelp" }
    @{ N = '--epic with an empty value in the equals form'; A = @('--board', $bd, '--epic='); F = 'epic'; M = "Option --epic has an empty value. $script:BdCreateHelp" }
    @{ N = '--turn with an empty value in the equals form'; A = @('--board', $bd, '--epic', '52', '--turn='); F = 'turn'; M = "Option --turn has an empty value. $script:BdCreateHelp" }
    @{ N = '--board given twice'; A = @('--board', $bd, '--board', $bd, '--epic', '52'); F = 'board'; M = 'Option --board is given twice. Give it once.' }
    @{ N = '--epic given twice'; A = @('--board', $bd, '--epic', '52', '--epic', '53'); F = 'epic'; M = 'Option --epic is given twice. Give it once.' }
    @{ N = '--turn given twice'; A = @('--board', $bd, '--epic', '52', '--turn', 'T01', '--turn', 'T02'); F = 'turn'; M = 'Option --turn is given twice. Give it once.' }
    @{ N = '--plan with no value after a board pair, which is reported as the missing value and not the pair'; A = @('--board', $bd, '--epic', '52', '--plan'); F = 'plan'; M = "Option --plan needs a value. $script:BdCreateHelp" }
    @{ N = '--size with an empty value after a board pair, which is reported as the empty value and not the pair'; A = @('--board', $bd, '--epic', '52', '--size', ''); F = 'size'; M = "Option --size has an empty value. $script:BdCreateHelp" }
    @{ N = 'An unknown option beside a board pair'; A = @('--board', $bd, '--epic', '52', '--frob'); F = ''; M = "Unknown option '--frob' for agentspace lease create. Run 'agentspace lease create --help'." }
    @{ N = 'An unexpected argument beside a board pair'; A = @('--board', $bd, '--epic', '52', 'extra'); F = ''; M = "Unexpected argument 'extra' for agentspace lease create. Run 'agentspace lease create --help'." }
  )
  foreach ($u in $usage) {
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#command-lines: $($u.N) is a usage error, refused before any request" -Arguments (Get-BdArgs -More $u.A) -Code usage -Field $u.F -Message $u.M
  }
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#command-lines: A missing --project is reported before a missing partner of --board' -Arguments (Get-BdArgs -NoProject -More @('--board', $bd)) -Code usage -Field project -Message "Option --project is required. $script:BdCreateHelp"
  # Help beside the board flags prints help and reads nothing.
  $helpRun = Invoke-BdLease -Ctx $ctx -Arguments (Get-BdArgs -Board $bd -Epic '52' -More @('--help'))
  $blocks = Get-ConformanceHelpBlocks
  $p = New-BdProblems
  if ($helpRun.ExitCode -ne 0) { $p.Add("Exit $($helpRun.ExitCode), not 0") }
  if ($helpRun.StderrBytes.Length -ne 0) { $p.Add('Stderr is not empty') }
  if ($blocks.ContainsKey('agentspace lease create')) { if ($helpRun.Stdout -cne $blocks['agentspace lease create']) { $p.Add('The help is not the one the doc gives') } } else { $p.Add('The doc has no help block for create') }
  foreach ($x in (Get-BdLogProblems -Ctx $ctx -Expect @())) { $p.Add($x) }
  Complete-BdCheck -Name 'lease-api.md#command-lines: --help beside --board and --epic prints create''s help and reads no board' -Problems $p
  $p = New-BdProblems
  # The case's own check that no refusal recorded a lease: the lease state file holds only the leases made above.
  $state = Read-CcState $lab
  $count = if ($state) { @($state['leases']).Count } else { 0 }
  if ($script:BdDpapiOk -and $count -ne 6) { $p.Add("The lease state file holds $count lease(s), not the 6 the good reads made, so a refusal recorded a lease") }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: No refusal recorded a lease, so the state file holds only the leases the good reads made' -Problems $p
  $whole = New-BdProblems
  foreach ($x in (Get-BdMethodProblems -Lines (Read-BdLogLines -Path $ctx.Cur.Log))) { $whole.Add($x) }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: In every case of the refusals lab the log holds no line whose method is not GET' -Problems $whole
}

# --- Group 2: a board lease and the same plan through --plan give the same lease (item 6), and item 5 -----------

function Compare-BdLeaseDocs {
  param([Parameter(Mandatory)]$Board, [Parameter(Mandatory)]$File)
  $p = New-BdProblems
  $skip = @('id', 'created_at', 'expires_at', 'released_at')
  $keys = @(@($Board.Keys) + @($File.Keys) | ForEach-Object { [string]$_ } | Where-Object { $skip -cnotcontains $_ } | Select-Object -Unique)
  foreach ($k in $keys) {
    if (-not $Board.ContainsKey($k)) { $p.Add("The member '$k' is in the --plan lease and not in the board lease"); continue }
    if (-not $File.ContainsKey($k)) { $p.Add("The member '$k' is in the board lease and not in the --plan lease"); continue }
    if ($k -ceq 'ssh') {
      if ($Board['ssh'] -isnot [hashtable] -or $File['ssh'] -isnot [hashtable] -or $Board['ssh']['user'] -cne $File['ssh']['user']) { $p.Add('The member ssh.user differs') }
      continue
    }
    if ((ConvertTo-CcCanonical $Board[$k]) -cne (ConvertTo-CcCanonical $File[$k])) { $p.Add("The member '$k' differs between the board lease and the --plan lease") }
  }
  return , $p
}

# The TTL of one lease as its own document gives it: expires_at minus created_at, within the time create took of
# ttl_seconds. No clock time of one run is compared with another's.
function Get-BdTtlProblems {
  param([Parameter(Mandatory)]$Doc, [string]$Which)
  $p = New-BdProblems
  if ($Doc['state'] -cne 'ready') { return , $p }
  $delta = ((ConvertFrom-CcTime ([string]$Doc['expires_at'])) - (ConvertFrom-CcTime ([string]$Doc['created_at']))).TotalSeconds
  if ([math]::Abs($delta - [double]$Doc['ttl_seconds']) -gt 20) { $p.Add("The $Which lease's expires_at minus created_at is not its ttl_seconds, within the time create took") }
  return , $p
}

Use-BdLab -Name 'the equal leases' -Body {
  param($ctx)
  $lab = $ctx.Lab
  $bd = $script:BdB1
  $cards = [ordered]@{}
  $cards['52'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA)
  $cards['252'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA)
  $cards['53'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA -Nl "`r`n")
  $cards['54'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA -BlockContent ($script:BdBomChar + $script:BdPA + "`n"))
  $cards['55'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPA -Prefix ("~~~agentspace-plan`n> ${script:BdFence}agentspace-plan`n ${script:BdFence}agentspace-plan`n`t${script:BdFence}agentspace-plan`n${script:BdFence}Agentspace-Plan`n${script:BdFence}json`n{}`n${script:BdFence}`n${script:BdBomChar}${script:BdFence}agentspace-plan`n"))
  $cards['60'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPB)
  $cards['61'] = New-BdCard -Description (New-BdDescription -Plan $script:BdPC)
  Start-BdBoard -Ctx $ctx -Key 'equal' -Fixture (New-BdFixture -Cards $cards) | Out-Null
  Set-BdGoodCredential -Ctx $ctx
  $ctx.Markers = Get-BdMarkersOf -View (Get-BdTicketView -Ctx $ctx -Ticket '52')
  $markers252 = Get-BdMarkersOf -View (Get-BdTicketView -Ctx $ctx -Ticket '252')
  [void](Get-BdNewLog -Ctx $ctx)
  $plans = Join-Path $lab.Root 'plans'
  $paths = @{}
  foreach ($pair in @(@('pa', $script:BdPA), @('pb', $script:BdPB), @('pc', $script:BdPC))) {
    Write-BdText -Lab $lab -Path (Join-Path $plans "$($pair[0]).json") -Text $pair[1]
    $paths[$pair[0]] = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $plans "$($pair[0]).json")
  }
  $p = New-BdProblems
  if ($ctx.Markers.Count -eq 0 -or $markers252.Count -eq 0) { $p.Add('The stand-in gave no markers for the ignored members') }
  elseif ((@($ctx.Markers | Sort-Object) -join "`n") -ceq (@($markers252 | Sort-Object) -join "`n")) { $p.Add('The ignored members of cards 52 and 252 hold the same markers, so a result that does not change with them shows nothing') }
  Complete-BdCheck -Name 'conformance.md#the-stand-in-board: The ignored members of two cards differ, so a lease that does not change with them shows the client ignores them' -Problems $p

  $bothProblems = {
    param($boardRun, $fileRun, $boardLog, $label)
    $p = New-BdProblems
    foreach ($x in (Get-ConformanceSuccessProblems -Run $boardRun)) { $p.Add("The board lease: $x") }
    foreach ($x in (Get-ConformanceSuccessProblems -Run $fileRun)) { $p.Add("The --plan lease: $x") }
    if ($boardRun.Json -is [hashtable] -and $fileRun.Json -is [hashtable]) {
      foreach ($x in (Compare-BdLeaseDocs -Board $boardRun.Json -File $fileRun.Json)) { $p.Add($x) }
      foreach ($x in (Get-BdTtlProblems -Doc $boardRun.Json -Which 'board')) { $p.Add($x) }
      foreach ($x in (Get-BdTtlProblems -Doc $fileRun.Json -Which '--plan')) { $p.Add($x) }
    }
    foreach ($x in (Get-BdLeakProblems -Ctx $ctx -Run $boardRun)) { $p.Add($x) }
    return , $p
  }

  # Unsatisfiable, for a project with no host and not on the list: size, sized_for and the TTL asked for need no host.
  $unsat = @(
    @{ N = 'a plan of 12 agents, size l, for its only turn'; Ticket = '60'; Plan = 'pb'; Turn = $null; Ttl = $null; Codes = @('no_capacity') }
    @{ N = 'the same plan with --ttl 3h'; Ticket = '60'; Plan = 'pb'; Turn = $null; Ttl = '3h'; Codes = @('no_capacity') }
    @{ N = 'a plan whose busiest turn no size holds, plan_too_large with the first message'; Ticket = '61'; Plan = 'pc'; Turn = $null; Ttl = $null; Codes = @('plan_too_large') }
    @{ N = 'the same plan with --turn naming the turn no size holds, plan_too_large with the --turn message'; Ticket = '61'; Plan = 'pc'; Turn = 'T02 huge'; Ttl = $null; Codes = @('plan_too_large') }
    @{ N = 'the same plan with --turn naming the small turn, which no_capacity answers'; Ticket = '61'; Plan = 'pc'; Turn = 'T01'; Ttl = $null; Codes = @('no_capacity') }
  )
  foreach ($u in $unsat) {
    if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name "lease-api.md#the-plan-block: A board lease and the same plan through --plan give the same size, sized_for and TTL, unsatisfiable for a project with no host, $($u.N)"; continue }
    $extra = @()
    if ($u.Turn) { $extra += @('--turn', $u.Turn) }
    if ($u.Ttl) { $extra += @('--ttl', $u.Ttl) }
    [void](Get-BdNewLog -Ctx $ctx)
    $boardRun = Invoke-BdLease -Ctx $ctx -Arguments (@('--project', $script:BdNoHost, '--board', $bd, '--epic', $u.Ticket) + $extra)
    $boardLog = Get-BdLogProblems -Ctx $ctx -Expect @((Get-BdTicketLine -Board $bd -Ticket $u.Ticket))
    $fileRun = Invoke-BdLease -Ctx $ctx -Arguments (@('--project', $script:BdNoHost, '--plan', $paths[$u.Plan]) + $extra)
    $fileLog = Get-BdLogProblems -Ctx $ctx -Expect @()
    $p = & $bothProblems $boardRun $fileRun $null $u.N
    foreach ($x in $boardLog) { $p.Add("Board lease: $x") }
    foreach ($x in $fileLog) { $p.Add("--plan lease: $x") }
    if ($boardRun.Json -is [hashtable]) {
      if ($boardRun.Json['state'] -cne 'unsatisfiable') { $p.Add("The board lease's state is '$($boardRun.Json['state'])', not unsatisfiable") }
      $codes = @($boardRun.Json['reasons'] | ForEach-Object { $_['code'] })
      if (($codes -join ',') -cne ($u.Codes -join ',')) { $p.Add("The board lease's reasons are [$($codes -join ', ')], not [$($u.Codes -join ', ')]") }
      if ($u.Codes[0] -ceq 'plan_too_large' -and $boardRun.Json['size'] -cne 'xl') { $p.Add('A plan_too_large lease is not size xl') }
      if (-not $boardRun.Json.ContainsKey('sized_for') -or $boardRun.Json['sized_for']['plan'] -cne $(if ($u.Plan -ceq 'pb') { 'Board plan B' } else { 'Board plan C' })) { $p.Add('sized_for.plan is not the plan''s own name') }
    }
    Complete-BdCheck -Name "lease-api.md#the-plan-block: A board lease and the same plan through --plan give the same size, sized_for and TTL, unsatisfiable for a project with no host, $($u.N)" -Problems $p
  }

  # Ready, on a seeded host: the same, and the TTL from expires_at minus created_at, with --turn and without.
  if (-not $lab.CaReady) {
    Write-BdResult -Name 'lease-api.md#ttl: A board lease and the same plan through --plan give the same TTL, ready on a seeded host' -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
    return
  }
  [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project $script:BdProject)
  $ready = @(
    @{ N = 'with no --turn, the busiest turn'; Ticket = '52'; Turn = $null; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
    @{ N = 'with --turn T01, which has a budget'; Ticket = '52'; Turn = 'T01'; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T01'; Agents = 3 } }
    @{ N = 'with --turn naming a turn with no budget, which gets the default TTL'; Ticket = '52'; Turn = 'T03 tail'; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T03 tail'; Agents = 2 } }
    @{ N = 'with --ttl 2h, which wins over the budget'; Ticket = '52'; Turn = $null; Ttl = '2h'; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
    @{ N = 'with --turn T01 and --ttl 90m'; Ticket = '52'; Turn = 'T01'; Ttl = '90m'; Expect = @{ Plan = 'Board plan A'; Turn = 'T01'; Agents = 3 } }
    @{ N = 'from a description in CRLF lines'; Ticket = '53'; Turn = $null; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
    @{ N = 'from a block whose content starts with a byte order mark'; Ticket = '54'; Turn = $null; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
    @{ N = 'from a description whose lines that are not the fence line, tildes, a quote, an indent, another case and a U+FEFF, are text'; Ticket = '55'; Turn = $null; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
    @{ N = 'from card 252, whose ignored members differ from card 52''s'; Ticket = '252'; Turn = $null; Ttl = $null; Expect = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } }
  )
  $ttls = @{}
  foreach ($u in $ready) {
    $name = "lease-api.md#ttl: A board lease and the same plan through --plan give the same size, sized_for and TTL, ready on a seeded host, $($u.N)"
    if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $name; continue }
    $extra = @()
    if ($u.Turn) { $extra += @('--turn', $u.Turn) }
    if ($u.Ttl) { $extra += @('--ttl', $u.Ttl) }
    [void](Get-BdNewLog -Ctx $ctx)
    $boardRun = Invoke-BdLease -Ctx $ctx -Arguments (@('--project', $script:BdProject, '--board', $bd, '--epic', $u.Ticket) + $extra)
    $boardLog = Get-BdLogProblems -Ctx $ctx -Expect @((Get-BdTicketLine -Board $bd -Ticket ([string][long]$u.Ticket)))
    $fileRun = Invoke-BdLease -Ctx $ctx -Arguments (@('--project', $script:BdProject, '--plan', $paths['pa']) + $extra)
    $fileLog = Get-BdLogProblems -Ctx $ctx -Expect @()
    $p = & $bothProblems $boardRun $fileRun $null $u.N
    foreach ($x in $boardLog) { $p.Add("Board lease: $x") }
    foreach ($x in $fileLog) { $p.Add("--plan lease: $x") }
    if ($boardRun.Json -is [hashtable]) {
      if ($boardRun.Json['state'] -cne 'ready') { $p.Add("The board lease's state is '$($boardRun.Json['state'])', not ready") }
      foreach ($x in (Get-BdSizedForProblems -Doc $boardRun.Json -Expect $u.Expect)) { $p.Add($x) }
      $ttls["$($u.Ticket)|$($u.Turn)|$($u.Ttl)"] = $boardRun.Json['ttl_seconds']
    }
    Complete-BdCheck -Name $name -Problems $p
  }
  # The TTLs differ with the turn and the budget: the busiest turn, the T01 budget and the default are three values.
  if ($script:BdDpapiOk) {
    $p = New-BdProblems
    $a = $ttls['52||']; $b = $ttls['52|T01|']; $c = $ttls['52|T03 tail|']; $d = $ttls['52||2h']
    if ($null -in @($a, $b, $c, $d)) { $p.Add('A lease of the group did not print a TTL') }
    else {
      if ($c -ne 28800) { $p.Add('A turn with no budget does not get the default TTL, 8 hours') }
      if ($d -ne 7200) { $p.Add('--ttl 2h does not win over the budget') }
      if (@($a, $b, $c) | Group-Object | Where-Object { $_.Count -gt 1 }) { $p.Add('Two of the three turns of the plan got the same TTL, so the TTL does not follow the turn') }
    }
    Complete-BdCheck -Name 'lease-api.md#ttl: The TTL a board lease gets follows its turn: the budget''s TTL, the default for a turn with none, and --ttl over both' -Problems $p
  }
  $whole = New-BdProblems
  foreach ($x in (Get-BdMethodProblems -Lines (Read-BdLogLines -Path $ctx.Cur.Log))) { $whole.Add($x) }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: In every case of the equal-leases lab the log holds no line whose method is not GET' -Problems $whole
  $recordName = 'lease-api.md#what-is-recorded: A plan_source is on the records of the board leases alone, never on a --plan lease'
  if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $recordName; return }
  $p = New-BdProblems
  $state = Read-CcState $lab
  if ($state) {
    $withSource = @($state['leases'] | Where-Object { $_.ContainsKey('plan_source') }).Count
    $fileLeases = @($state['leases'] | Where-Object { -not $_.ContainsKey('plan_source') }).Count
    if ($withSource -ne 14) { $p.Add("$withSource lease record(s) hold a plan_source, not the 14 board leases") }
    if ($fileLeases -ne 14) { $p.Add("$fileLeases lease record(s) hold none, not the 14 --plan leases") }
  } else { $p.Add('There is no lease state file') }
  Complete-BdCheck -Name $recordName -Problems $p
}

# --- Group 3: a board that answers otherwise, hangs, cannot be reached, or sends a body past the bound ---------

Use-BdLab -Name 'the answers' -Body {
  param($ctx)
  $lab = $ctx.Lab
  $bd = $script:BdB1
  $cards = [ordered]@{ '52' = (New-BdCard -Description (New-BdDescription -Plan $script:BdPA)) }
  $line52 = Get-BdTicketLine -Board $bd -Ticket '52'
  # Each status the stand-in can be asked to answer, with the outcome the spec gives it, one stand-in each.
  $statuses = @(
    @{ S = 401; Refused = $true }, @{ S = 403; Refused = $true }, @{ S = 400; Refused = $false }, @{ S = 405; Refused = $false }, @{ S = 406; Refused = $false },
    @{ S = 418; Refused = $false }, @{ S = 500; Refused = $false }, @{ S = 503; Refused = $false }
  )
  foreach ($st in $statuses) {
    Start-BdBoard -Ctx $ctx -Key "status$($st.S)" -Fixture (New-BdFixture -Cards $cards -HasAnswer -Answer @{ status = $st.S }) | Out-Null
    Set-BdGoodCredential -Ctx $ctx
    $msg = if ($st.Refused) { $script:BdMsgRefused } else { Get-BdNoAnswer $ctx.Cur.Port }
    $what = if ($st.Refused) { 'refused agentspace''s credential' } else { 'did not answer' }
    Test-BdRefusal -Ctx $ctx -Name "lease-api.md#the-board-read: A board that answers $($st.S) $what, internal with no field, after one GET and no second" -Arguments (Get-BdArgs -Board $bd -Epic '52') -Code internal -Message $msg -Log @($line52) -NeedsToken
    Stop-BdBoard -Ctx $ctx
  }
  # A bare 404, an empty body with no Content-Type: not localswim's not_found, so one GET and `internal`.
  Start-BdBoard -Ctx $ctx -Key 'bare404' -Fixture (New-BdFixture -Cards $cards -HasAnswer -Answer @{ status = 404; problem = $false }) | Out-Null
  Set-BdGoodCredential -Ctx $ctx
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: A bare 404 with no Problem document is internal, never invalid_value for board or epic, after one GET and no summary request' -Arguments (Get-BdArgs -Board $bd -Epic '52') -Code internal -Message (Get-BdNoAnswer $ctx.Cur.Port) -Log @($line52) -NeedsToken
  Stop-BdBoard -Ctx $ctx
  # A board that hangs: the program ends within 60 seconds, internal, the host and port and nothing else.
  Start-BdBoard -Ctx $ctx -Key 'hang' -Fixture (New-BdFixture -Cards $cards -HasAnswer -Answer 'hang') | Out-Null
  Set-BdGoodCredential -Ctx $ctx
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: A board that hangs ends the run within 60 seconds, internal naming the host and port alone, after one GET' -Arguments (Get-BdArgs -Board $bd -Epic '52') -Code internal -Message (Get-BdNoAnswer $ctx.Cur.Port) -Log @($line52) -NeedsToken -TimeoutSeconds 90 -MaxSeconds 60
  Stop-BdBoard -Ctx $ctx
  # A board that cannot be reached: the port of a stand-in that has ended, which the case opened.
  Start-BdBoard -Ctx $ctx -Key 'gone' -Fixture (New-BdFixture -Cards $cards) | Out-Null
  Set-BdGoodCredential -Ctx $ctx
  $gonePort = $ctx.Cur.Port
  Stop-BdBoard -Ctx $ctx
  Test-BdRefusal -Ctx $ctx -Name 'lease-api.md#the-board-read: A board that cannot be reached, where a stand-in ended, is internal naming the host and port alone' -Arguments (Get-BdArgs -Board $bd -Epic '52') -Code internal -Message (Get-BdNoAnswer $gonePort) -NeedsToken
}

Use-BdLab -Name 'the 64 MiB bound' -Body {
  param($ctx)
  $bd = $script:BdB1
  $cards = [ordered]@{ '52' = (New-BdCard -Description (New-BdDescription -Plan $script:BdPlanSmall)) }
  $line52 = Get-BdTicketLine -Board $bd -Ticket '52'
  $bound = 67108864
  foreach ($chunked in @($false, $true)) {
    $how = if ($chunked) { 'sent in chunks with no length' } else { 'sent with a Content-Length' }
    foreach ($n in @($bound, ($bound + 1))) {
      $key = "bound-$n-$chunked"
      Start-BdBoard -Ctx $ctx -Key $key -Fixture (New-BdFixture -Cards $cards -HasAnswer -Answer @{ body_bytes = $n; chunked = $chunked }) | Out-Null
      Set-BdGoodCredential -Ctx $ctx
      if ($n -eq $bound) {
        Test-BdGoodRead -Ctx $ctx -Name "lease-api.md#the-board-read: A body of exactly 67108864 bytes, $how, is read as a good epic" -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $bd -Epic '52') -Expect @{ Plan = 'Board small'; Turn = 'T01'; Agents = 2 } -Log @($line52) -NeedsToken
      } else {
        Test-BdRefusal -Ctx $ctx -Name "lease-api.md#the-board-read: A body of 67108865 bytes, $how, is invalid_value for epic with the 64 MiB reason, after one GET" -Arguments (Get-BdArgs -Board $bd -Epic '52') -Code invalid_value -Field epic -Message (Get-BdNoPlan '52' $script:BdWhy.Big) -Log @($line52) -NeedsToken
      }
      Stop-BdBoard -Ctx $ctx
    }
  }
}

# --- Group 4: the receipt, case by case, with the stand-in listening and its log empty -----------------------

function Test-BdReceiptCase {
  param(
    [Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Make, [Parameter(Mandatory)][string]$Message,
    [switch]$NeedsToken, [switch]$NoBot, [hashtable]$Set = @{}
  )
  if ($NeedsToken -and -not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $Name; return }
  Clear-BdReceipt -Lab $Ctx.Lab
  $port = $Ctx.Cur.Port
  & $Make $Ctx.Lab $port
  if (-not $NoBot -and $script:BdDpapiOk -and -not (Test-Path -LiteralPath (Get-BdBotPath $Ctx.Lab) -PathType Leaf)) {
    # A valid credential stands beside a faulty receipt, so that a build that wrongly connected would send a token.
    Set-BdBotText -Lab $Ctx.Lab -Text (New-BdBotText -Hex (Get-BdTokenHex))
  }
  Test-BdRefusal -Ctx $Ctx -Name $Name -Arguments (Get-BdArgs -Board $script:BdB1 -Epic '52') -Code internal -Message $Message -Set $Set
}

Use-BdLab -Name 'the receipt cases' -Body {
  param($ctx)
  $lab = $ctx.Lab
  $cards = [ordered]@{ '52' = (New-BdCard -Description (New-BdDescription -Plan $script:BdPA)) }
  Start-BdBoard -Ctx $ctx -Key 'receipt' -Fixture (New-BdFixture -Cards $cards) | Out-Null
  $missing = $script:BdMsgNoReceipt
  $bad = $script:BdMsgBadReceipt

  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: No receipt at all is internal with the message that localswim may not be running, and no request' -Message $missing -NoBot -Make { param($l, $port) }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A folder in the local folder and no receipt file is the same, and no request' -Message $missing -Make { param($l, $port) New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $l -Path (Get-BdLocalDir $l)) | Out-Null }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A folder at the receipt''s path is a receipt that could not be read, and no request' -Message $bad -Make { param($l, $port) New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $l -Path (Get-BdReceiptPath $l)) | Out-Null }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: An empty receipt could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text '' }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt that is not JSON could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text 'this is not json' }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt that is a JSON array could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text '[]' }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt that gives credential_directory twice could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"base":"http://127.0.0.1:' + $port + '","credential_directory":"' + 'a' + '","credential_directory":"' + 'b' + '"}') }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt that gives base twice could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"base":"http://127.0.0.1:' + $port + '","base":"http://127.0.0.1:' + $port + '","credential_directory":' + (ConvertTo-Json -InputObject (Get-BdCredDir $l)) + '}') }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt with no credential_directory could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"base":"http://127.0.0.1:' + $port + '"}') }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt with no base, which agentspace-push allows, could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"credential_directory":' + (ConvertTo-Json -InputObject (Get-BdCredDir $l)) + '}') }
  Test-BdReceiptCase -Ctx $ctx -Name 'lease-api.md#the-board-read: A receipt whose credential_directory is not a string could not be read, and no request' -Message $bad -Make { param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"base":"http://127.0.0.1:' + $port + '","credential_directory":5}') }
  foreach ($nonString in @(@('a number', '123'), @('null', 'null'), @('an array', '["http://127.0.0.1:1"]'), @('an object', '{}'), @('true', 'true'))) {
    $raw = $nonString[1]
    Test-BdReceiptCase -Ctx $ctx -Name "lease-api.md#the-board-read: A receipt whose base is $($nonString[0]), not a string, could not be read, and no request" -Message $bad -NeedsToken -Make ({ param($l, $port) Set-BdReceiptText -Lab $l -Text ('{"base":' + $raw + ',"credential_directory":' + (ConvertTo-Json -InputObject (Get-BdCredDir $l)) + '}') }.GetNewClosure())
  }
  # The one form a base may take is http://127.0.0.1:<port>: each other form is refused before any request, with
  # the stand-in's own port wherever a build that accepted the form would reach it.
  $bases = @(
    @('localhost in place of 127.0.0.1, with the stand-in''s port', 'http://localhost:{0}'), @('https in place of http', 'https://127.0.0.1:{0}'), @('a scheme in capitals', 'HTTP://127.0.0.1:{0}'),
    @('a host that is a name that never resolves', 'http://board.invalid:{0}'), @('127.0.0.2', 'http://127.0.0.2:{0}'), @('[::1]', 'http://[::1]:{0}'),
    @('a user name before the host', 'http://user@127.0.0.1:{0}'), @('a trailing slash', 'http://127.0.0.1:{0}/'), @('a path', 'http://127.0.0.1:{0}/path'), @('a query', 'http://127.0.0.1:{0}?q=1'),
    @('a fragment', 'http://127.0.0.1:{0}#f'), @('a port of 0', 'http://127.0.0.1:0'), @('a port of 65536', 'http://127.0.0.1:65536'), @('a port with a leading zero', 'http://127.0.0.1:0{0}'),
    @('a port with a letter after it', 'http://127.0.0.1:{0}a'), @('no port', 'http://127.0.0.1'), @('127.1', 'http://127.1:{0}'), @('0x7f.0.0.1', 'http://0x7f.0.0.1:{0}'),
    @('a trailing dot after the host', 'http://127.0.0.1.:{0}'), @('a space after the port', 'http://127.0.0.1:{0} '), @('a space before the scheme', ' http://127.0.0.1:{0}'), @('an empty string', '')
  )
  foreach ($b in $bases) {
    $template = $b[1]
    Test-BdReceiptCase -Ctx $ctx -Name "lease-api.md#the-board-read: A base with $($b[0]) is a receipt that could not be read, and no request reaches the stand-in" -Message $bad -NeedsToken -Make ({ param($l, $port) Set-BdReceiptText -Lab $l -Text (ConvertTo-Json -InputObject (Get-BdReceiptObject -Lab $l -Base ([string]::Format($template, $port))) -Compress) }.GetNewClosure())
  }
  # The reads that must succeed: members the reader ignores, a byte order mark, whitespace and member order.
  $line52 = Get-BdTicketLine -Board $script:BdB1 -Ticket '52'
  $expectA = @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 }
  foreach ($variant in @(
      @('A receipt with members the reader ignores and a UTF-8 byte order mark is read', { param($l, $port) Set-BdGoodCredential -Ctx $ctx -Port $port; Set-BdReceiptText -Lab $l -Bom -Text (ConvertTo-Json -InputObject (Get-BdReceiptObject -Lab $l -Base "http://127.0.0.1:$port") -Compress) }),
      @('A receipt written with line breaks, indentation and its members in another order is read', { param($l, $port) Set-BdGoodCredential -Ctx $ctx -Port $port; Set-BdReceiptText -Lab $l -Text ("{`r`n  `"credential_directory`": " + (ConvertTo-Json -InputObject (Get-BdCredDir $l)) + ",`r`n  `"extra`": [1, 2],`r`n  `"base`": `"http://127.0.0.1:$port`"`r`n}`r`n") })
    )) {
    $make = $variant[1]
    if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name "lease-api.md#the-board-read: $($variant[0])"; continue }
    Clear-BdReceipt -Lab $lab
    & $make $lab $ctx.Cur.Port
    Test-BdGoodRead -Ctx $ctx -Name "lease-api.md#the-board-read: $($variant[0]), with one GET and auth=ok" -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $script:BdB1 -Epic '52') -Expect $expectA -Log @($line52) -NeedsToken
  }
  $whole = New-BdProblems
  foreach ($x in (Get-BdMethodProblems -Lines (Read-BdLogLines -Path $ctx.Cur.Log))) { $whole.Add($x) }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: In every case of the receipt lab the log holds no line whose method is not GET' -Problems $whole
}

# --- Group 5: the credential, case by case --------------------------------------------------------------------

function Test-BdCredentialCase {
  param([Parameter(Mandatory)]$Ctx, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Make, [switch]$NeedsToken)
  if ($NeedsToken -and -not $script:BdDpapiOk) { Skip-BdNeedsToken -Name $Name; return }
  $lab = $Ctx.Lab
  Clear-BdReceipt -Lab $lab
  Set-BdReceiptText -Lab $lab -Text (ConvertTo-Json -InputObject (Get-BdReceiptObject -Lab $lab -Base "http://127.0.0.1:$($Ctx.Cur.Port)") -Compress)
  & $Make $lab
  Test-BdRefusal -Ctx $Ctx -Name $Name -Arguments (Get-BdArgs -Board $script:BdB1 -Epic '52') -Code internal -Message $script:BdMsgBadCredential
}

Use-BdLab -Name 'the credential cases' -Body {
  param($ctx)
  $cards = [ordered]@{ '52' = (New-BdCard -Description (New-BdDescription -Plan $script:BdPA)) }
  Start-BdBoard -Ctx $ctx -Key 'credential' -Fixture (New-BdFixture -Cards $cards) | Out-Null
  $prefix = 'lease-api.md#the-board-read: '
  $good = if ($script:BdDpapiOk) { Get-BdTokenHex } else { 'aa' }
  $setBot = { param($l, $text) Set-BdBotText -Lab $l -Text $text }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A credential_directory with no bot.json is a credential that could not be read, and no request" -Make { param($l) New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $l -Path (Get-BdCredDir $l)) | Out-Null }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A credential_directory that does not exist is a credential that could not be read, and no request" -Make { param($l) }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json that is a folder is a credential that could not be read, and no request" -Make { param($l) New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $l -Path (Get-BdBotPath $l)) | Out-Null }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}An empty bot.json is a credential that could not be read, and no request" -Make { param($l) Set-BdBotText -Lab $l -Text '' }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json that is not JSON is a credential that could not be read, and no request" -Make { param($l) Set-BdBotText -Lab $l -Text 'this is not json' }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with a fifth member is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -Extra ',"extra":1') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with role human is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -Role 'human') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with another format is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -Format 'other-credential') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with format_version 1.0 is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -FormatVersion '1.0') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with format_version as the string 1 is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -FormatVersion '"1"') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json with format_version 2 is a credential that could not be read, and no request" -NeedsToken -Make ({ param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $good -FormatVersion '2') }.GetNewClosure())
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json whose hex has an odd number of digits is a credential that could not be read, and no request" -Make { param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex '01000000d08c9ddf0') }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json whose hex has digits that are not hex is a credential that could not be read, and no request" -Make { param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex '01000000d08c9ddfzz') }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json whose blob is a DPAPI header and zero bytes, which DPAPI refuses, is a credential that could not be read, and no request" -Make { param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex $script:BdZeroBlobHex) }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json whose blob decrypts to a string that is too short for a token is a credential that could not be read, and no request" -NeedsToken -Make { param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex (New-BdBlobHex -Plain 'not-a-token')) }
  Test-BdCredentialCase -Ctx $ctx -Name "${prefix}A bot.json whose blob decrypts to 43 characters that are not all base64url is a credential that could not be read, and no request" -NeedsToken -Make { param($l) Set-BdBotText -Lab $l -Text (New-BdBotText -Hex (New-BdBlobHex -Plain ('+' + ('A' * 42)))) }
  # The credential is the receipt's alone: the board's own variables, set to other values, change nothing.
  if (-not $script:BdDpapiOk) { Skip-BdNeedsToken -Name 'lease-api.md#environment: The credential is the receipt''s alone, so variables that name another board address and another token change nothing' }
  else {
    Set-BdGoodCredential -Ctx $ctx
    $otherPort = Get-BdFreePort
    $set = Get-BdLocalswimNulls
    $set['LOCALSWIM_URL'] = "http://127.0.0.1:$otherPort"
    $set['LOCALSWIM_TOKEN'] = New-BdToken
    Test-BdGoodRead -Ctx $ctx -Name 'lease-api.md#environment: The credential is the receipt''s alone: variables that name another loopback port and another token change nothing, and the read reaches the stand-in with auth=ok' -Arguments (Get-BdArgs -Project $script:BdNoHost -Board $script:BdB1 -Epic '52') -Expect @{ Plan = 'Board plan A'; Turn = 'T02'; Agents = 6 } -Log @((Get-BdTicketLine -Board $script:BdB1 -Ticket '52')) -Set $set -NeedsToken
  }
  $whole = New-BdProblems
  foreach ($x in (Get-BdMethodProblems -Lines (Read-BdLogLines -Path $ctx.Cur.Log))) { $whole.Add($x) }
  Complete-BdCheck -Name 'lease-api.md#the-board-read: In every case of the credential lab the log holds no line whose method is not GET' -Problems $whole
}

Exit-ConformanceRun
