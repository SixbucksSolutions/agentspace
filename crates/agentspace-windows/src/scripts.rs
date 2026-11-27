//! The operator's own text for each Windows step (docs/operator.md, Windows hosts; D34).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! operator.md says what each remote step MUST do, never its text, and this module is that text,
//! written from the page alone. Every script here but the patch check's and the horizon's runs
//! under Windows PowerShell 5.1, wrapped in the encoded form by the caller or by
//! [`crate::encoded::remote_command`], and MUST NOT assume Git Bash or PowerShell 7 (operator.md,
//! What a Windows host is: every operator step before the bootstrap ends works under Windows
//! PowerShell 5.1). Each starts in the login user's profile folder and builds its paths from
//! `$HOME` (The remote command on Windows). Each script's lines end in LF alone.

/// The folder of the host agent's files and its state, on the host.
pub const PROGRAM_DATA: &str = r"C:\ProgramData\agentspace";

/// Windows PowerShell 5.1, by its full path from the system folder, so no `PATH` decides which
/// PowerShell runs a staged script.
const WINDOWS_POWERSHELL: &str = r#"& "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File"#;

/// Bootstrap on a Windows host, step 3: run `<home>\.agentspace-bootstrap.ps1` under Windows
/// PowerShell 5.1 with `-File`, never prompting, to its end; then remove the copy whatever its
/// status, and exit with the script's status. A run that could not start at all exits 1.
pub fn bootstrap_run() -> String {
    format!(
        "$f = Join-Path $HOME '.agentspace-bootstrap.ps1'\n\
         {WINDOWS_POWERSHELL} $f\n\
         $c = $LASTEXITCODE\n\
         Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue\n\
         if ($null -eq $c) {{ $c = 1 }}\n\
         exit $c\n"
    )
}

/// Sync on a Windows host, row 1: remove `<home>\.agentspace-system` and everything in it. A
/// folder that is not there is not an error; exit 0 only when it is gone.
pub const CLEAR_STAGING: &str = "$d = Join-Path $HOME '.agentspace-system'\n\
if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }\n\
if (Test-Path -LiteralPath $d) { exit 1 }\n\
exit 0\n";

/// Sync on a Windows host, row 4: run `<home>\.agentspace-system\install.ps1` under Windows
/// PowerShell 5.1 with `-File`, never prompting; then, only if it exited 0, remove the staging
/// folder. Exit with the install's status, or the removal's.
pub fn install() -> String {
    format!(
        "$d = Join-Path $HOME '.agentspace-system'\n\
         {WINDOWS_POWERSHELL} (Join-Path $d 'install.ps1')\n\
         $c = $LASTEXITCODE\n\
         if ($null -eq $c) {{ exit 1 }}\n\
         if ($c -ne 0) {{ exit $c }}\n\
         Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue\n\
         if (Test-Path -LiteralPath $d) {{ exit 1 }}\n\
         exit 0\n"
    )
}

/// Sync on a Windows host, row 5: make sure `<home>\.claude` exists. A folder already there is
/// not an error, and its access rules are left as they are; a new one takes its parent's.
pub const MAKE_CLAUDE_FOLDER: &str = "$ErrorActionPreference = 'Stop'\n\
try { [void][IO.Directory]::CreateDirectory((Join-Path $HOME '.claude')) } catch { exit 1 }\n\
exit 0\n";

