using System;
using System.Diagnostics;
using System.IO;
using System.Text;

// Starts the board holder keeper with no console window and waits for it, so the logon task
// "agentspace board holder" shows nothing on screen. After the localswim watcher's
// RelayTaskLauncher, which is built as a winexe for the same reason.
// Arguments: the full path of pwsh.exe, the full path of board-holder.ps1, then any arguments for
// the script (install-board-holder.ps1 passes -Watch). The launcher adds -NoProfile
// -NonInteractive -File itself. Launcher errors go to board-holder-launcher-errors.log in
// %USERPROFILE%\.agentspace.
internal static class BoardHolderLauncher
{
    private static int Main(string[] args)
    {
        if (args.Length < 2)
        {
            return 2;
        }
        string pwsh = args[0];
        string script = args[1];
        string root = Path.GetDirectoryName(script) ?? ".";
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
                WorkingDirectory = root,
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
            try
            {
                string state = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".agentspace");
                Directory.CreateDirectory(state);
                File.AppendAllText(Path.Combine(state, "board-holder-launcher-errors.log"),
                    DateTime.UtcNow.ToString("o") + " " + error.GetType().Name + ": " + error.Message + Environment.NewLine);
            }
            catch { }
            return 1;
        }
    }
}
