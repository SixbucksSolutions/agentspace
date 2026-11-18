@echo off
rem SSH ProxyCommand for agentspace: starts the instance if it is stopped, then
rem tunnels SSH over Session Manager. Nothing but the tunnel may write to stdout.
rem
rem RFC 2119 keywords in these comments; the capitals are load-bearing.
rem
rem Usage: ssh-proxy.cmd [<alias>]. With an alias, it runs agentspace.ps1 up -Alias <alias>, so
rem the localswim token push rides the SSH host alias the caller connected with (D9). Without
rem one, it runs up as it always has, and the push rides agentspace, Terry's own entry. A second
rem argument fails before anything is started. The exit code is the tunnel's.
rem
rem The threat model: the argument MUST come from a ProxyCommand line that lease-alias.ps1
rem writes, and lease-alias.ps1 MUST refuse any alias but letters, digits, '.', '_' and '-' that
rem does not start with '-'. This script does not defend against a hostile line: the outer
rem cmd.exe parses the whole line before this script runs, so whoever writes the line already
rem runs commands. agentspace.ps1 checks the alias exactly before any ssh runs.
setlocal
if not [%2]==[] goto toomany
set "PATH=C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;%PATH%"
if not [%1]==[] goto alias
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0agentspace.ps1" up 1>&2
goto tunnel
:alias
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0agentspace.ps1" up -Alias "%~1" 1>&2
:tunnel
if errorlevel 1 exit /b 1
set /p ID=<"%~dp0..\.instance-id"
"C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session --region us-west-2 --target %ID% --document-name AWS-StartSSHSession --parameters portNumber=22
exit /b %ERRORLEVEL%

:toomany
echo agentspace: ssh-proxy.cmd takes at most one argument, the SSH host alias. 1>&2
exit /b 1