/// Sync on a Windows host, row 8: install `<home>\.agentspace-localswim.exe` as
/// `C:\ProgramData\agentspace\localswim\localswim.exe`, creating the folder. A copy that is there,
/// which may be running, is moved aside first to `localswim.exe.old`, since Windows refuses to
/// replace a running program but lets it be renamed; `localswim.exe.old` is removed when it can
/// be, its failure ignored. Then, only if the install succeeded, the staged copy is removed. Exits
/// non-zero if either failed.
///
/// An old copy that cannot be removed, because it is still running from an earlier move, leaves
/// no name to move the current copy to; the current copy is then replaced in place, which works
/// when it is not running and fails the step when it is. A copy that fails after the move puts
/// the moved copy back.
pub const INSTALL_LOCALSWIM: &str = "$ErrorActionPreference = 'Stop'\n\
$s = Join-Path $HOME '.agentspace-localswim.exe'\n\
$t = 'C:\\ProgramData\\agentspace\\localswim\\localswim.exe'\n\
$o = 'C:\\ProgramData\\agentspace\\localswim\\localswim.exe.old'\n\
try {\n\
[void][IO.Directory]::CreateDirectory('C:\\ProgramData\\agentspace\\localswim')\n\
if ([IO.File]::Exists($t)) {\n\
Remove-Item -LiteralPath $o -Force -ErrorAction SilentlyContinue\n\
if ([IO.File]::Exists($o)) { [IO.File]::Copy($s, $t, $true) } else { [IO.File]::Move($t, $o); [IO.File]::Copy($s, $t) }\n\
} else { [IO.File]::Copy($s, $t) }\n\
} catch {\n\
if (-not [IO.File]::Exists($t) -and [IO.File]::Exists($o)) { try { [IO.File]::Move($o, $t) } catch {} }\n\
exit 1\n\
}\n\
Remove-Item -LiteralPath $o -Force -ErrorAction SilentlyContinue\n\
try { Remove-Item -LiteralPath $s -Force } catch { exit 1 }\n\
exit 0\n";

/// The patch check's command, as operator.md gives it (The patch run and its restart rule; pin
/// P18): the check under PowerShell 7 by its full path, run by Run Command's Windows PowerShell
/// 5.1. Not the encoded form; the caller sends it as a JSON value of `--parameters`.
pub const PATCH_CHECK: &str = r"& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -NonInteractive -File C:\ProgramData\agentspace\agentspace-patch-check.ps1";

/// The horizon command (The horizon on a Windows host; pins P12, P16 and P17), one line, run by
/// Run Command under Windows PowerShell 5.1 as SYSTEM, never prompting: make
/// `C:\ProgramData\agentspace` when it is missing; write the digits then one LF, never a CR, to a
/// temporary file there with `[IO.File]::WriteAllText`; move it over `lease-horizon`, replacing
/// it; exit 0 only when all three succeeded, and 1 at the first that fails. The digits stand as
/// one word of their own, a space on each side. Not the encoded form, so the digits stay
/// readable; the caller sends it as a JSON value of `--parameters`.
///
/// `[IO.File]::WriteAllText` with no encoding writes UTF-8 with no byte order mark, so the file's
/// bytes are the digits and `0x0A` alone. A horizon past PowerShell's largest integer is read as a
/// decimal, whose text is still its digits.
pub fn horizon(horizon: u64) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; try {{ $d = '{PROGRAM_DATA}'; $f = $d + '\\lease-horizon'; \
         $t = $f + '.new'; $h = {horizon} ; [void][IO.Directory]::CreateDirectory($d); \
         [IO.File]::WriteAllText($t, [string]$h + [char]10); \
         if ([IO.File]::Exists($f)) {{ [IO.File]::Replace($t, $f, [NullString]::Value) }} else {{ [IO.File]::Move($t, $f) }} \
         }} catch {{ exit 1 }}; exit 0"
    )
}

/// Grow on a Windows host, step 3's remote step, as `agent`, never prompting: extend the `C:`
/// partition to the largest size the disk supports for it, which grows its NTFS filesystem with
/// it, a partition already at that size being no error; only if that succeeded, print the `C:`
/// volume's size and free space in GiB; exit non-zero if either failed, 0 otherwise.
///
/// A partition within 1 MiB of its largest size counts as already there, since Windows refuses a
/// resize smaller than its own alignment.
pub const GROW: &str = "$ErrorActionPreference = 'Stop'\n\
try {\n\
$p = Get-Partition -DriveLetter C\n\
$m = (Get-PartitionSupportedSize -DriveLetter C).SizeMax\n\
if ($m -gt $p.Size + 1MB) { Resize-Partition -DriveLetter C -Size $m }\n\
} catch {\n\
[Console]::Error.WriteLine('Growing the C: partition failed: ' + $_.Exception.Message)\n\
exit 1\n\
}\n\
try {\n\
$v = Get-Volume -DriveLetter C\n\
[Console]::Out.WriteLine([string]::Format([Globalization.CultureInfo]::InvariantCulture, 'C: {0:0.0} GiB, {1:0.0} GiB free', ($v.Size / 1GB), ($v.SizeRemaining / 1GB)))\n\
} catch {\n\
[Console]::Error.WriteLine('Reading the C: volume failed: ' + $_.Exception.Message)\n\
exit 1\n\
}\n\
exit 0\n";

