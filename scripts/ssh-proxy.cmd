@echo off
rem SSH ProxyCommand for agentspace: starts the instance if it is stopped, then
rem tunnels SSH over Session Manager. Nothing but the tunnel may write to stdout.
rem
rem RFC 2119 keywords in these comments; the capitals are load-bearing.
rem
rem Usage: ssh-proxy.cmd [<alias>]. With an alias, it runs agentspace.ps1 up -Alias <alias>, so
rem the localswim token push rides the SSH host alias the caller connected with (D9). Without
rem one, it runs up as it always has, and the push rides agentspace, Terry's own entry. The alias
rem MUST be letters, digits, '.', '_' and '-' only, and MUST NOT start with '-'. Anything else,
rem an empty argument or a second one fails before anything is started.
rem
rem Cmd expands each argument before it parses the line holding it, so an argument with an
rem ampersand, a pipe, a caret, an angle bracket or a quote in it could run as a command. The
rem arguments MUST NOT be expanded into a command line before they are checked. With echo on,
rem cmd prints a rem line holding them, which it never parses, to a file in a folder of this
rem run's own; they are read back from it and checked with delayed expansion, which cmd applies
rem after parsing.
setlocal EnableExtensions DisableDelayedExpansion
set "PROXY_ALIAS="
set "TRIES=0"
:workdir
set /a TRIES+=1
if %TRIES% gtr 20 goto noworkdir
rem The md fails if the folder exists, so two connections started together never share one.
set "WORK=%TEMP%\agentspace-proxy-%RANDOM%%RANDOM%"
md "%WORK%" 2>nul || goto workdir
setlocal
set "PROMPT=@"
(
  echo on
  for %%a in (1) do (
    rem #%*#
  )
  @echo off
) > "%WORK%\args.txt"
endlocal
set "LINE="
for /f "usebackq delims=" %%L in ("%WORK%\args.txt") do set "LINE=%%L"
rd /s /q "%WORK%" 2>nul
setlocal EnableDelayedExpansion
if not defined LINE goto badread
if not "!LINE:~0,7!"=="@(rem #" goto badread
if not "!LINE:~-4!"=="# ) " goto badread
set "ARGS=!LINE:~7,-4!"
if not defined ARGS goto checked
rem Removing every allowed character leaves nothing of a valid alias. The replacement ignores
rem case, so the lowercase letters cover the capitals too; agentspace.ps1 checks again exactly.
set "REST=!ARGS!"
for %%C in (a b c d e f g h i j k l m n o p q r s t u v w x y z 0 1 2 3 4 5 6 7 8 9 . _ -) do (
  if defined REST set "REST=!REST:%%C=!"
)
if defined REST goto badalias
if "!ARGS:~0,1!"=="-" goto badalias
:checked
rem ARGS now holds allowed characters alone, so expanding it on a command line is safe.
endlocal & set "PROXY_ALIAS=%ARGS%"
set "UP=up"
if defined PROXY_ALIAS set "UP=up -Alias %PROXY_ALIAS%"
set "PATH=C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;%PATH%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0agentspace.ps1" %UP% 1>&2
if errorlevel 1 exit /b 1
set /p ID=<"%~dp0..\.instance-id"
"C:\Program Files\Amazon\AWSCLIV2\aws.exe" ssm start-session --region us-west-2 --target %ID% --document-name AWS-StartSSHSession --parameters portNumber=22
exit /b

:badalias
echo agentspace: ssh-proxy.cmd takes at most one argument, an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'. 1>&2
exit /b 1

:badread
echo agentspace: ssh-proxy.cmd could not read back its own arguments. 1>&2
exit /b 1

:noworkdir
echo agentspace: ssh-proxy.cmd could not make a folder of its own under %%TEMP%%. 1>&2
exit /b 1
