@echo off
REM A stub for ssh (a program outside agentspace) used by the keeper conformance checks. See docs/conformance.md.
rem The stand-in for ssh.exe in the keeper checks, conformance\keeper\test-keeper.ps1 and test-unmet-holder-dies-mid-pass.ps1.
rem It prints one line, then runs for about 20 minutes unless it is stopped, so it stays alive for as
rem long as a check needs. Its arguments are on its own command line, which is how a check finds it.
rem It connects nowhere and touches no file.
echo stub-started %*
ping -n 1200 127.0.0.1 >nul