/// The push's remote half (The token push to a Windows host; What the box does, steps 1 to 8),
/// under Windows PowerShell 5.1, with `zone` already checked by the zone's pattern, or empty for
/// no zone. The zone travels inside the script, in single quotes, which the pattern keeps out of
/// it; the token arrives on stdin alone and is never written to stdout or stderr.
///
/// 1. Reads at most two lines from stdin, a byte at a time, and stops reading after 30 seconds.
/// 2. Removes every CR, then a UTF-8 byte order mark at the start of the first line only, then
///    drops every line left empty.
/// 3. Writes the result, each line ending in LF, to a new temporary file with a name no other run
///    uses in `<home>\.config\localswim`, creating that folder when it is missing. The folder it
///    creates, the temporary file and the credential file are readable by `agent` and
///    Administrators alone: inheritance off, and full control for the running user and
///    `BUILTIN\Administrators`, each born with those rules. A folder that exists is used as it
///    is.
/// 4. Accepts it only with exactly two lines, one starting `LOCALSWIM_URL=` and one starting
///    `LOCALSWIM_TOKEN=`, each followed by at least one character, in either order.
/// 5. Replaces `bot.env` with the temporary file in one step. An old `bot.env` is given the two
///    rules first, since the replace keeps the replaced file's access rules.
/// 6. Prints the SHA-256 of `bot.env`'s bytes: 64 lowercase hex digits, two spaces and `-`.
/// 7. With a zone, sets it with `Set-TimeZone -Id`, giving up after 5 seconds and killing a
///    setter still running 1 second later, then prints `zone <zone>` or `zone-failed`, and exits
///    0 whatever the zone did.
/// 8. Without a zone, exits 0 once the hash is printed. Every failure before step 6 removes the
///    temporary file, leaves the old credential file, prints nothing and exits 1; a failed step 6
///    prints no hash line and exits 1.
///
/// A setter still running at 5 seconds is asked to stop, and one still running 1 second later is
/// killed: the script prints `zone-failed` and ends its own process with `[Environment]::Exit(0)`,
/// since a plain `exit` waits for the setter's thread to end. Whether Windows PowerShell 5.1 as
/// the login shell passes the push's stdin on is T05-S02's to measure (operator.md, What T05-S02
/// measures).
pub fn push(zone: &str) -> String {
    let zone_part = if zone.is_empty() {
        String::new()
    } else {
        PUSH_ZONE.replace("<ZONE>", zone)
    };
    format!("{PUSH}{zone_part}exit 0\n")
}

