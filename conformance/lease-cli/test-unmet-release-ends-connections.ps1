#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.07 programs, whose `release` ends no connection and writes
# no release.log. A v0.08 build MUST pass it. It is not a regression when it fails against v0.07
# (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check (D59, D63; card #0183, E0.08-T04-S02): what `agentspace lease release` does to the
# ssh clients on the laptop that reach the lease's alias. It is every row of docs/driver.md, "Conformance
# cases for v0.08", R1 to R15, each a case of its own (the architect's ruling, 2026-11-22, for v0.08-T04:
# every row maps to a result line by name). Every result name starts with the driver.md anchor the row
# cites, then the row's number, as `driver.md#each-with-its-tree: R1 ...`:
#   R1  each-with-its-tree        R6  which-processes-are-the-aliass-clients   R11 where-the-count-is-logged
#   R2  the-order-of-a-release    R7  which-processes-are-the-aliass-clients   R12 the-order-of-a-release
#   R3  an-ssh-clients-destination R8 the-order-of-a-release                   R13 each-with-its-tree
#   R4  which-processes-are-the-aliass-clients  R9  the-order-of-a-release      R14 the-order-of-a-release
#   R5  which-processes-are-the-aliass-clients  R10 the-order-of-a-release      R15 the-order-of-a-release
# Sections they cite: driver.md, "Ending a lease's connections" with "The order of a release", "Which
# processes are the alias's clients", "An ssh client's destination", "Each with its tree", "The count" and
# "Where the count is logged"; docs/state.md, "The release log"; docs/lease-api.md, "agentspace lease
# release".
#
# - Leases. Each case makes its own ready lease with `create` for the project `conf`, whose host record the
#   harness seeds as `ready` (New-CcReadyLease): `release` builds the alias from the request's project and
#   ID, reads neither the host registry nor the project list, and the driver does not tell hosts apart. R9
#   seeds a `pending` lease's record itself, with a `driver` and a UUIDv4 drawn at random for the run, since
#   no command makes one that stays pending. No lease has a fixed ID, so no process of Terry's can match an
#   alias of this check. R11 runs in a home of its own, because it makes `release.log` a folder.
# - Stand-ins. The check builds one tiny console program with the .NET Framework's csc.exe, in the
#   throwaway home (the harness has no builder), and copies it into a folder of its own for each stand-in,
#   under the image file name the case needs: ssh.exe, agentspace-ssh-proxy.exe, scp.exe, helper.exe or
#   desktop-standin.exe. It is never a copy of an installed agentspace program and never an ssh.exe of
#   Windows'. A stand-in waits until it is ended (or for 5 minutes, which only a case that died would
#   reach), and reads `stub.cfg` beside itself once, at its start, which is its whole instruction:
#     child|<label>|<program>|<arguments>             starts a child
#     respawn|<watched label>|<label>|<program>|<arguments>   starts one more process, once, when the
#                                                     watched child has exited (R15)
#     watch|<file>|<output file>                      writes `exists` or `absent` to the output file every
#                                                     100 milliseconds, by whether the file exists (R2)
#     run|<label>|<program>|<arguments>|<out>|<err>   starts a program as a child, its streams captured to
#                                                     files, and writes `<label>.exit` last (R7)
#   It records every process it starts, and itself, in `pids.txt` beside itself as `<label> <process ID>
#   <creation time as a file time>`.
# - A stand-in the check starts is started with its own image and the command line the case gives, first
#   token included (CreateProcess with lpApplicationName the stand-in and lpCommandLine the case's line,
#   docs/driver.md, "Conformance cases for v0.08"), so the quoted `C:\Windows\System32\OpenSSH\ssh.exe` row
#   of R3 runs the stand-in and never Windows' ssh.exe. A stand-in's child is started as `"<path>"
#   <arguments>`.
# - Every process a case starts is ended at its close, by the process IDs the stand-ins recorded and the
#   check's own CreateProcess returned, each checked first to still have the creation time recorded, so a
#   process ID that another process reused is never ended. The check asserts on the stand-ins it started
#   and on nothing else of the laptop's process list: it lists, reads and ends no other process, and never
#   reads or prints any process's command line.
# - Every stand-in gets the harness's environment for the throwaway home. The check never prints it.
# - Every write and delete the check makes calls Assert-ConformancePath at the call. The stand-ins' own
#   writes (`pids.txt`, the R2 and R7 files) are the stand-ins' (docs/conformance.md, "The program under
#   test is untrusted").
# - Whether a process ended is asked by a bounded poll, never by a fixed sleep that assumes a timing. R14
#   waits for a 60-second lease's `expires_at` to pass, which is bounded at 70 seconds after its creation.
# - R15's second client is started by a stand-in the moment its first has exited, which the spec asks for.
#   The case never asserts which of the two walks ended the second client, and runs once. It holds only
#   when the stand-in starts its second client within the release's run, about 150 milliseconds, so the
#   stand-in starts it by direct process creation, warmed first; a second client found started after the
#   release returned makes the result SKIPPED with that reason, never FAIL (architect, 2026-11-22, for
#   v0.08-T04; driver.md's R15 row at c932ef6).
# - csc.exe is looked up only under %SystemRoot%\Microsoft.NET\Framework64 and \Framework (v4.0.30319),
#   never on PATH or in the checkout, and the check reports SKIPPED when neither holds one (architect,
#   2026-11-22, for v0.08-T04).
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-release-ends-connections.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$utf8 = [Text.UTF8Encoding]::new($false)
$script:CeStands = [System.Collections.Generic.List[object]]::new()
$script:CeReadyMs = 30000
$script:CeEndMs = 10000
$aTree = 'driver.md#each-with-its-tree'
$aCount = 'driver.md#the-count'
$aOrder = 'driver.md#the-order-of-a-release'
$aClients = 'driver.md#which-processes-are-the-aliass-clients'
$aDest = 'driver.md#an-ssh-clients-destination'
$aLog = 'driver.md#where-the-count-is-logged'
$proxyImage = 'agentspace-ssh-proxy.exe'

# --- The stand-in program --------------------------------------------------------------------------

$script:CeStubSource = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class Stub
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct STARTUPINFO
    {
        public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
        public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
        public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string app, StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string cwd, ref STARTUPINFO si, out PROCESS_INFORMATION pi);

    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr h);

    [DllImport("kernel32.dll")]
    static extern bool GetProcessTimes(IntPtr h, out long creation, out long exit, out long kernel, out long user);

    static string dir;

    static Process Launch(string program, string arguments)
    {
        ProcessStartInfo psi = new ProcessStartInfo(program, arguments);
        psi.UseShellExecute = false;
        psi.CreateNoWindow = true;
        return Process.Start(psi);
    }

    // The quickest way to make a process, for a client that comes back the moment its first is ended.
    static bool LaunchNative(string label, string program, string arguments)
    {
        STARTUPINFO si = new STARTUPINFO();
        si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
        PROCESS_INFORMATION pi;
        StringBuilder cmd = new StringBuilder("\"" + program + "\" " + arguments);
        if (!CreateProcessW(program, cmd, IntPtr.Zero, IntPtr.Zero, false, 0x08000000, IntPtr.Zero, null, ref si, out pi)) return false;
        long creation, exit, kernel, user;
        bool timed = GetProcessTimes(pi.hProcess, out creation, out exit, out kernel, out user);
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        // The process may be ended before anything else reads it, so its creation time comes from the handle.
        if (timed) Record(label, pi.dwProcessId, creation);
        return timed;
    }

    static void Record(string label, int id, long start)
    {
        string line = label + " " + id + " " + start + "\n";
        for (int i = 0; i < 100; i++)
        {
            try { File.AppendAllText(Path.Combine(dir, "pids.txt"), line); return; }
            catch (IOException) { Thread.Sleep(10); }
        }
    }

    static void Record(string label, Process p) { Record(label, p.Id, p.StartTime.ToFileTimeUtc()); }

    static void Respawn(Process watched, string label, string program, string arguments)
    {
        watched.WaitForExit();
        LaunchNative(label, program, arguments);
    }

    // Writes whether a file exists, every 100 milliseconds, one line each time.
    static void Watch(string target, string outFile)
    {
        while (true)
        {
            string line = File.Exists(target) ? "exists\n" : "absent\n";
            try { File.AppendAllText(outFile, line); } catch (IOException) { }
            Thread.Sleep(100);
        }
    }

    static void CopyTo(Stream source, string file)
    {
        using (FileStream f = new FileStream(file, FileMode.Create, FileAccess.Write, FileShare.Read)) { source.CopyTo(f); }
    }

    // Starts a program as a child with its streams captured to files, waits for it, and writes its exit code last.
    static void RunProgram(string label, string program, string arguments, string outFile, string errFile)
    {
        string code = "-1";
        try
        {
            ProcessStartInfo psi = new ProcessStartInfo(program, arguments);
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardInput = true;
            psi.RedirectStandardOutput = true;
            psi.RedirectStandardError = true;
            Process p = Process.Start(psi);
            Record(label, p);
            p.StandardInput.Close();
            Thread t1 = new Thread(delegate() { CopyTo(p.StandardOutput.BaseStream, outFile); });
            Thread t2 = new Thread(delegate() { CopyTo(p.StandardError.BaseStream, errFile); });
            t1.Start(); t2.Start();
            p.WaitForExit();
            t1.Join(); t2.Join();
            code = p.ExitCode.ToString();
        }
        catch (Exception) { }
        File.WriteAllText(Path.Combine(dir, label + ".exit"), code);
    }

    static void Background(ThreadStart work)
    {
        Thread t = new Thread(work);
        t.IsBackground = true;
        t.Start();
    }

    public static int Main(string[] args)
    {
        Marshal.PrelinkAll(typeof(Stub));
        Process me = Process.GetCurrentProcess();
        dir = Path.GetDirectoryName(me.MainModule.FileName);
        Record("self", me);
        Dictionary<string, Process> started = new Dictionary<string, Process>();
        List<string[]> respawns = new List<string[]>();
        List<string[]> later = new List<string[]>();
        string cfg = Path.Combine(dir, "stub.cfg");
        if (File.Exists(cfg))
        {
            foreach (string raw in File.ReadAllLines(cfg))
            {
                string[] f = raw.Split('|');
                if (f.Length == 4 && f[0] == "child")
                {
                    Process c = Launch(f[2], f[3]);
                    started[f[1]] = c;
                    Record(f[1], c);
                }
                else if (f.Length == 5 && f[0] == "respawn") respawns.Add(f);
                else if ((f.Length == 3 && f[0] == "watch") || (f.Length == 6 && f[0] == "run")) later.Add(f);
            }
        }
        foreach (string[] f in respawns)
        {
            Process watched;
            if (!started.TryGetValue(f[1], out watched)) continue;
            string label = f[2], program = f[3], arguments = f[4];
            Background(delegate() { Respawn(watched, label, program, arguments); });
        }
        // Every child is running before a watch begins or a program is run, so that the program sees them.
        foreach (string[] f in later)
        {
            string[] g = f;
            if (g[0] == "watch") Background(delegate() { Watch(g[1], g[2]); });
            else Background(delegate() { RunProgram(g[1], g[2], g[3], g[4], g[5]); });
        }
        Thread.Sleep(300000);
        return 0;
    }
}
'@

