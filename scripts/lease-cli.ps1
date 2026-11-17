#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The entry point of `agentspace lease` (D8, D18), the contract in docs/lease-api.md. bin\agentspace.cmd
# and bin/agentspace run it as `pwsh -NoProfile -File scripts\lease-cli.ps1 <args>`. Laptop only.
#
# It owns the command line, --help, the error document and the exit codes, and nothing else:
# - It parses the whole line as "Command lines" says, applies the defaults, and normalizes every
#   value with scripts\lease-values.ps1 in the order of each command's table, so that the error
#   order in "Exit codes and errors" holds before any command runs.
# - It then dot-sources scripts\lease-values.ps1, scripts\lease-state.ps1 and the command's own
#   scripts\lease-cmd-<command>.ps1, and calls its Invoke-Lease<Command> function. Each command file
#   documents that function. A command is changed in its own file, never here.
# - Success writes the returned document to stdout as compact JSON plus one line feed, and exits 0.
#   --help writes its level's text, byte for byte as docs/lease-api.md gives it, and exits 0.
# - Failure writes one error document to stderr and nothing to stdout, and exits with the code's
#   exit code. A thrown lease error (New-LeaseError) is written as it is; any other exception is
#   internal (exit 1).
# - Output is written as UTF-8 bytes with no byte order mark and LF line ends, straight to the
#   process's standard streams, because PowerShell's own output would add CRLF on Windows.
#
# The arguments are read from [Environment]::GetCommandLineArgs(), not $args. When a script runs
# with -File, PowerShell binds its arguments as parameters first: it splits `--ttl=1:30` into
# `--ttl=1` and `30`, and drops `--ttl:` and `-h:` entirely. GetCommandLineArgs() holds them as the
# caller passed them. One case is out of reach: an argument such as `-:x` makes PowerShell fail
# before this script starts, with exit 1 and its own message on stderr.

$ErrorActionPreference = 'Stop'

$script:Help = @{
  top     = @'
Usage: agentspace <command> [<subcommand>] [<args>] [--help]

Leases compute for coding agents.
A lease is compute for one agent for a bounded time, reached over SSH.

Commands:
  lease       Create, show, renew, release and list leases

Options:
  -h, --help  Print this help and exit

Every command prints one JSON document on stdout, or an error document on stderr.
Run 'agentspace lease --help' for the lease subcommands, states and exit codes.
'@
  lease   = @'
Usage: agentspace lease <subcommand> [<args>] [--help]

Manages leases. A lease is compute for one agent for a bounded time.
A ready lease carries an SSH host alias, the user it logs in as, and an expiry.

Subcommands:
  create   Request a lease for a project
  get      Show one lease
  renew    Issue a ready lease a new certificate with a new expiry
  release  Give a lease back and delete its local credential and alias
  list     Show leases, optionally filtered by project and state

Options:
  -h, --help  Print this help and exit

States:
  pending        Accepted and not yet ready; poll with get
  ready          Connect with 'ssh <host alias>' until the expiry
  expired        The expiry has passed; new connections are refused
  released       Given back; its local credential and alias are gone
  unsatisfiable  Can never be met as asked; the lease's reasons say why

Exit codes:
  0  Success; stdout holds one JSON document
  1  Internal error
  2  Usage error, or a value that is malformed or out of range
  3  No lease has that ID
  4  The lease's state does not allow the subcommand
'@
  create  = @'
Usage: agentspace lease create --project <name> [--platform <os/arch>] [--profile <name>]
                               [--size <size>] [--ttl <duration>]

Requests a lease for one agent of a project, and prints it as JSON.
Check the lease's state before connecting: ready, pending or unsatisfiable.
A ready lease's host alias is in your OpenSSH configuration: ssh <host alias>.
A pending lease becomes ready or unsatisfiable; poll it with get.
An unsatisfiable lease lists its reasons. Nothing was issued, and the exit code is still 0.

Options:
  --project <name>      Required. Letters, digits and '-', up to 32; lowercased.
  --platform <os/arch>  Default linux/arm64. Also accepts aarch64 for arm64.
  --profile <name>      Default agent-linux. Accepts name or name@version.
  --size <size>         Default m.
  --ttl <duration>      Default 8h. From 1m to 24h, never clamped.
                        Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
                        A bare number is refused: give a unit.
  -h, --help            Print this help and exit

In v0 only linux/arm64, agent-linux and size m can be met.
Any other well-formed value gives an unsatisfiable lease, never a substitute.
'@
  get     = @'
Usage: agentspace lease get <id>

Prints one lease as JSON, as it stands now. Changes nothing.
A ready lease whose expiry has passed is shown as expired.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
'@
  renew   = @'
Usage: agentspace lease renew <id> [--ttl <duration>]

Issues a ready lease a new certificate, valid from now for the TTL, and prints the lease.
The key, host alias and user stay the same.
The old certificate is not revoked. It stays valid until its own expiry.
If this renew shortens the lease, that expiry is later than the new one.
Only a ready lease can be renewed. An expired lease cannot: create a new one.

Arguments:
  <id>              The lease ID, a UUIDv4 in any case

Options:
  --ttl <duration>  Default: the lease's current TTL. From 1m to 24h, never clamped.
                    Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
  -h, --help        Print this help and exit

Exit 3 means no lease has that ID.
Exit 4 means the lease is not ready; the error names its state.
'@
  release = @'
Usage: agentspace lease release <id>

Gives a lease back, and prints it as JSON.
Deletes the lease's local key, certificate and host alias.
No certificate is revoked. Each stays valid until its own expiry.
An older certificate's expiry is later than the lease's if a renew shortened it.
Connections already open are not closed.
Releasing a released or unsatisfiable lease changes nothing and exits 0.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
'@
  list    = @'
Usage: agentspace lease list [--project <name>] [--state <state>[,<state>...]]...

Prints the leases that match every filter, oldest first, as {"leases":[...]}.
Changes nothing. No match prints an empty list and exits 0.

Options:
  --project <name>  Only this project's leases
  --state <state>   Only leases in these states. Repeat it, or separate states with commas.
                    States: pending, ready, expired, released, unsatisfiable.
  -h, --help        Print this help and exit
'@
}

