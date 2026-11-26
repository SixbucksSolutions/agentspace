#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The stand-in board for the lease CLI's board cases (E0.11-T04-S01, card #0239; D86, D88, D89).
# Its interface is docs/conformance.md, "The stand-in board and its credential", and this script is
# built from that section and from docs/lease-api.md, "The board read", alone. It stands for
# localswim, the caller's board, and for nothing of agentspace's.
#
#   pwsh -NoProfile -File conformance/lease-cli/board-standin.ps1 -Port <n> -Fixture <path> -Log <path> -Ready <path>
#
# - It reads the fixture and writes the log and the ready file, and nothing else. It makes no
#   outbound connection, follows and makes no redirect, and answers no Location header.
# - It refuses with exit 2, before it listens and without writing the ready file: a missing or extra
#   argument, a -Port that is not 1 to 65535 in decimal, -Port 8811 (checked first, before the
#   fixture is read or the log is opened), a fixture that is missing, unreadable or breaks the
#   section's table, a log it cannot open, and a port it cannot bind. A reason goes to stderr and
#   names a member or an argument, never a value or a path.
# - It listens on 127.0.0.1 alone, with a TCP socket bound to IPAddress.Loopback with exclusive use of
#   the address, and needs no elevation. It writes the ready file only once the listener is bound: its
#   PID in decimal and one LF, UTF-8 with no byte order mark. A reader that finds the file empty
#   reads it again.
# - It serves its connections from one loop, one request at a time: each request is logged and
#   flushed, then answered or held, before the next one is read. Every answer carries
#   `Connection: close`, and the connection is closed once the client has closed its side, or after
#   5 seconds. A connection that sent nothing for 30 seconds is closed and not logged. It ends
#   itself, exit 0, 10 minutes after it starts.
# - The token is compared and dropped. The Authorization header's value is compared while the
#   request is parsed and kept in no variable past that; only `ok`, `wrong` or `none` is. The log
#   line is `<METHOD> <path and query as received> auth=<ok|wrong|none>`, LF, flushed per line, and
#   a request it could not parse is `INVALID - auth=none`. Should a method or a request target hold
#   the token's text (raw or percent-decoded), that part is logged as `[token-withheld]` in its
#   place, so a client that put the token in a URL is caught by the line and the token is not logged.
#
# The order of checks (conformance.md): an `answer` of `{"status": ...}` or `"hang"` overrides every
# reply; then the method (405, Problem `method_not_allowed`, `Allow: GET`); then the
# `/api/2026-11-10/` prefix (a bare 404); then Authorization (401, Problem `unauthorized`); then the
# lookup (404, Problem `not_found`, the same document for an unknown board and an unknown ticket,
# with a fresh request_id). A request that cannot be parsed is answered 400 (Problem
# `invalid_request`), as is a query the view cannot use. A HEAD gets its answer's headers and no body.
#
# The ticket view is a whole TicketInspection (localswim's docs/openapi.yaml, readBoard). The card's
# revision and archived come from the fixture, and prose.description is the fixture's description
# exactly as its JSON text is written there. prose is present only for include_prose=true, as
# localswim's is. Every other part is filled in, and every string of it a client has no use for
# begins `STANDIN-MARKER-`, so a case searches the program's output for that prefix:
#   card.title, the lane, owner and priority labels of every card, prose.comments[].text,
#   relationships[].card.title, parent.title and children[].title.
# Each marker ends with a tag of the board ID's first 8 characters and the ticket, and the number of
# comments (2 to 4), relationships (1 or 2) and children (1 or 2) follows the ticket. So two cards of
# the same revision, archived and description, on two boards or under two tickets, differ in every
# member the client ignores, which is how a case shows that changing them changes nothing.
#
# An `answer` of `{"body_bytes": n, "chunked": b}` answers the ticket view of a known card, with the
# right token, with that card's document (prose included) padded to exactly n bytes, streamed: by
# lengthening the last comment's text when the gap is smaller than one more comment, and otherwise
# by adding comments of `x` text, each under 4,000,000 characters. chunked true sends
# `Transfer-Encoding: chunked` and no Content-Length; false sends a Content-Length of n. An n
# smaller than any card's unpadded document is refused with exit 2 at the start.

