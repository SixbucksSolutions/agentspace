#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 4): the CA check compares
# the public key's two words exactly. It enforces:
#   - docs/state.md, "The CA" (Its checks, check 4, architect, 2026-11-19, as corrected, #0136): the
#     public key's first two words MUST equal the first two of the public key derived from the
#     private key, exactly, and the first MUST be `ssh-ed25519`. The line is trimmed of ASCII
#     whitespace at both ends and its words are split at any run of spaces and tabs only, as
#     ssh-keygen reads a key: a line break between the words ends the line, and a non-ASCII space
#     such as U+00A0 is no whitespace, so both are refused (check 4 and check 5 alike). A public key
#     whose first word is not `ssh-ed25519` is not a usable CA (Its files).
#   - docs/state.md, "Requirements the prototype does not meet yet" (the CA check compares the two
#     words exactly; the prototype compares them without regard to case).
#   - docs/lease-api.md, "Messages" (CA not usable): `create` and `renew` MUST fail with `internal`,
#     change nothing, and make no CA. A check MUST NOT change the CA (state.md, Its checks).
# The public key file is changed in the throwaway home between commands, and put back after each
# case. The cases that MUST be refused are the first word in another case, the first word as a key
# type that is not ed25519, the second word with its letters' case swapped, a line break (LF, CR, VT,
# FF or CRLF) between the two words, and a no-break space (U+00A0) before the first word; each is run
# for `create` and for `renew` of a lease made before. The cases that MUST be accepted are the same
# words with another comment, a carriage return and line feed after it, spaces around the line, and a
# tab or two spaces between the two words (each also for `renew`; #0136): they show that the words are
# compared and the rest of the line is not, so a program that refuses every change does not pass.
#
# - Only the throwaway home's own public key is written, through Assert-ConformancePath at the call,
#   with no program run between the call and the write. The check deletes nothing. A lease ID read
#   from the program's output names a file only after the document passed its schema. A failure
#   reason never quotes the program's output, and the check never prints the key or its comment (the
#   comment names the caller's machine, state.md, "The CA": it is replaced by a fixed word).
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-ca-public-key-words.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#its-checks'

# Writes the CA's public key file, in the throwaway home only.
function Set-CaPublicKey($Lab, [string]$Text) {
  $path = Join-Path $Lab.UserProfile '.agentspace\ca\agentspace_ca.pub'
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $path), [Text.UTF8Encoding]::new($false).GetBytes($Text))
}

# The letters of a string with their case swapped; digits and the other characters stay.
function ConvertTo-SwappedCase([string]$Text) {
  -join ($Text.ToCharArray() | ForEach-Object { if ([char]::IsUpper($_)) { [char]::ToLowerInvariant($_) } elseif ([char]::IsLower($_)) { [char]::ToUpperInvariant($_) } else { $_ } })
}

