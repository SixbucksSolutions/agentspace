//! The help texts, byte for byte from docs/operator.md (Command line; each command's "Help" block).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each text MUST equal its block in docs/operator.md, with a line feed after every line, the last
//! included and no carriage return (gap FA-13). A change to either MUST change both.

pub(crate) const TOP: &str = r##"Usage: agentspace-operator <command> [-GiB <n>] [-Alias <alias>] [-Quiet] [--help]

Manages the agentspace box from the laptop: its stack, its power, its system
files, the SSH CA it trusts, and the localswim Bot token it uses.

Commands:
  deploy  Create or update the stack, then run sync
  up      Start the box if needed, wait until it has patched, push the Bot token
  sync    Check the CA, install the box's system files, CLAUDE.md and the
          localswim CLI, and push the Bot token
  ca      Create the agentspace SSH CA once; later, check it and print it
  down    Stop the box
  status  Show the box's state, type, disk and Session Manager reachability
  grow    Grow the box's disk by -GiB (default 2), and its filesystem with it

Parameters:
  -GiB <n>        For grow: the GiB to add (default 2)
  -Alias <alias>  For up: the SSH host alias the token push uses (default agentspace)
  -Quiet          Print no progress or warnings; failures and status still print
  -h, --help      Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync and grow pass other
programs' output through. up writes nothing to stdout, so it is safe in an SSH
ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
"##;

pub(crate) const DEPLOY: &str = r##"Usage: agentspace-operator deploy [-Quiet]

Creates or updates the CloudFormation stack 'agentbox' in us-west-2 from
its template, with the parameters in ~/.agentspace/stack-parameters.json
and your ~/.ssh/id_ed25519.pub, then runs sync.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const UP: &str = r##"Usage: agentspace-operator up [-Alias <alias>] [-Quiet]

Starts the box if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send.

  -Alias <alias>  The SSH host alias the token push connects with
                  (default agentspace). Letters, digits, '.', '_' and '-',
                  not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the box is ready, 1 failure.
"##;

pub(crate) const SYNC: &str = r##"Usage: agentspace-operator sync [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public key
on the box, copies ~/.claude/CLAUDE.md there, builds and installs the localswim
CLI from C:\Projects\localswim, and sends the localswim Bot token and the
laptop's time zone. Starts the box first if it is stopped.

Fails before touching the box if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
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

pub(crate) const DOWN: &str = r##"Usage: agentspace-operator down [-Quiet]

Stops the box. Does not wait for it to stop. Open sessions on it end.
The next ssh agentspace, or any lease connection, starts it again.

Exit codes: 0 the stop was requested, 1 failure.
"##;

pub(crate) const STATUS: &str = r##"Usage: agentspace-operator status [-Quiet]

Shows the instance's ID, state, type, root disk size and volume, and whether
Session Manager can reach it. Never starts the box. Prints to stderr.

Exit codes: 0 success, 1 failure.
"##;

pub(crate) const GROW: &str = r##"Usage: agentspace-operator grow [-GiB <n>] [-Quiet]

Grows the box's root volume by -GiB (default 2) and, if the box is running,
its filesystem online; a stopped box grows its filesystem at next boot.
EBS volumes never shrink, and allow one change every 6 hours.

Exit codes: 0 success, 1 failure.
"##;