$script:CeLaunchSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class CeLaunch
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct STARTUPINFO
    {
        public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
        public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
        public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string app, StringBuilder cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string cwd, ref STARTUPINFO si, out PROCESS_INFORMATION pi);

    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr h);

    [DllImport("kernel32.dll")]
    static extern bool GetProcessTimes(IntPtr h, out long creation, out long exit, out long kernel, out long user);

    // Starts the program at `app` under the command line `line`, first token included, with the environment
    // `env` (NAME=VALUE entries) and no window. Returns the process ID and the creation time as a file time.
    public static long[] Start(string app, string line, string cwd, string[] env)
    {
        List<string> sorted = new List<string>(env);
        sorted.Sort(StringComparer.OrdinalIgnoreCase);
        StringBuilder block = new StringBuilder();
        foreach (string e in sorted) { block.Append(e); block.Append('\0'); }
        block.Append('\0');
        char[] chars = block.ToString().ToCharArray();
        IntPtr envPtr = Marshal.AllocHGlobal(chars.Length * 2);
        try
        {
            Marshal.Copy(chars, 0, envPtr, chars.Length);
            STARTUPINFO si = new STARTUPINFO();
            si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
            PROCESS_INFORMATION pi;
            StringBuilder cmd = new StringBuilder(line);
            // CREATE_NO_WINDOW, and CREATE_UNICODE_ENVIRONMENT for the block.
            if (!CreateProcessW(app, cmd, IntPtr.Zero, IntPtr.Zero, false, 0x08000400, envPtr, cwd, ref si, out pi))
                throw new Exception("CreateProcess failed, Windows error " + Marshal.GetLastWin32Error());
            long creation, exit, kernel, user;
            bool timed = GetProcessTimes(pi.hProcess, out creation, out exit, out kernel, out user);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            return new long[] { pi.dwProcessId, timed ? creation : 0 };
        }
        finally { Marshal.FreeHGlobal(envPtr); }
    }
}
'@

function Find-CeCsc {
  if (-not $env:SystemRoot) { return $null }
  foreach ($f in 'Framework64', 'Framework') {
    $p = Join-Path $env:SystemRoot "Microsoft.NET\$f\v4.0.30319\csc.exe"
    if (Test-Path -LiteralPath $p -PathType Leaf) { return $p }
  }
  return $null
}

# Builds the stand-in program once, in the throwaway home. Returns its path, or $null when it did not build.
function New-CeBase {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Csc)
  $src = Join-Path $Lab.Root 'stubbuild\Stub.cs'
  $exe = Join-Path $Lab.Root 'stubbuild\stub.exe'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path (Split-Path -Parent $src)) | Out-Null
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $Lab -Path $src), $script:CeStubSource, $utf8)
  $out = & $Csc /nologo /target:exe ("/out:" + (Assert-ConformancePath -ConformanceHome $Lab -Path $exe)) $src 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) { return $null }
  return $exe
}

function Get-CeStandDir($Lab, [string]$Case, [string]$Name) { Join-Path $Lab.Root "stubs\$Case\$Name" }

# One stand-in: a folder of its own holding a copy of the program under the image file name Image, and the
# stub.cfg that says what it does. Kids are @{ Label; Stand; Args } (a child, which is another New-CeStand
# result), Respawns are @{ Watch; Label; Stand; Args }, Extra is raw stub.cfg lines (watch and run), and
# Expect is how many more records than itself and its kids the stand-in makes (one for a run line).
function New-CeStand {
  param(
    [Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Base, [Parameter(Mandatory)][string]$Case,
    [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Image,
    [object[]]$Kids = @(), [object[]]$Respawns = @(), [string[]]$Extra = @(), [int]$Expect = 0
  )
  $dir = Get-CeStandDir $Lab $Case $Name
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $dir) | Out-Null
  $exe = Join-Path $dir $Image
  Copy-Item -LiteralPath $Base -Destination (Assert-ConformancePath -ConformanceHome $Lab -Path $exe) -Force
  $lines = @(foreach ($k in $Kids) { 'child|{0}|{1}|{2}' -f $k.Label, $k.Stand.Exe, $k.Args })
  $lines += @(foreach ($r in $Respawns) { 'respawn|{0}|{1}|{2}|{3}' -f $r.Watch, $r.Label, $r.Stand.Exe, $r.Args })
  $lines += @($Extra)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $dir 'stub.cfg')), ((@($lines) -join "`n") + "`n"), $utf8)
  $s = [pscustomobject]@{
    Name = $Name; Dir = $dir; Exe = $exe; Image = $Image; Kids = @($Kids | ForEach-Object { $_.Stand }); Expect = $Expect
    Launched = [System.Collections.Generic.List[object]]::new()
  }
  $script:CeStands.Add($s)
  return $s
}

# Every stand-in that starts with its top one: the top and its children, grandchildren included.
function Get-CeTree($Stand) {
  $Stand
  foreach ($k in $Stand.Kids) { Get-CeTree $k }
}

# The records a stand-in made in pids.txt, read for read only, sharing everything: it appends while it is read.
function Read-CePids($Stand) {
  $list = [System.Collections.Generic.List[object]]::new()
  $path = Join-Path $Stand.Dir 'pids.txt'
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $list }
  $text = ''
  try {
    $fs = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $text = [IO.StreamReader]::new($fs).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return $list }
  foreach ($line in ($text -split "`n")) {
    if ($line -match '^(\S+) (\d+) (\d+)$') { $list.Add([pscustomobject]@{ Label = $Matches[1]; ProcessId = [int]$Matches[2]; Start = [long]$Matches[3] }) }
  }
  return $list
}

# Every record of a stand-in: its own file's, and the one the check's CreateProcess returned.
function Get-CeAllRecs($Stand) { @(Read-CePids $Stand) + @($Stand.Launched) }

# The stand-in's own process: the one the check started, else the one it recorded.
function Get-CeSelf($Stand) {
  if ($Stand.Launched.Count -gt 0) { return $Stand.Launched[0] }
  @(Read-CePids $Stand | Where-Object { $_.Label -ceq 'self' })[0]
}

# True when the process a stand-in recorded is running and still has the creation time it recorded.
function Test-CeAlive($Rec) {
  if ($null -eq $Rec) { return $false }
  $p = $null
  try { $p = [Diagnostics.Process]::GetProcessById($Rec.ProcessId) } catch { return $false }
  try { return ((-not $p.HasExited) -and ($p.StartTime.ToFileTimeUtc() -eq $Rec.Start)) } catch { return $false } finally { $p.Dispose() }
}

function Test-CeStandAlive($Stand) { Test-CeAlive (Get-CeSelf $Stand) }

# Ends the process a stand-in recorded, only when it still has the creation time recorded.
function Stop-CeRec($Rec) {
  $p = $null
  try { $p = [Diagnostics.Process]::GetProcessById($Rec.ProcessId) } catch { return }
  try {
    if ((-not $p.HasExited) -and ($p.StartTime.ToFileTimeUtc() -eq $Rec.Start)) { $p.Kill() }
  } catch { } finally { $p.Dispose() }
}

# Ends every process any stand-in recorded, until a pass finds none, so a supervisor's late child goes too.
function Stop-CeAll {
  for ($pass = 0; $pass -lt 6; $pass++) {
    $any = $false
    foreach ($s in @($script:CeStands)) {
      foreach ($r in @(Get-CeAllRecs $s)) { if (Test-CeAlive $r) { $any = $true; Stop-CeRec $r } }
    }
    if (-not $any) { break }
    Start-Sleep -Milliseconds 100
  }
  $until = [DateTime]::UtcNow.AddSeconds(10)
  while ([DateTime]::UtcNow -lt $until) {
    $alive = $false
    foreach ($s in @($script:CeStands)) { foreach ($r in @(Get-CeAllRecs $s)) { if (Test-CeAlive $r) { $alive = $true } } }
    if (-not $alive) { break }
    Start-Sleep -Milliseconds 100
  }
}

# Starts a stand-in with its own image and the command line Line, first token included. The environment is
# the harness's for the throwaway home, with Environment's entries added or, for a $null value, removed.
function Start-CeStand {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Stand, [Parameter(Mandatory)][string]$Line, [hashtable]$Environment = @{})
  $vars = [System.Collections.Generic.Dictionary[string, string]]::new([StringComparer]::OrdinalIgnoreCase)
  $mine = [Environment]::GetEnvironmentVariables()
  foreach ($k in $mine.Keys) { $vars[[string]$k] = [string]$mine[$k] }
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $Lab -Override $Environment
  foreach ($n in $childEnv.Keys) {
    if ($null -eq $childEnv[$n]) { [void]$vars.Remove([string]$n) } else { $vars[[string]$n] = [string]$childEnv[$n] }
  }
  $pairs = [string[]]@($vars.Keys | ForEach-Object { "$_=$($vars[$_])" })
  $r = [CeLaunch]::Start($Stand.Exe, $Line, $Stand.Dir, $pairs)
  if ($r[1] -ne 0) { $Stand.Launched.Add([pscustomobject]@{ Label = 'launched'; ProcessId = [int]$r[0]; Start = [long]$r[1] }) }
}

# Runs a stand-in once under a command line that names no alias and ends it, so that its program has been
# read and scanned once before a supervisor starts it: a process made at the moment a client is ended
# then appears within milliseconds, and not after the 50 or more that a first run of a new file can take.
# It leaves nothing recorded.
function Invoke-CeWarm {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Stand)
  Start-CeStand -Lab $Lab -Stand $Stand -Line "$($Stand.Image) warm-up"
  $until = [DateTime]::UtcNow.AddMilliseconds($script:CeReadyMs)
  while (-not (@(Read-CePids $Stand | Where-Object { $_.Label -ceq 'self' })[0]) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 50 }
  foreach ($r in @(Get-CeAllRecs $Stand)) { Stop-CeRec $r }
  [void](Wait-CeEnded -Stands @($Stand))
  $Stand.Launched.Clear()
  $log = Join-Path $Stand.Dir 'pids.txt'
  if (Test-Path -LiteralPath $log) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $log) -Force }
}

# Waits, with a bound, until every stand-in of the trees has recorded itself and its children.
function Wait-CeStarted {
  param([Parameter(Mandatory)][object[]]$Tops)
  $all = @($Tops | ForEach-Object { Get-CeTree $_ })
  $until = [DateTime]::UtcNow.AddMilliseconds($script:CeReadyMs)
  while ($true) {
    $ready = $true
    foreach ($s in $all) {
      $want = 1 + $s.Kids.Count + $s.Expect
      $recs = @(Read-CePids $s)
      if ($recs.Count -lt $want) { $ready = $false; break }
      if (-not (Test-CeStandAlive $s)) { $ready = $false; break }
    }
    if ($ready) { return $true }
    if ([DateTime]::UtcNow -gt $until) { return $false }
    Start-Sleep -Milliseconds 50
  }
}

# Waits, with a bound, until none of the stand-ins is running.
function Wait-CeEnded {
  param([Parameter(Mandatory)][object[]]$Stands, [int]$Ms = $script:CeEndMs)
  $until = [DateTime]::UtcNow.AddMilliseconds($Ms)
  while ($true) {
    $alive = @($Stands | Where-Object { Test-CeStandAlive $_ })
    if ($alive.Count -eq 0) { return $true }
    if ([DateTime]::UtcNow -gt $until) { return $false }
    Start-Sleep -Milliseconds 50
  }
}

function Get-CeAliveNames([object[]]$Stands) { (@($Stands | Where-Object { Test-CeStandAlive $_ } | ForEach-Object Name) -join ', ') }
function Get-CeGoneNames([object[]]$Stands) { (@($Stands | Where-Object { -not (Test-CeStandAlive $_) } | ForEach-Object Name) -join ', ') }

# Reads a file a stand-in wrote, as bytes, sharing everything; $null when it is not there.
function Read-CeStandFile([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  try {
    $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
    try { $ms = [IO.MemoryStream]::new(); $fs.CopyTo($ms); return , $ms.ToArray() } finally { $fs.Dispose() }
  } catch { return $null }
}

# --- The log, the document and the files --------------------------------------------------------------

function Read-CeLog($Lab) {
  $path = Join-Path $Lab.StateDir 'release.log'
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  , [IO.File]::ReadAllBytes($path)
}

# How many complete lines release.log holds, and how many of them name the lease.
function Get-CeLogCounts($Lab, [string]$Id) {
  $bytes = Read-CeLog $Lab
  if ($null -eq $bytes) { return @{ All = 0; Lease = 0 } }
  @{ All = @(Get-CeAllLines $Lab).Count; Lease = @(Get-CeLogLines $Lab $Id).Count }
}

function Get-CeAllLines($Lab) {
  $bytes = Read-CeLog $Lab
  if ($null -eq $bytes) { return @() }
  @([regex]::Matches($utf8.GetString($bytes), '[^\n]*\n') | ForEach-Object Value)
}

# The lines of release.log that name the lease, each with its line feed.
function Get-CeLogLines($Lab, [string]$Id) {
  @(Get-CeAllLines $Lab | Where-Object { $_ -cmatch (' lease=' + [regex]::Escape($Id) + ' ') })
}

# The problems with a log line of one release: its time between the two bounds, the lease ID, and the
# text, which $TextPattern matches whole. The line is checked byte for byte through its text.
function Get-CeLineProblems {
  param([Parameter(Mandatory)][string]$Line, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$TextPattern, [Parameter(Mandatory)][DateTime]$From, [Parameter(Mandatory)][DateTime]$To)
  $p = [System.Collections.Generic.List[string]]::new()
  $m = [regex]::Match($Line, '\A(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z) lease=' + [regex]::Escape($Id) + ' (.*)\n\z')
  if (-not $m.Success) { $p.Add('The line is not a UTC time, one space, lease=<id>, one space, a text and one line feed'); return , $p }
  if ($Line.Contains("`r")) { $p.Add('The line holds a carriage return') }
  $t = ConvertFrom-CcTime $m.Groups[1].Value
  if ($t -lt $From.AddSeconds(-1) -or $t -gt $To.AddSeconds(1)) { $p.Add('The time is not within the release''s run') }
  if ($m.Groups[2].Value -cnotmatch $TextPattern) { $p.Add('The text is not the form driver.md gives') }
  return , $p
}

# The problems with release.log after one release that was meant to log: the file is UTF-8 with no byte
# order mark and ends in a line feed, it gained exactly one line, that line names the lease and its text
# matches $TextPattern whole. $Rel is Invoke-CeRelease's result.
function Get-CeLogProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)]$Rel, [Parameter(Mandatory)][string]$TextPattern)
  $p = [System.Collections.Generic.List[string]]::new()
  $bytes = Read-CeLog $Lab
  if ($null -eq $bytes) { $p.Add('release.log was not written'); return , $p }
  if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { $p.Add('release.log starts with a byte order mark') }
  if ($bytes.Length -gt 0 -and $bytes[$bytes.Length - 1] -ne 10) { $p.Add('release.log does not end in a line feed') }
  $now = Get-CeLogCounts $Lab $Id
  if ($now.All - $Rel.AllBefore -ne 1) { $p.Add("release.log gained $($now.All - $Rel.AllBefore) lines, not 1") }
  $lines = @(Get-CeLogLines $Lab $Id)
  if ($lines.Count - $Rel.LeaseBefore -ne 1) { $p.Add("release.log gained $($lines.Count - $Rel.LeaseBefore) lines for the lease, not 1") }
  elseif ($lines.Count -gt 0) {
    foreach ($q in (Get-CeLineProblems -Line $lines[$lines.Count - 1] -Id $Id -TextPattern $TextPattern -From $Rel.From -To $Rel.To)) { $p.Add($q) }
  }
  return , $p
}