$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: The CA check compares the public key's two words exactly" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $pubPath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile '.agentspace\ca\agentspace_ca.pub')
    $original = [IO.File]::ReadAllText($pubPath)
    $words = @($original.Trim() -split '\s+')
    if ($words.Count -lt 2 -or $words[0] -cne 'ssh-ed25519' -or $words[1] -cnotmatch '\A[A-Za-z0-9+/=]+\z') {
      Write-ConformanceResult -Name "$sec`: Setup, the throwaway CA's public key is an ssh-ed25519 line of the form the spec gives" -Status FAIL -Reason 'It is not, so there is nothing to change'
    } else {
      $w1 = $words[0]; $w2 = $words[1]
      $lease = New-CcReadyLease -Lab $lab -Project 'capub-a'
      if (-not $lease.Ok) {
        Write-ConformanceResult -Name "$sec`: Setup, a ready lease is made by create under the unchanged CA" -Status FAIL -Reason "create exited $($lease.Run.ExitCode), or did not print a ready lease that validates"
      } else {
        $id = [string]$lease.Doc['id']
        $ok = 'agentspace-conformance'   # A fixed comment: the real one names the machine.

        # ---- What MUST be accepted: the words are compared, the rest of the line is not --------------------
        $accepted = @(
          @{ Name = 'the same two words with another comment'; Text = "$w1 $w2 $ok`n" }
          @{ Name = 'the same two words with a carriage return and a line feed after the comment'; Text = "$w1 $w2 $ok`r`n" }
          @{ Name = 'the same two words with spaces before the first and after the comment'; Text = "  $w1 $w2 $ok  `n" }
          # The words are split at any run of spaces and tabs (U+0020, U+0009) only, so a tab or
          # several spaces between the two words keeps the CA usable: state.md, The CA, Its checks,
          # check 4 (architect, 2026-11-19, as corrected, #0136).
          @{ Name = 'a tab between the two words'; Text = "$w1`t$w2 $ok`n"; Between = $true }
          @{ Name = 'two spaces between the two words'; Text = "$w1  $w2 $ok`n"; Between = $true }
        )
        $n = 0
        foreach ($c in $accepted) {
          $n++
          Set-CaPublicKey $lab $c.Text
          $r = New-CcReadyLease -Lab $lab -Project "capub-ok$n"
          Complete-ConformanceCheck -Name "$sec`: With $($c.Name), a ready create succeeds" -Problems $(if ($r.Ok) { $null } else { [System.Collections.Generic.List[string]]@('create did not print a ready lease that validates') })
          if ($c.Between) {
            # The same words, for renew of the lease made before: it succeeds, the lease keeps its ID, the
            # record's expiry is the one the document gives, and the CA's public key is not changed.
            $pubBefore = Get-CcFileHash $pubPath
            $run = Invoke-CcLease -Lab $lab -Arguments @('renew', $id)
            $p = [System.Collections.Generic.List[string]]::new()
            $doc = ConvertFrom-CcJson $run.Stdout
            if (-not (Test-CcSuccess $run)) { $p.Add("renew exited $($run.ExitCode), or did not print one lease document that validates") }
            elseif (-not $doc -or $doc['id'] -cne $id -or $doc['state'] -cne 'ready') { $p.Add('renew did not print the same lease, ready') }
            else {
              $rec = Get-CcRecord $lab $id
              if (-not $rec -or $rec['expires_at'] -cne $doc['expires_at']) { $p.Add('The record''s expires_at is not the document''s') }
            }
            if ((Get-CcFileHash $pubPath) -ne $pubBefore) { $p.Add('The CA''s public key was changed') }
            Complete-ConformanceCheck -Name "$sec`: With $($c.Name), a ready renew succeeds and changes what a success changes" -Problems $p
          }
        }

        # ---- What MUST be refused: a word that differs, in case or in kind ------------------------------------
        $refused = @(
          @{ Name = 'the first word in upper case'; Text = "$($w1.ToUpperInvariant()) $w2 $ok`n" }
          @{ Name = 'the first word in mixed case'; Text = "Ssh-Ed25519 $w2 $ok`n" }
          @{ Name = 'the second word with its letters'' case swapped'; Text = "$w1 $(ConvertTo-SwappedCase $w2) $ok`n" }
          @{ Name = 'the first word a key type that is not ed25519'; Text = "ssh-rsa $w2 $ok`n" }
          # A line break between the words ends the line, so the key has one word on it: state.md,
          # The CA, Its checks, check 4 (architect, 2026-11-19, as corrected). ssh-keygen exits 255.
          @{ Name = 'a line feed between the two words'; Text = "$w1`n$w2 $ok`n" }
          @{ Name = 'a carriage return between the two words'; Text = "$w1`r$w2 $ok`n" }
          @{ Name = 'a carriage return and a line feed between the two words'; Text = "$w1`r`n$w2 $ok`n" }
          @{ Name = 'a vertical tab between the two words'; Text = "$w1$([char]0x0B)$w2 $ok`n" }
          @{ Name = 'a form feed between the two words'; Text = "$w1$([char]0x0C)$w2 $ok`n" }
          # A non-ASCII space is no whitespace, so it is not trimmed from the line's start: same rule.
          @{ Name = 'a no-break space before the first word'; Text = "$([char]0xA0)$w1 $w2 $ok`n" }
        )
        foreach ($c in $refused) {
          Set-CaPublicKey $lab $c.Text
          $pubChanged = Get-CcFileHash $pubPath
          $mapBefore = Get-HardeningFileMap $lab
          $stateBefore = Get-CcFileHash (Get-CcStatePath $lab)

          $run = Invoke-CcLease -Lab $lab -Arguments @('create', '--project', 'capub-bad')
          Complete-ConformanceCheck -Name "$sec`: With $($c.Name), a ready create fails with internal, exit 1, one error document" -Problems (Get-HardeningErrorProblems -Run $run -Code 'internal')
          $run = Invoke-CcLease -Lab $lab -Arguments @('renew', $id)
          Complete-ConformanceCheck -Name "$sec`: With $($c.Name), renew fails with internal, exit 1, one error document" -Problems (Get-HardeningErrorProblems -Run $run -Code 'internal')

          $p = [System.Collections.Generic.List[string]]::new()
          if ((Get-CcFileHash $pubPath) -ne $pubChanged) { $p.Add('The CA''s public key was changed') }
          if ((Get-CcFileHash (Get-CcStatePath $lab)) -ne $stateBefore) { $p.Add('The state file changed') }
          foreach ($x in (Get-HardeningMapDifferences -Before $mapBefore -After (Get-HardeningFileMap $lab))) { $p.Add($x) }
          Complete-ConformanceCheck -Name "$sec`: With $($c.Name), the failed create and renew changed nothing: not the CA, the record, the lease's credential, an alias file or the config" -Problems $p
        }
      }
    }
    # Put the genuine public key back, so the throwaway home is as it was made.
    Set-CaPublicKey $lab $original
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-ca-public-key-words.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun
