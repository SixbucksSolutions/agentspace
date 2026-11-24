//! The help texts, byte for byte from docs/operator.md (Command line; each command's "Help" block).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each text MUST equal its block in docs/operator.md, with a line feed after every line, the last
//! included and no carriage return (gap FA-13). A change to either MUST change both.

pub(crate) const TOP: &str = r##"Usage: agentspace-operator <command> [-Host <project> | -All] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet] [--help]

Manages agentspace's hosts from the laptop: each host's stack, its power, its
system files and toolchain, the SSH CA they trust, and the localswim Bot token
they use. Without -Host, a command acts on the box, swimcoach's host.

Commands:
  deploy     Create or update a host's stack, then run sync
  up         Start a host if needed, wait until it has patched, push the
             Bot token
  sync       Check the CA, install a host's system files, CLAUDE.md and the
             localswim CLI, and push the Bot token
  bootstrap  Install the agent toolchain on a host; safe to run again
  provision  Make a project's host: deploy, start, sync and bootstrap it
  ca         Create the agentspace SSH CA once; later, check it and print it
  down       Stop a host
  status     Show a host's state, type, disk and Session Manager reachability
  grow       Grow a host's disk by -GiB (default 2), and its filesystem with it
  guard      Stop every running host no live lease explains, after a
             15-minute grace, and send each running host its lease horizon

Parameters:
  -Host <project>  The project whose host to act on (default: the box);
                   provision needs it, and ca and guard take none
  -All             For status, down and sync: every host, one at a time
  -GiB <n>         For grow: the GiB to add (default 2)
  -Alias <alias>   For up: the SSH host alias the token push uses
                   (default: the host's own)
  -InstanceType <type>
                   For deploy: the instance type to give the host
  -Quiet           Print no progress or warnings; failures and status
                   still print
  -h, --help       Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync, bootstrap, provision
and grow pass other programs' output through. up writes nothing to stdout, so
it is safe in an SSH ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
"##;

pub(crate) const DEPLOY: &str = r##"Usage: agentspace-operator deploy [-Host <project>] [-InstanceType <type>] [-Quiet]

Creates or updates a host's CloudFormation stack in us-west-2 from its
template, records the host's instance type in ~/.agentspace/hosts.json, then
runs sync. Without -Host it deploys the box's stack, 'agentbox', with the
parameters in ~/.agentspace/stack-parameters.json, whose InstanceType is
ignored, and your ~/.ssh/id_ed25519.pub. With -Host <project> it deploys that
project's host's stack: when there is none, it creates it with the SubnetId
and VpcId in that file, your public key and the SSH CA's public key; when
there is one, it keeps every parameter but -InstanceType's as it was.

  -Host <project>       The project whose host to deploy (default: the box).
  -InstanceType <type>  The instance type to give the host, the box included,
                        such as m9g.2xlarge. Without it, a host keeps its
                        type, and a new host gets m9g.xlarge.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const UP: &str = r##"Usage: agentspace-operator up [-Host <project>] [-Alias <alias>] [-Quiet]

Starts a host if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send. Without -Host it acts on
the box.

  -Host <project>  The project whose host to start (default: the box).
  -Alias <alias>   The SSH host alias the token push connects with
                   (default: agentspace for the box, agentspace-host.<project>
                   for another host). Letters, digits, '.', '_' and '-',
                   not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the host is ready, 1 failure.
"##;

pub(crate) const SYNC: &str = r##"Usage: agentspace-operator sync [-Host <project> | -All] [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public
key on a host, waits until the host has finished this boot's patch run,
copies ~/.claude/CLAUDE.md there, builds and installs the localswim CLI from
C:\Projects\localswim, and sends the localswim Bot token and the laptop's
time zone. Starts the host first if it is stopped. Without -Host it acts on
the box; -All syncs every ready host, one at a time.

Fails before touching a host if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const BOOTSTRAP: &str = r##"Usage: agentspace-operator bootstrap [-Host <project>] [-Quiet]

Runs the agent toolchain's bootstrap on a host as its login user: gh, Node
through fnm, Claude Code, Codex, uv, build-essential, and Rust through
rustup. Starts the host first if it is stopped. Safe to run again. Without
-Host it acts on the box.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const PROVISION: &str = r##"Usage: agentspace-operator provision -Host <project> [-Quiet]

Makes a project's host, or makes it again: deploys its stack, starts it and
waits, runs sync and bootstrap, records it ready in ~/.agentspace/hosts.json,
and places the project's pending leases. Its deploy gives the host the
instance type the largest of those leases needs, unless a ready lease holds
the host. A project with no host is made only if it is listed in
~/.agentspace/projects.json. One run per host at a time; each run logs to
~/.agentspace/hosts/<project>/provision.log.

Lease commands start a run on their own when a lease needs a host; you can
also run it yourself.

Exit codes: 0 the host is ready, 1 failure.
"##;

pub(crate) const CA: &str = r##"Usage: agentspace-operator ca [-Quiet]

Creates the agentspace SSH CA (ed25519, no passphrase) in
%USERPROFILE%\.agentspace\ca the first time, locked to the current user.
Later runs change nothing: they check the key pair and its access rules, and
print its paths and fingerprint.

A half-made, mismatched or loosened CA fails with what is wrong. ca never
repairs or overwrites a CA. Run it without "Run as administrator".

Exit codes: 0 the CA exists and is sound, 1 failure.
"##;

pub(crate) const DOWN: &str = r##"Usage: agentspace-operator down [-Host <project> | -All] [-Quiet]

Stops a host, or with -All every host. Does not wait for it to stop. Open
sessions on it end. The next connection to it, a lease's or the operator's,
starts it again. Without -Host it stops the box.

Exit codes: 0 the stop was requested, 1 failure.
"##;

pub(crate) const STATUS: &str = r##"Usage: agentspace-operator status [-Host <project> | -All] [-Quiet]

Shows a host's instance ID, state, type, root disk size and volume, and
whether Session Manager can reach it; with -All, each host's, after a line
naming it, then its count of live leases and, for a running host that no
live lease explains, the time the guard will stop it. Never starts a host.
Prints to stderr. Without -Host it shows the box.

With -Host, for a host that is not the box, it then signs a fresh one-hour
certificate, so 'ssh agentspace-host.<project>' signs in for the next hour:
run it before you sign in there, and again after an hour. Sign in under a
live lease of the project, created first, or the guard stops the host.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const GROW: &str = r##"Usage: agentspace-operator grow [-Host <project>] [-GiB <n>] [-Quiet]

Grows a host's root volume by -GiB, a whole number of 1 or more (default 2),
and, if the host is running, its filesystem online; a stopped host grows its
filesystem at next boot. EBS volumes never shrink, and allow one change
every 6 hours. Without -Host it grows the box's.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const GUARD: &str = r##"Usage: agentspace-operator guard [-Quiet]

Stops every running host that no live lease explains, once 15 minutes have
passed since the later of its start and its project's last lease end, and
sends every running host its lease horizon, the end of its project's leases.
Leaves a host whose provisioning run is working on it. Acts on every host in
~/.agentspace/hosts.json, and takes no -Host or -All. Logs each stop, each
unexplained host and each failure to ~/.agentspace/guard.log.

The keeper runs it every 5 minutes; you can also run it yourself. A guard
that finds another one running exits at once.

Exit codes: 0 success, or another guard is running; 1 failure.
"##;