/// The push's script up to its hash line, steps 1 to 6. It is written short, `using namespace`
/// in place of full type names and one or two letters per variable, since the encoded form of
/// the script and a 64-character zone MUST fit 8,191 characters. PowerShell's names are
/// case-insensitive, so no two variables differ by case alone.
const PUSH: &str = r#"using namespace System.Collections.Generic
using namespace System.Security.AccessControl
using namespace System.IO
$ErrorActionPreference='Stop'
$hm=$HOME
$d="$hm\.config\localswim"
$f="$d\bot.env"
$t=$null
try{
$i=[Console]::OpenStandardInput();$b=[byte[]]::new(1)
$l=[List[object]]::new();$c=[List[byte]]::new()
$w=[Diagnostics.Stopwatch]::StartNew()
while($l.Count -lt 2){
$k=30000-$w.ElapsedMilliseconds
if($k -le 0){break}
$a=$i.ReadAsync($b,0,1)
if(-not $a.Wait([int]$k) -or $a.Result -lt 1){break}
if($b[0] -eq 10){$l.Add($c.ToArray());$c.Clear()}elseif($b[0] -ne 13){$c.Add($b[0])}
}
if($c.Count){$l.Add($c.ToArray())}
$o=[List[byte]]::new();$s=@()
for($j=0;$j -lt $l.Count;$j++){$q=[byte[]]$l[$j]
if($j -eq 0 -and $q.Length -ge 3 -and $q[0] -eq 239 -and $q[1] -eq 187 -and $q[2] -eq 191){$e=[byte[]]::new($q.Length-3);[Array]::Copy($q,3,$e,0,$e.Length);$q=$e}
if($q.Length){$o.AddRange($q);$o.Add(10);$s+=[Text.Encoding]::UTF8.GetString($q)}}
$me=[Security.Principal.WindowsIdentity]::GetCurrent().User
$ad=[Security.Principal.NTAccount]'BUILTIN\Administrators'
if(![Directory]::Exists($d)){[void][Directory]::CreateDirectory("$hm\.config")
$ds=[DirectorySecurity]::new();$ds.SetAccessRuleProtection($true,$false)
foreach($u in $me,$ad){$ds.AddAccessRule([FileSystemAccessRule]::new($u,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))}
[void][Directory]::CreateDirectory($d,$ds)}
$fs=[FileSecurity]::new();$fs.SetAccessRuleProtection($true,$false)
foreach($u in $me,$ad){$fs.AddAccessRule([FileSystemAccessRule]::new($u,'FullControl','Allow'))}
$t="$d\.bot.env."+[guid]::NewGuid().ToString('N')
$x=[File]::Create($t,4096,'None',$fs);try{$x.Write($o.ToArray(),0,$o.Count)}finally{$x.Close()}
$g=$s.Count -eq 2 -and (($s[0] -clike 'LOCALSWIM_URL=?*' -and $s[1] -clike 'LOCALSWIM_TOKEN=?*') -or ($s[1] -clike 'LOCALSWIM_URL=?*' -and $s[0] -clike 'LOCALSWIM_TOKEN=?*'))
if(!$g){throw 'refused'}
if([File]::Exists($f)){[File]::SetAccessControl($f,$fs);[File]::Replace($t,$f,[NullString]::Value)}else{[File]::Move($t,$f)}
$t=$null
}catch{if($t){try{[File]::Delete($t)}catch{}};exit 1}
try{$y=[Security.Cryptography.SHA256]::Create().ComputeHash([File]::ReadAllBytes($f))
[Console]::Out.Write((-join($y|ForEach-Object{$_.ToString('x2')}))+"  -`n")}catch{exit 1}
"#;

/// The push's step 7, appended only when a zone is given, `<ZONE>` standing for the checked zone:
/// `Set-TimeZone -Id <zone>` in a runspace of its own, given up after 5 seconds and killed with the
/// process 1 second later, then `zone <zone>` or `zone-failed`. With no zone the script holds no
/// `Set-TimeZone` at all.
const PUSH_ZONE: &str = r#"$ok=$false;$z=$false
try{$p=[PowerShell]::Create().AddScript('param($n) Set-TimeZone -Id $n -ErrorAction Stop').AddArgument('<ZONE>');$a=$p.BeginInvoke()
if($a.AsyncWaitHandle.WaitOne(5000)){[void]$p.EndInvoke($a);$ok=!$p.HadErrors}else{[void]$p.BeginStop($null,$null);$z=!$a.AsyncWaitHandle.WaitOne(1000)}}catch{}
if($ok){[Console]::Out.Write("zone <ZONE>`n")}else{[Console]::Out.Write("zone-failed`n")}
if($z){[Console]::Out.Flush();[Environment]::Exit(0)}
"#;