# The problems with a release that was meant to succeed, and with its document against the lease `create`
# printed: contract v0's document, unchanged (driver.md, "The order of a release").
function Get-CeDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Id, $Created, [bool]$Expires)
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-CcSuccess $Run)) { $p.Add("The release did not succeed as the contract says: $(Format-CcRun $Run)"); return , $p }
  $d = $Run.Json
  if ($d -isnot [hashtable]) { $p.Add('Stdout is not a lease document'); return , $p }
  if ($d['id'] -cne $Id) { $p.Add('The id is not the lease''s') }
  if ($d['state'] -cne 'released') { $p.Add("The state is '$($d['state'])', not released") }
  if ($d.ContainsKey('ssh')) { $p.Add('The document still holds ssh') }
  if ($d['released_at'] -isnot [string] -or $d['released_at'] -cnotmatch '\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\z') { $p.Add('released_at is missing or not a time') }
  if ($Expires) {
    if ($d['expires_at'] -cne $Created['expires_at']) { $p.Add('expires_at is not the one the lease had') }
  } elseif ($d.ContainsKey('expires_at')) { $p.Add('The document holds expires_at') }
  if ($null -ne $Created) {
    foreach ($k in 'project', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at') {
      if ($Created.ContainsKey($k) -and (ConvertTo-CcCanonical $d[$k]) -cne (ConvertTo-CcCanonical $Created[$k])) { $p.Add("The member $k changed") }
    }
  }
  return , $p
}

# The problems with a release that was meant to succeed, said once for a case whose row names no document.
function Get-CeExitProblems($Run) {
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-CcSuccess $Run)) { $p.Add("The release did not succeed as the contract says: $(Format-CcRun $Run)") }
  return , $p
}

function Add-CeProblemResult {
  param([Parameter(Mandatory)][string]$Name, $Problems)
  if ($null -eq $Problems -or @($Problems).Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (@($Problems) -join '; ') }
}

function Get-CeName([string]$Anchor, [string]$Row, [string]$Text) { "$Anchor`: $Row $Text" }

function New-CeProblems { , [System.Collections.Generic.List[string]]::new() }

# A ready lease made by create. Returns Id, Alias, Doc, AliasFile and LeaseDir, or $null (after a FAIL line)
# when create did not give one.
function New-CeCaseLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Anchor, [Parameter(Mandatory)][string]$Row, [string]$Ttl)
  $l = New-CcReadyLease -Lab $Lab -Project 'conf' -Ttl $Ttl
  if (-not $l.Ok) {
    Write-ConformanceResult -Name (Get-CeName $Anchor $Row 'Setup, a ready lease is made by create') -Status FAIL -Reason "create did not give a ready lease: $(Format-CcRun $l.Run)"
    return $null
  }
  $id = [string]$l.Doc['id']
  $alias = [string]$l.Doc['ssh']['host_alias']
  [pscustomobject]@{ Id = $id; Alias = $alias; Doc = $l.Doc; AliasFile = (Get-CcAliasPath $Lab $alias); LeaseDir = (Get-CcLeaseDir $Lab $id) }
}

# Runs one release, and records what a check needs of its run: its streams, when it ran, how long it took
# and what release.log held before.
function Invoke-CeRelease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id)
  $before = Get-CeLogCounts $Lab $Id
  $from = Get-CcNowSecond
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $run = Invoke-CcLease -Lab $Lab -Arguments @('release', $Id)
  $sw.Stop()
  [pscustomobject]@{ Run = $run; From = $from; To = [DateTime]::UtcNow; Seconds = $sw.Elapsed.TotalSeconds; AllBefore = $before.All; LeaseBefore = $before.Lease }
}

# A case. A throw in its body is one FAIL line named for the row, and its stand-ins are ended whatever happens.
function Invoke-CeCase {
  param([Parameter(Mandatory)][string]$Row, [Parameter(Mandatory)][scriptblock]$Body)
  try { & $Body }
  catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name "$Row The case ran to its end without an unexpected error" -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    try { Stop-CeAll } catch { }
    $script:CeStands.Clear()
  }
}

# --- The checks ---------------------------------------------------------------------------------------

