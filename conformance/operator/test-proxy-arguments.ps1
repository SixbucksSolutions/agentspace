# Usage-error conformance for the ProxyCommand's argument handling (docs/operator.md, "The ProxyCommand").
# Runs the program named by AGENTSPACE_SSH_PROXY_CMD. With the variable unset, the default is
# conformance/prototype/ssh-proxy.cmd when that file exists; otherwise every case is SKIPPED.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'helpers.ps1')

$proxy = $env:AGENTSPACE_SSH_PROXY_CMD
if (-not $proxy) {
    $default = Join-Path $PSScriptRoot '..\prototype\ssh-proxy.cmd'
    if (Test-Path -LiteralPath $default) { $proxy = [IO.Path]::GetFullPath($default) }
}

$secondText = 'agentspace: ssh-proxy.cmd takes at most one argument, the SSH host alias.'
$aliasText = 'agentspace: -Alias MUST be an SSH host alias'

# Arguments are written as cmd.exe receives them. An unquoted a;b is split by cmd into two
# arguments, which is the second-argument case the prototype's test also uses.
$cases = @(
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd a/b extra refused before up'; Arguments = 'a/b extra'; Text = $secondText }
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd a;b (two arguments) refused before up'; Arguments = 'a;b'; Text = $secondText }
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd -Fx fails in up on the alias, starting no tunnel'; Arguments = '-Fx'; Text = $aliasText }
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd "" fails in up on the alias, starting no tunnel'; Arguments = '""'; Text = $aliasText }
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd "a b" fails in up on the alias, starting no tunnel'; Arguments = '"a b"'; Text = $aliasText }
    [pscustomobject]@{ Name = 'operator.md#the-proxycommand: ssh-proxy.cmd a/b fails in up on the alias, starting no tunnel'; Arguments = 'a/b'; Text = $aliasText }
)

$iso = New-IsolatedHome
try {
    if (-not $proxy) {
        foreach ($case in $cases) {
            Write-CheckResult -Status SKIPPED -Name $case.Name -Reason 'AGENTSPACE_SSH_PROXY_CMD is not set, and conformance/prototype/ssh-proxy.cmd does not exist'
        }
    }
    elseif (-not (Test-AwsRefused -Iso $iso)) {
        foreach ($case in $cases) {
            Write-CheckResult -Status SKIPPED -Name $case.Name -Reason 'aws sts get-caller-identity succeeded under the isolated environment, so nothing was run'
        }
    }
    else {
        foreach ($case in $cases) {
            # Exit 1 for both groups: the second-argument refusal, and a failed up, which ends
            # the ProxyCommand before its tunnel step (operator.md, step 4).
            Invoke-RefusalCase -Iso $iso -Program $proxy -Arguments $case.Arguments -ExpectCode 1 -ExpectText $case.Text -Name $case.Name
        }
    }
}
finally {
    Remove-IsolatedHome -Iso $iso
}
Exit-CheckRun
