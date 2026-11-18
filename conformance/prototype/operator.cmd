@echo off
rem The default program for AGENTSPACE_OPERATOR_CMD: the prototype's operator under Windows PowerShell 5.1. See docs/conformance.md.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\..\scripts\agentspace.ps1" %*
exit /b %ERRORLEVEL%
