@echo off
rem The agentspace command (docs/lease-api.md) for cmd and PowerShell: runs scripts\lease-cli.ps1
rem with the arguments unchanged. It lives in bin\, not scripts\, so that `agentspace` never
rem resolves to scripts\agentspace.ps1, which PowerShell prefers over a .cmd in the same folder.
pwsh -NoProfile -File "%~dp0..\scripts\lease-cli.ps1" %*
exit /b %ERRORLEVEL%