# Each command's flags in the order of its table in docs/lease-api.md, which is also the order its
# values are checked in. Id names the command's one positional argument, if it has one.
$script:Commands = [ordered]@{
  create  = @{ Id = $false; Flags = @('project', 'platform', 'profile', 'size', 'ttl') }
  get     = @{ Id = $true; Flags = @() }
  renew   = @{ Id = $true; Flags = @('ttl') }
  release = @{ Id = $true; Flags = @() }
  list    = @{ Id = $false; Flags = @('project', 'state') }
}

$script:ExitCodes = @{ internal = 1; usage = 2; invalid_value = 2; out_of_range = 2; not_found = 3; invalid_state = 4 }

function Write-LeaseStream([System.IO.Stream]$Stream, [string]$Text) {
  $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Text)
  $Stream.Write($bytes, 0, $bytes.Length)
  $Stream.Flush()
}

# Writes a text with a line feed after every line, to stdout.
function Write-LeaseHelp([string]$Level) {
  $text = ($script:Help[$Level] -replace "`r`n", "`n") + "`n"
  Write-LeaseStream ([Console]::OpenStandardOutput()) $text
}

# Escapes control characters, which a message MUST NOT contain, as \u and four hex digits.
function Format-LeaseMessage([string]$Message) {
  $m = [regex]::Replace([string]$Message, '[\u0000-\u001f\u007f]', { param($c) '\u{0:x4}' -f [int][char]$c.Value })
  if ($m.Trim() -eq '') { 'agentspace lease failed.' } else { $m }
}

# Writes one error document to stderr and returns its exit code.
function Write-LeaseError([string]$Code, [string]$Message, [string]$Field, [string]$LeaseId, [string]$State) {
  $doc = [ordered]@{ code = $Code; message = (Format-LeaseMessage $Message) }
  if ($Field) { $doc['field'] = $Field }
  if ($LeaseId) { $doc['lease_id'] = $LeaseId }
  if ($State) { $doc['state'] = $State }
  Write-LeaseStream ([Console]::OpenStandardError()) (($doc | ConvertTo-Json -Compress -Depth 5) + "`n")
  $script:ExitCodes[$Code]
}

function Test-LeaseHelpToken([string]$Token) { $Token -ieq '-h' -or $Token -ieq '--help' }

# A usage error, kept until the whole line has been read: --help anywhere at a known level wins.
function New-LeaseUsage([string]$Message, [string]$Field) { @{ Message = $Message; Field = $Field } }