param(
  [string]$Port,
  [string]$Fixture,
  [string]$Log,
  [string]$Ready
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:MaxSafe = [long]9007199254740991
$script:ApiPrefix = '/api/2026-11-10/'
$script:LifetimeMs = [long]600000
$script:IdleMs = [long]10000
$script:SilentMs = [long]30000
$script:ClosingMs = [long]5000
$script:HeaderCap = 65536
$script:BodyWaitCap = 1048576
$script:PadMaxChars = 4000000
$script:CreatedAt = '2026-11-10 14:44:56+00:00'
$script:Utf8 = [System.Text.UTF8Encoding]::new($false)
$script:Utf8Strict = [System.Text.UTF8Encoding]::new($false, $true)
$script:Latin1 = [System.Text.Encoding]::Latin1
$script:Recv = [byte[]]::new(65536)
$script:Filler = [byte[]]::new(65536)
[Array]::Fill($script:Filler, [byte]0x78)
$script:Crlf = [byte[]](13, 10)

function Stop-StandinRefused([string]$Reason) {
  [Console]::Error.WriteLine("board-standin: $Reason")
  exit 2
}

# --- Arguments: -Port first, so 8811 is refused before anything is read, opened or bound -------------

if ($args.Count -gt 0) { Stop-StandinRefused 'it takes -Port, -Fixture, -Log and -Ready, and nothing else.' }
if ([string]::IsNullOrEmpty($Port) -or $Port -cnotmatch '^[1-9][0-9]{0,4}\z' -or [int]$Port -gt 65535) {
  Stop-StandinRefused '-Port MUST be a decimal port from 1 to 65535.'
}
$script:PortNumber = [int]$Port
if ($script:PortNumber -eq 8811) { Stop-StandinRefused "-Port 8811 is localswim's own port, which a stand-in MUST NOT use." }
foreach ($pair in @(@('Fixture', $Fixture), @('Log', $Log), @('Ready', $Ready))) {
  if ([string]::IsNullOrWhiteSpace($pair[1])) { Stop-StandinRefused "-$($pair[0]) MUST name a file." }
}
$here = (Get-Location).ProviderPath
$fixturePath = [System.IO.Path]::GetFullPath($Fixture, $here)
$logPath = [System.IO.Path]::GetFullPath($Log, $here)
$readyPath = [System.IO.Path]::GetFullPath($Ready, $here)

# --- The fixture ------------------------------------------------------------------------------------

function Get-StandinObject([System.Text.Json.JsonElement]$Element, [string]$What) {
  if ($Element.ValueKind -ne [System.Text.Json.JsonValueKind]::Object) { Stop-StandinRefused "$What is not a JSON object." }
  $map = [System.Collections.Generic.Dictionary[string, System.Text.Json.JsonElement]]::new([StringComparer]::Ordinal)
  foreach ($p in $Element.EnumerateObject()) {
    if ($map.ContainsKey($p.Name)) { Stop-StandinRefused "$What holds a member twice." }
    $map[$p.Name] = $p.Value
  }
  return , $map
}

function Assert-StandinMembers($Map, [string[]]$Required, [string[]]$Optional, [string]$What) {
  foreach ($name in $Map.Keys) {
    if ($Required -cnotcontains $name -and $Optional -cnotcontains $name) { Stop-StandinRefused "$What holds a member the fixture does not define." }
  }
  foreach ($name in $Required) {
    if (-not $Map.ContainsKey($name)) { Stop-StandinRefused "$What lacks its required member '$name'." }
  }
}

function Get-StandinInteger([System.Text.Json.JsonElement]$Element, [long]$Min, [long]$Max, [string]$What) {
  if ($Element.ValueKind -ne [System.Text.Json.JsonValueKind]::Number) { Stop-StandinRefused "$What is not a number." }
  $raw = $Element.GetRawText()
  if ($raw -cnotmatch '^(0|[1-9][0-9]{0,15})\z') { Stop-StandinRefused "$What is not an integer written as digits alone." }
  $value = [decimal]$raw
  if ($value -lt $Min -or $value -gt $Max) { Stop-StandinRefused "$What is out of its range, $Min to $Max." }
  return [long]$value
}

function Get-StandinBoolean([System.Text.Json.JsonElement]$Element, [string]$What) {
  if ($Element.ValueKind -eq [System.Text.Json.JsonValueKind]::True) { return $true }
  if ($Element.ValueKind -eq [System.Text.Json.JsonValueKind]::False) { return $false }
  Stop-StandinRefused "$What is not a boolean."
}

try {
  $fixtureBytes = [System.IO.File]::ReadAllBytes($fixturePath)
}
catch {
  Stop-StandinRefused 'the fixture is missing or cannot be read.'
}
$offset = 0
if ($fixtureBytes.Length -ge 3 -and $fixtureBytes[0] -eq 0xEF -and $fixtureBytes[1] -eq 0xBB -and $fixtureBytes[2] -eq 0xBF) { $offset = 3 }
try {
  $fixtureText = $script:Utf8Strict.GetString($fixtureBytes, $offset, $fixtureBytes.Length - $offset)
  $doc = [System.Text.Json.JsonDocument]::Parse($fixtureText, [System.Text.Json.JsonDocumentOptions]::new())
}
catch {
  Stop-StandinRefused 'the fixture is not one UTF-8 JSON document.'
}
$fixtureText = $null
$fixtureBytes = $null

$root = Get-StandinObject $doc.RootElement 'The fixture'
Assert-StandinMembers $root @('token', 'boards') @('answer') 'The fixture'

$tokenElement = $root['token']
if ($tokenElement.ValueKind -ne [System.Text.Json.JsonValueKind]::String) { Stop-StandinRefused "The fixture's token is not a string." }
$script:Token = $tokenElement.GetString()
if ($script:Token -cnotmatch '^[A-Za-z0-9_-]{43}\z') { Stop-StandinRefused "The fixture's token is not 43 characters of A-Z, a-z, 0-9, - and _." }
$script:BearerValue = 'Bearer ' + $script:Token

$script:Boards = [System.Collections.Generic.Dictionary[string, object]]::new([StringComparer]::Ordinal)
$boardsMap = Get-StandinObject $root['boards'] "The fixture's boards"
foreach ($boardId in $boardsMap.Keys) {
  if ($boardId -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') {
    Stop-StandinRefused 'A board ID of the fixture is not a version 4 UUID in lowercase.'
  }
  $boardMap = Get-StandinObject $boardsMap[$boardId] 'A board of the fixture'
  Assert-StandinMembers $boardMap @('cards') @() 'A board of the fixture'
  $cardsMap = Get-StandinObject $boardMap['cards'] "A board's cards"
  $cards = [System.Collections.Generic.Dictionary[string, object]]::new([StringComparer]::Ordinal)
  foreach ($ticket in $cardsMap.Keys) {
    if ($ticket -cnotmatch '^[1-9][0-9]{0,15}\z' -or [decimal]$ticket -gt $script:MaxSafe) {
      Stop-StandinRefused 'A ticket of the fixture is not decimal digits from 1 to 9007199254740991 with no leading zero.'
    }
    $cardMap = Get-StandinObject $cardsMap[$ticket] 'A card of the fixture'
    Assert-StandinMembers $cardMap @('revision', 'archived', 'description') @() 'A card of the fixture'
    $revision = Get-StandinInteger $cardMap['revision'] 1 $script:MaxSafe "A card's revision"
    $archived = Get-StandinBoolean $cardMap['archived'] "A card's archived"
    $description = $cardMap['description']
    if ($description.ValueKind -ne [System.Text.Json.JsonValueKind]::String) { Stop-StandinRefused "A card's description is not a string." }
    $cards[$ticket] = [pscustomobject]@{
      Revision       = $revision
      Archived       = $archived
      DescriptionRaw = $description.GetRawText()
    }
  }
  $script:Boards[$boardId] = $cards
}

# AnswerKind: none, status, hang or body.
$script:AnswerKind = 'none'
$script:AnswerStatus = 0
$script:AnswerProblem = $true
$script:BodyBytes = [long]0
$script:BodyChunked = $false
if ($root.ContainsKey('answer')) {
  $answer = $root['answer']
  if ($answer.ValueKind -eq [System.Text.Json.JsonValueKind]::Null) { }
  elseif ($answer.ValueKind -eq [System.Text.Json.JsonValueKind]::String) {
    if ($answer.GetString() -cne 'hang') { Stop-StandinRefused "The fixture's answer is a string other than ""hang""." }
    $script:AnswerKind = 'hang'
  }
  elseif ($answer.ValueKind -eq [System.Text.Json.JsonValueKind]::Object) {
    $answerMap = Get-StandinObject $answer "The fixture's answer"
    if ($answerMap.ContainsKey('status')) {
      Assert-StandinMembers $answerMap @('status') @('problem') "The fixture's answer"
      $script:AnswerKind = 'status'
      $script:AnswerStatus = [int](Get-StandinInteger $answerMap['status'] 400 599 "The answer's status")
      if ($answerMap.ContainsKey('problem')) {
        if ($answerMap['problem'].ValueKind -ne [System.Text.Json.JsonValueKind]::False) {
          Stop-StandinRefused "The answer's problem, when given, MUST be false."
        }
        $script:AnswerProblem = $false
      }
    }
    else {
      Assert-StandinMembers $answerMap @('body_bytes', 'chunked') @() "The fixture's answer"
      $script:AnswerKind = 'body'
      $script:BodyBytes = Get-StandinInteger $answerMap['body_bytes'] 1 $script:MaxSafe "The answer's body_bytes"
      $script:BodyChunked = Get-StandinBoolean $answerMap['chunked'] "The answer's chunked"
    }
  }
  else {
    Stop-StandinRefused "The fixture's answer is not null, ""hang"" or an object."
  }
}
$doc.Dispose()
$root = $null
$tokenElement = $null

# --- The documents it answers ------------------------------------------------------------------------

function Get-StandinId([long]$Kind, [long]$Index, [long]$Ticket) {
  return ('{0:x8}-{1:x4}-4{2:x3}-8{3:x3}-{4:x12}' -f $Kind, ($Index % 0x10000), 0, 0, ($Ticket % 0x1000000000000))
}

function Get-StandinOtherTicket([long]$Ticket, [long]$Step) {
  if ($Ticket -le $script:MaxSafe - $Step) { return $Ticket + $Step }
  return $Ticket - $Step
}

function Get-StandinCardContext([long]$Ticket, [long]$Revision, [bool]$Archived, [string]$Title, [string]$Tag, [long]$Index) {
  $archivedText = if ($Archived) { 'true' } else { 'false' }
  return '{"id":"' + (Get-StandinId 0x5a0d0001 $Index $Ticket) + '","ticket":' + $Ticket + ',"revision":' + $Revision +
    ',"title":"' + $Title + '","archived":' + $archivedText +
    ',"lane":{"id":"' + (Get-StandinId 0x5a0d0002 0 $Ticket) + '","key":"in_progress","label":"STANDIN-MARKER-lane-' + $Tag + '","role":"ordinary","retired":false}' +
    ',"owner":{"id":"' + (Get-StandinId 0x5a0d0003 0 $Ticket) + '","label":"STANDIN-MARKER-owner-' + $Tag + '"}' +
    ',"priority":{"id":"' + (Get-StandinId 0x5a0d0004 0 $Ticket) + '","key":"normal","label":"STANDIN-MARKER-priority-' + $Tag + '","rank":2}}'
}

function Get-StandinCommentHead([string]$Board, [long]$Ticket, [long]$Index) {
  return '{"id":"' + (Get-StandinId 0x5a0d0005 $Index $Ticket) + '","created_at":"' + $script:CreatedAt + '","board_id":"' + $Board +
    '","card_id":"' + (Get-StandinId 0x5a0d0001 0 $Ticket) + '","text":"'
}

function Get-StandinCommentTail([long]$Ticket) {
  return '","author_id":"' + (Get-StandinId 0x5a0d0006 0 $Ticket) + '"}'
}

# The ticket view's JSON text. With $Prose it ends with `]}}`, the comments array's end, prose's and
# the document's, and $Extra `x` characters lengthen the last comment's text.
function Get-StandinTicketJson([string]$Board, [long]$Ticket, $Card, [bool]$Prose, [int]$Extra) {
  $tag = $Board.Substring(0, 8) + '-ticket-' + $Ticket
  $sb = [System.Text.StringBuilder]::new()
  $boardRevision = if ($Card.Revision -le $script:MaxSafe - 1000) { $Card.Revision + 1000 } else { $Card.Revision }
  [void]$sb.Append('{"board_id":"').Append($Board).Append('","board_revision":').Append($boardRevision)
  [void]$sb.Append(',"card":').Append((Get-StandinCardContext $Ticket $Card.Revision $Card.Archived ('STANDIN-MARKER-title-' + $tag) $tag 0))
  [void]$sb.Append(',"relationships":[')
  $kinds = @(@('blocks', 'outgoing'), @('references', 'incoming'), @('relates_to', 'symmetric'), @('duplicates', 'outgoing'))
  $relationshipCount = 1 + ($Ticket % 2)
  for ($i = 0; $i -lt $relationshipCount; $i++) {
    if ($i -gt 0) { [void]$sb.Append(',') }
    $other = Get-StandinOtherTicket $Ticket (10 + $i)
    [void]$sb.Append('{"id":"').Append((Get-StandinId 0x5a0d0007 $i $Ticket)).Append('","revision":3,"kind":"').Append($kinds[$i][0])
    [void]$sb.Append('","direction":"').Append($kinds[$i][1]).Append('","card":')
    [void]$sb.Append((Get-StandinCardContext $other 3 $false ('STANDIN-MARKER-relationship-' + $i + '-' + $tag) $tag (10 + $i))).Append('}')
  }
  [void]$sb.Append('],"parent":')
  [void]$sb.Append((Get-StandinCardContext (Get-StandinOtherTicket $Ticket 1) 2 $false ('STANDIN-MARKER-parent-' + $tag) $tag 1))
  [void]$sb.Append(',"children":[')
  $childCount = 1 + (($Ticket + 1) % 2)
  for ($i = 0; $i -lt $childCount; $i++) {
    if ($i -gt 0) { [void]$sb.Append(',') }
    [void]$sb.Append((Get-StandinCardContext (Get-StandinOtherTicket $Ticket (2 + $i)) 4 $false ('STANDIN-MARKER-child-' + $i + '-' + $tag) $tag (2 + $i)))
  }
  [void]$sb.Append(']')
  if ($Prose) {
    [void]$sb.Append(',"prose":{"description":').Append($Card.DescriptionRaw).Append(',"comments":[')
    $commentCount = 2 + ($Ticket % 3)
    for ($i = 0; $i -lt $commentCount; $i++) {
      if ($i -gt 0) { [void]$sb.Append(',') }
      [void]$sb.Append((Get-StandinCommentHead $Board $Ticket $i)).Append('STANDIN-MARKER-comment-').Append($i).Append('-').Append($tag)
      if ($i -eq $commentCount - 1 -and $Extra -gt 0) { [void]$sb.Append([char]'x', $Extra) }
      [void]$sb.Append((Get-StandinCommentTail $Ticket))
    }
    [void]$sb.Append(']}')
  }
  [void]$sb.Append('}')
  return $sb.ToString()
}

# The bytes one padding comment adds beyond its text: a comma, its head and its tail.
function Get-StandinPadOverhead([string]$Board, [long]$Ticket) {
  return [long]$script:Utf8.GetByteCount(',' + (Get-StandinCommentHead $Board $Ticket 100) + (Get-StandinCommentTail $Ticket))
}

if ($script:AnswerKind -eq 'body') {
  foreach ($boardId in $script:Boards.Keys) {
    foreach ($ticket in $script:Boards[$boardId].Keys) {
      $unpadded = $script:Utf8.GetByteCount((Get-StandinTicketJson $boardId ([long]$ticket) $script:Boards[$boardId][$ticket] $true 0))
      if ($script:BodyBytes -lt $unpadded) {
        Stop-StandinRefused "The answer's body_bytes is smaller than the unpadded ticket view of a card of the fixture."
      }
    }
  }
}

function Get-StandinReason([int]$Status) {
  $reasons = @{
    200 = 'OK'; 400 = 'Bad Request'; 401 = 'Unauthorized'; 402 = 'Payment Required'; 403 = 'Forbidden'; 404 = 'Not Found'
    405 = 'Method Not Allowed'; 406 = 'Not Acceptable'; 408 = 'Request Timeout'; 409 = 'Conflict'; 410 = 'Gone'
    412 = 'Precondition Failed'; 413 = 'Content Too Large'; 415 = 'Unsupported Media Type'; 422 = 'Unprocessable Content'
    428 = 'Precondition Required'; 429 = 'Too Many Requests'; 500 = 'Internal Server Error'; 501 = 'Not Implemented'
    502 = 'Bad Gateway'; 503 = 'Service Unavailable'; 504 = 'Gateway Timeout'
  }
  if ($reasons.ContainsKey($Status)) { return $reasons[$Status] }
  if ($Status -ge 500) { return 'Server Error' }
  return 'Client Error'
}

function Get-StandinProblemCode([int]$Status) {
  $codes = @{
    400 = 'invalid_request'; 401 = 'unauthorized'; 403 = 'forbidden'; 404 = 'not_found'; 405 = 'method_not_allowed'
    406 = 'not_acceptable'; 409 = 'conflict'; 412 = 'precondition_failed'; 413 = 'payload_too_large'
    415 = 'unsupported_media_type'; 428 = 'precondition_required'; 500 = 'internal_error'; 503 = 'unavailable'
  }
  if ($codes.ContainsKey($Status)) { return $codes[$Status] }
  if ($Status -ge 500) { return 'internal_error' }
  return 'invalid_request'
}

function Get-StandinProblemBytes([int]$Status) {
  $title = Get-StandinReason $Status
  $json = '{"type":"about:blank","title":"' + $title + '","status":' + $Status + ',"code":"' + (Get-StandinProblemCode $Status) +
    '","detail":"The stand-in board answered ' + $Status + ' ' + $title + '.","request_id":"' + [guid]::NewGuid().ToString('D') +
    '","outcome":"not_applicable","errors":[]}'
  return , $script:Utf8.GetBytes($json)
}

# --- The log -----------------------------------------------------------------------------------------

try {
  $script:LogStream = [System.IO.FileStream]::new($logPath, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
}
catch {
  Stop-StandinRefused 'the log cannot be opened for writing.'
}

function Write-StandinLog([string]$Line) {
  $bytes = $script:Utf8.GetBytes($Line + "`n")
  $script:LogStream.Write($bytes, 0, $bytes.Length)
  $script:LogStream.Flush()
}

# --- Requests ----------------------------------------------------------------------------------------

function Test-StandinHoldsToken([string]$Text) {
  if ($Text.Contains($script:Token, [StringComparison]::Ordinal)) { return $true }
  try { $decoded = [System.Uri]::UnescapeDataString($Text) } catch { return $true }
  return $decoded.Contains($script:Token, [StringComparison]::Ordinal)
}

# Returns $null while the request is incomplete, or an object with Valid, Method, Target and Auth.
# The Authorization value is compared here and kept nowhere.
function Read-StandinRequest([System.IO.MemoryStream]$Buffer, [bool]$Ended) {
  $invalid = [pscustomobject]@{ Valid = $false; Method = ''; Target = ''; Auth = 'none' }
  $text = $script:Latin1.GetString($Buffer.GetBuffer(), 0, [int]$Buffer.Length)
  $end = $text.IndexOf("`r`n`r`n", [StringComparison]::Ordinal)
  if ($end -lt 0) {
    if ($Ended -or $Buffer.Length -gt $script:HeaderCap) { return $invalid }
    return $null
  }
  if ($end -gt $script:HeaderCap) { return $invalid }
  $lines = $text.Substring(0, $end).Split([string[]]@("`r`n"), [StringSplitOptions]::None)
  if ($lines[0] -cnotmatch '^([!#$%&''*+.^_`|~0-9A-Za-z-]+) ([\x21-\x7E]+) HTTP/1\.[01]\z') { return $invalid }
  $method = $Matches[1]
  $target = $Matches[2]
  $authCount = 0
  $authOk = $false
  $contentLength = [long]-1
  for ($i = 1; $i -lt $lines.Length; $i++) {
    $line = $lines[$i]
    if ($line.IndexOfAny([char[]]@("`r", "`n", [char]0)) -ge 0) { return $invalid }
    $colon = $line.IndexOf(':')
    if ($colon -lt 1) { return $invalid }
    $name = $line.Substring(0, $colon)
    if ($name -cnotmatch '^[!#$%&''*+.^_`|~0-9A-Za-z-]+\z') { return $invalid }
    if ($name -ieq 'Authorization') {
      $authCount++
      $authOk = [string]::Equals($line.Substring($colon + 1).Trim([char[]]@(' ', "`t")), $script:BearerValue, [StringComparison]::Ordinal)
    }
    elseif ($name -ieq 'Content-Length') {
      $value = $line.Substring($colon + 1).Trim([char[]]@(' ', "`t"))
      if ($value -cnotmatch '^[0-9]{1,15}\z') { return $invalid }
      $parsed = [long]$value
      if ($contentLength -ge 0 -and $contentLength -ne $parsed) { return $invalid }
      $contentLength = $parsed
    }
  }
  $line = $null
  $text = $null
  $lines = $null
  if ($contentLength -gt 0 -and $contentLength -le $script:BodyWaitCap -and -not $Ended) {
    if ($Buffer.Length -lt $end + 4 + $contentLength) { return $null }
  }
  $auth = if ($authCount -eq 0) { 'none' } elseif ($authCount -eq 1 -and $authOk) { 'ok' } else { 'wrong' }
  return [pscustomobject]@{ Valid = $true; Method = $method; Target = $target; Auth = $auth }
}

function Send-StandinBytes($Conn, [byte[]]$Head, [byte[]]$Body) {
  $stream = [System.Net.Sockets.NetworkStream]::new($Conn.Socket, $false)
  try {
    $stream.WriteTimeout = 30000
    $stream.Write($Head, 0, $Head.Length)
    if ($null -ne $Body -and $Body.Length -gt 0) { $stream.Write($Body, 0, $Body.Length) }
    $stream.Flush()
  }
  finally { $stream.Dispose() }
}

function Send-StandinReply($Conn, [int]$Status, [string]$ContentType, [byte[]]$Body, [bool]$HeadOnly, [string[]]$More) {
  $sb = [System.Text.StringBuilder]::new()
  [void]$sb.Append('HTTP/1.1 ').Append($Status).Append(' ').Append((Get-StandinReason $Status)).Append("`r`n")
  if ($ContentType) { [void]$sb.Append('Content-Type: ').Append($ContentType).Append("`r`n") }
  $length = if ($null -eq $Body) { 0 } else { $Body.Length }
  [void]$sb.Append('Content-Length: ').Append($length).Append("`r`n")
  foreach ($m in $More) { if ($m) { [void]$sb.Append($m).Append("`r`n") } }
  [void]$sb.Append("Cache-Control: no-store`r`nConnection: close`r`n`r`n")
  $head = $script:Latin1.GetBytes($sb.ToString())
  if ($HeadOnly) { $Body = $null }
  Send-StandinBytes $Conn $head $Body
}

function Send-StandinProblem($Conn, [int]$Status, [bool]$HeadOnly, [string[]]$More) {
  Send-StandinReply $Conn $Status 'application/problem+json' (Get-StandinProblemBytes $Status) $HeadOnly $More
}

function Send-StandinBare($Conn, [int]$Status) {
  Send-StandinReply $Conn $Status '' $null $false $null
}

function Write-StandinPiece([System.IO.Stream]$Out, [byte[]]$Bytes, [int]$Offset, [int]$Count) {
  if ($Count -le 0) { return }
  if ($script:BodyChunked) {
    $size = $script:Latin1.GetBytes(('{0:x}' -f $Count) + "`r`n")
    $Out.Write($size, 0, $size.Length)
  }
  $Out.Write($Bytes, $Offset, $Count)
  if ($script:BodyChunked) { $Out.Write($script:Crlf, 0, 2) }
  $script:BodySent += [long]$Count
}

# The ticket view padded to exactly BodyBytes bytes, streamed.
function Send-StandinPadded($Conn, [string]$Board, [long]$Ticket, $Card) {
  $n = $script:BodyBytes
  $base = [System.Text.Encoding]::UTF8.GetBytes((Get-StandinTicketJson $Board $Ticket $Card $true 0))
  $gap = $n - $base.Length
  $overhead = Get-StandinPadOverhead $Board $Ticket
  $framing = if ($script:BodyChunked) { 'Transfer-Encoding: chunked' } else { 'Content-Length: ' + $n }
  $head = $script:Latin1.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: application/json`r`n$framing`r`nCache-Control: no-store`r`nConnection: close`r`n`r`n")
  $net = [System.Net.Sockets.NetworkStream]::new($Conn.Socket, $false)
  $net.WriteTimeout = 30000
  $out = [System.IO.BufferedStream]::new($net, 262144)
  $script:BodySent = [long]0
  try {
    $out.Write($head, 0, $head.Length)
    if ($gap -le 0) {
      Write-StandinPiece $out $base 0 $base.Length
    }
    elseif ($gap -le $overhead) {
      $longer = $script:Utf8.GetBytes((Get-StandinTicketJson $Board $Ticket $Card $true ([int]$gap)))
      Write-StandinPiece $out $longer 0 $longer.Length
    }
    else {
      Write-StandinPiece $out $base 0 ($base.Length - 3)
      $count = [long][Math]::Ceiling([double]$gap / [double]($overhead + $script:PadMaxChars))
      $text = $gap - $count * $overhead
      $each = [long][Math]::Floor([double]$text / [double]$count)
      $more = $text - $each * $count
      $tail = $script:Utf8.GetBytes((Get-StandinCommentTail $Ticket))
      for ($k = 0; $k -lt $count; $k++) {
        $padHead = $script:Utf8.GetBytes(',' + (Get-StandinCommentHead $Board $Ticket (100 + $k)))
        Write-StandinPiece $out $padHead 0 $padHead.Length
        $left = $each + $(if ($k -lt $more) { 1 } else { 0 })
        while ($left -gt 0) {
          $step = [int][Math]::Min([long]$script:Filler.Length, $left)
          Write-StandinPiece $out $script:Filler 0 $step
          $left -= $step
        }
        Write-StandinPiece $out $tail 0 $tail.Length
      }
      $end = $script:Latin1.GetBytes(']}}')
      Write-StandinPiece $out $end 0 3
    }
    if ($script:BodyChunked) {
      $last = $script:Latin1.GetBytes("0`r`n`r`n")
      $out.Write($last, 0, $last.Length)
    }
    $out.Flush()
  }
  finally {
    $out.Dispose()
  }
  if ($script:BodySent -ne $n) {
    [Console]::Error.WriteLine("board-standin: internal fault: the padded body was $($script:BodySent) bytes, not $n.")
  }
}

# Answers one request, or holds it. Returns 'close' or 'hold'.
function Invoke-StandinRequest($Conn, $Request) {
  if (-not $Request.Valid) {
    Write-StandinLog 'INVALID - auth=none'
  }
  else {
    $loggedMethod = if (Test-StandinHoldsToken $Request.Method) { '[token-withheld]' } else { $Request.Method }
    $loggedTarget = if (Test-StandinHoldsToken $Request.Target) { '[token-withheld]' } else { $Request.Target }
    Write-StandinLog ($loggedMethod + ' ' + $loggedTarget + ' auth=' + $Request.Auth)
  }
  $headOnly = $Request.Valid -and $Request.Method -ceq 'HEAD'

  if ($script:AnswerKind -eq 'hang') { return 'hold' }
  if ($script:AnswerKind -eq 'status') {
    if ($script:AnswerProblem) { Send-StandinProblem $Conn $script:AnswerStatus $headOnly $null }
    else { Send-StandinBare $Conn $script:AnswerStatus }
    return 'close'
  }
  if (-not $Request.Valid) { Send-StandinProblem $Conn 400 $false $null; return 'close' }
  if ($Request.Method -cne 'GET') { Send-StandinProblem $Conn 405 $headOnly @('Allow: GET'); return 'close' }

  $target = $Request.Target
  $question = $target.IndexOf('?')
  $path = if ($question -ge 0) { $target.Substring(0, $question) } else { $target }
  $query = if ($question -ge 0) { $target.Substring($question + 1) } else { '' }
  if (-not $path.StartsWith($script:ApiPrefix, [StringComparison]::Ordinal)) { Send-StandinBare $Conn 404; return 'close' }
  if ($Request.Auth -ne 'ok') { Send-StandinProblem $Conn 401 $false $null; return 'close' }

  $rest = $path.Substring($script:ApiPrefix.Length)
  if ($rest -cnotmatch '^boards/([^/]+)\z') { Send-StandinProblem $Conn 404 $false $null; return 'close' }
  $board = $Matches[1].ToLowerInvariant()
  if (-not $script:Boards.ContainsKey($board)) { Send-StandinProblem $Conn 404 $false $null; return 'close' }

  $params = [System.Collections.Generic.Dictionary[string, string]]::new([StringComparer]::Ordinal)
  foreach ($part in $query.Split('&')) {
    if ($part.Length -eq 0) { continue }
    $eq = $part.IndexOf('=')
    $name = if ($eq -ge 0) { $part.Substring(0, $eq) } else { $part }
    $value = if ($eq -ge 0) { $part.Substring($eq + 1) } else { '' }
    $name = [System.Uri]::UnescapeDataString($name)
    $value = [System.Uri]::UnescapeDataString($value)
    if ($params.ContainsKey($name)) { Send-StandinProblem $Conn 400 $false $null; return 'close' }
    $params[$name] = $value
  }
  $view = if ($params.ContainsKey('view')) { $params['view'] } else { '' }

  if ($view -ieq 'summary') {
    Send-StandinReply $Conn 200 'application/json' ($script:Utf8.GetBytes('{"id":"' + $board + '"}')) $false $null
    return 'close'
  }
  if ($view -ine 'ticket') { Send-StandinProblem $Conn 400 $false $null; return 'close' }

  $ticketText = if ($params.ContainsKey('ticket')) { $params['ticket'] } else { '' }
  if ($ticketText -cnotmatch '^[0-9]+\z') { Send-StandinProblem $Conn 400 $false $null; return 'close' }
  $ticketText = $ticketText.TrimStart('0')
  if ($ticketText.Length -eq 0 -or $ticketText.Length -gt 16 -or [decimal]$ticketText -gt $script:MaxSafe) {
    Send-StandinProblem $Conn 400 $false $null; return 'close'
  }
  $prose = $false
  if ($params.ContainsKey('include_prose')) {
    if ($params['include_prose'] -ieq 'true') { $prose = $true }
    elseif ($params['include_prose'] -ine 'false') { Send-StandinProblem $Conn 400 $false $null; return 'close' }
  }
  $cards = $script:Boards[$board]
  if (-not $cards.ContainsKey($ticketText)) { Send-StandinProblem $Conn 404 $false $null; return 'close' }
  $card = $cards[$ticketText]
  $ticket = [long]$ticketText

  if ($script:AnswerKind -eq 'body') {
    Send-StandinPadded $Conn $board $ticket $card
    return 'close'
  }
  $json = Get-StandinTicketJson $board $ticket $card $prose 0
  Send-StandinReply $Conn 200 'application/json' ($script:Utf8.GetBytes($json)) $false $null
  return 'close'
}

# --- The listener ------------------------------------------------------------------------------------

# A plain socket, not a TcpListener: on .NET 10 a TcpListener given ExclusiveAddressUse before Start
# answers a null Server after it, which Socket.Select cannot take (found by this script's self-test).
$script:Listener = [System.Net.Sockets.Socket]::new([System.Net.Sockets.AddressFamily]::InterNetwork, [System.Net.Sockets.SocketType]::Stream, [System.Net.Sockets.ProtocolType]::Tcp)
try {
  $script:Listener.ExclusiveAddressUse = $true
  $script:Listener.Bind([System.Net.IPEndPoint]::new([System.Net.IPAddress]::Loopback, $script:PortNumber))
  $script:Listener.Listen(16)
}
catch {
  $script:Listener.Close()
  $script:LogStream.Dispose()
  Stop-StandinRefused 'it cannot bind 127.0.0.1 at the port it was given.'
}
$bound = [System.Net.IPEndPoint]$script:Listener.LocalEndPoint
if (-not $bound.Address.Equals([System.Net.IPAddress]::Loopback) -or $bound.Port -ne $script:PortNumber) {
  $script:Listener.Close()
  $script:LogStream.Dispose()
  Stop-StandinRefused 'it is not bound to 127.0.0.1 at the port it was given.'
}

try {
  $readyBytes = $script:Utf8.GetBytes([string]$PID + "`n")
  $readyStream = [System.IO.FileStream]::new($readyPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
  try { $readyStream.Write($readyBytes, 0, $readyBytes.Length); $readyStream.Flush() }
  finally { $readyStream.Dispose() }
}
catch {
  $script:Listener.Close()
  $script:LogStream.Dispose()
  Stop-StandinRefused 'the ready file cannot be written.'
}

$script:Conns = [System.Collections.Generic.List[object]]::new()
$script:Clock = [System.Diagnostics.Stopwatch]::StartNew()

function Close-StandinConnection($Conn) {
  try { $Conn.Socket.Close() } catch { }
  try { $Conn.Buffer.Dispose() } catch { }
  [void]$script:Conns.Remove($Conn)
}

function Step-StandinConnection($Conn, [bool]$Readable, [long]$Now) {
  $ended = $false
  if ($Readable) {
    $available = $Conn.Socket.Available
    if ($available -le 0) { $ended = $true }
    else {
      $got = $Conn.Socket.Receive($script:Recv, 0, [Math]::Min($script:Recv.Length, $available), [System.Net.Sockets.SocketFlags]::None)
      if ($got -le 0) { $ended = $true }
      elseif ($Conn.State -eq 'reading') {
        $Conn.Buffer.Write($script:Recv, 0, $got)
        $Conn.LastMs = $Now
      }
    }
  }
  if ($Conn.State -eq 'reading') {
    if ($Conn.Buffer.Length -gt 0) {
      $idle = ($Now - $Conn.LastMs) -ge $script:IdleMs
      $request = Read-StandinRequest $Conn.Buffer ($ended -or $idle)
      if ($null -ne $request) {
        $outcome = 'close'
        try { $outcome = Invoke-StandinRequest $Conn $request }
        catch { $outcome = 'close' }
        if ($outcome -eq 'hold' -and -not $ended) {
          $Conn.State = 'held'
        }
        else {
          try { $Conn.Socket.Shutdown([System.Net.Sockets.SocketShutdown]::Send) } catch { }
          $Conn.State = 'closing'
          $Conn.CloseByMs = $Now + $script:ClosingMs
          if ($ended) { Close-StandinConnection $Conn }
        }
        return
      }
    }
    if ($ended -or ($Conn.Buffer.Length -eq 0 -and ($Now - $Conn.LastMs) -ge $script:SilentMs)) { Close-StandinConnection $Conn }
    return
  }
  if ($Conn.State -eq 'held') {
    if ($ended) { Close-StandinConnection $Conn }
    return
  }
  if ($ended -or $Now -ge $Conn.CloseByMs) { Close-StandinConnection $Conn }
}

# Select is given empty lists, never $null, for the lists it does not use.
$script:NoSockets = [System.Collections.Generic.List[System.Net.Sockets.Socket]]::new()
$script:LoopFault = ''
try {
  while ($script:Clock.ElapsedMilliseconds -lt $script:LifetimeMs) {
    $check = [System.Collections.Generic.List[System.Net.Sockets.Socket]]::new()
    $check.Add($script:Listener)
    foreach ($c in $script:Conns) { $check.Add($c.Socket) }
    $script:NoSockets.Clear()
    $noErrors = [System.Collections.Generic.List[System.Net.Sockets.Socket]]::new()
    [System.Net.Sockets.Socket]::Select($check, $script:NoSockets, $noErrors, 200000)
    $now = $script:Clock.ElapsedMilliseconds
    if ($check.Contains($script:Listener)) {
      while ($script:Listener.Poll(0, [System.Net.Sockets.SelectMode]::SelectRead)) {
        $socket = $script:Listener.Accept()
        $socket.NoDelay = $true
        $socket.SendTimeout = 30000
        $script:Conns.Add([pscustomobject]@{
            Socket    = $socket
            Buffer    = [System.IO.MemoryStream]::new()
            LastMs    = $now
            State     = 'reading'
            CloseByMs = [long]0
          })
      }
    }
    foreach ($c in @($script:Conns)) {
      try { Step-StandinConnection $c ($check.Contains($c.Socket)) $now }
      catch { Close-StandinConnection $c }
    }
  }
}
catch {
  # Only the innermost exception's type and the script's line are printed: no message, value or path.
  $script:LoopFault = $_.Exception.GetBaseException().GetType().Name + ' at line ' + $_.InvocationInfo.ScriptLineNumber
}
finally {
  foreach ($c in @($script:Conns)) { Close-StandinConnection $c }
  try { $script:Listener.Close() } catch { }
  try { $script:LogStream.Dispose() } catch { }
}
if ($script:LoopFault) {
  [Console]::Error.WriteLine("board-standin: its serving loop failed ($($script:LoopFault)).")
  exit 1
}
exit 0
