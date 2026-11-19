#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The defaults for the conformance variables (docs/conformance.md, "Naming the program under test").
# This file and the shims beside it are the only files in conformance/ that name the prototype.
# The harness (conformance/lease-cli/harness.ps1) reads it, and only for a variable that is unset or
# empty. Each value is a path, relative to this folder, to the program the variable names.
# AGENTSPACE_CMD and AGENTSPACE_PEER_CMD are one role, so they MAY name one program, as they do here.
# A story that adds a default for another variable MUST add its line here, and nowhere else.
@{
  AGENTSPACE_CMD          = '..\..\bin\agentspace.cmd'
  AGENTSPACE_PEER_CMD     = '..\..\bin\agentspace.cmd'
  AGENTSPACE_OPERATOR_CMD = 'operator.cmd'
  AGENTSPACE_KEEPER_CMD   = 'keeper.cmd'
  AGENTSPACE_SSH_PROXY_CMD = 'ssh-proxy.cmd'
}
