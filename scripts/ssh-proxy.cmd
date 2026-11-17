@echo off
rem SSH ProxyCommand for agentspace: starts the instance if it is stopped, then
rem tunnels SSH over Session Manager. Nothing but the tunnel may write to stdout.
setlocal
set "PATH=C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;%PATH%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0agentspace.ps1" up 1>&2
if errorlevel 1 exit /b 1
set /p ID=<"%~dp0..\.instance-id"
"C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session --region us-west-2 --target %ID% --document-name AWS-StartSSHSession --parameters portNumber=22
