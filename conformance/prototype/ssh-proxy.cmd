@echo off
rem The default program for AGENTSPACE_SSH_PROXY_CMD: the prototype's ProxyCommand. See docs/conformance.md.
call "%~dp0..\..\scripts\ssh-proxy.cmd" %*
exit /b %ERRORLEVEL%