# The caller's arguments, as passed (see the header for why not $args).
function Get-LeaseArguments([object[]]$Fallback) {
  $all = [Environment]::GetCommandLineArgs()
  $self = [System.IO.Path]::GetFullPath($PSCommandPath)
  for ($i = 1; $i -lt $all.Count; $i++) {
    try { $p = [System.IO.Path]::GetFullPath($all[$i]) } catch { continue }
    if ($p -ieq $self -and $all[$i - 1] -imatch '\A-(f|fi|fil|file)\z') {
      return , [string[]]@($all | Select-Object -Skip ($i + 1))
    }
  }
  , [string[]]@($Fallback)
}

# Parses the line. Returns @{ Help = <level> } or @{ Usage = <usage> } or
# @{ Command = <name>; Id = <raw or $null>; Values = @{ <flag> = <raw value or list> } }.
function Read-LeaseCommandLine([string[]]$Tokens) {
  $level = 'top'
  $usage = $null
  $id = $null
  $values = @{}
  $i = 0
  while ($i -lt $Tokens.Count) {
    $t = $Tokens[$i]
    if (Test-LeaseHelpToken $t) { return @{ Help = $level } }
    if ($level -eq 'top' -or $level -eq 'lease') {
      if ($t.StartsWith('-')) {
        $where = if ($level -eq 'top') { 'agentspace' } else { 'agentspace lease' }
        if (-not $usage) { $usage = New-LeaseUsage "Unknown option $(Format-LeaseQuoted $t). Run '$where --help' for its options." }
      } elseif ($level -eq 'top') {
        if ($t -cnotmatch '\A[A-Za-z]+\z' -or $t -ine 'lease') {
          return @{ Usage = (New-LeaseUsage "Unknown command $(Format-LeaseQuoted $t). Run 'agentspace --help' for the commands.") }
        }
        $level = 'lease'
      } else {
        $name = $t.ToLowerInvariant()
        if (-not $script:Commands.Contains($name) -or $t -cnotmatch '\A[A-Za-z]+\z') {
          return @{ Usage = (New-LeaseUsage "Unknown subcommand $(Format-LeaseQuoted $t). Run 'agentspace lease --help' for the subcommands.") }
        }
        $level = $name
      }
      $i++
      continue
    }

    $spec = $script:Commands[$level]
    $helpHint = "Run 'agentspace lease $level --help'."
    if ($t.StartsWith('-')) {
      $flag = $null
      $value = $null
      $hasValue = $false
      if ($t -cmatch '\A--(?<name>[A-Za-z]+)(?<eq>=(?<value>.*))?\z' -and $spec.Flags -contains $Matches['name'].ToLowerInvariant()) {
        $flag = $Matches['name'].ToLowerInvariant()
        if ($Matches['eq']) { $value = $Matches['value']; $hasValue = $true }
      }
      if (-not $flag) {
        if (-not $usage) { $usage = New-LeaseUsage "Unknown option $(Format-LeaseQuoted $t) for agentspace lease $level. $helpHint" }
        $i++
        continue
      }
      if (-not $hasValue -and $i + 1 -lt $Tokens.Count -and -not $Tokens[$i + 1].StartsWith('--')) {
        $i++
        $value = $Tokens[$i]
        if (Test-LeaseHelpToken $value) { return @{ Help = $level } }
        $hasValue = $true
      }
      if (-not $hasValue) {
        if (-not $usage) { $usage = New-LeaseUsage "Option --$flag needs a value. $helpHint" $flag }
      } elseif ($value -eq '') {
        if (-not $usage) { $usage = New-LeaseUsage "Option --$flag has an empty value. $helpHint" $flag }
      } elseif ($flag -eq 'state') {
        if (-not $values.ContainsKey('state')) { $values['state'] = [System.Collections.Generic.List[string]]::new() }
        $values['state'].Add($value)
      } elseif ($values.ContainsKey($flag)) {
        if (-not $usage) { $usage = New-LeaseUsage "Option --$flag is given twice. Give it once." $flag }
      } else {
        $values[$flag] = $value
      }
      $i++
      continue
    }

    if ($spec.Id -and $null -eq $id) {
      $id = $t
    } elseif (-not $usage) {
      $usage = New-LeaseUsage "Unexpected argument $(Format-LeaseQuoted $t) for agentspace lease $level. $helpHint"
    }
    $i++
  }

  if ($usage) { return @{ Usage = $usage } }
  if ($level -eq 'top') { return @{ Usage = (New-LeaseUsage "No command given. Run 'agentspace --help' for the commands.") } }
  if ($level -eq 'lease') { return @{ Usage = (New-LeaseUsage "No subcommand given. Run 'agentspace lease --help' for the subcommands.") } }
  if ($script:Commands[$level].Id -and $null -eq $id) {
    return @{ Usage = (New-LeaseUsage "No lease ID given. Run 'agentspace lease $level --help'." 'id') }
  }
  if ($level -eq 'create' -and -not $values.ContainsKey('project')) {
    return @{ Usage = (New-LeaseUsage "Option --project is required. Run 'agentspace lease create --help'." 'project') }
  }
  @{ Command = $level; Id = $id; Values = $values }
}

# Normalizes the values in the order of the command's table and returns the function's arguments.
function ConvertTo-LeaseCommandArguments([hashtable]$Line) {
  $v = $Line.Values
  switch ($Line.Command) {
    'create' {
      [ordered]@{
        Project    = ConvertTo-LeaseProjectValue $v['project']
        Platform   = if ($v.ContainsKey('platform')) { ConvertTo-LeasePlatformValue $v['platform'] } else { 'linux/arm64' }
        Profile    = if ($v.ContainsKey('profile')) { ConvertTo-LeaseProfileValue $v['profile'] } else { 'agent-linux' }
        Size       = if ($v.ContainsKey('size')) { ConvertTo-LeaseSizeValue $v['size'] } else { 'm' }
        TtlSeconds = if ($v.ContainsKey('ttl')) { ConvertTo-LeaseTtlValue $v['ttl'] } else { 28800 }
      }
    }
    'renew' {
      [ordered]@{
        LeaseId    = ConvertTo-LeaseIdValue $Line.Id
        TtlSeconds = if ($v.ContainsKey('ttl')) { ConvertTo-LeaseTtlValue $v['ttl'] } else { 0 }
      }
    }
    'list' {
      [ordered]@{
        Project = if ($v.ContainsKey('project')) { ConvertTo-LeaseProjectValue $v['project'] } else { '' }
        State   = if ($v.ContainsKey('state')) { ConvertTo-LeaseStateFilterValue $v['state'].ToArray() } else { [string[]]@() }
      }
    }
    default { [ordered]@{ LeaseId = ConvertTo-LeaseIdValue $Line.Id } }
  }
}

$code = 1
try {
  . (Join-Path $PSScriptRoot 'lease-values.ps1')
  $tokens = Get-LeaseArguments $args
  $line = Read-LeaseCommandLine $tokens
  if ($line.Help) {
    Write-LeaseHelp $line.Help
    $code = 0
  } elseif ($line.Usage) {
    $code = Write-LeaseError -Code usage -Message $line.Usage.Message -Field $line.Usage.Field
  } else {
    $arguments = ConvertTo-LeaseCommandArguments $line
    . (Join-Path $PSScriptRoot 'lease-state.ps1')
    . (Join-Path $PSScriptRoot "lease-cmd-$($line.Command).ps1")
    $function = 'Invoke-Lease' + $line.Command.Substring(0, 1).ToUpperInvariant() + $line.Command.Substring(1)
    $result = & $function @arguments
    $json = ConvertTo-Json -InputObject $result -Compress -Depth 20
    Write-LeaseStream ([Console]::OpenStandardOutput()) ($json + "`n")
    $code = 0
  }
} catch {
  $lease = $null
  try { $lease = Get-LeaseErrorException $_.Exception } catch { }
  if ($lease) {
    $d = $lease.Data
    $code = Write-LeaseError -Code $d['agentspace.lease.code'] -Message $lease.Message -Field $d['agentspace.lease.field'] `
      -LeaseId $d['agentspace.lease.lease_id'] -State $d['agentspace.lease.state']
  } else {
    $what = if ($line -and $line.Command) { "agentspace lease $($line.Command)" } else { 'agentspace lease' }
    $code = Write-LeaseError -Code internal -Message "$what failed: $($_.Exception.Message)"
  }
}
exit $code
