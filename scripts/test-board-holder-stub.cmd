@echo off
rem The stand-in for ssh.exe in scripts\test-board-holder.ps1 (story 0.2.T1.2).
rem It prints one line, then runs for about 20 minutes unless it is stopped, so it stays alive for as
rem long as a test needs. Its arguments are on its own command line, which is how a test finds it.
rem It connects nowhere and touches no file.
echo stub-started %*
ping -n 1200 127.0.0.1 >nul
