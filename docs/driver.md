# agentspace driver boundary, contract v0

RFC 2119 keywords; the capitals are load-bearing.

**The reference for the boundary between the lease CLI and a driver, written so that a second
driver can be built, and the CLI left unchanged, without reading any of agentspace's code
(decisions D20, D34).** It names every operation the CLI asks of a driver, each one's inputs,
results and failures, what the CLI does with each, what a driver MUST NOT let reach the CLI's
output (D1), and how today's EC2 driver meets all of it. The reasoning behind each rule is in
[decisions.md](decisions.md), cited as `D<n>`.

**What a driver is (D6, D9):** the part of agentspace that places leases on one backend and
supplies the way to reach them. The lease CLI never knows the backend (D1), so everything it needs
from one crosses this boundary, and nothing else does. A driver places a lease, renews its
credential and tears it down. It decides whether a request can be met at all (D5), and it owns how
a connection reaches the host (D9).

**What it does not hold (D34).** Each of these facts has its own reference, and this page cites it
rather than restating it:
- **The lease CLI's commands, fields, states, values, messages and exit codes:**
  [docs/lease-api.md](lease-api.md).
- **Every file agentspace writes on the caller's machine**, among them the lease state file, and
  the lease's key, certificate and alias from creation to removal: [docs/state.md](state.md),
  [the lease state file](state.md#the-lease-state-file) and
  [a lease's credential](state.md#a-leases-credential).
- **The operator**, which sets up the CA, starts and stops the box, and runs the ProxyCommand
  that today's driver hands back: [docs/operator.md](operator.md). From v0.07 (#0047) that
  includes `provision`, the run that makes a project's host, whose steps and records are
  [operator.md, provision](operator.md#provision)'s.
- **The host registry, the project list and a host's folder**, which the EC2 driver reads and
  writes from v0.07: [state.md, The host registry](state.md#the-host-registry),
  [The project list](state.md#the-project-list) and [A host's folder](state.md#a-hosts-folder).
  The alias of a lease on a host that is not the box is
  [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)'s.

**Where this page is the owner:** the operations and their rules; the shape of a failure at the
boundary; the rule for what is `unsatisfiable`; what reachability is, and who supplies it; and the
EC2 driver as it stands, among it where it places a lease by its project's host, when it starts a
provisioning run and what it records then, and `place_pending`, which a finished run calls.

**Hardening carried in.** Card #0041, first point (credential errors reach the caller unwrapped),
is carried in here as a requirement on every failure that crosses the boundary
([Failure](#failure), [Hardening carried in](#hardening-carried-in)). The rest of #0041, and
#0029, are [lease-api.md](lease-api.md#hardening-carried-in)'s.

**A requirement the prototype does not meet is written as a requirement, and the prototype's
behavior follows as a note.** Where a note says "not behavior to copy", the rewrite MUST meet the
requirement and MUST NOT repeat the note.

**Status (E0.06-T06-S01, 2026-11-20): the prototype was removed from the repository.** Every note
on this page that names the prototype, and its `scripts/lease-driver.ps1` and
`scripts/lease-driver-ec2.ps1`, is history: it says where a requirement came from. None tells a
reader to run the prototype, and none binds a program that is built now. What binds is the rule
each note stands beside, and the tables and questions below that compare the rewrite with the
prototype stay as the record of the change.

## Contents

- [Conventions](#conventions): [the boundary](#the-boundary), [failure](#failure),
  [what a driver never exposes](#what-a-driver-never-exposes),
  [ordering and concurrency](#ordering-and-concurrency),
  [the driver's name](#the-drivers-name)
- [The operations](#the-operations): [place a lease](#place-a-lease),
  [renew a lease](#renew-a-lease), [release a lease](#release-a-lease),
  [check a pending lease](#check_pending)
- [Unsatisfiable requests](#unsatisfiable-requests)
- [Reachability](#reachability)
- [The EC2 driver](#the-ec2-driver): [where it places a lease](#where-it-places-a-lease),
  [starting a provisioning run](#starting-a-provisioning-run), [`place_pending`](#place_pending),
  [lock order](#lock-order), [conformance cases for v0.07](#conformance-cases-for-v007)
- [Writing another driver](#writing-another-driver)
- [Hardening carried in](#hardening-carried-in)
- [Open questions](#open-questions)

## Conventions

### The boundary

**The boundary is four operations: place a lease, renew a lease, release a lease, and check a
pending lease ([`check_pending`](#check_pending), from v0.07, D48).** The CLI asks nothing else of
a driver. `list` never calls one, `get` calls only `check_pending`, and only for a lease whose
stored state is `pending`, and no command calls one for a lease that is `unsatisfiable`. Which
commands ask a driver for a lease in each state is in [Release a lease](#release-a-lease) and
[`check_pending`](#check_pending).
- **A driver that never answers `pending` needs no `check_pending` of its own:** the CLI calls
  `check_pending` only for a lease that a driver placed `pending`, so it never calls one for such a
  driver, and the boundary's default, which does nothing, is all that driver has. So the four
  operations ask nothing more of such a driver than the three did
  ([Writing another driver](#writing-another-driver)).
- In the prototype, no command reaches a driver for a lease that is `pending`, because the
  prototype's driver never answers `pending`. How a `pending` lease becomes `ready` was
  [Q3](#open-questions)'s, closed for the EC2 driver by D48.

**Each operation is a request that carries named inputs, answered by a result or a failure.** This
page describes them by what they carry and do, never by how they are called:
- **A request** holds the inputs the operation's section lists. Every value in it is already
  normalized and checked by the CLI ([lease-api.md](lease-api.md#values)). A driver MAY check it
  again, and MUST NOT normalize it differently.
- **A result** holds the members the operation's section lists. A driver MUST NOT return a member
  that is not listed. The CLI ignores one, and it is never an error.
- **A failure** is the other possible answer ([Failure](#failure)). An operation answers one or
  the other, never both.

**In the prototype the boundary is a call inside one process.** The CLI and the driver are loaded
into the same program, a request is a set of named arguments, a result is an object, and a failure
is a thrown exception. **The transport is the rewrite's choice**: a call in the same process, a
child process, a socket or a hosted API all meet this page, because no decision fixes it
([Q1](#open-questions)). What this page does fix is each operation's inputs, results, failures,
side effects, ordering and idempotence, which a transport MUST carry unchanged.

**Nothing about the boundary is visible from outside the CLI, except through the CLI.** A driver is
checked by driving the CLI and looking at what it prints, the files it leaves, and what a client of
the lease's alias reaches. Which commands those are is [lease-api.md](lease-api.md)'s, and which
files, [state.md](state.md)'s.

**A driver MUST NOT read the CLI's standard input, write to its standard output or standard
error, or prompt.** Every command runs unattended with no terminal, and its streams belong to the
output rules of [lease-api.md](lease-api.md#output). A program that a driver starts MUST NOT write
to those streams either, and its output MUST be captured for the failure's cause, never shown.
- **The prototype captures the output of signing and of the CA check, but not of making the key or
  of checking its passphrase,** whose error text most likely reaches the CLI's standard error. That
  would break "exactly one error document" ([lease-api.md](lease-api.md#exit-codes-and-errors)),
  and is not behavior to copy. Only a run settles it ([Q17](#open-questions)).

**Where the platform enters.** The request carries the platform, the profile and the size the
caller asked for, and the contract holds no assumption about the host's operating system
(D4). Wherever this page names Linux, an `ubuntu` user, `sshd` or `systemd`, it is describing
today's box in [The EC2 driver](#the-ec2-driver), and is a property of that driver, never of the
contract. A Windows host (v0.12, #0051) is a platform like any other: unsatisfiable until a
driver supports it (D5), and how a lease on one logs in is still open
([Q10](#open-questions)).

- Note on where this came from: the prototype's boundary is `scripts/lease-driver.ps1`, and its one
  driver is `scripts/lease-driver-ec2.ps1`. The first holds the interface and names no backend; the
  second is the only file meant to name one.

### Failure

**A failure has two parts: a message and a cause.**
- **The message** is a few words for a person, in agentspace's own words. It MUST NOT be empty and
  MUST NOT start with a space.
- **The cause** is whatever the driver wants kept for diagnosis: the underlying error, with its
  paths and another program's output. It MAY be absent.

**The CLI turns every failure into the `internal` error (exit 1)**
([lease-api.md](lease-api.md#exit-codes-and-errors)). A driver MUST NOT produce another code:
`not_found`, `invalid_state`, `usage`, `invalid_value` and `out_of_range` are the CLI's, and are
decided before any driver is asked.
- **The CLI MUST print every driver failure as `internal`, whatever the failure holds, and MUST
  NOT look inside the cause for an error of its own** (architect, 2026-11-19, gap FA-22). A driver
  MUST NOT produce a lease error, so a conforming driver never puts this rule to the test.
  - Note on where this came from: the prototype prints a lease error that a driver throws
    directly, or wraps inside a failure, with that error's own code, in the failure's place. No
    driver of the prototype's does either, so nothing a caller has seen differs.

**The CLI puts the failure's message in the error document, and nothing else about it.** The error
document's message is `agentspace lease <sub> failed: ` followed by the failure's message
([lease-api.md](lease-api.md#messages), "Anything else"), where `<sub>` is `create`, `renew` or
`release`. The CLI MUST NOT put the cause, or any form of the failure that includes the cause, in
the document. It escapes control characters in the message as lease-api.md says, and nothing more.
Where lease-api.md's message table gives wording for the failure (a missing CA, an alias that
cannot be set up, a credential or alias that cannot be deleted), that wording is what the driver's
message SHOULD be, without the prefix the CLI adds.

**Every failure's message MUST meet the rules for an `internal` message in
[lease-api.md](lease-api.md#exit-codes-and-errors) (card #0041), and MUST NOT name the driver
([The driver's name](#the-drivers-name)).**

**Every step that can fail MUST be wrapped, not only the one that is easiest to word** (#0041,
first point). That covers each step of issuing, renewing and deleting a credential: the CA check,
making the key, checking that it has no passphrase, signing, moving the new certificate into
place, deleting the files, and every step of the alias. The message is made at the step that
failed, and the underlying error is kept as the cause. The prototype wraps only a failed alias
step, and lets the rest through as they are, so a failed signing reaches the caller with a path on
the laptop and `ssh-keygen`'s own text. That is the defect this requirement closes, and it is never
behavior to copy.

**A failure MUST leave the lease's files as the operation's section says.** A failure that cannot
clean up after itself MUST still report the failure that began it, with its own message, never the
cleanup's error in its place. The prototype lets a cleanup's error replace it in two cases
([Q8](#open-questions)).

**A CLI with no driver MUST fail `create` with `internal`, in words that name no driver.** The
prototype's message, `No lease driver is registered.`, meets that.

### What a driver never exposes

**D1 binds everything that can reach the CLI's output:** the lease document, a reason, an error
message, and the host alias. A driver MUST NOT put any of these in a result or a failure's
message:
- **The driver's own name.** The CLI keeps it, and it MUST NOT reach any output
  ([The driver's name](#the-drivers-name)).
- **The backend's name,** its products, its APIs, a region, an account, an instance type, an
  instance, a volume, or a host's name or address.
- **Which machine serves the lease,** or that it is shared or dedicated (D25).
- **The private key,** or any part of it, and any token.
- **An absolute path on the caller's machine,** or the caller's user name
  ([lease-api.md](lease-api.md#exit-codes-and-errors) says the same of an `internal` message).

**What a driver MAY put in a result is only what lease-api.md says a lease shows**: a host alias,
a login user, an expiry, and reasons ([lease-api.md](lease-api.md#fields)). The host alias names
the lease, never the host behind it.

**What a driver MAY keep to itself, and in the files state.md owns,** is everything else, among
them the transport command that reaches the host ([Reachability](#reachability)). That command
sits in the lease's alias file on the caller's machine. It is not part of any output, and a caller
MUST NOT read it as a contract. `ssh -G <alias>` prints it all the same, which is
[Q15](#open-questions)'s.

### Ordering and concurrency

**The CLI takes the lease state file's lock before it asks a driver anything, and holds it until
it has recorded the outcome** ([state.md](state.md#the-lease-state-file) owns the file, and
[its lock](state.md#concurrency)). So, for one state folder:
- **At most one operation is in flight at a time.** A driver MUST NOT rely on that, because two
  CLI processes with different state folders ([lease-api.md](lease-api.md#environment)) run their
  operations at once, on the same caller's machine and the same host.
- **An operation's time counts against every other changing command's 30-second wait**
  ([lease-api.md](lease-api.md#concurrency-and-atomicity)). A driver SHOULD finish an operation in
  well under 30 seconds, and MUST NOT wait on anything that needs the lock or a person. Whether
  the prototype does is [Q12](#open-questions).
- **A driver that has to start or create compute to place a lease SHOULD start it and answer
  `pending`, instead of waiting inside the operation** ([Place a lease](#place-a-lease)). The EC2
  driver does so from v0.07 for a project whose host is not `ready`: it starts a provisioning run
  and never waits for it ([Starting a provisioning run](#starting-a-provisioning-run)).
- **`get` does not take the lock** ([lease-api.md](lease-api.md#concurrency-and-atomicity)), so
  [`check_pending`](#check_pending) runs outside it, and MUST NOT take it or write the lease state
  file.
- **A process other than the CLI MAY change a lease too:** a finished provisioning run calls
  [`place_pending`](#place_pending), which takes the lease state lock as a changing command does,
  waits for it as one does, and holds it until it has written ([state.md, The lease state
  lock](state.md#the-lease-state-lock)). It takes the lock once per lease it places, so no hold
  spans several placements.

**What the CLI does, in order, around each operation:**

| Command | In the lock | The driver is asked | Then the CLI |
|---|---|---|---|
| `create` | Reads the records; picks a new lease ID whose alias no kept lease uses | To place the lease | Records the lease, validates the records, writes them |
| `renew` | Finds the lease; checks that it is `ready`, so an expired lease is refused before any driver is asked ([lease-api.md](lease-api.md#agentspace-lease-renew)) | To renew the lease's certificate | Records the new `expires_at` and `ttl_seconds`, writes |
| `release` | Finds the lease | To release it, when its stored state is `ready` or `released`, or `pending` with a `driver` (pin P16, architect, 2026-11-20, for v0.07-T01; [Release a lease](#release-a-lease)) | Records `released`, drops `ssh`, sets `released_at`, writes |
| `get` | Nothing: `get` takes no lock. It reads the records and finds the lease | To check it, when its stored state is `pending` ([`check_pending`](#check_pending)) | Prints the lease as it read it, whatever the check did; writes nothing |

**A failure of the driver ends the command at once, with the records untouched**, but for a
failure of `check_pending`, which `get` drops ([`check_pending`](#check_pending)). **A failure in
recording a `pending` lease asks no driver anything:** the lease has no credential or alias, and
what the EC2 driver started stays, its host registry record and any run, which makes the host and
finds no lease of this `create` to place; the host is kept (D25). A failure after
a lease was placed `ready`, in recording it, is different: the CLI then asks the driver to release
the lease it just placed, and reports the first failure, naming in it whatever that release could not
remove ([lease-api.md, Concurrency and atomicity](lease-api.md#concurrency-and-atomicity), whose
wording binds; gap FA-02). So **a driver's release MUST work for a lease that has just been placed and has no
record**, given only the driver's name, the lease ID, the project and the ssh folder override.
A release that fails there does not replace the first failure; the files it could not remove stay,
and the first failure's message names them ([Q8](#open-questions), closed).

**A renew or a release whose driver step succeeded, and whose recording then failed, is a failed
renew or release** ([lease-api.md](lease-api.md#concurrency-and-atomicity), #0041). The prototype
leaves the new certificate in place after such a renew, and deletes the credential before such a
release. The boundary MUST let the CLI meet lease-api.md's rule, which needs either a way to undo
a driver's step, or recording before the step takes effect. Which is the rewrite's design
([Q4](#open-questions)); this page requires the outcome.

### The driver's name

**A driver has a name, which is an opaque string.** The CLI stores it with every lease it places,
hands it back on renew and release, and never compares it with a literal or prints it. It MUST NOT
reach any output ([state.md](state.md#the-lease-state-file) holds the field). So the CLI cannot
tell one driver from another, which is D1.
- **A name MUST be non-empty, MUST NOT contain a control character** (U+0000 to U+001F, or
  U+007F), and MUST stay the same for as long as any lease it placed is kept, so that renew and
  release find their driver.
- **A name SHOULD NOT name its backend (D1).** The CLI writes it into a file on the caller's
  machine, the lease state file, though never into an output. The prototype's name does name its
  backend. Whether D1 reaches such files is [Q15](#open-questions)'s.
- **The CLI MUST record the name with every lease that is ever ready,** and a `ready` or
  `released` record that lacks one is read as the EC2 driver's in contract v0 (#0041, second
  point; the rules for writing and reading the field are
  [state.md](state.md#the-lease-state-file)'s). The prototype's schema leaves the field optional,
  and its CLI fails to renew or release a `ready` record without it, which is not behavior to
  copy.
- **A name the CLI does not know,** at renew or release, is a failure, and the records stay as
  they were. Its message MUST NOT name the driver. The prototype's message does, `No lease driver
  is registered as '<name>'.`, which breaks D1 and is never behavior to copy. The wording is
  otherwise free, as long as it says in agentspace's own words that the lease was placed by a
  driver this agentspace does not have.

**Which driver is asked to place a lease is not fixed beyond one.** The prototype asks the first
driver it registered, and there is one. How a CLI with several drivers chooses, and what a request
no single driver can meet becomes, are [Q2](#open-questions)'s.

**How a driver announces itself to the CLI is the rewrite's choice.** In the prototype each driver
registers its name and its three operations as it loads, and a second registration under the same
name replaces the first. The CLI, not the driver, puts the registered name in a place result: a
driver MUST NOT be able to answer with a different one, and the name stays the one it registered
under.

## The operations

| Operation | Called by | Asked of the driver for | Called only when |
|---|---|---|---|
| [Place a lease](#place-a-lease) | `create` | A lease ID, project, platform, profile, size and TTL | The CLI has chosen an ID whose alias no kept lease uses |
| [Renew a lease](#renew-a-lease) | `renew` | A lease's new certificate | The lease is `ready` |
| [Release a lease](#release-a-lease) | `release`, and `create` when it fails after a `ready` place | A lease's local credential and alias, gone | The lease's stored state is `ready` or `released`, or it has just been placed |
| [Check a pending lease](#check_pending) | `get` | Nothing it returns: the driver carries on with the lease's placement, if it needs to | The lease's stored state is `pending` |

### Place a lease

**Places one lease: decides whether the request can be met, and if it can, issues the lease's
credential, makes the lease reachable, and says how.**

**The request:**

| Input | Value | Notes |
|---|---|---|
| Lease ID | A lowercase UUIDv4, normalized | New. No record of it exists yet, and no kept lease has its host alias ([lease-api.md](lease-api.md#fields)). |
| Project | A normalized [project](lease-api.md#project) name | The lease's project. The host alias is built from it. |
| Platform | A normalized [platform](lease-api.md#platform) | Any well-formed one, supported or not. |
| Profile | A normalized [profile](lease-api.md#profile) | Any well-formed one, supported or not. |
| Size | A normalized [size](lease-api.md#size) | Any well-formed one, supported or not. |
| TTL | A whole number of seconds, from 60 to 86400 | The CLI has refused anything else. A driver MUST NOT clamp it. |
| OpenSSH client folder | Optional text | The caller's `AGENTSPACE_SSH_DIR` when it is set and not empty, and otherwise absent. The folder whose configuration the alias is added to. For tests and worktrees ([lease-api.md](lease-api.md#environment)). |
| Transport override | Optional text | The caller's `AGENTSPACE_PROXY_COMMAND` when it is set and not empty, and otherwise absent. Replaces the driver's own way to reach the host ([Reachability](#reachability)). For tests and worktrees. |

**The request carries no state folder.** Where the lease's key and certificate go is
[state.md](state.md#a-leases-credential)'s, and the CLI's own state folder
([lease-api.md](lease-api.md#environment)) is the CLI's alone. From v0.07 the driver's context
also holds it, and the EC2 driver uses it only to hand it to a provisioning run it starts, for
[`place_pending`](#place_pending) (pin P12, architect, 2026-11-20, for v0.07-T01).

**The result, always one of these states:**

| State | Other members | Meaning |
|---|---|---|
| `ready` | Host alias, user, expires at | The lease's credential and alias are in place, and a connection through the alias is expected to succeed until the expiry |
| `unsatisfiable` | Reasons, 1 to 4 | The request cannot be met as asked ([Unsatisfiable requests](#unsatisfiable-requests)). Nothing was issued |
| `pending` | None | Accepted, not yet ready. No credential has been issued. The prototype's driver never answers it; the EC2 driver answers it from v0.07 for a project whose host is not `ready` ([Where it places a lease](#where-it-places-a-lease)) |

- **A result's members that do not belong to its state MUST be left out.** A driver MUST NOT
  answer any other state.
- **Host alias:** the lease's alias, as [lease-api.md](lease-api.md#fields) says for
  `ssh.host_alias`. A driver MUST return exactly the alias the lease's files are made for, and
  that is the alias the CLI avoided colliding with. The CLI does not compare the two: it records
  what it is given, after its own check of the record.
- **User:** the user a connection through the alias logs in as. It is the certificate's
  principal ([lease-api.md](lease-api.md#the-credential)), and the alias MUST log in as it. The
  CLI's check of its records is narrower than lease-api.md's rule for `ssh.user`
  ([lease-api.md](lease-api.md#fields)): it is the pattern in
  [`schemas/state/leases.state.schema.json`](../schemas/state/leases.state.schema.json), because
  the record is validated before it is written. **A driver MUST return a user that matches both**,
  or the `create` ends in `internal` ([Q16](#open-questions)).
- **Expires at:** UTC, to the whole second, and MUST equal the certificate's valid-before time,
  which is its issue time plus the TTL exactly ([lease-api.md](lease-api.md#times)). The CLI
  records it, truncated to the second, as the lease's `expires_at`. The certificate's valid-after
  time is back-dated as [lease-api.md](lease-api.md#the-credential) says (D24, #0029): a driver
  SHOULD back-date it, and the back-dating never moves the expiry.
- **Reasons:** each a code and a message, in the order and with the rules of
  [Unsatisfiable requests](#unsatisfiable-requests). The CLI records only those two members.
- **The CLI adds the driver's name to the record** and prints none of it.
- **The result is an alias and a user, never a host or a port.** Where the host is, and how to
  get to it, is in the alias's files and is the driver's ([Reachability](#reachability)); D6's
  "SSH endpoint" is that alias.

**A result that the CLI's own checks of its records refuse is a failed `create`.** The CLI does
not check a `ready` result's members, or a reason's code, itself. Records are validated before
every write ([state.md](state.md#the-lease-state-file)), so a `ready` result with a host alias or
user missing, empty or in the wrong form, a state other than the three above, or an
`unsatisfiable` result with no reason, ends the command with `internal` ("the change is not
valid", [lease-api.md](lease-api.md#messages)), after the CLI has released a `ready` one as below.
A driver MUST NOT depend on that check.
- **A missing expiry fails earlier,** with the prototype's runtime's own parameter error as the
  message, not "the change is not valid". That is not behavior to copy.

**What a `ready` result means for the caller's machine** (the files are
[state.md](state.md#a-leases-credential)'s): the lease has its own new key pair, a certificate for
it signed by agentspace's CA, and an alias that `ssh` finds through the caller's OpenSSH client
configuration. Their exact contents are state.md's and lease-api.md's. The driver MUST NOT make
any other change to that configuration. **A `ready` result never promises that the host is up,**
only that a connection is expected to succeed ([lease-api.md](lease-api.md#states)). Today's
driver makes no connection to the host and starts nothing there; that is a property of today's
driver, and a driver that creates compute for a lease MAY do either (D6).

**Who writes the alias, and the `Include` line.** The contract requires that, when place answers
`ready`, the alias exists and is usable ([lease-api.md](lease-api.md#the-credential)). It does not
say whether the driver or the CLI performs the writing. In the prototype the driver does, through
steps that the driver calls and that hold nothing specific to one backend but the login user, the
transport and the host-key settings ([Q5](#open-questions)).

**The CA is checked, never created.** A driver MUST check that agentspace's CA is usable before it
issues anything, and MUST NOT create it ([lease-api.md](lease-api.md#the-credential), #0029's
third point). The check MUST NOT change anything on the caller's machine.

**What the CLI does with each result:**

| Result | The CLI |
|---|---|
| `ready` | Records the lease `ready` with its expiry, host alias and user, and the driver's name; prints it; exits 0 |
| `unsatisfiable` | Records the lease `unsatisfiable` with its reasons and the driver's name; prints it; exits 0; asks no more of any driver for it |
| `pending` | Records the lease `pending` with the driver's name, and no `expires_at`, `ssh` or `reasons` ([state.md](state.md#the-lease-state-file)); prints it; exits 0. The lease leaves `pending` by the driver's own work, never by the CLI's: for the EC2 driver, a finished provisioning run calls [`place_pending`](#place_pending), and `get` asks [`check_pending`](#check_pending) in the meantime ([Q3](#open-questions), closed for the EC2 driver) |
| A failure | Records nothing, and ends the command with `internal`, exit 1 |

**A failed place MUST remove every file and folder it made, and MUST NOT disturb anything that
was there before it ran. When a removal fails, what it MUST do is
[lease-api.md](lease-api.md#concurrency-and-atomicity)'s** (#0029). A driver that fails after issuing
the credential MUST remove it before it fails, and MUST remove only what it made itself. The table
says what today's driver leaves, which is nothing except in the alias row.

| Failure | When | Left behind |
|---|---|---|
| The CA is missing or unusable | The CA check fails: no CA, a half-made one, or a mismatched or loosened one, by the five checks of [state.md](state.md#its-checks) and no others. The CA folder's place is `ca`'s to check, not the driver's. A driver MUST NOT create the CA ([lease-api.md](lease-api.md#the-credential), #0029). From v0.07 the EC2 driver checks it before a `pending` answer too ([Where it places a lease](#where-it-places-a-lease)) | Nothing; for a `pending` answer, nothing in the host registry either |
| The credential could not be issued | Making the key, checking that it has no passphrase, or signing fails | Nothing from the credential: every file this call made is removed, and the lease's folder too if this call made it. A folder that already existed, and held neither key nor certificate, stays (gap FA-01) |
| The lease ID already has a credential | A key or certificate for this ID exists. A driver MUST refuse, and MUST NOT overwrite or remove it | The existing files, untouched |
| The alias could not be set up | The caller's OpenSSH configuration cannot take it, an alias file for this alias belongs to another lease or to no lease, the transport cannot be found, the transport or a path holds a line break, the alias is not a valid name, or a write fails | The credential is removed, and a foreign file at the alias's place is kept. See below for what the alias step itself can leave |
| The project's host cannot be looked up | From v0.07: the host registry cannot be read or does not validate, the project list, which is read only for a project with no record, cannot be read or does not validate, or the host registry's lock cannot be had within 30 seconds ([Where it places a lease](#where-it-places-a-lease)) | Nothing |
| A provisioning run cannot be started | From v0.07: a run is due and the operator program cannot be found ([Starting a provisioning run](#starting-a-provisioning-run)). A start that fails after the program was found is not a failure of place | Nothing: the host registry is not written |
| Anything else | A file cannot be written | Nothing |

**An alias failure found after the `Include` line was added leaves that line, and it is allowed
to.** A foreign or colliding alias file, or a write that fails, comes after the step that makes
sure of the `Include` line, so it leaves that line (or a configuration holding only it, when there
was none), a dated backup of the configuration when the line was added to an existing one, and the
alias folder. They stay, and a driver MUST NOT remove them
([state.md](state.md#creation-in-order), which settles [Q18](#open-questions)): none of them is a
lease's key, certificate or alias, so they are not what a failed place MUST NOT leave. A failure
that comes earlier, such as a missing transport, a line break or an invalid alias name, leaves
none of these.

**The alias failure's message MUST say what a person has to fix** in agentspace's own words, with
no absolute path ([lease-api.md](lease-api.md#messages) has the wording to follow). The
prototype's message says only that the alias could not be set up, and names the lease ID. That is
a defect already named there, and not behavior to copy. The same holds for a failure to find the
transport: the message says that the lease's way to reach its host cannot be found, and what to do
about it, and carries no path. No wording is fixed for it beyond that rule.

**Idempotence:** none, and none is needed. Place is called once for an ID, and a second call for
the same ID MUST fail without changing the first one's files (the third row above).

### Renew a lease

**Gives a `ready` lease a new certificate, valid from now for the TTL, and nothing else.**

**The request:**

| Input | Value | Notes |
|---|---|---|
| Driver name | The name stored at place | Selects the driver ([The driver's name](#the-drivers-name)). |
| Lease ID | A lowercase UUIDv4, normalized | The lease is `ready` and its `expires_at` has not passed. The CLI has checked both. |
| Project | The project stored at place | |
| TTL | A whole number of seconds, from 60 to 86400 | The CLI's own: the flag, or else the lease's current `ttl_seconds`. A driver MUST NOT clamp it. |

**The result: expires at,** UTC, to the whole second, equal to the new certificate's valid-before
time (its issue time plus the TTL, [lease-api.md](lease-api.md#times)). The CLI sets the
lease's `expires_at` to it and `ttl_seconds` to the TTL it asked for, and prints the lease.

**What a driver does:**
- **It replaces the certificate and nothing else.** The key, the alias, the user and the lease ID
  MUST NOT change, so a client that saved the alias keeps working
  ([lease-api.md](lease-api.md#the-credential)).
- **The new certificate MUST NOT be visible until it is complete,** and the previous one MUST stay
  in place until then. A renew that fails leaves the previous certificate, and nothing else of the
  renew, behind.
- **It checks the CA first,** as place does, and MUST NOT create it. The prototype's check creates
  a CA when there is none, as place's does ([Place a lease](#place-a-lease)); that is not behavior
  to copy.
- **It does not revoke the previous certificate** (D10). That one stays valid until its own
  expiry.
- **It needs the lease's key.** A lease with no key is a failure, in agentspace's own words.
- **It makes no connection to the host,** and reads neither the OpenSSH client folder nor the
  transport override. The CLI passes neither.

**Failures of renew,** each leaving the lease's certificate, expiry and files as they were:

| Failure | When |
|---|---|
| The CA is missing or unusable | The same check as place's |
| The credential could not be renewed | The lease's key is missing, or signing or moving the certificate in place fails |

**Idempotence:** each call signs a new certificate from its own time, so a call repeated at a later
second gives a later expiry, and nothing else differs. A retry is safe
([lease-api.md](lease-api.md#agentspace-lease-renew)).

### Release a lease

**Takes the lease's local key, certificate and alias away, so that no new connection can be made
with them.** A driver MAY tear down compute it made for the lease (D6). Release MUST still remove
the local key, certificate and alias, and MUST succeed, when the host cannot be reached. Today's
driver tells the host nothing.

**The request:**

| Input | Value | Notes |
|---|---|---|
| Driver name | The name stored at place | |
| Lease ID | A lowercase UUIDv4, normalized | |
| Project | The project stored at place | The alias is found from it and the ID. |
| OpenSSH client folder | Optional text | As for place, from the same variable, and absent when it is unset or empty. A check MUST keep the same value from place to release ([lease-api.md](lease-api.md#environment)). |

**The result: nothing.** Today's driver returns a record with absolute paths and flags, which the
CLI discards. A transport that carried it would carry those paths, so a driver MUST return
nothing.

**What a driver does:**
- **It removes the lease's alias first.** If the file at the alias's place belongs to another lease,
  or to no lease, the driver MUST fail, MUST delete nothing else, and MUST NOT delete that file.
  An alias file that is absent is a success, and the driver goes on to the rest.
- **It then removes the lease's key, certificate and anything else it made for the lease,**
  including a leftover temporary file of a renew that was cut short.
- **It does not edit the caller's OpenSSH client configuration.** The line that pulls the alias
  folder in stays ([state.md](state.md#a-leases-credential) says what release leaves).
- **It needs no CA** and runs no CA check, so release works on a machine whose CA is missing.
- **It does not revoke any certificate (D10),** and does not close an open connection.

**A release that fails part way MUST NOT leave a lease `ready` or `expired` with its alias, key or
certificate partly gone** ([lease-api.md](lease-api.md#concurrency-and-atomicity), #0041). The
boundary MUST let the CLI meet that: by the CLI recording `released` before it asks, by a driver
step that removes the three together, or by an undo ([Q4](#open-questions)). The prototype removes
the alias, then the folder, and then the CLI records. A failure to remove the folder, after the
alias went, leaves a `ready` lease with no alias. That is not behavior to copy.

**Idempotence: release MUST be repeatable, and a repeat MUST finish what an earlier one left.**
A lease with nothing left to remove, or one that never existed, is a success and not a failure
([lease-api.md](lease-api.md#agentspace-lease-release)). This is what lets the CLI try again, call
release after a failed `create`, and, as lease-api.md requires, call it again for a `released`
lease to delete what a failed release left. Where the CLI's `release` answers `not_found` for an
unknown lease, that is the CLI's own answer and no driver has been asked.

**When the CLI asks.** A driver is asked to release a lease that has just been placed, or whose
stored state is `ready` (which includes one that has `expired`, since `expired` is never stored),
or is `released`. It is asked for no lease that is `unsatisfiable`:
- **`ready`:** the prototype asks.
- **`released`:** lease-api.md requires `release` to delete whatever a failed release left, so the
  CLI MUST ask the driver again. The prototype does not: it returns a `released` lease unchanged
  and touches no files. That is #0041's defect
  ([lease-api.md](lease-api.md#agentspace-lease-release)), and it stays unmet until the rewrite.
- **`pending`, with a `driver`:** from v0.07 the CLI asks that driver to release it, as it asks for
  a `ready` one (pin P16, architect, 2026-11-20, for v0.07-T01). A `pending` lease has no
  credential or alias by contract, but a placement killed part way can have left that lease's
  files, and lease-api.md's `released` state says its local credential and alias are gone. So the
  driver's release removes the alias file only when its owner mark names this lease, then the
  lease's folder, and a missing file is not a failure; for a lease that has no files it removes
  nothing. It runs under the lease state lock, so it serializes with
  [`place_pending`](#place_pending), which re-reads each record and skips one that is no longer
  `pending`: a lease released first is never placed. **With no home folder, the cleanup is
  skipped and the release still succeeds**, as lease-api.md's exit-code order requires (pin P16,
  architect, 2026-11-20, for v0.07-T01); a later release of the `released` lease, with the home
  set, removes what is left, as for any `released` lease above. The project's host, and a run
  making it, are kept (D25). Leftovers of a lease that is still `pending` are also removed first by
  its next [`place_pending`](#place_pending) (pin P14.5, architect, 2026-11-20, for v0.07-T01).
- **`pending`, with no `driver`:** the CLI records `released` and asks no driver.
- **`unsatisfiable`:** nothing was issued, so nothing is asked.

**Failures of release,** each leaving the lease as the failed step found it, so that a retry
finishes the job:

| Failure | When |
|---|---|
| The alias is another lease's | The file at the alias's place does not name this lease. Nothing is deleted |
| The credential or alias could not be deleted | A file cannot be removed |

A failure's message SHOULD be the wording lease-api.md's table gives for release, and MUST be in
agentspace's own words and MUST NOT carry an absolute path. The prototype lets the underlying
error through, path and all ([Failure](#failure)).

### check_pending

**Checks a pending lease: it lets a driver carry on with a placement it answered `pending`, each
time a caller polls the lease with `get` (D48; card #0143).** It changes no lease, and nothing
`get` prints. It is new in v0.07; the code's boundary has it with a default that does nothing
(D55), which is all a driver that never answers `pending` needs.

**The request:**

| Input | Value | Notes |
|---|---|---|
| Driver name | The name stored at place | Selects the driver ([The driver's name](#the-drivers-name)). |
| Lease ID | A lowercase UUIDv4, normalized | The lease's stored state is `pending`. |
| Project | The project stored at place | |
| OpenSSH client folder | Optional text | As for place, from the same variable. Whatever the driver starts to place the lease later uses it ([Starting a provisioning run](#starting-a-provisioning-run)). |
| Transport override | Optional text | As for place, from the same variable, and for the same reason. |

**The result: nothing.** A failure is the other answer, and **the CLI drops it: `get` prints the
lease as it read it, exits 0, and prints nothing of the failure** (architect's to confirm). So
`check_pending` never changes `get`'s output or its exit code, and a `get` that would have
succeeded without it succeeds with it.

**What a driver MUST and MUST NOT do in it:**
- **It MUST NOT write the lease state file or take the lease state lock**, and MUST NOT issue a
  credential or write an alias. A lease leaves `pending` only through the driver's own placement,
  under the lock ([`place_pending`](#place_pending) for the EC2 driver), so `get` stays read-only
  for leases ([lease-api.md](lease-api.md#agentspace-lease-get); state.md: `get` MUST NOT write the
  lease state file).
- **It MUST NOT wait for the placement to finish, or for a lock that a command changing leases
  holds** ([lease-api.md](lease-api.md#concurrency-and-atomicity): `get` MUST NOT wait for one).
  **It MUST end within 20 seconds** (pin P12, architect, 2026-11-20, for v0.07-T01), started work
  or not, since a caller polls `get`. The EC2 driver tries `hosts.lock` without waiting, and its
  start of a run is one call that never waits for the run
  ([Starting a provisioning run](#starting-a-provisioning-run)).
- **When the start itself has not returned within the 20 seconds, `get` returns at the bound
  regardless**, prints the lease as it read it and exits 0, and uses nothing the start returns
  later. The start counts as made: the record's `last_attempt_at` was written before it, so the
  5-minute rule holds, and no other `get` starts a second run in that time (architect's to
  confirm).
- **It MUST NOT read the CLI's standard input, write to its standard output or standard error, or
  prompt** ([The boundary](#the-boundary)), and what it starts MUST NOT write to them either.
- **It MAY start work that outlives `get`**, which is how the EC2 driver carries on making a host,
  or placing a lease on a host that is already `ready`
  ([Starting a provisioning run](#starting-a-provisioning-run)).
- **Idempotence:** any number of calls is safe. A driver bounds how often a call starts work; the
  EC2 driver starts a run at most once every 5 minutes for a host.

**When the CLI asks:** once per `get` of a lease whose stored state is `pending`, after it has read
the records and before it prints. `list` never asks, and neither do `create`, `renew` and
`release`. **A `pending` record with no `driver`, or one whose `driver` names a driver the CLI does
not have, is asked of no driver**, and `get` prints it as always (architect's to confirm). Every
record a v0.07 `create` writes carries `driver` ([state.md](state.md#the-lease-state-file)). So
`get` of such a record starts nothing.

## Unsatisfiable requests

**A request the driver cannot meet MUST end `unsatisfiable`, with a reason for every value it
cannot meet, and never be met by something else (D5).**
- **The driver decides it, in place, before it issues or changes anything.** The CLI checks only a
  value's form, so a well-formed platform, profile or size is always passed on. The result is a
  state and not a failure, and the CLI exits 0 with it
  ([lease-api.md](lease-api.md#agentspace-lease-create)).
- **Every reason that applies MUST be listed, each code once, in the order and with the wording
  rules of [lease-api.md](lease-api.md#fields).** A reason's message quotes the normalized value
  that cannot be met, and names no backend (D1).
- **`no_capacity`** is for a request that is supported, but for which no capacity can be found.
  The prototype's driver never has a reason to answer it. The EC2 driver answers it from v0.07 for
  a project that has no host and is not on the project list (D54;
  [Where it places a lease](#where-it-places-a-lease)). It applies only to a supported request, so
  it is never listed beside an `unsupported_*` reason.
- **A driver MUST NOT swap one value for another,** whether it is the platform, the profile or
  the size, however close a match it has.
- **An unsatisfiable lease needs nothing from any driver afterwards:** no credential, no alias and
  no host were touched, so `release` of it is the CLI's no-op.
- **A request no driver can meet is `unsatisfiable`** (D5: "no backend can meet"). With one
  driver, its reasons are the lease's. With several, how reasons combine is
  [Q2](#open-questions)'s.

**What a driver supports is its own,** and the CLI never reads it. The EC2 driver supports exactly
one value of each ([The EC2 driver](#the-ec2-driver)). The contract never says which values a
driver supports, so a later driver adds platforms, profiles and sizes without a new contract
version ([lease-api.md](lease-api.md#versioning)).

## Reachability

**How a connection through a lease's alias reaches the lease's host is the driver's (D9).** The
CLI and its caller hold no part of it.

**What the contract requires of every driver:**
- **After a `ready` place, `ssh <host alias>` MUST reach the lease's host and log in as the
  returned user, with the lease's own key and certificate,** using only the caller's OpenSSH client
  configuration ([lease-api.md](lease-api.md#the-credential)). A client that reads that
  configuration and nothing else, such as the Claude desktop app, MUST reach it too.
- **The way to reach the host MUST come from the driver that placed the lease (D9).** Different
  drivers bring different paths: a tunnel through a cloud's management channel, a direct route, a
  bastion. The CLI MUST NOT contain any of them.
- **The way to reach the host MAY start the host.** A first connection after a quiet spell MAY take
  minutes, and a caller allows for it ([lease-api.md](lease-api.md#states)). Place MUST NOT wait
  for a host to start or come up ([Ordering and concurrency](#ordering-and-concurrency)). A
  driver that has to start or create compute SHOULD start it and answer `pending`.
- **The way to reach the host MUST work from a process outside the caller's shell,** which has only
  the OpenSSH client configuration, and MUST NOT depend on the caller's current folder or on
  variables set in an interactive session.
- **The way to reach the host SHOULD be given the alias's own name,** written out literally, so
  that anything the transport does for the connection that is per lease rides the lease's
  credential and not the operator's own (D9).
- **A transport that is a command, which ssh runs in place of a TCP connection, MUST keep the
  SSH connection's stream clean:** nothing but the tunnel writes to its standard output, and its
  messages go to standard error. Today's is [operator.md](operator.md#the-proxycommand)'s.
- **What the transport prints while a lease connects MUST NOT name the backend**
  ([D1](decisions.md#d1-clients-never-know-the-backend), which holds at connect time too; card
  #0087). Its standard error reaches the caller's `ssh`, and so the caller.
  - **On success** it MUST NOT print an instance or host ID, a region, a machine type, a cloud
    state's name or a cloud tool's text. It MAY print a progress line, in agentspace's own words,
    saying that the lease's host is starting or ready.
  - **On failure** it prints agentspace's own message, which MAY name the lease and what the caller
    can do, and never the backend.
  - **The backend's own detail MAY be printed only when the caller asks for it,** through a
    diagnostic setting that [operator.md](operator.md) names, for diagnosis.
  - Note on where this came from: today's transport does not meet this. The operator's `up` prints
    the instance's ID and EC2's states, the Session Manager's and the patch check's messages, the
    AWS sign-in message and the AWS CLI's own error text ([operator.md](operator.md#messages)).
    Card #0087 carries it, met by v0.06 (#0055). It is not behavior to copy.

**The transport override** is the means by which a check, or a worktree, supplies a transport
instead of the driver's own. When the CLI hands a driver a non-empty transport override at place,
the driver MUST use it in the alias in place of its own, and MUST NOT look for or need what its
own transport needs, such as its program. A driver MUST refuse an override that holds a line
break. How the text is written into the alias, among it how a percent sign is escaped, is
[state.md](state.md#the-alias-file)'s ([Q6](#open-questions), closed). The override is not needed
to create a lease without the default transport's cache: place never reads that cache
([How it is reached](#how-it-is-reached)).

**What is not the driver's:** the alias's other settings, which are the same for every driver
today, among them the lease's key and certificate files, the board's forwards (D12, D30) and the
keep-alive settings. They are [state.md](state.md#a-leases-credential)'s, and
[lease-api.md](lease-api.md#the-credential) says what a caller may rely on. Whether the host key
check belongs to the driver is [Q7](#open-questions)'s.

## The EC2 driver

**The one driver v0 has: it places each project's leases on that project's host, an EC2 instance**
(D25, D43). Until v0.07 (#0047) every lease went to the one shared box; from v0.07 the box is
swimcoach's host, and each other project gets a host of its own at its first lease, made by a
provisioning run the driver starts (D48). It is the only part meant to know the backend, and what
it names stays in the files state.md owns and in the operator's messages, never in the CLI's
output. Today the shared credential and alias steps also hold its user, transport and host-key
settings ([Q5](#open-questions), [Q7](#open-questions)).

**What the marks below mean.** Where D43 to D55 and card #0143 left a fact open, the dev lead of
v0.07-T01 pinned it, so that the four specs written in that turn agree (D55):
- **"Architect, 2026-11-20, for v0.07-T01"** marks a pin the architect has ruled on. It binds as
  written.
- **"Architect's to confirm"** marks this page's writer's choice, where no pin settles the fact.
  It is sent to the architect, and binds as written until the architect rules.

### What it supports

**Exactly three values, one of each (D5):**

| Request | Supported | Anything else |
|---|---|---|
| Platform | `linux/arm64` | `unsupported_platform` |
| Profile | `agent-linux`, with no version | `unsupported_profile`; `agent-linux@1` is one |
| Size | `m` | `unsupported_size` |

**Matching is exact on the normalized values.** `windows/amd64`, `linux/amd64`, and any
profile with a version are `unsatisfiable`. The reasons use lease-api.md's three messages. Until
v0.07 the driver never answered `no_capacity` or `pending`; from v0.07 it answers each for a
supported request, by its project's host ([Where it places a lease](#where-it-places-a-lease)).

### Where it places a lease

**A supported request is placed by its project's host, found in the host registry
([state.md, The host registry](state.md#the-host-registry)) by its `project`, compared exactly**
(both are normalized; D45, D48; pin P1, architect, 2026-11-20, for v0.07-T01). A missing
`hosts.json` reads as one record, the box's: project `swimcoach`, stack `agentbox`, state `ready`
(D45). The box is the record whose stack is `agentbox`. The project list is
[state.md, The project list](state.md#the-project-list), and a missing `projects.json` lists
nothing (D54).

| The project's host | Place answers | Issues | Starts |
|---|---|---|---|
| A record in state `ready` | `ready`, as before v0.07, with the host's key lines in the alias (D47): the box's lease aliases keep v0.06's bytes, `HostKeyAlias agentspace` and no `UserKnownHostsFile` ([state.md, The alias file](state.md#the-alias-file)); every other host's are [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)'s | The credential and the alias, by [How it places a lease](#how-it-places-a-lease) | Nothing |
| A record in state `creating` or `failed` | `pending` | Nothing | A provisioning run, when one is due ([Starting a provisioning run](#starting-a-provisioning-run)) |
| No record, and the project is on the project list | `pending` | Nothing | It adds the project's record, state `creating`, and starts a provisioning run when one is due |
| No record, and the project is not on the project list | `unsatisfiable`, with one reason, `no_capacity` (D54) | Nothing | Nothing, and writes nothing to the host registry |

- **A project that has a record is placed by it, listed or not** (D54). The list governs only
  making a host.
- **The `no_capacity` reason's message names the project and no backend (D1)**, and SHOULD read:
  `No capacity can be found for project '<project>'; agentspace's operator has not approved it.`
  (architect's to confirm). `<project>` is the normalized project.
- **The host is looked up only for a supported request.** An unsupported one is `unsatisfiable`
  with its `unsupported_*` reasons alone, and place reads neither file; so a laptop with no CA,
  no host registry and no project list still gets that answer, as before v0.07.
- **The registry is read under its lock, `hosts.lock`** ([state.md, The host registry](state.md#the-host-registry)),
  taken as state.md says, for as long as the decision and any write of it take, and released before
  any run is started. It follows the lease state lock's rules, a lock held for longer being waited
  for up to 30 seconds, and place takes it while it holds the lease state lock, as the lock order
  allows ([Lock order](#lock-order)). A `ready` answer needs only the record's `state`: place
  MUST NOT read, check for or depend on the record's `instance_id`
  ([How it is reached](#how-it-is-reached)).
- **The project list is read only for a project that has no record in the host registry** (pin P14.3, architect, 2026-11-20,
  for v0.07-T01). A project that has a record is placed by it whatever the list holds, so
  a project list that is missing, unreadable or not valid never fails a lease of a project that
  has a host, swimcoach's among them (D43).
- **A host registry that cannot be read, or does not validate against its schema, fails the
  place** with `internal`, writing nothing, **and so does a lock on the registry that cannot be
  had within its 30 seconds, and a project list, when it is read, that cannot be read or does not
  validate.** For the host registry this is pin P9 (architect, 2026-11-20, for v0.07-T01): only a
  missing `hosts.json` reads as the box's record alone, and one that exists and is not valid, by
  its bytes, its JSON, its schema or a project listed twice, is never read as missing, and is
  never repaired by agentspace. For the project list it is this page's choice (architect's to
  confirm). Neither file is read as empty: an unreadable list read as empty would answer a
  terminal `unsatisfiable` for a project that MAY have a host made. The message is in
  agentspace's own words, and SHOULD be
  `The lease's host could not be looked up: <why>. Nothing was recorded.`, where `<why>` names
  the file by what it is, as in `agentspace's host records are not valid`.
- **A `pending` answer issues nothing:** no key, no certificate and no alias, as for
  `unsatisfiable`. The credential and the alias are made when the lease is placed `ready`
  ([`place_pending`](#place_pending)).
- **A `pending` answer still checks the CA first, as before it issues** (pin P14.1, architect, 2026-11-20, for v0.07-T01): by step 2 of [How it places a lease](#how-it-places-a-lease), after the host lookup has
  decided on `pending` and before anything is written. A CA that fails a check fails the place with
  `internal`, records no lease, writes nothing to the host registry and starts nothing, so
  lease-api.md's rule that `create` with no CA fails ([lease-api.md](lease-api.md#the-credential))
  holds for a `pending` answer as for a `ready` one. An `unsatisfiable` answer runs no CA check,
  as before v0.07. [`place_pending`](#place_pending) checks the CA again when it places the lease.
- **Place still makes no call to AWS and never waits for a host** (D48;
  [Reachability](#reachability)). A provisioning run does both, in a process of its own.

### How it places a lease

**Place makes no call to AWS and waits for no host.** For a host that is `ready` it starts nothing;
for one that is not, it MAY start a provisioning run and answers `pending`
([Where it places a lease](#where-it-places-a-lease)). Every step below is on the caller's
machine, and the steps after the first are those of a lease placed `ready`, whether by place or by
[`place_pending`](#place_pending):
1. **Decides** whether the request is supported, and answers `unsatisfiable` with every reason
   when it is not. Nothing has been issued. **From v0.07, it then finds the project's host**, and
   answers `pending` or `unsatisfiable` there, as [Where it places a lease](#where-it-places-a-lease)
   says, unless the host is `ready`. Before a `pending` answer it runs step 2's CA check, and
   nothing after it.
2. **Checks the CA by the checks of [state.md, Its checks](state.md#its-checks), and those alone.**
   A CA that fails one fails the place, and place MUST NOT create a missing CA (#0029, point 3).
   It never runs the operator's `ca` ([operator.md](operator.md#ca)).
   - Note on where this came from: the prototype runs the operator's `ca` as a separate program,
     under Windows PowerShell 5.1, and reads only its exit code, so on a laptop with no CA its
     place creates one and goes on to sign. That is a difference the spec drops.
3. **Makes a new key pair for the lease and checks that it has no passphrase,** with any tool
   that writes OpenSSH's formats ([state.md, The key](state.md#the-key); gap FA-03). The key's
   type, place and checks, and the refusal of a lease ID that already has a key, are
   [state.md](state.md#a-leases-credential)'s and
   [lease-api.md](lease-api.md#the-credential)'s.
   - Note on where this came from: the prototype runs `ssh-keygen` from Windows OpenSSH, by full
     path, for this step and the next. v0.05's build makes the key and the certificate in its own
     code (D38).
4. **Signs the key's public half with the CA's private key.** The
   certificate's principal is `ubuntu`. Its key ID, validity and options are
   [state.md](state.md#a-leases-credential)'s and [lease-api.md](lease-api.md#the-credential)'s.
   The valid-after time SHOULD be back-dated there; the prototype's is the issue time itself,
   which is not behavior to copy.
5. **Writes the lease's alias** into the caller's OpenSSH client configuration, with the
   transport the next section describes and the host's key lines
   ([Where it places a lease](#where-it-places-a-lease), D47). If the configuration cannot take the
   alias, or the default transport's program cannot be found, place fails here, after the key is
   made and signed, and the credential is removed as for any alias failure. **Place MUST NOT check
   for the host's instance ID**, cached or not ([How it is reached](#how-it-is-reached)).
   - Note on where this came from: without an override, the prototype first checks that its
     checkout holds `.instance-id`, and fails place at this step when it does not. That is a
     difference the spec drops (D21), and never behavior to copy.
6. **Answers `ready`,** with the alias, the user `ubuntu`, and the certificate's valid-before time.

**If step 3 or 4 fails partway, every file this call made is removed, and the lease's folder too,
but only if this call made it,** so that a key with no certificate is never left. **A key made in a
folder that already existed is removed like any other file this call made** (architect,
2026-11-19, gap FA-01): lease-api.md's rule that a failed `create` removes every file it made
binds, and the folder itself stays. Another lease's files are never touched.
- Note on where this came from: the prototype keeps a key it made in a folder that already
  existed. No call from the CLI reaches that case, since a new ID's folder never exists before
  place, and it is not behavior to copy. **If step 5
fails, the credential is removed:** the driver first releases the lease as release does, and, if
that is refused (for example because a foreign file sits at the alias), removes the lease's own
folder directly. It keeps the foreign file. The failure it reports is a message of its own with the
original as the cause.

### Starting a provisioning run

**A provisioning run is `agentspace-operator provision -Host <project>`**, which makes the
project's host by D46's four steps, or runs them again on a host that is already `ready`, each
step being safe to repeat, and ends by placing the project's `pending` leases with
[`place_pending`](#place_pending) (D46, D48). Its steps, its log and its own records are
[operator.md, provision](operator.md#provision)'s. **The driver starts one from place (a `create`)
and from [`check_pending`](#check_pending) (a `get` of a `pending` lease):**
- **From place,** for a project whose record is `creating` or `failed`, or that has no record and
  is on the project list. Place for a project whose record is `ready` answers `ready` and starts
  nothing ([Where it places a lease](#where-it-places-a-lease)).
- **From `check_pending`,** for the same projects, and also for a project whose record is `ready`
  (pin P3, architect, 2026-11-20, for v0.07-T01; D48): a lease left `pending` on a `ready` host,
  because a run's `place_pending` could not place it, is placed by the next run, never by `get`.

A person MAY run `provision -Host <project>` by hand as well.

**When the driver starts a run, and what it records** (pin P2, architect, 2026-11-20, for
v0.07-T01). Steps 1 to 4 are made holding `hosts.lock`, in one hold
([state.md, The host registry](state.md#the-host-registry)), and in this order: the reads that
decide, then the program, then the one write. So a `create` that fails at any step before the
write leaves the registry as it was.
1. **It reads the project's record, and, only when there is none, the project list** (pin P14.3, architect, 2026-11-20,
   for v0.07-T01). From place, no record and not on the list is `unsatisfiable`, as
   [Where it places a lease](#where-it-places-a-lease) says, and nothing below happens; from
   `check_pending` it is nothing at all (below). From place, a `pending` answer's CA check comes
   here, after this read ([Where it places a lease](#where-it-places-a-lease)), and reads only.
2. **It decides whether a run is due, by reads alone.** A run is due only when no run holds the
   project's `provision.lock`, and the record's `last_attempt_at` is absent or at least 5 minutes
   before now; a record that step 4 is about to add has none. The lock is in the host's folder
   ([state.md, A host's folder](state.md#a-hosts-folder)); the driver tries to take it once,
   without waiting, as state.md says the lock is held, and releases it at once if it took it. A
   host's folder or lock file that does not exist means that no run holds it, and the try creates
   neither (architect's to confirm). The folder is made only at the start (below), and the lock
   only by a run.
3. **When a run is due, it checks that the operator program can be found** (the operator
   program, below), and only then. One that cannot be found fails the place with `internal` (architect's to confirm): it is an install
   that waiting will not mend. Nothing is written, so the host registry is as it was, without even
   the record that step 4 would have added. The message is in agentspace's own words, names no
   path, and SHOULD be, by the reason the program cannot be found (architect's to confirm):
   - **With `AGENTSPACE_PROVISION_CMD` unset or empty, no `agentspace-operator.exe` beside the
     CLI:** `The lease's host cannot be made: agentspace's operator program is not installed beside
     this program. Nothing was recorded.`
   - **`AGENTSPACE_PROVISION_CMD` naming no existing file:** `The lease's host cannot be made: the
     program that AGENTSPACE_PROVISION_CMD names does not exist. Nothing was recorded.`
   - **A `.cmd` or `.bat` program with `SystemRoot` unset or empty:** `The lease's host cannot be
     made: Windows' command interpreter cannot be found, because SystemRoot is not set. Nothing was
     recorded.`

   From `check_pending` the same is a failure the CLI drops, and nothing is written either.
4. **It writes the registry once, and only what applies:**
   - **No record, and the project is on the project list:** it adds
     `{"project":"<project>","stack":"agentspace-host-<project>","state":"creating"}` (D44, D55).
     With no `hosts.json`, the file it writes holds the box's record as well, as state.md says of
     the registry's first change (pin P9, architect, 2026-11-20, for v0.07-T01).
   - **When a run is due:** it sets the record's `last_attempt_at` to now and, only when the record
     is new or in state `creating` or `failed`, its `state` to `creating`, in the same write.
   - **When no run is due:** it writes `last_attempt_at` into no record, so an added record has
     none, and a record that existed is not written at all.
5. **It releases `hosts.lock`, and only then, when a run is due, starts it, detached** (below).
   When no run is due it starts none, and the lease is `pending` all the same.

- **A record in state `ready` never has its `state` changed by the driver** (architect, 2026-11-20,
  for v0.07-T01; D48). Starting a run for it writes only its `last_attempt_at`, and a start that
  fails leaves it `ready`. A host that is not `ready` sends its project's `ready` leases to the box,
  where ssh's strict key check refuses them ([How it is reached](#how-it-is-reached)), so a `ready`
  host taken out of `ready` would cut every lease of its project. `creating` and `failed` apply
  only to a host that was never `ready`.
- **So at most one run is started for a host in any 5 minutes**, by any number of `create`s and
  `get`s: the rule covers a failed run, a run that died with its record still `creating`, and the
  moment between the start and the run taking its lock.
- **`last_attempt_at` is the start time of the last run**, UTC, in state.md's timestamp form, and
  "now" is the clock of the caller's machine. A run writes its own when it takes its lock, so a
  run by hand counts too ([operator.md, provision](operator.md#provision)).
- **A run that is not due never makes the `create` fail:** the program is looked for only for a
  run that is due (step 3), so a `create` whose run is not due answers `pending` even when the program
  cannot be found.
- **The locks are taken in [the order below](#lock-order)**: place holds the lease state lock when
  it takes `hosts.lock`, and only tries `provision.lock`.

**The operator program** is the one the driver's context names, which the CLI resolves when it
starts (card #0143; D55 pins it into the code's `Context`; pin P12, architect, 2026-11-20, for
v0.07-T01):
- **`AGENTSPACE_PROVISION_CMD`, when it is set and not empty:** the full path of the program the
  driver starts for a provisioning run, in place of agentspace's own, so that a check can stand one
  in ([lease-api.md](lease-api.md#environment)). It is a program's documented variable, as
  `AGENTSPACE_PROXY_COMMAND` is. A relative path is made full against the CLI's working folder,
  as state.md makes a relative folder full, before the detached start, since the run's own working
  folder is the home folder (pin P12, architect, 2026-11-20, for v0.07-T01).
- **Otherwise, `agentspace-operator.exe` in the folder the CLI's own executable runs from**, as the
  CLI finds the ProxyCommand program ([operator.md, How each program finds the others](operator.md#how-each-program-finds-the-others)).
- **A path that does not name an existing file is a program that cannot be found.**
- **Its command line is the program, then exactly `provision`, `-Host` and the normalized
  project**, and nothing else ([operator.md, provision](operator.md#provision)): `"<program>"
  provision -Host <project>`. **A `.cmd` or `.bat` program**, its extension compared without regard
  to case, is run through Windows' command interpreter as a ProxyCommand line is:
  `<system folder>\cmd.exe /d /c "<program>" provision -Host <project>`, with `<system folder>` as
  [state.md, The alias file](state.md#the-alias-file) gives it, and a `SystemRoot` that is unset or
  empty makes the program one that cannot be found.

**The detached start (D48, D55; pin P12, architect, 2026-11-20, for v0.07-T01).** A provisioning
run takes about ten minutes, and MUST outlive the `create` or `get` that started it, the agent's
tool call that ran that command, and every process above them. So the driver starts it through
agentspace's detached start, which creates it with WMI's `Win32_Process.Create`, the call by which
the keeper's holder is started (D42; [keeper.md, Starting a holder](keeper.md#starting-a-holder)).
That call returns the run's process ID directly. **A run has no launch files**, no request, claim
or result file and no handshake: `provision.lock`, which the run holds for its life, is its proof
of life. What the start MUST achieve, from outside:
- **The run is outside the Job Object of the CLI and of every process above it**, as keeper.md's
  item 1 says of a holder.
- **It has no window, starts in a new process group, and its working folder is the home folder**,
  as the CLI has it, so that it never holds a worktree or a checkout open.
- **The driver gives the run no standard handles: its standard input, standard output and
  standard error are null**, and so never the CLI's ([The boundary](#the-boundary); pin P14.2,
  architect, 2026-11-20, for v0.07-T01). A process created through `Win32_Process.Create` cannot
  be handed a file handle, so the driver MUST NOT open the host's `provision.log`, and passes no
  handle to the run.
- **The run opens `provision.log` for append itself, as its first step, and writes every line of
  its log there** ([state.md, A host's folder](state.md#a-hosts-folder); pins P4 and P14.2,
  architect, 2026-11-20, for v0.07-T01). A run that cannot open its log exits 1 and writes nothing
  else ([operator.md, provision](operator.md#provision)).
- **Before the start, after it has released `hosts.lock`, the driver makes the host's folder,
  `hosts\<project>\`, when it is missing** (pin P14.2, architect, 2026-11-20, for v0.07-T01), so
  that the run can open its log there.
  - **A failure to make the folder is a start that fails** (below): the lease is `pending`, and
    nothing else changes.
  - **That folder is the one thing in a host's folder the driver ever makes.** It never makes or
    opens `provision.log`, and never makes `provision.lock`. A missing folder or lock still means
    that no run is running (step 2).
- **It inherits no handle of the CLI's**, and in particular never the lease state lock, which the
  `create` that started it still holds, or `hosts.lock`.
- **Its environment is the user's logon environment, with `USERPROFILE` set to the CLI's, and with
  each of `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` present if
  and only if the CLI had it set and not empty** (pins P15.1 and P15.2, architect, 2026-11-20, for
  v0.07-T01). A value of one of the three that the logon environment holds and the CLI did not
  have is removed. `USERPROFILE`, `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` name folders,
  and each is made full against the CLI's working folder, as state.md makes a relative folder full.
  `AGENTSPACE_PROXY_COMMAND` is a command line, not a folder, and is passed exactly as the CLI had
  it (pin P12, architect, 2026-11-20, for v0.07-T01). Any other variable set only in the CLI's
  environment, not in the logon environment, never reaches the run. They are what [`place_pending`](#place_pending) needs to place
  the leases in the same home, state folder and OpenSSH client folder, and with the same
  transport, as the command that started the run: `provision` builds `place_pending`'s context
  from them as the lease CLI builds its own. No other variable of the CLI's reaches the run, so
  nothing of an agent's session does.
- **At most one run is started per start**, and the start is one call that returns the run's
  process ID or fails. It never waits for the run, so that place stays well under the 30-second
  wait ([Ordering and concurrency](#ordering-and-concurrency)) and `check_pending` within its
  bound.
- **The driver never waits for the run**, and nothing it answers depends on the run's outcome: the
  lease is `pending` whether the start succeeded or not.
- **A start that fails changes nothing more, and fails nothing** (architect's to confirm): the
  record stays as step 4 wrote it, so the next start is due 5 minutes later, and place still
  answers `pending`. A record that step 4 found `ready` stays `ready`, with only its
  `last_attempt_at` changed.

**From `check_pending`, the rule is the same, with these differences:**
- **It tries `hosts.lock` once, without waiting, and when another process holds it, starts nothing
  and succeeds** (pin P12, architect, 2026-11-20, for v0.07-T01). The holder is a place or a run
  acting on the registry, and `get` MUST NOT wait on it ([check_pending](#check_pending)); the
  next `get` checks again.
- **A record in state `ready` is treated as `creating` and `failed` are, by steps 2 to 5**, but
  for its `state`, which stays `ready` (pin P3, architect, 2026-11-20, for v0.07-T01): when a run
  is due, it writes only `last_attempt_at` and starts the run, which places the lease.
- **No record, and the project is on the project list:** it adds the record, as place does, and
  goes on by the rule. **No record, and not on the list** (a registry changed by hand since the
  `create`): it does nothing, and the lease stays `pending`. `get` never makes a lease
  `unsatisfiable`.
- **It runs no CA check**: [`place_pending`](#place_pending) checks the CA when it places the lease
  (pin P14.1, architect, 2026-11-20, for v0.07-T01).
- **It never writes the lease state file and never calls [`place_pending`](#place_pending)**, and
  a registry or list that cannot be read is a failure the CLI drops. **A pending lease whose
  driver is missing or unknown** starts nothing ([check_pending](#check_pending)).

**After the run.** A run that succeeds records the host `ready`, or keeps it `ready`, and then calls
[`place_pending`](#place_pending) for the project (pin P3, architect, 2026-11-20, for v0.07-T01). A
failure of that call is the run's to log, and leaves the host `ready`; the leases it could not
place stay `pending`, and the next run P2's rule allows, started by a `get` of one of them, places
them. **A failed run leaves the leases `pending`, never `unsatisfiable`** (D48, D5). It records a
host that was never `ready` as `failed`, and on a host that was `ready` it logs the failed step and
leaves it `ready`. Once 5 minutes have passed since its last start, the run is started again: for a
host that is not `ready`, by a later `create` or `get` of the project, and for a `ready` one, by a
later `get` of a lease that is still `pending`. A caller that gives up releases the lease, and the
host is kept (D25).

### place_pending

**Places every `pending` lease of one project once its host is `ready`: the transition lease-api.md
names "`pending` to `ready`, by agentspace, when placement finishes"** (D48; card #0143). It is a
library call of the EC2 driver, made by every provisioning run that succeeds, as its last step,
after it has recorded the host `ready` or kept it `ready` (pin P3, architect, 2026-11-20, for
v0.07-T01; [operator.md, provision](operator.md#provision)), and not a boundary operation: the
lease CLI never calls it, and neither does [`check_pending`](#check_pending).

**Its inputs:** the project, normalized; and the home folder, state folder, OpenSSH client folder,
transport override and system folder, which the calling program reads from its own environment as
the lease CLI reads them ([lease-api.md](lease-api.md#environment); [state.md, The home
folder](state.md#the-home-folder)), and so, in a run the driver started, as the command that
started it had them ([The detached start](#starting-a-provisioning-run)). The driver's context
carries the state folder for it, as the code's `state_dir` (pin P12, architect, 2026-11-20, for
v0.07-T01, a code pin committed before T03). The default transport's program is found in the
folder the calling program's own executable runs from, as the CLI finds it
([How it is reached](#how-it-is-reached); [state.md, The alias file](state.md#the-alias-file), D8;
pin P15.4, architect, 2026-11-20, for v0.07-T01). In a run, that is `agentspace-operator.exe`'s
folder, which D37 puts beside the CLI and the ProxyCommand program. **When the program is not
there, `place_pending` fails that lease as `create` would** (step 5 of
[How it places a lease](#how-it-places-a-lease)), and the lease stays `pending`. A transport
override, the CLI's `AGENTSPACE_PROXY_COMMAND` when it had it set, replaces the whole line, and
the program is then not looked for. **A stand-in that `AGENTSPACE_PROVISION_CMD` names never
calls `place_pending`.**

**It places only the leases of the state folder it is given.** A `pending` lease in a state
folder other than the one whose command started the run is placed only by a later run started
from its own folder, by a `create` or `get` that names that folder.

**What it does, in order:**
1. **Reads the project's host record**, holding `hosts.lock` for that read alone, and no lease
   state lock, as [the lock order](#lock-order) allows. A registry that cannot be read, or whose
   lock cannot be had within its 30 seconds, fails the call, and every lease stays `pending`.
   **A record that is not `ready`, or none, places nothing**, and the call succeeds.
2. **Places the project's `pending` leases one at a time, each in a hold of its own of the lease
   state lock** (pin P14.4, architect, 2026-11-20, for v0.07-T01), so that no hold spans several placements, and a
   `create`, `renew` or `release` waits behind one placement at most. For each lease in turn:
   1. **Takes the lease state lock**, and waits for it as a changing lease command does, up to 30
      seconds ([state.md, The lease state lock](state.md#the-lease-state-lock)). One it cannot
      take fails the call, and every lease not yet placed stays `pending`.
   2. **Reads the lease state file again**, as every lease command does, so a lease released or
      changed since the last hold is seen as it now stands. One that cannot be read, or does not
      validate, fails the call: it releases the lock, and changes nothing more.
   3. **Picks the first record, in the file's order, whose stored state is `pending`, whose
      `project` is the project, whose `driver` is this driver's name, and that this call has not
      tried yet.** When there is none, it releases the lock, and the call is done.
   4. **Removes that lease's leftover credential files first** (pin P14.5, architect, 2026-11-20, for v0.07-T01): a
      key, certificate or alias file for its ID, removed as release removes them
      ([Release a lease](#release-a-lease)). A `pending` lease has none by contract, so any there
      can only be a placement's that was killed part way. A removal that fails, a foreign file at
      the alias's place among them, is a failure of this lease.
   5. **Issues the lease's credential and alias** by steps 2 to 5 of
      [How it places a lease](#how-it-places-a-lease): the CA check, the key, the certificate and
      the alias, with the record's lease ID and project, its `ttl_seconds` as the TTL, and the
      host's key lines ([Where it places a lease](#where-it-places-a-lease)). **A CA that fails its
      check fails this lease** (pin P14.1, architect, 2026-11-20, for v0.07-T01), and MUST NOT be created.
   6. **Writes the lease state file with that record `ready`**: it adds `expires_at`, the
      certificate's valid-before time, and `ssh`, the alias and the user `ubuntu`, and changes no
      other member, `created_at`, `ttl_seconds` and `driver` included. The file is checked against
      its schema before the write, and replaced atomically, as every write of it is
      ([state.md](state.md#the-lease-state-file)). One write per lease (architect's to confirm), so
      that a failure costs only that lease.
   7. **A failure at step 4, 5 or 6 leaves that lease `pending`**, with every file this call made
      for it removed as a failed place removes them ([Place a lease](#place-a-lease)), and is
      named in the call's result for the run's log. When the write fails, the driver releases the
      lease's files as release does.
   8. **Releases the lease state lock**, and goes on to the next lease.

- **A lease's TTL starts when it becomes `ready`** (D48): its `expires_at` is its certificate's
  valid-before time, so the time it spent `pending` costs its caller none of it.
- **Every other record is left as it was**: a lease released while it was `pending`, another
  project's, and another driver's.
- **It never answers `unsatisfiable`, starts nothing, makes no call to AWS, and never writes the host
  registry.**
- **Its result, to the run that called it,** names each lease it placed by ID, and each it could
  not by ID with the failure's message, for the run's log. Each message meets
  [Failure](#failure)'s rules. The call as a whole fails only at step 1, or at step 2's lock or
  read; a lease it placed before such a failure stays `ready`.
- **A lease it places is like one place made `ready`**: renew, release, the keeper and the
  ProxyCommand treat the two alike.
- **A failure of the call is the run's to log, and leaves the host `ready`** (pin P3, architect,
  2026-11-20, for v0.07-T01). A lease it could not place stays `pending` until a later run places
  it, or its caller releases it. That run is started by the next `get` of the lease that P2's rule
  allows ([Starting a provisioning run](#starting-a-provisioning-run), from `check_pending`), or by
  a person with `provision -Host <project>`. So no such lease is stranded.

### Lock order

**Three locks are waited for, always in this order: a host's `provision.lock`, then the lease state
lock, then `hosts.lock`** (pin P10, architect, 2026-11-20, for v0.07-T01). A program that holds a
later lock MUST NOT wait for an earlier one, so no two programs ever wait on each other.
- **A try without waiting is outside the order**, and cannot deadlock: place, holding the lease
  state lock and `hosts.lock`, MAY try a host's `provision.lock` without waiting, and releases it
  at once if it took it ([Starting a provisioning run](#starting-a-provisioning-run), step 2).
  `check_pending`, holding `hosts.lock` alone, MAY try it the same way.
- **`hosts.lock` follows the lease state lock's rules** ([state.md, The lease state
  lock](state.md#the-lease-state-lock)): the open handle is the lock, the file is never deleted,
  and a lock held by another process is waited for up to 30 seconds. **The one exception is
  `check_pending`, which tries it once without waiting** (pin P12, architect, 2026-11-20, for
  v0.07-T01).
- **`provision.lock` is taken without waiting by every program**, and a run holds it for its life
  ([state.md, A host's folder](state.md#a-hosts-folder)).
- **A run ends in this order:** under `hosts.lock` alone it records the host `ready`, or keeps it
  `ready`, with the instance ID it learned, and releases that lock; it then calls
  [`place_pending`](#place_pending), which takes `hosts.lock` alone for its read of the host
  record, and then the lease state lock once per lease; it holds `provision.lock` throughout.
  `place_pending` reads the lease state file again in each hold, so a lease released meanwhile is
  never placed.

### Renew and release

**Renew** checks the CA, signs a copy of the lease's public key under a temporary name inside the
lease's folder, and then moves the new certificate over the old one, so the old certificate stays
until the new one exists. The temporary files are removed whatever happens. It does not touch the
alias.

**Release** removes the alias file, if it belongs to this lease, and then the lease's whole folder.
If the alias file belongs to another lease or to none, release fails and deletes nothing. If it is
absent, release goes on. It runs no CA check and starts no program. It does not touch the OpenSSH
client configuration, and it tells the box nothing.

**Both are the same for a lease on any host** (D48): neither reads the host registry or the project
list, neither starts a run, and a lease that [`place_pending`](#place_pending) placed is renewed
and released as one place made `ready`.

### What it records

**On the caller's machine only, and only the files [state.md](state.md#a-leases-credential)
describes:** the lease's key pair, its certificate, and its alias file, and the `Include` line in
the OpenSSH client configuration that pulls aliases in (place adds it when it is missing, and
release leaves it; how is state.md's).

**From v0.07, it also writes the host registry, and nothing else of its own** (D45, D48;
[Starting a provisioning run](#starting-a-provisioning-run)): a project's record, added in state
`creating` when the project is on the project list and has none, and, when it starts a run, the
record's `last_attempt_at` set to now, with its `state` set to `creating` unless it is `ready`. Their
form is [state.md, The host registry](state.md#the-host-registry)'s. It never writes the project
list, which only the architect writes, with Terry's yes (D54), never a record's `instance_id`, or
`ready` or `failed`, which are the run's ([operator.md, provision](operator.md#provision)), and
never the `state` of a record that is `ready` (D48). The lease state
file is the CLI's, but for [`place_pending`](#place_pending)'s writes of the leases it places.

**The driver keeps no other record.** Everything it needs to renew or release a lease comes from
the lease's ID and project, the OpenSSH client folder, and those files. **Nothing is recorded on a
host and nothing in AWS by the driver itself**: a provisioning run that the driver starts makes and
records the host's stack, which is the run's ([operator.md, provision](operator.md#provision)). A
host learns of a lease only when a connection arrives, through the certificate's key ID in its SSH
log (D7).

### How it is reached

**The alias's transport is the operator's ProxyCommand, with the alias as its argument:** the full
path of agentspace's ProxyCommand program, which the CLI finds in the folder its own executable
runs from, then the alias's name, written out literally; a `.cmd` or `.bat` program is run through
Windows' `cmd.exe` (D8). The line, byte for byte, is [state.md](state.md#the-alias-file)'s.
[operator.md](operator.md#the-proxycommand) owns what the program does, including that it runs the
operator's `up` for the alias and then tunnels SSH to the box. Its AWS calls, in order, are the
ProxyCommand row of [operator.md](operator.md#calls-by-command), and the programs it starts are in
[operator.md](operator.md#programs-it-starts). Place itself starts only `ssh-keygen`, and, in the
prototype, Windows PowerShell 5.1 for `ca`, which is not behavior to copy
([How it places a lease](#how-it-places-a-lease)). From v0.07, place and `check_pending` also start
a provisioning run, detached, and never wait for it
([Starting a provisioning run](#starting-a-provisioning-run)).

**Which host a connection reaches is the ProxyCommand's to find, from the lease itself** (D49, D55;
pin P7, architect, 2026-11-20, for v0.07-T01): the lease's record's `project`, then that
project's record in the host registry if it is `ready`, and otherwise the box. The alias line does not change for that. A lease alias for a host
that is not the box checks the host's key under that host's own name, so a fall back to the box
fails ssh's strict key check rather than reaching the wrong machine
([state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)).

**The default transport reads a host's instance ID when a connection is made, and place never
reads it** (D21). Until v0.07 it was the operator's instance-ID cache,
`%USERPROFILE%\.agentspace\instance-id.json` (D40); from v0.07 it is a member of the host's record
in the host registry, which replaces the cache (D45). What the ProxyCommand does when it is missing
or stale is [operator.md](operator.md#laptop-files)'s. **`create` MUST NOT read it, check for it or
depend on it:** a missing or stale instance ID fails the connection, never the `create`. Place
reads a host record's `state` and nothing more of it
([Where it places a lease](#where-it-places-a-lease)). So a check, or a worktree, needs no instance
ID to create a lease, with or without the override.
- **If the default transport's program cannot be found, place fails** (step 5 above), with a
  message in agentspace's own words and no absolute path
  ([lease-api.md](lease-api.md#messages)).
- Note on where this came from: the prototype's default line runs
  `<checkout>\scripts\ssh-proxy.cmd`, beside the CLI's own script, and its place checks for the
  checkout's `.instance-id`, its copy of the cache, which a git worktree does not have. That check
  is a difference the spec drops (D21).

**What place cannot know:** whether the lease's host trusts the CA, or is running, or can be
reached. The operator sets up the CA, and `sync` installs its public half on each host (operator.md,
[The box as the laptop sees it](operator.md#the-box-as-the-laptop-sees-it)); a new host also gets it
at its first boot (D44). A lease placed before that is `ready` and refuses every connection. That is
why place MUST NOT create the CA itself. A host record's `ready` means only that a connection is
expected to succeed (D52; [lease-api.md](lease-api.md#states)), not that the host's sign-ins or
checkout are there.

### What is true of today's box

**These are properties of this driver and its hosts, never of the contract (D4):**
- **The box is swimcoach's host** (D43): the `agentbox` stack's instance, with its disk, sign-ins,
  checkout and host key as they were, and the record whose stack is `agentbox` in the host registry.
  Every other project's host is a stack of its own, `agentspace-host-<project>`, from the same
  template (D44), made for a project on the project list at its first lease (D48, D54). For v0.07
  the list holds `agentspace` alone.
- **Every host runs Linux on arm64 as Ubuntu,** with `sshd` trusting the CA, `systemd`, and a
  login user named `ubuntu` with a home and `sudo`. A new host gets its software from `box/system`
  and the bootstrap (D46).
- **Every lease logs in as `ubuntu`** (D3). The driver says so in three places (the certificate's
  principal, the returned user, the alias's login user), and every driver MUST keep all three the
  same. D25 replaced D3's isolation by a user per project with a host per project, which arrives
  in v0.07.
- **Each project's leases land on that project's host from v0.07** (D25, D43); until then every
  project's landed on the box. No lease was moved (D43).
- **The box's leases keep v0.06's alias bytes**, and are checked under the key name `agentspace` in
  the user's own `known_hosts` (D47); every other host's key is checked under
  `agentspace-host.<project>` in agentspace's own known-hosts file
  ([state.md, Agentspace's host keys](state.md#agentspaces-host-keys)).
- **Each host stops itself when idle,** and the transport starts it (D11, D51).

### Conformance cases for v0.07

**Each behavior below can be shown through the lease CLI with stand-ins, and MUST be a conformance
case of E0.07-T04-S01** (card #0143; D53). Each runs in the harness's throwaway home
([conformance.md](conformance.md)), writes `hosts.json`, `projects.json` and a held `provision.lock`
there itself where it needs them, and sets `AGENTSPACE_PROVISION_CMD` to a stand-in that records
its arguments, working folder, standard input, the four variables of its environment that the
detached start sets, and its start time, and makes no host. **A check whose `create` or `get` could
start a run MUST set `AGENTSPACE_PROVISION_CMD` to such a stand-in** (pin P12, architect,
2026-11-20, for v0.07-T01), so that no check ever starts a real provisioning run. In a throwaway
home with no `projects.json`, a supported `create` for any project but `swimcoach` now answers
`unsatisfiable` with `no_capacity`, so an existing lease CLI case that uses another project changes
with this release; the harness's change for it is E0.07-T04-S01's (pin P12).

| Case | What it shows | Where the rule is |
|---|---|---|
| C1 | A supported `create` for a project with no record and not on the list answers `unsatisfiable` with exactly one reason, `no_capacity`, whose message quotes the project; the record carries `driver`; no lease folder or alias file is made; the host registry is not written; the stand-in is not started | [Where it places a lease](#where-it-places-a-lease) |
| C2 | An unsupported `create` for such a project lists its `unsupported_*` reasons and no `no_capacity`, and writes no host registry | [Where it places a lease](#where-it-places-a-lease) |
| C3 | A `create` for `swimcoach` with no `hosts.json` answers `ready` with v0.06's alias bytes, starts nothing, and leaves `hosts.json` absent | [Where it places a lease](#where-it-places-a-lease) |
| C4 | A `create` for a project whose record is `ready` and not the box answers `ready`, with the alias of [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box), and starts nothing | [Where it places a lease](#where-it-places-a-lease) |
| C5 | A `create` for a listed project with no record answers `pending`, exit 0, a document that validates, with no `expires_at`, `ssh` or `reasons`; makes no lease folder or alias file; adds the record `creating` with `last_attempt_at` within the command's run, beside the box's record; and starts the stand-in once, with exactly `provision -Host <project>` | [Starting a provisioning run](#starting-a-provisioning-run) |
| C6 | The `create` does not wait for the run: a stand-in that runs for a minute is still running after the `create` has exited, and goes on to finish | [Starting a provisioning run](#starting-a-provisioning-run), the detached start |
| C7 | The stand-in's working folder is the home folder, it reads nothing from its standard input, nothing it writes to its standard output or standard error reaches the CLI's (stdout holds exactly one document, stderr nothing), its `USERPROFILE` is the CLI's, made full, `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` are the CLI's, each made full, given relative, and `AGENTSPACE_PROXY_COMMAND` is the CLI's exactly, not made full, where the CLI had them set and not empty, and each is absent where the CLI had it unset or empty | [Starting a provisioning run](#starting-a-provisioning-run), the detached start |
| C8 | A second `create` or a `get` of the project within 5 minutes of `last_attempt_at` answers `pending` and starts nothing, and leaves `last_attempt_at` as it was | [Starting a provisioning run](#starting-a-provisioning-run), step 2 |
| C9 | A record `failed` or `creating` whose `last_attempt_at` is 5 minutes or more before now: a `create` starts the stand-in once, and sets `state` `creating` and `last_attempt_at` to now | [Starting a provisioning run](#starting-a-provisioning-run), steps 2 and 4 |
| C10 | With the project's `provision.lock` held by the check, a `create` or `get` starts nothing, and leaves the record's `state` and `last_attempt_at` as they were | [Starting a provisioning run](#starting-a-provisioning-run), step 2 |
| C11 | A `get` of a `pending` lease leaves the lease state file byte for byte as it was, whether it starts the stand-in or not, and prints the lease as stored | [check_pending](#check_pending) |
| C12 | A `get` of a `pending` lease whose host's record is `ready`, with `last_attempt_at` absent or 5 minutes or more before now, starts the stand-in once, with exactly `provision -Host <project>`; sets the record's `last_attempt_at` to now; leaves the lease state file byte for byte as it was, with no lease folder or alias file made; and prints the lease `pending`. A second `get` within 5 minutes starts nothing | [Starting a provisioning run](#starting-a-provisioning-run), from `check_pending` |
| C13 | A `get` of a `pending` lease succeeds, exit 0 with the lease, when `hosts.json` is not valid, when `AGENTSPACE_PROVISION_CMD` names no file, and while the check holds `hosts.lock`; in each, it starts nothing and returns within 20 seconds | [check_pending](#check_pending) |
| C14 | `list` never starts the stand-in, whatever its leases' states | [The boundary](#the-boundary) |
| C15 | `release` of a `pending` lease records it `released` with no `expires_at`, starts nothing and leaves the host registry as it was | [Release a lease](#release-a-lease) |
| C16 | A `create` that would start a run, with `AGENTSPACE_PROVISION_CMD` naming no file, fails with `internal`, records no lease, and leaves the host registry as it was; its message has no path | [Starting a provisioning run](#starting-a-provisioning-run) |
| C17 | A supported `create` with a `hosts.json` that does not validate fails with `internal` and records nothing; so does one for a project with no record in a valid `hosts.json`, with a `projects.json` that does not validate | [Where it places a lease](#where-it-places-a-lease) |
| C18 | A `.cmd` stand-in is started through `cmd.exe /d /c`, and receives the same three arguments | [Starting a provisioning run](#starting-a-provisioning-run), the program |
| C19 | A record in state `ready` never has its `state` changed by the driver: a `get` that starts the stand-in for it, and a `get` whose `AGENTSPACE_PROVISION_CMD` names no file, each leave its `state` `ready` and its `instance_id` as they were, the first changing only `last_attempt_at` and the second nothing | [Starting a provisioning run](#starting-a-provisioning-run), steps 3 and 4 |
| C20 | A `create` for a project whose record is `ready`, while another lease of the project is `pending`, answers `ready` and starts nothing, whatever the record's `last_attempt_at` | [Starting a provisioning run](#starting-a-provisioning-run), from place |
| C21 | With a `projects.json` that does not validate, a `create` for a project that has a record succeeds as it would with a valid one: `swimcoach` with no `hosts.json` answers `ready`, and a project whose record is `creating` answers `pending` | [Where it places a lease](#where-it-places-a-lease) |
| C22 | With `hosts.lock` held by the check: a `create` for a listed project with no record, whose hold is let go after under 30 seconds, succeeds as C5 says once it is let go; one whose hold lasts past 30 seconds fails with `internal`, records no lease, leaves the host registry as it was and starts nothing | [Where it places a lease](#where-it-places-a-lease) |
| C23 | A `create` for a listed project with no record, while the check holds the project's `provision.lock`, answers `pending`, adds the record `creating` with no `last_attempt_at`, and starts nothing | [Starting a provisioning run](#starting-a-provisioning-run), steps 2 and 4 |
| C24 | A `create` for a project whose record is `creating` or `failed`, and that is not on the project list (none, or one that does not list it), answers `pending` and starts the stand-in once: the record wins over the list | [Where it places a lease](#where-it-places-a-lease) |
| C25 | A `get` of a `pending` lease whose project has no record (the check removes it after the `create`): for a listed project, it adds the record `creating` and starts the stand-in once; for one not listed, it writes nothing to the host registry, starts nothing and prints the lease `pending`, exit 0 | [Starting a provisioning run](#starting-a-provisioning-run), from `check_pending` |
| C26 | A `get` of a `pending` record with no `driver`, or with a `driver` the CLI does not have, written by the check, prints it, exit 0, starts nothing and leaves the host registry as it was | [check_pending](#check_pending) |
| C27 | With no host's folder before the `create`, the driver made `hosts\<project>\` and the stand-in was started; the case never asserts on `provision.log`, since a stand-in writes no log (operator.md's `provision` case does) | [Starting a provisioning run](#starting-a-provisioning-run), the detached start |
| C28 | A variable set only in the CLI's environment, under a name of the check's own that the logon environment does not hold, is absent from the stand-in's environment | [Starting a provisioning run](#starting-a-provisioning-run), the detached start |
| C29 | The run inherits no lock: while a stand-in started by a `create` is still running, a second `create` in the same state folder, and a `release` of the first lease, each succeed within their 30 seconds | [Starting a provisioning run](#starting-a-provisioning-run), the detached start |
| C30 | A relative `AGENTSPACE_PROVISION_CMD`, with the CLI's working folder other than the home folder, is made full against the CLI's working folder: the stand-in it names is started | [Starting a provisioning run](#starting-a-provisioning-run), the operator program |
| C31 | With `AGENTSPACE_PROVISION_CMD` naming no file, a `create` whose run is due fails with `internal` (C16), while the same `create` with no run due, for a record whose `last_attempt_at` is under 5 minutes before now, answers `pending` and leaves the host registry as it was | [Starting a provisioning run](#starting-a-provisioning-run), step 3 |
| C32 | With no CA, a supported `create` for a listed project with no record fails with `internal`, records no lease, leaves the host registry as it was and starts nothing; its message is lease-api.md's for a missing CA | [Where it places a lease](#where-it-places-a-lease) |

**What no stand-in for the lease CLI shows:** [`place_pending`](#place_pending), which only a
provisioning run calls, and a run's own records, among them that a run on a `ready` host keeps it
`ready` whether its steps succeed or fail. Their cases are the operator's, through `provision` and
operator.md's test seam (E0.07-T04-S02), and D53's live check of a first agentspace lease that
becomes `ready`.

## Writing another driver

**To be driven by the CLI without change, a driver MUST:**
1. **Supply the three operations,** with the inputs, results and failures above, over whatever
   transport the rewrite chooses ([Q1](#open-questions)), and an opaque, stable name. A driver
   that ever answers `pending` also supplies [`check_pending`](#check_pending); one that never
   does keeps the boundary's default, which does nothing.
2. **Decide support before it does anything,** and answer `unsatisfiable` with every reason that
   applies ([Unsatisfiable requests](#unsatisfiable-requests)).
3. **Check the CA without creating it,** before it issues a credential or renews one.
4. **Leave the lease's key, certificate and alias as [state.md](state.md#a-leases-credential)
   and [lease-api.md](lease-api.md#the-credential) say,** at the end of a `ready` place, and
   remove them at release. A driver MAY share the code that does it with another driver; the
   prototype's driver calls shared steps ([Q5](#open-questions)).
5. **Supply the way to reach the host** and put it in the alias ([Reachability](#reachability)),
   and honor the transport override.
6. **Wrap every failure** ([Failure](#failure)).
7. **Keep every output free of the backend** ([What a driver never exposes](#what-a-driver-never-exposes)).
8. **Make a failed place leave nothing, make renew replace only the certificate, and make release
   repeatable** ([The operations](#the-operations)).

**A driver MUST NOT need anything from the CLI that this page does not list,** and a CLI that works
with the EC2 driver MUST work with it unchanged.

## Hardening carried in

**Card #0041, first point: credential errors reach `internal` unwrapped, with the laptop's paths,
the user name and the CA check's output.** Carried in as the rule in [Failure](#failure): every
failure at this boundary, from a credential step, an alias step or a deletion, has a message in
agentspace's own words with none of those things, and the original kept as the cause. The CLI
puts only the message in the error document. [lease-api.md](lease-api.md#exit-codes-and-errors)
states what an `internal` message MUST NOT carry, and where the original goes is
[Q9](#open-questions)'s.

**Card #0041's other points** are [lease-api.md](lease-api.md#hardening-carried-in)'s, and meet
this page in two places:
- **Second point (a `ready` record without its driver):** [The driver's name](#the-drivers-name).
  How such a record is read, and that the name is always written, are
  [state.md](state.md#the-lease-state-file)'s.
- **Third point (release and renew order):** [Ordering and concurrency](#ordering-and-concurrency)
  and [Release a lease](#release-a-lease).

**Card #0029's points** are lease-api.md's. At this boundary its fourth point (a failed signing
leaves the key behind) is [Place a lease](#place-a-lease)'s requirement that a failed place leaves
nothing, and its third (the first lease creates a CA no host trusts) is
[Place a lease](#place-a-lease)'s and [Renew a lease](#renew-a-lease)'s rule that a driver MUST
NOT create the CA. The prototype does not meet the third point yet; see
[How it places a lease](#how-it-places-a-lease). Its first point (clock skew) is the back-dating
that [Place a lease](#place-a-lease) asks of a driver.

**What v0.05's Rust build shows (E0.05-T04-S01, 2026-11-19, [the run record](runs/e0.05-t04-s01.md)).**
The EC2 driver is registered in the program. The prototype does not meet the points below, and the rows
stay because they describe it, which runs through `bin\` until v0.06 (#0055) retires it (D37, D38;
architect, 2026-11-19). A point is marked yes only where the conformance file it names checks the
point's MUST.

| Point | The prototype | Met by v0.05's Rust build |
|---|---|---|
| #0041, first point (credential errors unwrapped) | Lets some underlying errors through, with laptop paths | Yes: `conformance/lease-cli/test-unmet-internal-no-path.ps1`, and `test-lease-driver-outcomes.ps1` for the failure of an alias step |
| #0041, second point (a `ready` record without its driver) | Fails `renew` and `release` of it with `internal` | Yes: `conformance/lease-cli/test-unmet-record-no-driver.ps1` |
| #0041, third point (release and renew order) | Deletes before it writes the record, and moves the new certificate before the record | Yes: `conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1`, `test-unmet-release-partial.ps1` and `test-unmet-release-released.ps1` |
| #0029, first point (clock skew) | The valid-after time is the issue time, where a SHOULD asks for 5 minutes before it | Not shown: no conformance file checks it (#0110) |
| #0029, third point (the driver never creates the CA) | Runs the operator's `ca`, which creates a CA when there is none | Yes: `conformance/lease-cli/test-unmet-no-ca.ps1` |
| #0029, fourth point (a failed place leaves nothing) | Names no leftover in its home-relative form | Yes: `conformance/lease-cli/test-unmet-failed-create-cleanup.ps1`, and `test-lease-driver-outcomes.ps1` for a failed alias step, which leaves nothing and does not block a later `create` |
| #0087 (the transport names the backend) | Names it while a lease connects | Not a lease CLI row: the ProxyCommand is the operator's, which the Rust build does not have yet (v0.06, #0055) |

**Card #0087: the transport names the backend while a lease connects.** Carried in as the
requirement in [Reachability](#reachability) that what the transport prints MUST NOT name the
backend (D1). The prototype does not meet it, and v0.06 (#0055) does.

## Open questions

**What reading the prototype and its docs could not settle.** After the clean-room review
(E0.04-T04-S01, #0080), every question here is either closed or marked "Later, not blocking", and
none blocks building a drop-in of contract v0.

1. **Later, not blocking:** nothing about the boundary is visible from outside the CLI
   ([The boundary](#the-boundary)), and the contract is written so that any transport carries it,
   so a drop-in picks its own.
   **Q1: What is the transport between the CLI and a driver in the rewrite?** The prototype calls
   a driver inside one process. No decision fixes the rewrite's, and the contract is written so
   that any transport carries it. Whether drivers are compiled into the CLI, started as programs,
   or reached over a network is a design call for v0.05 (D28), and decides how a conformance
   check can plug in a stub driver, which the prototype has no way to do.
2. **Later, not blocking:** v0 has one driver ([The EC2 driver](#the-ec2-driver)), so there is no
   choice to make among several.
   **Q2: How does the CLI choose a driver, and what does `unsatisfiable` mean with several?** The
   prototype asks the first driver it loaded. D5 says a request no backend can meet is
   `unsatisfiable`, but nothing says whether every driver is asked, in what order, whose reasons
   are shown, or how a driver is chosen for a project that D25 gives a host of its own.
3. **Closed for the EC2 driver by D48 (v0.07, card #0143); later, not blocking, for any other.**
   The EC2 driver answers `pending` for a project whose host is not `ready`, and the lease becomes
   `ready` when the provisioning run that makes the host calls [`place_pending`](#place_pending),
   which places it under the lease state lock; `get` asks [`check_pending`](#check_pending), which
   starts a run again when one is due and never writes a lease. A lease that a run's
   `place_pending` could not place, on a host that is already `ready`, is placed by the next run,
   which a `get` of it starts under the same 5-minute rule (pin P3, architect, 2026-11-20, for
   v0.07-T01; D48), so no gap is left for the EC2 driver. A `pending` lease of this driver never
   becomes `unsatisfiable`: a failed run leaves it `pending` (D48, D5). It is given up by
   `release`, which asks the driver to release it, removing anything a killed placement left, and
   the host is kept (pin P16, architect, 2026-11-20, for v0.07-T01; D25). So a driver that holds
   something per lease while it is `pending` now hears of a release through its release operation.
   **Q3, as asked: How does a `pending` lease become `ready` or `unsatisfiable`, and how is one given up?**
   The states allow it ([lease-api.md](lease-api.md#states): "agentspace, when placement
   finishes"), but the boundary has no operation to ask a driver how a placement stands, to cancel
   one, or to hear that it finished. The prototype's driver never answers `pending`, so none has
   been needed, and its `release` of a `pending` lease asks no driver.
4. **Later, not blocking:** [lease-api.md](lease-api.md#concurrency-and-atomicity) fixes what a
   failed renew or release MUST leave, and any of the ways named below meets it, so the mechanism
   is a drop-in's choice.
   **Q4: How does the boundary let the CLI undo a renew, or record before it takes effect, and
   how does release avoid leaving a `ready` lease with part of its credential gone?**
   lease-api.md requires a failed renew to leave the previous certificate, and a failed release
   never to leave a `ready` lease without its credential (#0041). The prototype's driver changes
   the files first, and the CLI records afterwards, so a recording failure leaves them changed,
   and a release that removes the alias and then fails on the folder leaves a `ready` lease with
   no alias. Either a two-step operation (prepare, then commit), an undo, or recording first
   would meet it.
5. **Later, not blocking:** a caller sees the same outcome either way, so where the steps live is a
   drop-in's design choice.
   **Q5: Should the credential and the alias be the CLI's steps, with the driver supplying only
   the login user, the transport and the host-key rule?** In the prototype the driver calls the
   routines that issue the credential and write the alias, though nothing in them is specific to
   EC2 except the user `ubuntu`, the transport and the host-key settings. If they are the CLI's, a
   second driver is smaller and the CLI's output rules apply to them once. If they stay the
   driver's, every driver repeats them. D9 fixes only that the transport is the driver's.
6. **Q6: Closed by [state.md, The alias file](state.md#the-alias-file): how a transport override is
   written into the alias.** It is the alias file's own rule: a line break is refused, and each
   percent sign is doubled, because `ssh` expands them.
7. **Later, not blocking:** v0 has one host, whose rule today's alias carries (D9), so the question
   arises only with a second host.
   **Q7: Is the alias's host-key check the driver's?** Today's alias names the box's known-hosts
   entry and demands a match, which is true only of the one box (D9). A second host needs its own
   rule, and no decision says who supplies it. **For the EC2 driver, D47 supplies it from v0.07:**
   the box's aliases keep `HostKeyAlias agentspace`, and every other host's name its own key in
   agentspace's own known-hosts file ([What is true of today's box](#what-is-true-of-todays-box)).
   Whether the rule belongs to the driver or to the shared alias step stays with
   [Q5](#open-questions).
8. **Q8: Closed for a failed `create` by [lease-api.md](lease-api.md#concurrency-and-atomicity)
   (E0.04-T05-S05, 2026-11-18): what happens when cleanup fails.** What a failed `create` MUST do
   when a removal fails, and what its message MUST name, is lease-api.md's. The prototype
   lets a cleanup's error replace the failure that began it in two cases, and the error is raw: a
   failed credential step whose folder removal then fails, and a failed alias step whose direct
   folder removal fails. This page requires the first failure to be the one reported.
9. **Q9: Closed for contract v0 by [lease-api.md](lease-api.md#exit-codes-and-errors)
   (E0.04-T04-S01, 2026-11-18): where the cause of a failure goes.** Nowhere: the CLI MAY drop it,
   as the prototype does, and MUST NOT put it in any output, or in any file
   [state.md](state.md) lists. "Kept" in
   [Failure](#failure) means kept apart from the message, up to the CLI. Later, not blocking: a
   place to keep it, such as a log ([lease-api.md](lease-api.md#open-questions), Q10).
10. **Later, not blocking:** Windows hosts come with v0.12 (#0051), and until then D4's lease model
    is a Linux user and a certificate.
    **Q10: What does a lease on a Windows host look like at this boundary?** The result carries a
    login user and an alias, which assume an SSH login. D4 says the lease model assumes a Linux
    user and a certificate until v0.12 (#0051), and the vision's V2 asks whether a certificate
    stays the way in. Whether the three operations survive that unchanged is not known.
11. **Later, not blocking:** today's driver needs nothing more than the driver's name, the lease ID
    and the project, and no v0 driver has a per-lease backend.
    **Q11: Where does a driver keep its own per-lease state?** The CLI hands back only the driver's
    name, the lease ID and the project. Today's driver needs nothing more. A per-lease backend (a
    container, D6) would need an identity to find the compute again, and no file is assigned to it
    in state.md.
12. **Later, not blocking:** the 30-second wait is fixed by
    [lease-api.md](lease-api.md#concurrency-and-atomicity), so how long the prototype takes is a
    run's observation, not behavior a drop-in copies.
    **Q12: How long does place take, in the prototype?** It starts a second program for the CA check
    and runs `ssh-keygen` three times, inside the CLI's lock. Only a run shows whether it comes
    near the 30-second wait that a concurrent `renew` or `release` has
    ([lease-api.md](lease-api.md#open-questions), Q11).
13. **Later, not blocking:** [state.md](state.md#release-and-what-it-leaves) states the behavior to
    build, that release fails until a person moves or deletes the file, so another outcome would be
    a later change.
    **Q13: What does a foreign file at a lease's alias place do to its release?** Release refuses
    to delete it and fails, so the lease can never be released until a person removes the file.
    Whether that is the right outcome, or a `released` lease with the file left, is for the
    architect.
14. **Q14: What does a caller see of a connection's own output?** Settled by the architect on
    2026-11-18: D1 holds at connect time too, so the transport MUST keep the backend out of what it
    prints. It is now a requirement in [Reachability](#reachability), carried by card #0087. The
    number is kept so that the questions after it keep theirs.
15. **Later, not blocking:** this page's SHOULD for a driver's name stands either way
    ([The driver's name](#the-drivers-name)), and whether D1 reaches files on the caller's machine
    is policy, not behavior to build.
    **Q15: Does D1 reach files on the caller's machine?** A client cannot tell the backend from the
    CLI's output, but the lease state file stores the driver's name, and the alias file's
    transport line names the ProxyCommand program's full path, which `ssh -G <alias>` prints.
    D1 says a client MUST NOT be able to tell the backend, and does not say whether a client that
    reads those files counts. This page says only that a driver's name SHOULD NOT name its backend.
16. **Q16: Closed (E0.04-T04-S01, 2026-11-18): which limits bind `ssh.user`.** Both stay as
    written ([state.md](state.md#the-lease-state-file)). lease-api.md's wider rule (1 to 64
    letters, digits, `.`, `_` and `-`, not starting with `.` or `-`) is what a caller MUST accept,
    so a later host needs no new contract version. The state schema's narrower one (a lowercase
    letter or `_`, then up to 31 lowercase letters, digits, `_` or `-`) binds a v0 driver's result,
    and a user it refuses fails the `create`. Widening the state schema later is internal, and
    changes no contract.
17. **Later, not blocking:** [The boundary](#the-boundary) states the rule to build, that a started
    program's output is captured for the failure's cause and never shown, so only whether the
    prototype meets it is open.
    **Q17: Does `ssh-keygen`'s error text reach the CLI's standard error in the prototype?** Making
    the key and checking its passphrase capture nothing, so reading says it most likely does. Only
    a run with a failing `ssh-keygen` settles it.
18. **Q18: Closed by [state.md](state.md#creation-in-order) (E0.04-T04-S01, 2026-11-18): a failed
    `create` leaves the `Include` line, a configuration backup and the alias folder it made, and
    MUST NOT remove them.** None is a lease, a key, a certificate or an alias, which is what
    [lease-api.md](lease-api.md#concurrency-and-atomicity) forbids a failed `create` to leave, and
    its message says that no lease was recorded, never that nothing was changed.
