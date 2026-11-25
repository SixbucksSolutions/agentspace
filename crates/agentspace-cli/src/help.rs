//! The seven help texts, byte for byte as docs/lease-api.md prints them, each line ending in LF.
//! A change to either MUST change both; the tests read the doc and compare.

pub const TOP: &str = r##"Usage: agentspace <command> [<subcommand>] [<args>] [--help]

Leases compute for coding agents.
A lease is compute for one agent for a bounded time, reached over SSH.

Commands:
  lease       Create, show, renew, release and list leases

Options:
  -h, --help  Print this help and exit

Every command prints one JSON document on stdout, or an error document on stderr.
Run 'agentspace lease --help' for the lease subcommands, states and exit codes.
"##;

pub const LEASE: &str = r##"Usage: agentspace lease <subcommand> [<args>] [--help]

Manages leases. A lease is compute for one agent for a bounded time.
A ready lease carries an SSH host alias, the user it logs in as, and an expiry.

Subcommands:
  create   Request a lease for a project
  get      Show one lease
  renew    Issue a ready lease a new certificate with a new expiry
  release  Give a lease back and delete its local credential and alias
  list     Show leases, optionally filtered by project and state

Options:
  -h, --help  Print this help and exit

States:
  pending        Accepted and not yet ready; poll with get
  ready          Connect with 'ssh <host alias>' until the expiry
  expired        The expiry has passed; new connections are refused
  released       Given back; its local credential and alias are gone
  unsatisfiable  Can never be met as asked; the lease's reasons say why

Exit codes:
  0  Success; stdout holds one JSON document
  1  Internal error
  2  Usage error, or a value that is malformed or out of range
  3  No lease has that ID
  4  The lease's state does not allow the subcommand
"##;

pub const CREATE: &str = r##"Usage: agentspace lease create --project <name> [--platform <os/arch>] [--profile <name>]
                               [--size <size> | --plan <file>] [--ttl <duration>]

Requests a lease for one agent of a project, and prints it as JSON.
Check the lease's state before connecting: ready, pending or unsatisfiable.
A ready lease's host alias is in your OpenSSH configuration: ssh <host alias>.
A pending lease becomes ready or unsatisfiable; poll it with get.
An unsatisfiable lease lists its reasons. Nothing was issued, and the exit code is still 0.

Options:
  --project <name>      Required. Letters, digits and '-', up to 32; lowercased.
  --platform <os/arch>  Default linux/arm64. Also accepts aarch64 for arm64.
  --profile <name>      Default agent-linux. Accepts name or name@version.
  --size <size>         Default m. One of s, m, l or xl: at least 2, 4, 8 or 16 vCPUs,
                        with 4 GiB of memory for each.
  --plan <file>         Size the lease for a plan: a JSON file of turns, stories and
                        subagents. Picks the smallest size that holds its busiest turn.
                        Not with --size.
  --ttl <duration>      Default 8h. From 1m to 24h, never clamped.
                        Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
                        A bare number is refused: give a unit.
  -h, --help            Print this help and exit

In v1 only linux/arm64, agent-linux and the sizes s, m, l and xl can be met.
Any other well-formed value gives an unsatisfiable lease, never a substitute.
So does a plan whose busiest turn no size holds.
"##;

pub const GET: &str = r##"Usage: agentspace lease get <id>

Prints one lease as JSON, as it stands now. Changes nothing.
A ready lease whose expiry has passed is shown as expired.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
"##;

pub const RENEW: &str = r##"Usage: agentspace lease renew <id> [--ttl <duration>]

Issues a ready lease a new certificate, valid from now for the TTL, and prints the lease.
The key, host alias and user stay the same.
The old certificate is not revoked. It stays valid until its own expiry.
If this renew shortens the lease, that expiry is later than the new one.
Only a ready lease can be renewed. An expired lease cannot: create a new one.

Arguments:
  <id>              The lease ID, a UUIDv4 in any case

Options:
  --ttl <duration>  Default: the lease's current TTL. From 1m to 24h, never clamped.
                    Examples: 8h, 90m, 1h30m, '2 hours', PT8H.
  -h, --help        Print this help and exit

Exit 3 means no lease has that ID.
Exit 4 means the lease is not ready; the error names its state.
"##;

pub const RELEASE: &str = r##"Usage: agentspace lease release <id>

Gives a lease back, and prints it as JSON.
Ends this computer's ssh connections to the lease, then deletes the lease's local key,
certificate and host alias. A pending lease has no connections to end.
No certificate is revoked. Each stays valid until its own expiry.
An older certificate's expiry is later than the lease's if a renew shortened it.
Connections made from other computers are not ended.
Releasing a released or unsatisfiable lease again keeps its record and exits 0.

Arguments:
  <id>        The lease ID, a UUIDv4 in any case

Options:
  -h, --help  Print this help and exit

Exit 3 means no lease has that ID.
"##;

pub const LIST: &str = r##"Usage: agentspace lease list [--project <name>] [--state <state>[,<state>...]]...

Prints the leases that match every filter, oldest first, as {"leases":[...]}.
Changes nothing. No match prints an empty list and exits 0.

Options:
  --project <name>  Only this project's leases
  --state <state>   Only leases in these states. Repeat it, or separate states with commas.
                    States: pending, ready, expired, released, unsatisfiable.
  -h, --help        Print this help and exit
"##;
