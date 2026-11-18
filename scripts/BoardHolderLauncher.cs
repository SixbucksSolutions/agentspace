using System;
using System.Diagnostics;
using System.IO;
using System.Text;

// Starts the board holder keeper with no console window and waits for it, so the logon task
// "agentspace board holder" shows nothing on screen. After the localswim watcher's
// RelayTaskLauncher, which is built as a winexe for the same reason.
// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// Arguments: the full path of pwsh.exe, the full path of board-holder.ps1, then any arguments for
// the script (install-board-holder.ps1 passes -Watch). The launcher adds -NoProfile
// -NonInteractive -File itself.
// - Only plain arguments, such as -Watch, are passed on. Each is wrapped in quotes as it is, so an
//   argument that holds a quote or ends in a backslash would be split or joined wrongly by pwsh's
//   own parsing; the launcher MUST refuse one, with exit 2, rather than pass it on changed.
// - Fewer than 2 arguments is refused with exit 2.
// - pwsh runs in the launcher's own working directory, which the task sets to the repo root. The
//   task definition is the one place that chooses it.
// - The launcher exits with pwsh's own exit code, so a keeper that ends with an error is a failed
//   run of the task, and the task's repeating trigger starts the keeper again (D30). It exits 1 when pwsh could not be
//   started, and 2 for refused arguments.
//
// Launcher errors go to board-holder-launcher-errors.log in the state folder:
// %AGENTSPACE_STATE_DIR% when it is set, else %USERPROFILE%\.agentspace, as for the keeper (D21).
internal static class BoardHolderLauncher
{
    private static int Main(string[] args)
    {
        if (args.Length < 2)
        {
            Log("Refused: the launcher needs the path of pwsh.exe and of board-holder.ps1, and got " + args.Length + " argument(s).");
            return 2;
        }
        for (int i = 2; i < args.Length; i++)
        {
            if (args[i].IndexOf('"') >= 0 || args[i].EndsWith("\\", StringComparison.Ordinal))
            {
                Log("Refused: argument " + (i + 1) + " holds a quote or ends in a backslash; only plain arguments such as -Watch are passed on.");
                return 2;
            }
        }
        string pwsh = args[0];
        string script = args[1];
        try
        {
            var arguments = new StringBuilder("-NoProfile -NonInteractive -File \"" + script + "\"");
            for (int i = 2; i < args.Length; i++)
            {
                arguments.Append(" \"").Append(args[i]).Append('"');
            }
            var start = new ProcessStartInfo
            {
                FileName = pwsh,
                Arguments = arguments.ToString(),
                WorkingDirectory = Environment.CurrentDirectory,
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden
            };
            using (var process = Process.Start(start))
            {
                process.WaitForExit();
                return process.ExitCode;
            }
        }
        catch (Exception error)
        {
            Log(error.GetType().Name + ": " + error.Message);
            return 1;
        }
    }

    // Appends one line to the launcher's error log. A failure to write it is swallowed.
    private static void Log(string message)
    {
        try
        {
            string state = Environment.GetEnvironmentVariable("AGENTSPACE_STATE_DIR");
            if (string.IsNullOrEmpty(state))
            {
                state = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".agentspace");
            }
            Directory.CreateDirectory(state);
            File.AppendAllText(Path.Combine(state, "board-holder-launcher-errors.log"),
                DateTime.UtcNow.ToString("o") + " " + message + Environment.NewLine);
        }
        catch { }
    }
}
