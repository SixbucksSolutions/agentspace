#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The default program for each conformance variable (docs/conformance.md, "Naming the program under
# test"). A variable that is unset or empty takes its default from this file, and from nowhere else;
# a variable that is set always wins. The harness runs this file and reads the hashtable it returns.
#
# Each default is the full path of a program in the install folder, %USERPROFILE%\.agentspace\bin
# (docs/operator.md, "Programs and install layout"; D37).
#
# The install folder MUST be resolved from the real profile folder, and never from the USERPROFILE
# variable: the harness points USERPROFILE at a throwaway home for every program it starts, and a
# check or a suite MAY already have done so in its own process. The profile folder is asked of
# Windows, which answers from the account and not from the environment, so the answer is the same
# whichever order a check happens to call things in. This file reads and lists nothing in the folder
# it names; it only builds paths.

$realProfile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
if ([string]::IsNullOrEmpty($realProfile)) { return @{} }
$installFolder = Join-Path $realProfile '.agentspace\bin'

@{
  AGENTSPACE_CMD            = Join-Path $installFolder 'agentspace.exe'
  AGENTSPACE_OPERATOR_CMD   = Join-Path $installFolder 'agentspace-operator.exe'
  AGENTSPACE_KEEPER_CMD     = Join-Path $installFolder 'agentspace-keeper.exe'
  AGENTSPACE_SSH_PROXY_CMD  = Join-Path $installFolder 'agentspace-ssh-proxy.exe'
}