$lab = $null
$lab11 = $null
try {
  $lab = New-ConformanceHome
  $csc = Find-CeCsc
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$aTree`: R1 to R15 A release ends the lease's connections" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } elseif (-not $csc) {
    Write-ConformanceResult -Name "$aTree`: R1 to R15 A release ends the lease's connections" -Status SKIPPED -Reason 'No csc.exe of the .NET Framework was found, so no stand-in program could be built'
  } else {
    $base = New-CeBase -Lab $lab -Csc $csc
    if (-not $base) {
      Write-ConformanceResult -Name "$aTree`: Setup, the stand-in program builds" -Status FAIL -Reason 'csc.exe did not build the stand-in program'
    } else {
      if (-not ('CeLaunch' -as [type])) { Add-Type -TypeDefinition $script:CeLaunchSource }

      # R14's lease is made first, so that its 60 seconds pass while the other cases run.
      $r14 = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R14' -Ttl '1m'
      $r14Created = [DateTime]::UtcNow

      # === R1: one client with its tree ===========================================================================
      Invoke-CeCase 'R1' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aTree -Row 'R1'; if (-not $l) { return }
        $gc = New-CeStand -Lab $lab -Base $base -Case r1 -Name grandchild -Image 'helper.exe'
        $px = New-CeStand -Lab $lab -Base $base -Case r1 -Name proxy -Image $proxyImage -Kids @(@{ Label = 'grandchild'; Stand = $gc; Args = 'worker' })
        $c = New-CeStand -Lab $lab -Base $base -Case r1 -Name ssh -Image 'ssh.exe' -Kids @(@{ Label = 'proxy'; Stand = $px; Args = $l.Alias })
        Start-CeStand -Lab $lab -Stand $c -Line "ssh.exe $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($c))) { Write-ConformanceResult -Name (Get-CeName $aTree 'R1' 'Setup, the three stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $endedOk = Wait-CeEnded -Stands @($c, $px, $gc)
        $left = Get-CeAliveNames @($c, $px, $gc)
        $p = New-CeProblems; if (-not $endedOk) { $p.Add("Still running after the release: $left") }
        Add-CeProblemResult -Name (Get-CeName $aTree 'R1' 'release of a ready lease ends ssh.exe <a>, its child agentspace-ssh-proxy.exe <a> and that one''s grandchild') -Problems $p
        $p = Get-CeDocProblems -Run $rel.Run -Id $l.Id -Created $l.Doc -Expires $true
        if (Test-Path -LiteralPath $l.AliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $l.LeaseDir) { $p.Add('The lease folder is still there') }
        Add-CeProblemResult -Name (Get-CeName $aTree 'R1' 'release exits 0, prints the lease document as v0.07''s release does, writes nothing to standard error, and leaves no alias file or lease folder') -Problems $p
        Add-CeProblemResult -Name (Get-CeName $aCount 'R1' 'release.log gains exactly one line, lease=<id> and Ended 1 of the lease''s connections., counting the ssh.exe client alone') -Problems (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 1 of the lease''s connections\.\z')
      }

      # === R2: the order of a release =============================================================================
      Invoke-CeCase 'R2' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R2'; if (-not $l) { return }
        $dir = Get-CeStandDir $lab 'r2' 'watcher'
        $watchFile = Join-Path $dir 'watch.txt'
        $s = New-CeStand -Lab $lab -Base $base -Case r2 -Name watcher -Image 'ssh.exe' -Extra @("watch|$($l.AliasFile)|$watchFile")
        Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($l.Alias)"
        $until = [DateTime]::UtcNow.AddMilliseconds($script:CeReadyMs)
        $lines = @()
        while ($true) {
          $b = Read-CeStandFile $watchFile
          $lines = if ($b) { @($utf8.GetString($b) -split "`n" | Where-Object { $_ -ne '' }) } else { @() }
          if ($lines.Count -ge 3 -and (Test-CeStandAlive $s)) { break }
          if ([DateTime]::UtcNow -gt $until) { break }
          Start-Sleep -Milliseconds 50
        }
        if ($lines.Count -lt 3) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R2' 'Setup, the stand-in wrote three lines before the release') -Status FAIL -Reason "It wrote $($lines.Count) lines within 30 seconds"; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $endedOk = Wait-CeEnded -Stands @($s)
        $b = Read-CeStandFile $watchFile
        $text = if ($b) { $utf8.GetString($b) } else { '' }
        $complete = @([regex]::Matches($text, '[^\n]*\n') | ForEach-Object { $_.Value.TrimEnd("`n") })
        $p = New-CeProblems
        if (-not $endedOk) { $p.Add('The stand-in is still running after the release') }
        if ($complete.Count -eq 0) { $p.Add('The stand-in wrote no line') }
        elseif ($complete[$complete.Count - 1] -cne 'exists') { $p.Add("The stand-in's last line before it was ended says '$($complete[$complete.Count - 1])', not exists: the alias file was deleted before the client was ended") }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R2' 'a stand-in ssh.exe <a> that writes every 100 milliseconds whether the alias file exists has written that it does in its last line before it was ended') -Problems $p
        $p = Get-CeExitProblems $rel.Run
        if (Test-Path -LiteralPath $l.AliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $l.LeaseDir) { $p.Add('The lease folder is still there') }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R2' 'the alias file and the lease folder are gone after the release') -Problems $p
      }

      # === R3: every "Yes" row of an ssh client's destination =====================================================
      Invoke-CeCase 'R3' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aDest -Row 'R3'; if (-not $l) { return }
        $a = $l.Alias
        $upper = $a.ToUpperInvariant()
        $rows = @(
          @{ T = 'ssh.exe <a>'; L = "ssh.exe $a" },
          @{ T = '"C:\Windows\System32\OpenSSH\ssh.exe" -T -o ProxyCommand="x y" -o ClearAllForwardings=yes <a>'; L = '"C:\Windows\System32\OpenSSH\ssh.exe" -T -o ProxyCommand="x y" -o ClearAllForwardings=yes ' + $a },
          @{ T = 'ssh.exe -N -o ExitOnForwardFailure=yes <a>'; L = "ssh.exe -N -o ExitOnForwardFailure=yes $a" },
          @{ T = 'ssh.exe ubuntu@<a>'; L = "ssh.exe ubuntu@$a" },
          @{ T = 'ssh.exe ssh://ubuntu@<a>:22'; L = "ssh.exe ssh://ubuntu@${a}:22" },
          @{ T = 'ssh.exe -l ubuntu -p 22 <a> uname -a'; L = "ssh.exe -l ubuntu -p 22 $a uname -a" },
          @{ T = 'ssh.exe -s <a> sftp'; L = "ssh.exe -s $a sftp" },
          @{ T = 'ssh.exe -vvv -oPort=22 <a in capitals>'; L = "ssh.exe -vvv -oPort=22 $upper" },
          @{ T = 'ssh.exe -- <a>'; L = "ssh.exe -- $a" },
          @{ T = 'ssh.exe <a> -v'; L = "ssh.exe $a -v" }
        )
        $stands = @()
        $n = 0
        foreach ($r in $rows) {
          $n++
          $s = New-CeStand -Lab $lab -Base $base -Case r3 -Name ('yes{0:d2}' -f $n) -Image 'ssh.exe'
          Start-CeStand -Lab $lab -Stand $s -Line $r.L
          $stands += $s
        }
        if (-not (Wait-CeStarted -Tops $stands)) { Write-ConformanceResult -Name (Get-CeName $aDest 'R3' 'Setup, the ten stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        [void](Wait-CeEnded -Stands $stands)
        for ($i = 0; $i -lt $rows.Count; $i++) {
          $p = New-CeProblems; if (Test-CeStandAlive $stands[$i]) { $p.Add('It is still running after the release') }
          Add-CeProblemResult -Name (Get-CeName $aDest 'R3' "$($rows[$i].T) is ended by the release") -Problems $p
        }
        $p = Get-CeExitProblems $rel.Run
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 10 of the lease''s connections\.\z')) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aDest 'R3' 'one release ends all ten, and its release.log line says Ended 10 of the lease''s connections.') -Problems $p
      }

      # === R4: every "No" row, and the other alias's clients ======================================================
      Invoke-CeCase 'R4' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aClients -Row 'R4'; if (-not $l) { return }
        $o = New-CeCaseLease -Lab $lab -Anchor $aClients -Row 'R4'; if (-not $o) { return }
        $a = $l.Alias
        $b = $o.Alias
        $cut = $a.Substring(0, $a.Length - 4)
        $rows = @(
          @{ T = 'ssh.exe -J <a> other'; L = "ssh.exe -J $a other" },
          @{ T = 'ssh.exe -F <a> other'; L = "ssh.exe -F $a other" },
          @{ T = 'ssh.exe -i<a> other'; L = "ssh.exe -i$a other" },
          @{ T = 'ssh.exe other <a>'; L = "ssh.exe other $a" },
          @{ T = 'ssh.exe other -- <a>'; L = "ssh.exe other -- $a" },
          @{ T = 'ssh.exe <a>0'; L = "ssh.exe ${a}0" },
          @{ T = 'ssh.exe <a cut by its last four characters>'; L = "ssh.exe $cut" },
          @{ T = 'ssh.exe ssh://<a>/path'; L = "ssh.exe ssh://$a/path" },
          @{ T = 'ssh.exe -p'; L = 'ssh.exe -p' },
          @{ T = 'ssh.exe <b>, another lease''s alias'; L = "ssh.exe $b" },
          @{ T = 'scp.exe <a>:file .'; I = 'scp.exe'; L = "scp.exe ${a}:file ." },
          @{ T = 'agentspace-ssh-proxy.exe <b>, another lease''s alias'; I = $proxyImage; L = "$proxyImage $b" },
          @{ T = 'agentspace-ssh-proxy.exe alone'; I = $proxyImage; L = $proxyImage },
          @{ T = 'helper.exe <a>, another image file name whose only argument is <a>'; I = 'helper.exe'; L = "helper.exe $a" }
        )
        $control = New-CeStand -Lab $lab -Base $base -Case r4 -Name control -Image 'ssh.exe'
        $stands = @()
        $n = 0
        foreach ($r in $rows) {
          $n++
          $image = if ($r.I) { $r.I } else { 'ssh.exe' }
          $s = New-CeStand -Lab $lab -Base $base -Case r4 -Name ('no{0:d2}' -f $n) -Image $image
          Start-CeStand -Lab $lab -Stand $s -Line $r.L
          $stands += $s
        }
        Start-CeStand -Lab $lab -Stand $control -Line "ssh.exe $a"
        if (-not (Wait-CeStarted -Tops (@($control) + $stands))) { Write-ConformanceResult -Name (Get-CeName $aClients 'R4' 'Setup, the stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        [void](Wait-CeEnded -Stands @($control))
        for ($i = 0; $i -lt $rows.Count; $i++) {
          $p = New-CeProblems; if (-not (Test-CeStandAlive $stands[$i])) { $p.Add('It was ended, though it is not a client of the released lease''s alias') }
          Add-CeProblemResult -Name (Get-CeName $aClients 'R4' "$($rows[$i].T) is still running after the release") -Problems $p
        }
        $p = Get-CeExitProblems $rel.Run
        if (Test-CeStandAlive $control) { $p.Add('The control ssh.exe <a> was not ended, so the release walked for no client') }
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 1 of the lease''s connections\.\z')) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R4' 'the one control client, ssh.exe <a>, is ended and counted: release.log says Ended 1 of the lease''s connections.') -Problems $p
      }

      # === R5: a proxy program with no ssh.exe above it ===========================================================
      Invoke-CeCase 'R5' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aClients -Row 'R5'; if (-not $l) { return }
        $alone = New-CeStand -Lab $lab -Base $base -Case r5 -Name alone -Image $proxyImage
        $extra = New-CeStand -Lab $lab -Base $base -Case r5 -Name extra -Image $proxyImage
        Start-CeStand -Lab $lab -Stand $alone -Line "$proxyImage $($l.Alias)"
        Start-CeStand -Lab $lab -Stand $extra -Line "$proxyImage $($l.Alias) extra"
        if (-not (Wait-CeStarted -Tops @($alone, $extra))) { Write-ConformanceResult -Name (Get-CeName $aClients 'R5' 'Setup, the stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        [void](Wait-CeEnded -Stands @($alone))
        $p = New-CeProblems; if (Test-CeStandAlive $alone) { $p.Add('It is still running after the release') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R5' 'agentspace-ssh-proxy.exe <a> with no ssh.exe above it is ended') -Problems $p
        $p = New-CeProblems; if (-not (Test-CeStandAlive $extra)) { $p.Add('It was ended, though a token follows the alias') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R5' 'agentspace-ssh-proxy.exe <a> extra is not ended') -Problems $p
        $p = Get-CeExitProblems $rel.Run
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 1 of the lease''s connections\.\z')) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R5' 'the proxy client is counted: release.log says Ended 1 of the lease''s connections.') -Problems $p
      }

      # === R6: the keeper's holder form, and the keeper's files ===================================================
      Invoke-CeCase 'R6' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aClients -Row 'R6'; if (-not $l) { return }
        $h = New-CeStand -Lab $lab -Base $base -Case r6 -Name holder -Image 'ssh.exe'
        Start-CeStand -Lab $lab -Stand $h -Line "ssh.exe -N -o ExitOnForwardFailure=yes $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($h))) { Write-ConformanceResult -Name (Get-CeName $aClients 'R6' 'Setup, the holder stand-in started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        # The keeper's files in the throwaway state folder: a holder record that names the stand-in, and the logs.
        $self = Get-CeSelf $h
        $started = [DateTime]::FromFileTimeUtc($self.Start).ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", [Globalization.CultureInfo]::InvariantCulture)
        $files = [ordered]@{
          'board-holder.json' = "{`"pid`":$($self.ProcessId),`"process_started_at`":`"$started`",`"lease_id`":`"$($l.Id)`",`"alias`":`"$($l.Alias)`"}`n"
          'board-holder.log' = "2000-01-01T00:00:00Z A line of the keeper's log, which a release never reads.`n"
          'board-holder-ssh.log' = "A line of the holder's output.`n"
        }
        New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $lab.StateDir) | Out-Null
        foreach ($k in $files.Keys) {
          [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.StateDir $k)), $utf8.GetBytes([string]$files[$k]))
        }
        $before = @{}
        foreach ($k in $files.Keys) { $before[$k] = Get-CcFileHash (Join-Path $lab.StateDir $k) }
        $namesBefore = (Get-CcFileNames $lab.StateDir | Where-Object { $_ -like 'board-holder*' }) -join ','
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        [void](Wait-CeEnded -Stands @($h))
        $p = New-CeProblems; if (Test-CeStandAlive $h) { $p.Add('The holder stand-in is still running after the release') }
        $p2 = Get-CeExitProblems $rel.Run
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 1 of the lease''s connections\.\z')) { $p2.Add($q) }
        foreach ($q in $p2) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R6' 'a stand-in in the keeper''s holder form, ssh.exe -N -o ExitOnForwardFailure=yes <a>, is ended and counted: Ended 1 of the lease''s connections.') -Problems $p
        $p = New-CeProblems
        foreach ($k in $files.Keys) { if ((Get-CcFileHash (Join-Path $lab.StateDir $k)) -cne $before[$k]) { $p.Add("$k changed or is gone") } }
        $namesAfter = (Get-CcFileNames $lab.StateDir | Where-Object { $_ -like 'board-holder*' }) -join ','
        if ($namesAfter -cne $namesBefore) { $p.Add('The keeper''s files in the state folder are not the same set') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R6' 'the keeper''s files in the throwaway state folder (the holder record, the keeper''s log and the holder''s output) are byte for byte as they were') -Problems $p
      }

      # === R7: a release started as the child of a client =========================================================
      Invoke-CeCase 'R7' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aClients -Row 'R7'; if (-not $l) { return }
        $cli = Get-ConformanceProgram -Variable 'AGENTSPACE_CMD'
        if (-not $cli) { Write-ConformanceResult -Name (Get-CeName $aClients 'R7' 'Setup, the lease CLI is named') -Status FAIL -Reason 'Neither AGENTSPACE_CMD nor its default names a program'; return }
        $dir = Get-CeStandDir $lab 'r7' 'parent'
        $beside = New-CeStand -Lab $lab -Base $base -Case r7 -Name beside -Image 'helper.exe'
        $parent = New-CeStand -Lab $lab -Base $base -Case r7 -Name parent -Image 'ssh.exe' -Kids @(@{ Label = 'beside'; Stand = $beside; Args = 'worker' }) `
          -Extra @("run|rel|$cli|lease release $($l.Id)|$(Join-Path $dir 'rel.out')|$(Join-Path $dir 'rel.err')") -Expect 1
        $from = Get-CcNowSecond
        $before = Get-CeLogCounts $lab $l.Id
        $tripwire = Get-ConformanceTripwirePath -ConformanceHome $lab
        Start-CeStand -Lab $lab -Stand $parent -Line "ssh.exe $($l.Alias)" -Environment @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy; AGENTSPACE_PROVISION_CMD = $tripwire }
        # The release is the stand-in's child, started when the stand-in starts. Waits for its exit code, with a bound.
        $until = [DateTime]::UtcNow.AddSeconds(60)
        while (-not (Test-Path -LiteralPath (Join-Path $dir 'rel.exit') -PathType Leaf) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
        $to = [DateTime]::UtcNow
        if (-not (Test-Path -LiteralPath (Join-Path $dir 'rel.exit') -PathType Leaf)) {
          Write-ConformanceResult -Name (Get-CeName $aClients 'R7' 'The release, started as the child of ssh.exe <a>, finished') -Status FAIL -Reason 'It had not finished within 60 seconds'
          return
        }
        $exitText = $utf8.GetString((Read-CeStandFile (Join-Path $dir 'rel.exit'))).Trim()
        $ob = Read-CeStandFile (Join-Path $dir 'rel.out'); $eb = Read-CeStandFile (Join-Path $dir 'rel.err')
        $out = if ($ob) { $utf8.GetString($ob) } else { '' }
        $err = if ($eb) { $utf8.GetString($eb) } else { '' }
        $run = [pscustomobject]@{ ExitCode = $(if ($exitText -match '\A-?\d+\z') { [int]$exitText } else { -999 }); Stdout = $out; Stderr = $err; Json = (ConvertFrom-CcJson $out) }
        $p = New-CeProblems
        if (-not (Test-CeStandAlive $parent)) { $p.Add('The spared ssh.exe <a>, the release''s parent, was ended') }
        if (-not (Test-CeStandAlive $beside)) { $p.Add('The second child beside the release, under the spared client, was ended') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R7' 'the ssh.exe <a> that the release is a child of, and the second child beside the release, keep running') -Problems $p
        $p = Get-CeDocProblems -Run $run -Id $l.Id -Created $l.Doc -Expires $true
        if (Test-Path -LiteralPath $l.AliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $l.LeaseDir) { $p.Add('The lease folder is still there') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R7' 'the release succeeds with exit 0 and its usual document, and deletes the alias file and the lease folder') -Problems $p
        $rel = [pscustomobject]@{ From = $from; To = $to; AllBefore = $before.All; LeaseBefore = $before.Lease }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R7' 'release.log gains Ended 0 of the lease''s connections; 1 could not be ended., the spared client counted once though both walks find it') -Problems (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 0 of the lease''s connections; 1 could not be ended\.\z')
        $own = $null; try { $own = [Diagnostics.Process]::GetProcessById($PID) } catch { }
        $p = New-CeProblems; if ($null -eq $own -or $own.HasExited) { $p.Add('The check''s own process is gone') }
        Add-CeProblemResult -Name (Get-CeName $aClients 'R7' 'the check''s own process, an ancestor of the release and no client, is never ended') -Problems $p
      }

      # === R8: a released lease released again ====================================================================
      Invoke-CeCase 'R8' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R8'; if (-not $l) { return }
        $first = Invoke-CeRelease -Lab $lab -Id $l.Id
        $p = Get-CeDocProblems -Run $first.Run -Id $l.Id -Created $l.Doc -Expires $true
        if ($p.Count -gt 0) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R8' 'Setup, the first release succeeds') -Status FAIL -Reason ($p -join '; '); return }
        $s = New-CeStand -Lab $lab -Base $base -Case r8 -Name late -Image 'ssh.exe'
        Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($s))) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R8' 'Setup, the stand-in started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $second = Invoke-CeRelease -Lab $lab -Id $l.Id
        $endedOk = Wait-CeEnded -Stands @($s)
        $p = New-CeProblems; if (-not $endedOk) { $p.Add('It is still running after the second release') }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R8' 'release of a released lease ends an ssh.exe <a> started after the first release') -Problems $p
        $p = Get-CeExitProblems $second.Run
        if ($p.Count -eq 0) {
          if ($second.Run.Json['state'] -cne 'released') { $p.Add("The state is '$($second.Run.Json['state'])', not released") }
          if ($second.Run.Json['released_at'] -cne $first.Run.Json['released_at']) { $p.Add('released_at is not the first one') }
        }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R8' 'release of a released lease exits 0 and prints the lease with its first released_at') -Problems $p
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R8' 'the second release logs a line: Ended 1 of the lease''s connections.') -Problems (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $second -TextPattern '\AEnded 1 of the lease''s connections\.\z')
      }

      # === R9: a pending lease that carries a driver ==============================================================
      Invoke-CeCase 'R9' {
        $o = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R9'; if (-not $o) { return }
        $yId = [guid]::NewGuid().ToString()
        Add-CcRecord $lab ([ordered]@{
            id = $yId; project = 'conf'; state = 'pending'; platform = 'linux/arm64'; profile = 'agent-linux'
            size = 'm'; ttl_seconds = 28800; created_at = (ConvertTo-CcTime (Get-CcNowSecond).AddMinutes(-2)); driver = 'ec2'
          })
        $child = New-CeStand -Lab $lab -Base $base -Case r9 -Name child -Image 'helper.exe'
        $s = New-CeStand -Lab $lab -Base $base -Case r9 -Name ssh-b -Image 'ssh.exe' -Kids @(@{ Label = 'child'; Stand = $child; Args = 'worker' })
        Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($o.Alias)"
        if (-not (Wait-CeStarted -Tops @($s))) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R9' 'Setup, the stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $yId
        $gone = Get-CeGoneNames @($s, $child)
        $p = New-CeProblems; if ($gone) { $p.Add("Ended: $gone") }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R9' 'release of a pending lease that carries a driver ends nothing: ssh.exe <b>, another lease''s client, keeps running') -Problems $p
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R9' 'release of a pending lease exits 0 and prints its document as v0.07''s does: released, with no expires_at, and nothing on standard error') -Problems (Get-CeDocProblems -Run $rel.Run -Id $yId -Created $null -Expires $false)
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R9' 'release of a pending lease logs Ended 0 of the lease''s connections.') -Problems (Get-CeLogProblems -Lab $lab -Id $yId -Rel $rel -TextPattern '\AEnded 0 of the lease''s connections\.\z')
      }

      # === R10: an alias file that belongs to another lease =======================================================
      Invoke-CeCase 'R10' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R10'; if (-not $l) { return }
        $s = New-CeStand -Lab $lab -Base $base -Case r10 -Name ssh -Image 'ssh.exe'
        Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($s))) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R10' 'Setup, the stand-in started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        # The alias file's owner mark now names another lease, drawn at random for the run.
        $text = [IO.File]::ReadAllText($l.AliasFile, $utf8)
        $other = [guid]::NewGuid().ToString()
        $changed = [regex]::Replace($text, '\A# agentspace lease [0-9A-Fa-f-]+;', "# agentspace lease $other;")
        if ($changed -ceq $text) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R10' 'Setup, the alias file''s owner mark names another lease') -Status FAIL -Reason 'The alias file does not start with an owner mark in the form state.md gives'; return }
        $changedBytes = $utf8.GetBytes($changed)
        [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $l.AliasFile), $changedBytes)
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $p = New-CeProblems
        if (-not (Test-CcFailure -Run $rel.Run -Exit 1 -Code 'internal')) { $p.Add("The release did not fail with internal as before v0.08: $(Format-CcRun $rel.Run)") }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R10' 'release of a lease whose alias file belongs to another lease fails with internal, exit 1, as before v0.08') -Problems $p
        $p = New-CeProblems
        if (-not (Test-CeStandAlive $s)) { $p.Add('The stand-in ssh.exe <a> was ended') }
        if (-not (Test-Path -LiteralPath $l.AliasFile -PathType Leaf)) { $p.Add('The alias file was deleted') }
        elseif ((Get-CcFileHash $l.AliasFile) -cne ((Get-FileHash -InputStream ([IO.MemoryStream]::new($changedBytes)) -Algorithm SHA256).Hash)) { $p.Add('The alias file changed') }
        if (-not (Test-Path -LiteralPath $l.LeaseDir -PathType Container)) { $p.Add('The lease folder was deleted') }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R10' 'the stand-in ssh.exe <a> keeps running, and nothing is deleted') -Problems $p
        $now = Get-CeLogCounts $lab $l.Id
        $p = New-CeProblems
        if ($now.Lease - $rel.LeaseBefore -ne 0) { $p.Add('release.log gained a line for the lease') }
        if ($now.All - $rel.AllBefore -ne 0) { $p.Add("release.log gained $($now.All - $rel.AllBefore) lines") }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R10' 'release.log gains no line') -Problems $p
      }

      # === R11: release.log is a folder, in a home of its own =====================================================
      Invoke-CeCase 'R11' {
        $script:lab11 = New-ConformanceHome
        $h = $script:lab11
        if (-not $h.CaReady) { Write-ConformanceResult -Name (Get-CeName $aLog 'R11' 'release still ends the stand-in with release.log a folder') -Status SKIPPED -Reason "No throwaway CA could be made: $($h.CaReason)"; return }
        $l = New-CeCaseLease -Lab $h -Anchor $aLog -Row 'R11'; if (-not $l) { return }
        New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.StateDir 'release.log')) | Out-Null
        $s = New-CeStand -Lab $h -Base $base -Case r11 -Name ssh -Image 'ssh.exe'
        Start-CeStand -Lab $h -Stand $s -Line "ssh.exe $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($s))) { Write-ConformanceResult -Name (Get-CeName $aLog 'R11' 'Setup, the stand-in started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $h -Id $l.Id
        $endedOk = Wait-CeEnded -Stands @($s)
        $p = New-CeProblems; if (-not $endedOk) { $p.Add('The stand-in is still running after the release') }
        Add-CeProblemResult -Name (Get-CeName $aLog 'R11' 'with release.log made a folder, release still ends the stand-in ssh.exe <a>') -Problems $p
        $p = Get-CeDocProblems -Run $rel.Run -Id $l.Id -Created $l.Doc -Expires $true
        if ($rel.Run.StderrBytes.Length -ne 0) { $p.Add("Standard error holds $($rel.Run.StderrBytes.Length) bytes") }
        if (Test-Path -LiteralPath $l.AliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $l.LeaseDir) { $p.Add('The lease folder is still there') }
        Add-CeProblemResult -Name (Get-CeName $aLog 'R11' 'release succeeds with exit 0 and the same document, and writes nothing to standard error') -Problems $p
      }

      # === R12: no client, and an unsatisfiable lease =============================================================
      Invoke-CeCase 'R12' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R12'; if (-not $l) { return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $p = Get-CeDocProblems -Run $rel.Run -Id $l.Id -Created $l.Doc -Expires $true
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 0 of the lease''s connections\.\z')) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R12' 'release with no client running succeeds as v0.07''s did and logs Ended 0 of the lease''s connections.') -Problems $p

        # A project with no host record and no entry on the project list: create answers unsatisfiable.
        $run = Invoke-CcLease -Lab $lab -Arguments @('create', '--project', 'nocapacity')
        $doc = ConvertFrom-CcJson $run.Stdout
        if (-not (Test-CcSuccess $run) -or $doc -isnot [hashtable] -or $doc['state'] -cne 'unsatisfiable') {
          Write-ConformanceResult -Name (Get-CeName $aOrder 'R12' 'Setup, create answers unsatisfiable for a project with no host') -Status FAIL -Reason "create did not give an unsatisfiable lease: $(Format-CcRun $run)"
          return
        }
        $uId = [string]$doc['id']
        $rel = Invoke-CeRelease -Lab $lab -Id $uId
        $now = Get-CeLogCounts $lab $uId
        $p = New-CeProblems
        if (-not (Test-CcSuccess $rel.Run)) { $p.Add("The release did not succeed as the contract says: $(Format-CcRun $rel.Run)") }
        elseif ((ConvertTo-CcCanonical $rel.Run.Json) -cne (ConvertTo-CcCanonical $doc)) { $p.Add('The document is not the lease unchanged') }
        if ($now.Lease - $rel.LeaseBefore -ne 0) { $p.Add('release.log gained a line for the lease') }
        if ($now.All - $rel.AllBefore -ne 0) { $p.Add("release.log gained $($now.All - $rel.AllBefore) lines") }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R12' 'release of an unsatisfiable lease prints it unchanged and logs no line') -Problems $p
      }

      # === R13: ten clients =======================================================================================
      Invoke-CeCase 'R13' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aTree -Row 'R13'; if (-not $l) { return }
        $stands = @()
        for ($i = 1; $i -le 10; $i++) {
          $s = New-CeStand -Lab $lab -Base $base -Case r13 -Name ('c{0:d2}' -f $i) -Image 'ssh.exe'
          Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($l.Alias)"
          $stands += $s
        }
        if (-not (Wait-CeStarted -Tops $stands)) { Write-ConformanceResult -Name (Get-CeName $aTree 'R13' 'Setup, the ten stand-ins started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        [void](Wait-CeEnded -Stands $stands)
        $p = New-CeProblems
        if ($rel.Seconds -gt 15) { $p.Add("The release took $([int]$rel.Seconds) seconds, over 15") }
        Add-CeProblemResult -Name (Get-CeName $aTree 'R13' 'with ten stand-in clients of <a>, release returns within 15 seconds of its start') -Problems $p
        $p = New-CeProblems
        $left = Get-CeAliveNames $stands
        if ($left) { $p.Add("Still running after the release: $left") }
        foreach ($q in (Get-CeExitProblems $rel.Run)) { $p.Add($q) }
        foreach ($q in (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 10 of the lease''s connections\.\z')) { $p.Add($q) }
        Add-CeProblemResult -Name (Get-CeName $aTree 'R13' 'release ends all ten, and release.log says Ended 10 of the lease''s connections.') -Problems $p
      }

      # === R15: a client that comes back is ended too, with no timing assumed =====================================
      Invoke-CeCase 'R15' {
        $l = New-CeCaseLease -Lab $lab -Anchor $aOrder -Row 'R15'; if (-not $l) { return }
        $first = New-CeStand -Lab $lab -Base $base -Case r15 -Name 'ssh-first' -Image 'ssh.exe'
        $second = New-CeStand -Lab $lab -Base $base -Case r15 -Name 'ssh-second' -Image 'ssh.exe'
        $sup = New-CeStand -Lab $lab -Base $base -Case r15 -Name supervisor -Image 'desktop-standin.exe' `
          -Kids @(@{ Label = 'first'; Stand = $first; Args = $l.Alias }) -Respawns @(@{ Watch = 'first'; Label = 'second'; Stand = $second; Args = $l.Alias })
        Invoke-CeWarm -Lab $lab -Stand $second
        Start-CeStand -Lab $lab -Stand $sup -Line 'desktop-standin.exe'
        if (-not (Wait-CeStarted -Tops @($sup))) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R15' 'Setup, the supervisor stand-in started its first client') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $toFile = $rel.To.ToFileTimeUtc()
        # The second client starts when the first has exited. Waits, with a bound, for its record.
        $until = [DateTime]::UtcNow.AddMilliseconds($script:CeEndMs)
        while (-not (@(Read-CePids $sup | Where-Object { $_.Label -ceq 'second' })[0]) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 50 }
        $secondRec = @(Read-CePids $sup | Where-Object { $_.Label -ceq 'second' })[0]
        $null = Wait-CeEnded -Stands @($first)
        $untilEnd = [DateTime]::UtcNow.AddMilliseconds($script:CeEndMs)
        while ($null -ne $secondRec -and (Test-CeAlive $secondRec) -and [DateTime]::UtcNow -lt $untilEnd) { Start-Sleep -Milliseconds 50 }
        $secondAlive = ($null -ne $secondRec -and (Test-CeAlive $secondRec))
        $late = ($secondAlive -and $secondRec.Start -gt $toFile)
        if ($late) {
          Write-ConformanceResult -Name (Get-CeName $aOrder 'R15' 'A client that came back while the release ran is ended too, and no matching client is left') -Status SKIPPED -Reason 'The stand-in started its second client after the release had returned, so the build was not shown wrong'
        } else {
          $p = New-CeProblems
          if ($null -eq $secondRec) { $p.Add('The first client was not ended, so the second was never started') }
          if (Test-CeStandAlive $first) { $p.Add('The first client is still running') }
          if ($secondAlive) { $p.Add('The second client is still running') }
          if (-not (Test-CeStandAlive $sup)) { $p.Add('The supervisor, which is no client, was ended') }
          Add-CeProblemResult -Name (Get-CeName $aOrder 'R15' 'with a supervisor that is no match and starts one new ssh.exe <a> once its first is ended, no ssh.exe <a> is left after the release and the supervisor keeps running') -Problems $p
        }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R15' 'release of the lease whose client came back exits 0 with the lease document as v0.07''s') -Problems (Get-CeDocProblems -Run $rel.Run -Id $l.Id -Created $l.Doc -Expires $true)
        $p = New-CeProblems
        $lines = @(Get-CeLogLines $lab $l.Id)
        if ($lines.Count - $rel.LeaseBefore -ne 1) { $p.Add("release.log gained $($lines.Count - $rel.LeaseBefore) lines for the lease, not 1") }
        else {
          foreach ($q in (Get-CeLineProblems -Line $lines[$lines.Count - 1] -Id $l.Id -TextPattern '\AEnded (\d+) of the lease''s connections\.\z' -From $rel.From -To $rel.To)) { $p.Add($q) }
          $nm = [regex]::Match($lines[$lines.Count - 1], 'Ended (\d+) of')
          if ($nm.Success -and [int]$nm.Groups[1].Value -lt 2 -and $null -ne $secondRec -and -not $late) { $p.Add('The count is under 2') }
        }
        Add-CeProblemResult -Name (Get-CeName $aLog 'R15' 'the release.log line is Ended <n> of the lease''s connections. with <n> at least 2, and no could-not-be-ended part') -Problems $p
      }

      # === R14: an expired lease, released once its expires_at has passed =========================================
      Invoke-CeCase 'R14' {
        if (-not $r14) { return }
        $l = $r14
        $p = New-CeProblems
        if ($l.Doc['ttl_seconds'] -ne 60) { $p.Add('The lease was not made with the shortest TTL, 60 seconds') }
        if ($p.Count -gt 0) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R14' 'Setup, the lease has a TTL of 60 seconds') -Status FAIL -Reason ($p -join '; '); return }
        $s = New-CeStand -Lab $lab -Base $base -Case r14 -Name ssh -Image 'ssh.exe'
        Start-CeStand -Lab $lab -Stand $s -Line "ssh.exe $($l.Alias)"
        if (-not (Wait-CeStarted -Tops @($s))) { Write-ConformanceResult -Name (Get-CeName $aOrder 'R14' 'Setup, the stand-in started') -Status FAIL -Reason 'A stand-in did not record itself within 30 seconds'; return }
        # Waits, with a bound of 70 seconds from the lease's creation plus what the stand-in took, until the
        # lease's expires_at has passed and get says it is expired.
        $expires = ConvertFrom-CcTime ([string]$l.Doc['expires_at'])
        $limit = $r14Created.AddSeconds(70)
        $state = ''
        while ($true) {
          if ([DateTime]::UtcNow -gt $expires.AddSeconds(1)) {
            $g = Invoke-CcLease -Lab $lab -Arguments @('get', $l.Id)
            $state = if ($g.Json -is [hashtable]) { [string]$g.Json['state'] } else { '' }
            if ($state -ceq 'expired') { break }
          }
          if ([DateTime]::UtcNow -gt $limit.AddSeconds(15)) { break }
          Start-Sleep -Milliseconds 500
        }
        if ($state -cne 'expired') { Write-ConformanceResult -Name (Get-CeName $aOrder 'R14' 'Setup, the lease is expired once its expires_at has passed') -Status FAIL -Reason "get said '$state', not expired"; return }
        $rel = Invoke-CeRelease -Lab $lab -Id $l.Id
        $endedOk = Wait-CeEnded -Stands @($s)
        $p = New-CeProblems; if (-not $endedOk) { $p.Add('The stand-in is still running after the release') }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R14' 'release of an expired lease ends the stand-in ssh.exe <a>') -Problems $p
        $p = Get-CeDocProblems -Run $rel.Run -Id $l.Id -Created $l.Doc -Expires $true
        if (Test-Path -LiteralPath $l.AliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $l.LeaseDir) { $p.Add('The lease folder is still there') }
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R14' 'release exits 0 and prints the lease released with its expires_at, and the alias file and the lease folder are gone') -Problems $p
        Add-CeProblemResult -Name (Get-CeName $aOrder 'R14' 'release.log gains Ended 1 of the lease''s connections.') -Problems (Get-CeLogProblems -Lab $lab -Id $l.Id -Rel $rel -TextPattern '\AEnded 1 of the lease''s connections\.\z')
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-release-ends-connections.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  # Ends every process any stand-in recorded, before the homes are removed.
  try { Stop-CeAll } catch { }
  if ($script:lab11) { Remove-ConformanceHome -ConformanceHome $script:lab11 }
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun
