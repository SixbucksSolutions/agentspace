#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The lease CLI's state file (D8, D21): every lease created on this laptop, as one JSON document.
# Laptop only: nothing here touches the box or AWS.
#
# Where: %USERPROFILE%\.agentspace\leases.json. The environment variable AGENTSPACE_STATE_DIR, when
# set, names another folder; tests and worktrees MUST use it, so they never touch the real leases.
#
# The document is {"version":0,"leases":[<record>...]}, checked against
# schemas/state/leases.state.schema.json on every read and before every write. A record holds the
# lease document's fields (docs/lease-api.md, "Fields") with its state as stored: pending, ready,
# released or unsatisfiable. expired is never stored; ConvertTo-LeaseOutput derives it when
# now >= expires_at. A record MAY also hold driver, the opaque driver name that
# Invoke-LeaseDriverPlace returned, which MUST NOT reach any output. Times are strings in the
# contract's form YYYY-MM-DDTHH:MM:SSZ and are kept exactly as written: reads use -DateKind String,
# since ConvertFrom-Json otherwise turns them into DateTime values.
#
# Concurrency (docs/lease-api.md, "Concurrency and atomicity"):
# - Update-LeaseState holds leases.lock, opened with FileShare.None, for the whole read, change and
#   write, so two commands never change the file at the same time. It waits up to 30 seconds for it.
# - It writes a temporary file in the same folder and moves it over leases.json, so a reader sees
#   the old document or the new one, never part of one. If anything fails before the move,
#   leases.json is left exactly as it was.
# - Read-LeaseState takes no lock. It opens the file with FileShare ReadWrite and Delete, so it
#   never blocks the move.
#
# Functions:
#   Get-LeaseStatePath                    The state file's full path.
#   Read-LeaseState                       The records, as a List[object] of ordered hashtables.
#   Update-LeaseState -Script {param($Records) ...}
#                                         Runs the script block on the records under the lock, then
#                                         validates and writes them. The block changes $Records in
#                                         place (add, edit, remove). Whatever it returns is returned.
#                                         A block that throws changes nothing, and its error, a lease
#                                         error included, reaches the caller unchanged.
#   Find-LeaseRecord -Records -LeaseId    The record with that normalized ID, or $null.
#   ConvertTo-LeaseOutput -Record [-Now]  The lease document a command prints for a record.

. (Join-Path $PSScriptRoot 'lease-values.ps1')

$script:LeaseStateSchema = Join-Path (Split-Path -Parent $PSScriptRoot) 'schemas\state\leases.state.schema.json'

function Get-LeaseStateDir {
  if ($env:AGENTSPACE_STATE_DIR) { $env:AGENTSPACE_STATE_DIR } else { Join-Path $env:USERPROFILE '.agentspace' }
}

function Get-LeaseStatePath { Join-Path (Get-LeaseStateDir) 'leases.json' }

# Returns $true when the text is a valid state document.
function Test-LeaseStateJson([string]$Json) {
  try { [bool](Test-Json -Json $Json -SchemaFile $script:LeaseStateSchema -ErrorAction Stop) } catch { $false }
}

function Read-LeaseStateText([string]$Path) {
  $fs = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read,
    [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete)
  try {
    $sr = [System.IO.StreamReader]::new($fs, [System.Text.UTF8Encoding]::new($false, $true))
    $sr.ReadToEnd()
  } finally { $fs.Dispose() }
}

function Read-LeaseState {
  $path = Get-LeaseStatePath
  $records = [System.Collections.Generic.List[object]]::new()
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return , $records }
  $text = Read-LeaseStateText $path
  if (-not (Test-LeaseStateJson $text)) {
    throw (New-LeaseError -Code internal -Message 'The lease records could not be read: the state file is not valid. Nothing was changed.')
  }
  $doc = $text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 20
  foreach ($r in $doc['leases']) { $records.Add($r) }
  , $records
}

function Write-LeaseStateText([string]$Path, [string]$Text) {
  $dir = Split-Path -Parent $Path
  $tmp = Join-Path $dir ("leases.json.{0}.tmp" -f [guid]::NewGuid().ToString('n'))
  try {
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Text)
    $fs = [System.IO.FileStream]::new($tmp, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write,
      [System.IO.FileShare]::None, 4096, [System.IO.FileOptions]::WriteThrough)
    try { $fs.Write($bytes, 0, $bytes.Length); $fs.Flush($true) } finally { $fs.Dispose() }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ($true) {
      try { [System.IO.File]::Move($tmp, $Path, $true); break }
      catch [System.IO.IOException], [System.UnauthorizedAccessException] {
        if ([DateTime]::UtcNow -gt $deadline) { throw }
        Start-Sleep -Milliseconds 20
      }
    }
  } finally {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
  }
}

# The script block runs in a child scope of this function, and PowerShell looks variables up
# through the scopes it was called from, so a local here named like one of the caller's variables
# would hide the caller's. Every local is therefore prefixed __lease (found in E1.02, when the
# test's $records read this function's own list).
function Update-LeaseState {
  param([Parameter(Mandatory)][scriptblock]$Script)
  $__leaseDir = Get-LeaseStateDir
  if (-not (Test-Path -LiteralPath $__leaseDir -PathType Container)) { New-Item -ItemType Directory -Force -Path $__leaseDir | Out-Null }
  $__leaseLockPath = Join-Path $__leaseDir 'leases.lock'
  $__leaseDeadline = [DateTime]::UtcNow.AddSeconds(30)
  $__leaseLock = $null
  while (-not $__leaseLock) {
    try {
      $__leaseLock = [System.IO.FileStream]::new($__leaseLockPath, [System.IO.FileMode]::OpenOrCreate,
        [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    } catch [System.IO.IOException] {
      if ([DateTime]::UtcNow -gt $__leaseDeadline) {
        throw (New-LeaseError -Code internal -Message 'The lease records are busy: another agentspace lease command held them for 30 seconds. Try again.')
      }
      Start-Sleep -Milliseconds 50
    }
  }
  try {
    $__leaseRecords = Read-LeaseState
    $__leaseResult = & $Script $__leaseRecords
    $__leaseIds = @($__leaseRecords | ForEach-Object { $_['id'] })
    if (@($__leaseIds | Sort-Object -Unique).Count -ne $__leaseIds.Count) {
      throw (New-LeaseError -Code internal -Message 'The lease records could not be written: two records share an ID. Nothing was changed.')
    }
    $__leaseText = ([ordered]@{ version = 0; leases = $__leaseRecords.ToArray() } | ConvertTo-Json -Compress -Depth 20) + "`n"
    if (-not (Test-LeaseStateJson $__leaseText)) {
      throw (New-LeaseError -Code internal -Message 'The lease records could not be written: the change is not valid. Nothing was changed.')
    }
    Write-LeaseStateText (Get-LeaseStatePath) $__leaseText
    $__leaseResult
  } finally {
    $__leaseLock.Dispose()
  }
}

function Find-LeaseRecord {
  param([Parameter(Mandatory)][AllowEmptyCollection()][object]$Records, [Parameter(Mandatory)][string]$LeaseId)
  foreach ($r in $Records) { if ($r['id'] -ceq $LeaseId) { return $r } }
  $null
}

# The lease document for a record, as of Now (default: the current second). A ready record whose
# expires_at has passed comes out expired, without ssh. driver never comes out, and a field that
# does not apply to the state is left out.
function ConvertTo-LeaseOutput {
  param([Parameter(Mandatory)][System.Collections.IDictionary]$Record, [Nullable[DateTime]]$Now)
  $at = if ($null -ne $Now) { ([DateTime]$Now).ToUniversalTime() } else { Get-LeaseNow }
  $state = $Record['state']
  if ($state -ceq 'ready' -and $at -ge (ConvertFrom-LeaseTime $Record['expires_at'])) { $state = 'expired' }
  $o = [ordered]@{
    id          = $Record['id']
    project     = $Record['project']
    state       = $state
    platform    = $Record['platform']
    profile     = $Record['profile']
    size        = $Record['size']
    ttl_seconds = [int64]$Record['ttl_seconds']
    created_at  = $Record['created_at']
  }
  if ($state -in 'ready', 'expired', 'released' -and $Record.Contains('expires_at')) { $o['expires_at'] = $Record['expires_at'] }
  if ($state -ceq 'released') { $o['released_at'] = $Record['released_at'] }
  if ($state -ceq 'ready') { $o['ssh'] = [ordered]@{ host_alias = $Record['ssh']['host_alias']; user = $Record['ssh']['user'] } }
  if ($state -ceq 'unsatisfiable') {
    $o['reasons'] = @($Record['reasons'] | ForEach-Object { [ordered]@{ code = $_['code']; message = $_['message'] } })
  }
  $o
}
