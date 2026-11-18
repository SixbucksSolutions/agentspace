# The defaults for the program variables that the keeper checks read. See docs/conformance.md, "Naming the program under test".
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A check MUST read AGENTSPACE_KEEPER_CMD and AGENTSPACE_SSH_PROXY_CMD from the environment first.
# Only when one is unset or empty does a check dot-source this file and call Get-PrototypeProcessDefault,
# so that this folder is the one place that names the prototype's programs. The rewrite MUST NOT read it.

function Get-PrototypeProcessDefault([string]$Variable) {
  switch ($Variable) {
    'AGENTSPACE_KEEPER_CMD' { return (Join-Path $PSScriptRoot 'keeper.cmd') }
    'AGENTSPACE_SSH_PROXY_CMD' { return (Join-Path $PSScriptRoot 'ssh-proxy.cmd') }
    default { return $null }
  }
}
