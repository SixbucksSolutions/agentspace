@echo off
rem The default program for AGENTSPACE_KEEPER_CMD: the prototype's keeper under PowerShell 7. See docs/conformance.md.
pwsh -NoProfile -File "%~dp0..\..\scripts\board-holder.ps1" %*
exit /b %ERRORLEVEL%
