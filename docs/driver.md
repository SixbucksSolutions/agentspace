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
provisioning run and what it records then, and `place_pending`, which a finished run calls. From
v0.09 (#0049) that includes the EC2 driver's sizes, its map from a size to an instance type, when a
host meets a size, and the type a run gives its host (D65, D66, D67, D68, as D71 changed it; card
#0197).

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
- [The EC2 driver](#the-ec2-driver): [what it supports](#what-it-supports),
  [sizes and instance types](#sizes-and-instance-types),
  [where it places a lease](#where-it-places-a-lease),
  [starting a provisioning run](#starting-a-provisioning-run),
  [a resizing run](#a-resizing-run), [`place_pending`](#place_pending),
  [lock order](#lock-order), [ending a lease's connections](#ending-a-leases-connections),
  [conformance cases for v0.07](#conformance-cases-for-v007),
  [conformance cases for v0.08](#conformance-cases-for-v008),
  [conformance cases for v0.09](#conformance-cases-for-v009),
  [conformance cases for v0.10](#conformance-cases-for-v010)
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
the operation is over**: until it has recorded the outcome, or, for a release, until the driver's
release step has returned, since a release's record is written between its two steps
([Release a lease](#release-a-lease); [state.md](state.md#the-lease-state-file) owns the file, and
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
  driver does so from v0.07 for a project whose host is not `ready`, and from v0.09 for one whose
  `ready` host is to be resized (D66): it starts a provisioning run and never waits for it
  ([Starting a provisioning run](#starting-a-provisioning-run)).
- **`get` does not take the lock** ([lease-api.md](lease-api.md#concurrency-and-atomicity)), so
  [`check_pending`](#check_pending) runs outside it, and MUST NOT take it or write the lease state
  file.
- **A process other than the CLI MAY change a lease too:** a finished provisioning run calls
  [`place_pending`](#place_pending), which takes the lease state lock as a changing command does,
  waits for it as one does, and holds it until it has written ([state.md, The lease state
  lock](state.md#the-lease-state-lock)). It takes the lock once per lease it places, or, from
  v0.09, answers `unsatisfiable` (D68), so no hold spans several leases.
- **From v0.09 a provisioning run also takes the lease state lock at its deploy step**, to read
  the lease state file and find the type it gives its host, and holds it from before it removes
  the host's recorded type until it has decided, never during a call to AWS
  ([A resizing run](#a-resizing-run); architect, 2026-11-23, for v0.09-T01, pin P26). It waits for
  the lock as a changing command does, and writes nothing to the file, so a lease command waits
  behind it only while it removes the type and reads the file. And the EC2 driver's place reads
  the file, read-only, under the hold its caller already has
  ([Where it places a lease](#where-it-places-a-lease)).

**What the CLI does, in order, around each operation:**

| Command | In the lock | The driver is asked | Then the CLI |
|---|---|---|---|
| `create` | Reads the records; picks a new lease ID whose alias no kept lease uses | To place the lease | Records the lease, validates the records, writes them |
| `renew` | Finds the lease; checks that it is `ready`, so an expired lease is refused before any driver is asked ([lease-api.md](lease-api.md#agentspace-lease-renew)) | To renew the lease's certificate | Records the new `expires_at` and `ttl_seconds`, writes |
| `release` | Finds the lease; when it will ask the release step, asks first the driver's check step, release's first step ([Release a lease](#release-a-lease)), and fails with `internal` when it fails, with nothing ended; records `released`, drops `ssh`, sets `released_at` (unless it is already), writes | For release's second step, the release step, when its stored state is `ready` or `released`, or `pending` with a `driver` (pin P16, architect, 2026-11-20, for v0.07-T01; [Release a lease](#release-a-lease)): it checks the owner mark again, ends the alias's clients, then deletes the alias file and the lease folder (D59; [the order](#the-order-of-a-release)) | Nothing more: a failure of the release step fails the release with `internal`, and the record stays `released` |
| `get` | Nothing: `get` takes no lock. It reads the records and finds the lease | To check it, when its stored state is `pending` ([`check_pending`](#check_pending)) | Prints the lease as it read it, whatever the check did; writes nothing |

**A failure of the driver ends the command at once, with the records untouched**, but for a
failure of `check_pending`, which `get` drops ([`check_pending`](#check_pending)), and for a
release's, whose record is written `released` first and stays so
([The order of a release](#the-order-of-a-release)). **A failure in
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

**A renew whose driver step succeeded, and whose recording then failed, is a failed renew**
([lease-api.md](lease-api.md#concurrency-and-atomicity), #0041). The prototype leaves the new
certificate in place after such a renew, and deletes the credential before it records a release.
The boundary MUST let the CLI meet lease-api.md's rule for renew, which needs either a way to undo
a driver's step, or recording before the step takes effect. Which is the rewrite's design
([Q4](#open-questions)); this page requires the outcome. **For release it is settled from v0.08**
(architect, 2026-11-21, for v0.08-T01): the CLI records `released` between release's check step
and its release step, so a recording failure leaves the lease's files and connections as they were,
and a failed release step leaves a `released` record that a retry finishes
([Release a lease](#release-a-lease)).

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
| [Release a lease](#release-a-lease) | `release`, and `create` when it fails after a `ready` place | In two steps: a check that the alias's place holds no other lease's file, then the lease's connections on the caller's machine ended and its local credential and alias gone | The lease's stored state is `ready` or `released`, or `pending` with a `driver`, or it has just been placed |
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

**The request carries no plan, and no driver sees one** (D75; architect, 2026-11-24, for v0.10-T01,
pin P26 as amended). A lease made with `--plan` is asked of a driver as one made with `--size` is,
with the size the CLI picked from the plan, so the request, its result and every rule below are as
they were in v0.09. **A lease whose plan no size holds, `plan_too_large`, is answered by `create`
before any driver's place is called**, so place is never called for it; its record still carries
the name of the driver the CLI resolved, since `driver`, optional on a record in general, is
required on every record that holds `sized_for` or `plan`
([state.md, What a plan lease adds](state.md#what-a-plan-lease-adds)).

**The request carries no state folder.** Where the lease's key and certificate go is
[state.md](state.md#a-leases-credential)'s, and the CLI's own state folder
([lease-api.md](lease-api.md#environment)) is the CLI's alone. From v0.07 the driver's context
also holds it, and the EC2 driver uses it only to hand it to a provisioning run it starts, for
[`place_pending`](#place_pending) (pin P12, architect, 2026-11-20, for v0.07-T01). From v0.08 a
driver's release step also appends one line to `release.log` in it
([Where the count is logged](#where-the-count-is-logged)).

**The result, always one of these states:**

| State | Other members | Meaning |
|---|---|---|
| `ready` | Host alias, user, expires at | The lease's credential and alias are in place, and a connection through the alias is expected to succeed until the expiry |
| `unsatisfiable` | Reasons, 1 to 4 | The request cannot be met as asked ([Unsatisfiable requests](#unsatisfiable-requests)). Nothing was issued |
| `pending` | None | Accepted, not yet ready. No credential has been issued. The prototype's driver never answers it; the EC2 driver answers it from v0.07 for a project whose host is not `ready`, and from v0.09 for one whose `ready` host is to be resized for it, or has no recorded type the map holds, or that has a `pending` lease already ([Where it places a lease](#where-it-places-a-lease)) |

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
| `pending` | Records the lease `pending` with the driver's name, and no `expires_at`, `ssh` or `reasons` ([state.md](state.md#the-lease-state-file)); prints it; exits 0. The lease leaves `pending` by the driver's own work, never by the CLI's: for the EC2 driver, a finished provisioning run calls [`place_pending`](#place_pending), which makes it `ready` or, from v0.09, `unsatisfiable` with `no_capacity` (D68), and `get` asks [`check_pending`](#check_pending) in the meantime ([Q3](#open-questions), closed for the EC2 driver) |
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
| The project's host cannot be looked up | From v0.07: the host registry cannot be read or does not validate, the project list, which is read only for a project with no record, cannot be read or does not validate, or the host registry's lock cannot be had within 30 seconds. From v0.09 also: for a project whose record is `ready`, the lease state file cannot be read or does not validate when place reads it for the project's live leases ([Where it places a lease](#where-it-places-a-lease)) | Nothing |
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

**Ends the lease's connections on the caller's machine, and takes the lease's local key,
certificate and alias away, so that no new connection can be made with them.** A driver MAY tear
down compute it made for the lease (D6). Release MUST still end the connections it can, remove the
local key, certificate and alias, and succeed, when the host cannot be reached. Today's driver
tells the host nothing (D59).

**From v0.08, release is one operation in two steps**, which the CLI asks for in turn, in one hold
of the lease state lock (D59; architect, 2026-11-21, for v0.08-T01, pin P23):
1. **The check step** checks that the file at the alias's place, if there is one, is this lease's.
   It changes nothing.
2. **The release step** ends the lease's connections and removes its credential and alias.

**Between the two, the CLI writes the record `released`**, unless it is already, or there is no
record, as for a lease that a failed `create` has just placed. Neither step is an operation of its
own: a driver supplies both as its release, the CLI never asks for the release step without asking
for the check step first in the same hold, and a check step that fails ends the release there. The
EC2 driver's order is [The order of a release](#the-order-of-a-release).

**The request, the same for both steps:**

| Input | Value | Notes |
|---|---|---|
| Driver name | The name stored at place | |
| Lease ID | A lowercase UUIDv4, normalized | |
| Project | The project stored at place | The alias is found from it and the ID. |
| OpenSSH client folder | Optional text | As for place, from the same variable, and absent when it is unset or empty. A check MUST keep the same value from place to release ([lease-api.md](lease-api.md#environment)). |

**The result of each step: nothing.** Today's driver returns a record with absolute paths and
flags, which the CLI discards. A transport that carried it would carry those paths, so a driver
MUST return nothing.

**What a driver does in the check step:**
- **It reads the file at the alias's place.** If it belongs to another lease, or to no lease, the
  step MUST fail with "The alias is another lease's" (below), and MUST end no connection, delete
  nothing and log nothing
  ([state.md, Release, and what it leaves](state.md#release-and-what-it-leaves), step 1, which
  also says what counts as a file of no lease). An alias file that is absent is a
  success.

**What a driver does in the release step:**
- **It first checks the alias's place again**, as the check step does, and fails the same way,
  ending nothing, deleting nothing and logging nothing, so that the release step is safe on its
  own whatever changed the file since (architect, 2026-11-21, for v0.08-T01, pin P23).
- **From v0.08, it then ends the lease's connections on the caller's machine** (D59;
  [lease-api.md, `agentspace lease release`](lease-api.md#agentspace-lease-release)): every client
  there that reaches the lease through its alias, with what carries that client's traffic. Only
  then does it remove the alias, so that the release reaches every connection the alias had before
  its credential goes (architect, 2026-11-21, for v0.08-T01, pin P23). **Ending them is best
  effort:** a connection that cannot be ended MUST NOT fail the release or change what the CLI
  prints, and the driver goes on to remove the alias. A connection that is ended is not restored,
  whatever the rest of the release does. The EC2 driver's way is
  [Ending a lease's connections](#ending-a-leases-connections).
- **It removes the lease's alias next.** An alias file that is absent is a success, and the driver
  goes on to the rest.
- **From v0.08, it then ends the lease's connections a second time** (D59's change for v0.08-T01;
  architect, 2026-11-21, for v0.08-T01, pin P23): any client that came back while the first
  ending ran is ended. This second ending is best effort as the first is. **It runs whether or not
  the alias file's deletion succeeded.**
- **From v0.10, it then ends them again, for as long as the ending before ended a client** (#0196;
  E0.10-T03-S02). A client below a program that has exited, while the client above it still holds
  that program, is no part of the upper client's tree, and is not counted on its own while the
  upper client runs ([Each with its tree](#each-with-its-tree)). So an ending that ends the upper
  client leaves it running, and only a later ending finds it with no client above it; a chain of
  two such programs needs a third ending. Before v0.10 no rule ended such a client after the
  second ending, so it could outlive a release that returned at once, with nothing logged for it.
  v0.08's live release left a proxy holding the alias running a second after it returned, and its
  record does not show which process that was; this is one way it could have happened, and the
  other, a process started after the last ending, is below (finding 1 of
  [its run record](runs/e0.08-t06-s02.md), #0196).
  - **After each ending from the second on that ended at least one client, the release MUST end
    them again, within the bounds below.** It MUST stop at the first ending that ends none, or that
    cannot look for the clients at all.
  - **A release MUST make at most six endings in all, and MUST start none after the second once 10
    seconds have passed since the first started.** Each ending returns within 10 seconds of its own
    start ([Each with its tree](#each-with-its-tree)), so a release's endings take at most 20
    seconds between them, as its two did before v0.10, and a release stays inside the 30-second
    wait of every other changing command ([Ordering and concurrency](#ordering-and-concurrency)).
  - **Each further ending is one call of `end_alias_clients`**, as the second is, and best effort
    as every ending is. A release with no client, or whose second ending ends none, makes two
    endings, as before v0.10.
- **It appends one line to `release.log` in the CLI's state folder**, how many connections all of
  its endings ended between them, in the form of
  [Where the count is logged](#where-the-count-is-logged), whichever driver it is (architect,
  2026-11-21, for v0.08-T01, pin P23). **A release of three endings or more MUST count them as that
  section counts two** (E0.10-T03-S02): `<n>` is the clients every ending that read the process
  list ended, summed; `<k>` is the clients that the last ending that read it could not end, each
  once, so that a client an earlier ending could not end, and a later one ended or found gone, is
  not in it; and `<e>` is the Windows error of the first ending that could not read it.
- **It then removes the lease's key, certificate and anything else it made for the lease,**
  including a leftover temporary file of a renew that was cut short.
- **It does not edit the caller's OpenSSH client configuration.** The line that pulls the alias
  folder in stays ([state.md](state.md#a-leases-credential) says what release leaves).
- **It needs no CA** and runs no CA check, so release works on a machine whose CA is missing.
- **It does not revoke any certificate (D10).** Until v0.08 it closed no open connection either;
  from v0.08 it ends those on the caller's machine, as above, and none on the host (D59).

**A release MUST leave no process holding the alias that its endings can end** (#0196;
E0.10-T03-S02). When the release step returns, every client of the alias that was running when its
last ending read the process list has been ended, or is counted in its `release.log` line as one
that could not be ended ([What cannot be ended](#what-cannot-be-ended)), or is in the tree of a
client so counted. A client, here, is a process that
[Which processes are the alias's clients](#which-processes-are-the-aliass-clients) takes as one;
v0.10 does not change those rules. Two cases are outside this, and the release makes no claim about
them:
- **A process started after the last ending read the process list.** Deleting the alias file does
  not stop every client from coming back: the Claude desktop app's `ssh.exe` names its own
  `claude-ssh-proxy.exe` as its ProxyCommand, which runs the alias's ProxyCommand line from the
  app's own `proxy-command.txt` (decisions.md, D12's findings for E3.03, and D9's for
  E0.02-T03-S01). So the app can start the alias's `agentspace-ssh-proxy.exe` after the alias file
  is gone, and a release cannot end a process that does not yet exist.
- **Endings stopped by the bounds above**, at six or at 10 seconds, while the last still ended a
  client. Only a client that comes back as fast as it is ended can cause it.

**A release that fails part way MUST NOT leave a lease `ready` or `expired` with its alias, key or
certificate partly gone** ([lease-api.md](lease-api.md#concurrency-and-atomicity), #0041). From
v0.08 the two steps meet that (architect, 2026-11-21, for v0.08-T01, pin P23): the CLI records
`released` after the check step and before the release step, so whatever a failed release step
leaves, the record is `released`, and a retry finishes it. The prototype removes the alias, then
the folder, and then the CLI records. A failure to remove the folder, after the alias went, leaves
a `ready` lease with no alias. That is not behavior to copy.

**Idempotence: release MUST be repeatable, and a repeat MUST finish what an earlier one left**,
ending any connection of the lease that remains on the caller's machine as the first did.
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

| Failure | Step | When |
|---|---|---|
| The alias is another lease's | The check step | The file at the alias's place does not name this lease. Nothing is ended, logged or deleted, and the CLI writes no record |
| The alias is another lease's | The release step | The same, found by its own check. Nothing is ended, logged or deleted, and the record stays `released` |
| The credential or alias could not be deleted | The release step | A file cannot be removed. The connections ended stay ended, every ending after the alias file's deletion runs and its line is logged, and the record stays `released` |

A failure's message SHOULD be the wording lease-api.md's table gives for release, and MUST be in
agentspace's own words and MUST NOT carry an absolute path. The prototype lets the underlying
error through, path and all ([Failure](#failure)).

**A connection that cannot be ended is not a failure of release** (D59; architect, 2026-11-21, for
v0.08-T01). Neither is a failure to look for the connections at all. The release goes on, and what
it could not end is logged, never printed
([Ending a lease's connections](#ending-a-leases-connections)).

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
lease as it read it, exits 0, and prints nothing of the failure** (architect, 2026-11-20, for v0.07-T01). So
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
  5-minute rule holds, and no other `get` starts a second run in that time (architect, 2026-11-20,
  for v0.07-T01).
- **It MUST NOT read the CLI's standard input, write to its standard output or standard error, or
  prompt** ([The boundary](#the-boundary)), and what it starts MUST NOT write to them either.
- **It MAY start work that outlives `get`**, which is how the EC2 driver carries on making a host,
  or placing a lease on a host that is already `ready`
  ([Starting a provisioning run](#starting-a-provisioning-run)).
- **Idempotence:** any number of calls is safe. A driver bounds how often a call starts work; the
  EC2 driver starts a run at most once every 5 minutes for a host, unless the last run succeeded
  (D77; [Starting a provisioning run](#starting-a-provisioning-run), step 2).

**When the CLI asks:** once per `get` of a lease whose stored state is `pending`, after it has read
the records and before it prints. `list` never asks, and neither do `create`, `renew` and
`release`. **A `pending` record with no `driver`, or one whose `driver` names a driver the CLI does
not have, is asked of no driver**, and `get` prints it as always (architect, 2026-11-20, for v0.07-T01). Every
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
  [Where it places a lease](#where-it-places-a-lease)). From v0.09 it also answers it for a size
  larger than the recorded type of the project's `ready` host while another live `ready` lease of
  the project holds that host, from place at once ([On a `ready` host](#on-a-ready-host), row 5),
  and from [`place_pending`](#place_pending) after a run (D66, D68; architect, 2026-11-23, for
  v0.09-T01, pins P1 and P2). A host with no recorded type is never refused so: it answers
  `pending`. It applies only to a supported request, so it is never listed beside an
  `unsupported_*` reason.
- **A driver MUST NOT swap one value for another,** whether it is the platform, the profile or
  the size, however close a match it has.
- **A host larger than the size asked for meets the request, and is not a swap (D66).** D5 forbids
  handing back less than was asked. A lease placed on a larger host keeps, as its `size`, the size
  asked for, and nothing in it shows the difference (D1).
- **An unsatisfiable lease needs nothing from any driver afterwards:** no credential, no alias and
  no host were touched, so `release` of it is the CLI's no-op.
- **A request no driver can meet is `unsatisfiable`** (D5: "no backend can meet"). With one
  driver, its reasons are the lease's. With several, how reasons combine is
  [Q2](#open-questions)'s.

**What a driver supports is its own,** and the CLI never reads it. The EC2 driver supports one
platform, one profile and, from v0.09, four sizes ([What it supports](#what-it-supports)). The
contract never says which values a driver supports, so a later driver adds platforms, profiles and
sizes without a new contract version ([lease-api.md](lease-api.md#versioning)). Supporting a size
that v0 answered `unsatisfiable` is not a new contract version either
([decisions.md, v0.09: size classes](decisions.md#v009-size-classes)).

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
v0.07-T01 pinned it, or this page's writer chose it, so that the four specs written in that turn
agree (D55):
- **"Architect, 2026-11-20, for v0.07-T01"** marks such a fact as the architect ruled on it. It
  binds as written.
- **"Architect, 2026-11-21, for v0.08-T01"** marks a fact that D56 to D63 leave open, chosen by
  this page's writer in v0.08-T01 so that the specs of that turn agree, which the architect has
  ruled on. It binds as written. The release's order, its walks in every release, its match rules,
  its best-effort bound and the file `release.log` are such facts. **"Architect, 2026-11-21, for
  v0.08-T01, S03 marks"** marks the same, ruled in a later batch: the destination grammar, the tree
  rules and the count of [Ending a lease's connections](#ending-a-leases-connections).
  **"Architect, 2026-11-21, for v0.08-T01, pin P23"** marks the same, ruled on the review of
  v0.08-T01's driver changes: the release's two steps and the release step's own check of the
  owner mark, the reason for the release's order, its second walk with its own 10 seconds and how
  both walks are counted in one line, what a spared ancestor's tree keeps, that removals which are
  not a release end nothing and log nothing, and that every driver logs to the CLI's
  `release.log`. **"Architect, 2026-11-21, for v0.08-T01, pin P26"** marks the same, ruled on the
  review of those specs: a spared or unendable client is counted once in a release, the two forms
  of `release.log`'s line, and a conformance case for the second walk that assumes nothing about
  timing. Each binds as written.
- **"Architect, 2026-11-23, for v0.09-T01, pin Pn"** marks a fact that D64 to D71 and card #0197
  left open, pinned by the dev lead of v0.09-T01 or chosen by this page's writer so that the specs
  of that turn agree, which the architect has ruled on. It binds as written. Pins P1, P2, P3, P11,
  P15, P18, P20, P26 and P34 are also named in change notes in D66, D67 and D68, and D66's note on
  pins P2 and P3 records pin P2c's fact.
- **"Architect's to confirm"** is the convention for a fact chosen by this page's writer on which
  the architect has not yet ruled. Such a fact binds as written once the architect rules on it,
  and the ruling's citation then replaces the mark. **No fact on this page carries the mark**:
  every fact chosen in v0.09-T01 was ruled on, and carries the citation above.
- **Every fact chosen in v0.08-T01 carries one of the citations above**, so no fact before v0.09
  waits on a ruling.

### What it supports

**One platform, one profile and, from v0.09, four sizes (D5, D65):**

| Request | Supported | Anything else |
|---|---|---|
| Platform | `linux/arm64` | `unsupported_platform` |
| Profile | `agent-linux`, with no version | `unsupported_profile`; `agent-linux@1` is one |
| Size | `s`, `m`, `l` and `xl`, for `linux/arm64` ([Sizes and instance types](#sizes-and-instance-types)) | `unsupported_size`; `xxl` is one |

**Matching is exact on the normalized values.** `windows/amd64`, `linux/amd64`, any profile with a
version, and any size but the four are `unsatisfiable`. Until v0.09 the one supported size was `m`
(D5); from v0.09 the four are, and `m` stays the default (D65,
[lease-api.md, Size](lease-api.md#size)). A request whose platform is not supported is
`unsatisfiable` with `unsupported_platform`, whatever its size, and its size is judged on its own:
an unsupported size adds `unsupported_size`, and a supported one adds nothing (D5; architect,
2026-11-23, for v0.09-T01, pin P9). So `windows/amd64` with size `l` lists `unsupported_platform`
alone, as it did with size `m` before v0.09.

**The reasons' messages are lease-api.md's** ([lease-api.md, Fields](lease-api.md#fields)). The
platform's and the profile's are unchanged. **From v0.09 the EC2 driver MUST emit the
`unsupported_size` message byte for byte as below** (D65, D71; architect, 2026-11-23, for
v0.09-T01, pin P8), and the same holds for D66's `no_capacity` message
([On a `ready` host](#on-a-ready-host)). The other reasons' messages stay as lease-api.md says
they SHOULD read:

```
Size '<size>' is not supported; v0 supports sizes s, m, l and xl.
```

`<size>` is the normalized size, so `--size XXL` gives `Size 'xxl' is not supported; v0 supports
sizes s, m, l and xl.` It replaces v0.08's `Size '<size>' is not supported; v0 supports size m
only.`

**Until v0.07 the driver never answered `no_capacity` or `pending`**; from v0.07 it answers each
for a supported request, by its project's host, and from v0.09 by that host's size too
([Where it places a lease](#where-it-places-a-lease)).

### Sizes and instance types

**The map from a size to an instance type is the EC2 driver's alone** (D67). lease-api.md names a
size by its vCPUs and memory and never by a type ([lease-api.md, Size](lease-api.md#size); D1), and
the operator takes a type and knows no size ([operator.md, deploy](operator.md#deploy)). The four
sizes, smallest first, and each one's type (D65):

| Size | The EC2 driver's type |
|---|---|
| `s` | `m9g.large` |
| `m` | `m9g.xlarge` |
| `l` | `m9g.2xlarge` |
| `xl` | `m9g.4xlarge` |

**`m` stays `m9g.xlarge`**, the type both hosts had before v0.09, so every lease made as before
v0.09 asks for the host it already had (D65). Each step up doubles the vCPUs and the memory, so a
type later in the table is at least as large as an earlier one in both, which is what makes
"meets" below a comparison of places.

**The crate `agentspace-driver-ec2` exposes the map as these four items, with exactly these
signatures** (D71's pins). A builder of v0.09-T02's story MUST give them these names and
signatures, and every other part of agentspace that needs the map MUST call them, never restate the
table:

- **`pub const SIZES: [&str; 4]`** is `["s", "m", "l", "xl"]`, in that order, smallest first. A
  size's place is its index in it.
- **`pub fn instance_type_for(size: &str) -> Option<&'static str>`** returns the table's type for a
  size in `SIZES`, and `None` for any other text. **The size is compared exactly, with its case**,
  as the normalized value the CLI hands a driver ([The boundary](#the-boundary)), so
  `instance_type_for("XL")` and `instance_type_for(" m")` are `None` (architect, 2026-11-23, for
  v0.09-T01, pin P17: the driver is only ever given a normalized size, so an exact comparison costs
  nothing and keeps the function's answer the table's).
- **`pub fn meets(instance_type: &str, size: &str) -> bool`** is true when, and only when, the
  instance type is one the map holds, the size is in `SIZES`, and the type's size is at or after
  that size's place in `SIZES`. **A type the map does not hold meets no size**, `m9g.8xlarge` and
  the empty text among them, and **a size not in `SIZES` is met by no type.** The type is compared
  exactly, with its case, against the table's types (architect, 2026-11-23, for v0.09-T01, pin
  P17, for the reason above: a type comes only from the host registry, which records it as
  `describe-stacks` reads it back, D67). So `meets("m9g.2xlarge", "m")` is true,
  `meets("m9g.xlarge", "l")` is false, and `meets("m9g.xlarge", "m")` is true.
- **`pub fn resize_target(leases: &[agentspace_state::Record], project: &str) -> Option<&'static str>`**
  returns the type of the largest size among the records whose stored `state` is `pending`, whose
  `project` equals `project`, compared exactly as the normalized strings both are, whose `driver`
  is this driver's name ([The driver's name](#the-drivers-name)), and whose `size` is in `SIZES`;
  and `None` when there is no such record. These are the `pending` leases a run would place, and
  the same selection that holds a project's new leases `pending`
  ([On a `ready` host](#on-a-ready-host)), so a record that only a hand edit makes, with another
  driver, no `driver` or a size outside `SIZES`, never sets a host's type (D66; architect,
  2026-11-23, for v0.09-T01, pins P18 and P20). It reads nothing but its arguments, and looks at no
  other member of a record, `created_at` among them. A record of another state, of another
  project, of another driver or with a size outside `SIZES` is ignored. So with `pending` records
  of the project at `s` and `l`, it gives `m9g.2xlarge`.

**What a host's type is, to the driver:** the `instance_type` member of the host's record in the
host registry ([state.md, The host registry](state.md#the-host-registry), which owns the member and
its schema; D67). **A record with no `instance_type` meets no size** (D67), and neither does one
whose `instance_type` the map does not hold. The driver reads the member and never writes it: it is
written by every `deploy` that names or implies a host, `provision`'s deploy step included, after
the stack's deploy succeeds (D71; [operator.md, deploy](operator.md#deploy)). Who removes it, and
when:
- **A run's deploy step removes the member before it decides its type, whether it then passes a
  type or none**, under the lease state lock and `hosts.lock` ([A resizing run](#a-resizing-run);
  architect, 2026-11-23, for v0.09-T01, pin P26). So a run whose deploy step fails, or is killed,
  leaves the record with no type, by design (pin P30).
- **A deploy that is not a run's, a person's, removes the member first, under `hosts.lock`, before
  the stack's deploy runs, only when it passes an `InstanceType`** (D67; architect, 2026-11-23, for
  v0.09-T01, pin P11). **One that passes none keeps the member**, and when it fails keeps the type
  it had, since it changed no type on the stack (architect, 2026-11-23, for v0.09-T01, pin P30).
- **Every deploy writes the type `describe-stacks` reads back once the stack's deploy has
  succeeded**, under `hosts.lock`, holding no lock during the AWS calls but the host's
  `provision.lock`, which every deploy holds for its life (pins P11, P23 and P34). **A
  read-back that fails or is malformed removes the member**, whichever deploy made it (pin P30).

So a record is left with no type, which costs one run, and never with a type the host might not
have: a deploy whose client is killed goes on running on AWS's side, so a kept type could be
larger than the host (D67; pin P11). The detail is [state.md, The host
registry](state.md#the-host-registry)'s and [operator.md, deploy](operator.md#deploy)'s and
[operator.md, provision](operator.md#provision)'s. The driver never asks AWS for a host's type
(D48).

### Where it places a lease

**A supported request is placed by its project's host, found in the host registry
([state.md, The host registry](state.md#the-host-registry)) by its `project`, compared exactly**
(both are normalized; D45, D48; pin P1, architect, 2026-11-20, for v0.07-T01). A missing
`hosts.json` reads as state.md's seed, one record, the box's: project `swimcoach`, stack
`agentbox`, state `ready` (D45). **From v0.09 that implied record has `instance_type`
`m9g.xlarge`, D65's `m`, the template's default and the box's type before v0.09, only when a
missing `hosts.json` is read** ([state.md, The host registry](state.md#the-host-registry); D66;
architect, 2026-11-23, for v0.09-T01, pin P2c). So a laptop with no `hosts.json` places every
lease of `swimcoach` by [On a `ready` host](#on-a-ready-host), as for any `ready` record at
`m9g.xlarge`: a default lease is `ready`, as before v0.09, unless a lease of the project is
`pending`. **The seed record a write puts on disk has no `instance_type`** ([state.md](state.md#the-host-registry)), like any
record a program before v0.09 wrote (architect, 2026-11-23, for v0.09-T01, pin P31): the type is
implied only on a read of a missing file, and is never written from one. So once a write has
made the registry from a missing file, for example the first `create` of a listed project with no
record ([Starting a provisioning run](#starting-a-provisioning-run), step 4), the box's record has
no type until the box's first deploy or run writes the type it reads back (D67), and a lease of
`swimcoach` in between answers `pending` by row 2 of [On a `ready` host](#on-a-ready-host), at the
cost of one run (pin P31). The box is the record whose stack is `agentbox`. The project list is
[state.md, The project list](state.md#the-project-list), and a missing `projects.json` lists
nothing (D54).

| The project's host | Place answers | Issues | Starts |
|---|---|---|---|
| A record in state `ready` | By the size asked for, the host's `instance_type` and the project's other live `ready` leases, as [On a `ready` host](#on-a-ready-host) says: `ready`, `pending` or `unsatisfiable`. A `ready` answer carries the host's key lines in the alias (D47): the box's lease aliases keep v0.06's bytes, `HostKeyAlias agentspace` and no `UserKnownHostsFile` ([state.md, The alias file](state.md#the-alias-file)); every other host's are [state.md, A lease alias for a host that is not the box](state.md#a-lease-alias-for-a-host-that-is-not-the-box)'s | For `ready`, the credential and the alias, by [How it places a lease](#how-it-places-a-lease); otherwise nothing | For `pending`, a resizing run when one is due ([Starting a provisioning run](#starting-a-provisioning-run)); otherwise nothing |
| A record in state `creating` or `failed` | `pending` | Nothing | A provisioning run, when one is due ([Starting a provisioning run](#starting-a-provisioning-run)) |
| No record, and the project is on the project list | `pending` | Nothing | It adds the project's record, state `creating`, and starts a provisioning run when one is due |
| No record, and the project is not on the project list | `unsatisfiable`, with one reason, `no_capacity` (D54) | Nothing | Nothing, and writes nothing to the host registry |

- **A project that has a record is placed by it, listed or not** (D54). The list governs only
  making a host.
- **The `no_capacity` reason's message names the project and no backend (D1)**, and SHOULD read:
  `No capacity can be found for project '<project>': it has no host and is not on agentspace's project list.`
  (architect, 2026-11-20, for v0.07-T01). `<project>` is the normalized project.
- **The host is looked up only for a supported request.** An unsupported one is `unsatisfiable`
  with its `unsupported_*` reasons alone, and place reads neither file; so a laptop with no CA,
  no host registry and no project list still gets that answer, as before v0.07.
- **The registry is read under its lock, `hosts.lock`** ([state.md, The host registry](state.md#the-host-registry)),
  taken as state.md says, for as long as the decision and any write of it take, and released before
  any run is started. It follows the lease state lock's rules, a lock held for longer being waited
  for up to 30 seconds, and place takes it while it holds the lease state lock, as the lock order
  allows ([Lock order](#lock-order)). **When `hosts.json` is missing, an answer that writes
  nothing to the registry, `ready` or row 5's refusal by the box's implied record, MAY be made
  without `hosts.lock`**, as before v0.09; every answer that writes the registry, a `pending` one
  included, takes it, and writes the seed record with no `instance_type` (architect, 2026-11-23,
  for v0.09-T03, pin P42; pins P2c and P31). An answer for a `ready` record needs only the record's
  `state` and, from v0.09, its `instance_type`: place MUST NOT read, check for or depend on the
  record's `instance_id` ([How it is reached](#how-it-is-reached)).
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
  never repaired by agentspace. For the project list it is this page's choice (architect, 2026-11-20,
  for v0.07-T01). Neither file is read as empty: an unreadable list read as empty would answer a
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
  [Reachability](#reachability)). A provisioning run does both, in a process of its own. From
  v0.09 that holds for a resize too: place never changes a host's type itself, and never learns it
  from AWS (D66, D67).

#### On a `ready` host

**From v0.09, place decides a supported request for a project whose record is `ready` by four
things** (D66, D67, and D71's pins):
- **The size asked for**, the request's normalized size, which is one of `SIZES` (an unsupported
  size never reaches here).
- **The host's type**, the record's `instance_type`. **A record with none, or with a type the map
  does not hold, meets no size** (D67; [Sizes and instance types](#sizes-and-instance-types)).
- **Whether another live `ready` lease of the project exists**, read from the lease state file as
  below. "Live" is
  [state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)'s
  word (D56): **a live `ready` lease is a record stored `ready` whose `expires_at` is after the
  reading's time**, so an expired lease is not one. It is of any driver, and a stored `ready`
  record with no `expires_at`, or one that does not parse, which only a hand edit makes past the
  schema, counts as live, as `agentspace_state::is_live` reads it for the guard too (architect,
  2026-11-23, for v0.09-T03, pin P43). A `pending` lease is live, but **a lease that is
  only `pending` does not hold a host's size**, since it has no session there; an expired, a
  `released` and an `unsatisfiable` lease hold nothing (D66). "Of the project" is state.md's: the
  record's `project` equals the request's project, compared exactly. Every such lease in the file
  is another one, since the lease being placed has no record yet.
- **Whether the project has a `pending` lease that a run would place**, read from the same file:
  a record stored `pending`, of the project, whose `driver` is this driver's name and whose `size`
  is in `SIZES`, the selection `resize_target` reads ([Sizes and instance
  types](#sizes-and-instance-types); D66; architect, 2026-11-23, for v0.09-T01, pins P2, P18 and
  P20). A `pending` record with another driver, no `driver` or a size outside `SIZES`, which only
  a hand edit makes, is not one, so it cannot hold the project's leases `pending` for good.

**Where "exactly" means `instance_type` equals `instance_type_for(<size>)`, "larger" means
`meets(<instance_type>, <size>)` and not exactly, and "smaller" means a type the map holds for
which `meets(<instance_type>, <size>)` is false, place answers by the first row of this table that
applies, and by nothing else:**

| Row | When | Place answers | Writes to the host registry | Starts |
|---|---|---|---|---|
| 1 | The project has a `pending` lease that a run would place | `pending` | `last_attempt_at`, only when a run is due, and never `state` | A run, when one is due ([Starting a provisioning run](#starting-a-provisioning-run)) |
| 2 | The record has no `instance_type`, or one the map does not hold | `pending`, whatever other leases hold the host | `last_attempt_at`, only when a run is due, and never `state` | A run, when one is due |
| 3 | Exactly, whatever other leases hold the host, and whether or not a run holds the project's `provision.lock` | `ready`, as before v0.09 | Nothing | Nothing |
| 4 | Larger, and another live `ready` lease of the project exists | `ready`, on the larger host | Nothing | Nothing |
| 5 | Smaller, and another live `ready` lease of the project exists | `unsatisfiable`, with one reason, `no_capacity`, and the message below | Nothing | Nothing |
| 6 | Larger or smaller, and no other live `ready` lease | `pending` | `last_attempt_at`, only when a run is due, and never `state` | A resizing run, when one is due |

- **Row 1: while the project has any `pending` lease, every new lease of it is `pending`, whatever
  the rest of the table says, and the run places it** (D66; architect, 2026-11-23, for v0.09-T01,
  pin P2). A lease placed `ready` on the host's old size while a resize run is due or running
  would otherwise be cut when that run stops the host: with the host at `m9g.xlarge`, a lease
  asking for `l` goes `pending`, and a lease asking for `m` would be placed `ready` and then lose
  its sessions to the run that resizes the host to `l`. **Row 1 starts a run when one is due**, by
  [Starting a provisioning run](#starting-a-provisioning-run)'s steps 2 to 5, the 5-minute rule on
  `last_attempt_at` among them, as every other `pending` answer of place does (architect,
  2026-11-23, for v0.09-T01, pin P27): a run that is due is one that a `get` of the earlier
  `pending` lease would start, so starting it here only starts it sooner; a run that is not due
  starts nothing, and the run under way, or the next one, places the lease.
- **Row 2: a record with no `instance_type`, or with one the map does not hold, answers `pending`
  and starts a run, whatever other leases hold the host, and never `no_capacity`** (D66, D67;
  architect, 2026-11-23, for v0.09-T01, pins P2 and P14: a well-formed type outside the map meets
  no size). The run records the type without resizing when a live `ready` lease holds the host
  ([A resizing run](#a-resizing-run)), and its [`place_pending`](#place_pending) then places the
  lease, or answers it `no_capacity`, by the type it found. So no lease is refused across v0.09's
  install only because its host's type was not yet recorded: each host's first lease after the
  install answers `pending`, and its run records the type (D67). A type the map does not hold
  comes only from a hand edit: of `hosts.json`, or a person's `deploy` with such a type
  ([operator.md, deploy](operator.md#deploy)). When it is larger than the size asked for, the
  refusal text of [`place_pending`](#place_pending), "at a smaller size", is not exact, but its
  bytes stay as D71 pins them. **Row 2 starts a run when one is due**, by the same steps and the
  same 5-minute rule as row 1 (pin P27).
- **Row 3 places a lease at exactly the host's type at once, even while a run holds the
  project's `provision.lock`**, and place never tries that lock for it (architect, 2026-11-23, for
  v0.09-T01, pin P26). A run's deploy step removes the record's `instance_type` and reads the
  lease state file while it holds the lease state lock ([A resizing run](#a-resizing-run)), which
  every `create` holds for its whole place, so a `create` is either wholly before that step or
  wholly after it. **Wholly before**, its `ready` lease is in the file the step reads, so the run
  passes no type and the host is not resized under it. **Wholly after**, the record has no type
  until the run's read-back writes the type the host has, so the `create` answers `pending` by
  row 2, or, once the type is written, is placed on a host already at it.
- **Row 6 resizes a host, in either direction, only while nothing holds it** (D66): a larger
  host is resized down when no live `ready` lease holds it, so a host left at `xl` by one turn does
  not cost four times `m`'s rate for every lease after it. **A host in its grace after its last
  lease (D56) is resized like a stopped one**, since no live `ready` lease holds it. Whether a run
  is due, and what is written, are [Starting a provisioning run](#starting-a-provisioning-run)'s
  steps 2 to 5, for a `ready` record: it writes only `last_attempt_at`, removing `last_success_at`
  in the same write (D77), and its `state` stays `ready` (D48). A run that is not due leaves the lease `pending`, and a later `get` starts one
  ([`check_pending`](#check_pending)). Rows 1 and 2 start a run the same way.
- **Row 5: a smaller host, with another live `ready` lease, is refused at once** (D66). Resizing a
  running host stops it (D39, D40), which ends every session on it, and waiting could last the
  other lease's whole TTL, up to 24 hours (D24).
- **The `no_capacity` message names the project and the size asked for, and what the caller can
  do, and never the backend** (D1, D66). **The EC2 driver MUST emit it byte for byte as below**
  (D71; architect, 2026-11-23, for v0.09-T01, pin P8), with `<project>` the normalized project and
  `<size>` the normalized size:

  ```
  No capacity can be found for project '<project>' at size '<size>': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.
  ```

  It answers at once and changes nothing: no lease folder, no alias, no host registry write and no
  run. **A later `create` is judged afresh**, as for every `no_capacity`
  ([lease-api.md, Fields](lease-api.md#fields)). It is not v0.07's `no_capacity` message for a
  project with no host, which stays as [the bullets above](#where-it-places-a-lease) give it.
- **An `unsatisfiable` answer here runs no CA check**, as every `unsatisfiable` answer does; a
  `pending` answer here checks the CA first, as every `pending` answer does (pin P14.1, architect,
  2026-11-20, for v0.07-T01).
- **The project's leases are read from the lease state file of the driver's context's state
  folder**, the CLI's own ([state.md, The lease state file](state.md#the-lease-state-file)),
  **read-only, under the lease state lock that the CLI, its caller, already holds** (D66: "as the
  guard reads them", with no AWS call, D48). The driver MUST NOT take that lock itself, since its
  caller holds it, and MUST NOT write the file; it MAY read it as the CLI did, in the same hold, so
  it reads what the CLI read. It takes one reading's time for the whole decision, and applies
  expiry by it, as state.md's functions do. **It reads the file for every supported request on a
  `ready` record**, since rows 1 and 4 to 6 depend on it, and for no other request.
  - **A missing lease state file means no leases**, as it does to every reader but the guard and
    `status -All` ([state.md, The lease state file](state.md#the-lease-state-file)).
  - **A lease state file that cannot be read or does not validate fails the place with
    `internal`**, writes nothing and starts nothing, with the host lookup's message,
    `The lease's host could not be looked up: <why>. Nothing was recorded.`, where `<why>` SHOULD be
    `agentspace's lease records could not be read` (architect, 2026-11-23, for v0.09-T01, pin
    P19). The CLI
    validated the file before it asked the driver, so this is a file changed or lost under the
    lock, which only a program other than agentspace's can do.
  - **Leases recorded under another `AGENTSPACE_STATE_DIR` are not seen**, as the guard does not
    see them
    ([state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)):
    a live `ready` lease or a `pending` lease in another state folder neither holds the host nor
    holds this place `pending`. Only a check or a worktree uses another state folder.

### How it places a lease

**Place makes no call to AWS and waits for no host.** For a host that is not `ready` it MAY start a
provisioning run and answers `pending`; for one that is `ready`, it starts nothing before v0.09,
and from v0.09 it MAY start a run in the rows of [On a `ready` host](#on-a-ready-host) that answer
`pending` ([Where it places a lease](#where-it-places-a-lease)). Every step below is on the caller's
machine, and the steps after the first are those of a lease placed `ready`, whether by place or by
[`place_pending`](#place_pending):
1. **Decides** whether the request is supported, and answers `unsatisfiable` with every reason
   when it is not. Nothing has been issued. **From v0.07, it then finds the project's host**, and
   answers `pending` or `unsatisfiable` there, as [Where it places a lease](#where-it-places-a-lease)
   says, unless the host is `ready`. **From v0.09 a `ready` host is decided by
   [On a `ready` host](#on-a-ready-host)**, and only a `ready` answer there goes on to step 2 and
   the steps after it. Before a `pending` answer it runs step 2's CA check, and nothing after it.
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
fails, the credential is removed:** the driver first removes the lease's files as release removes
them, ending nothing and logging nothing ([The order of a release](#the-order-of-a-release), "For
which leases"), and, if that is refused (for example because a foreign file sits at the alias),
removes the lease's own folder directly. It keeps the foreign file. The failure it reports is a
message of its own with the original as the cause.

### Starting a provisioning run

**A provisioning run is `agentspace-operator provision -Host <project>`**, which makes the
project's host by D46's four steps, or runs them again on a host that is already `ready`, each
step being safe to repeat, and ends by placing the project's `pending` leases with
[`place_pending`](#place_pending) (D46, D48). Its steps, its log and its own records are
[operator.md, provision](operator.md#provision)'s. **The driver starts one from place (a `create`)
and from [`check_pending`](#check_pending) (a `get` of a `pending` lease):**
- **From place,** for a project whose record is `creating` or `failed`, or that has no record and
  is on the project list. **From v0.09, also for a project whose record is `ready`, when
  [On a `ready` host](#on-a-ready-host)'s rows 1, 2 or 6 answer `pending`**: the project has a
  `pending` lease a run would place, or the record has no type the map holds, or the host is not
  at exactly the size asked for and no other live `ready` lease of the project holds it (D66, D67).
  Place for a `ready` record answers by that table, and starts nothing in any other row
  ([Where it places a lease](#where-it-places-a-lease)).
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
   **From place, for a `ready` record, [On a `ready` host](#on-a-ready-host)'s decision comes here
   too**, with its read of the lease state file; steps 2 to 5 follow only for its rows 1, 2 and 6,
   and every other row writes nothing and starts nothing.
2. **It decides whether a run is due, by reads alone.** A run is due only when no program holds the
   project's `provision.lock`, and either the record's `last_attempt_at` is absent or at least 5
   minutes before now, or the run that started at `last_attempt_at` succeeded (D77; architect,
   2026-11-24, for v0.10-T01, pins P22 and P24). **That run succeeded when the record's
   `last_success_at` is present and at or after its `last_attempt_at`**, the two compared as
   instants, so equal seconds read as a success. A record that step 4 is about to add has neither.
   The lock is in the host's folder
   ([state.md, A host's folder](state.md#a-hosts-folder)); the driver tries to take it once,
   without waiting, as state.md says the lock is held, and releases it at once if it took it. A
   host's folder or lock file that does not exist means that no run holds it, and the try creates
   neither (architect, 2026-11-20, for v0.07-T01). The folder is made only at the start (below), and the lock
   only by a run.
3. **When a run is due, it checks that the operator program can be found** (the operator
   program, below), and only then. One that cannot be found fails the place with `internal` (architect, 2026-11-20, for v0.07-T01): it is an install
   that waiting will not mend. Nothing is written, so the host registry is as it was, without even
   the record that step 4 would have added. The message is in agentspace's own words, names no
   path, and SHOULD be, by the reason the program cannot be found (architect, 2026-11-20, for v0.07-T01):
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
     the registry's first change (pin P9, architect, 2026-11-20, for v0.07-T01). **The box's record
     it writes is the seed's, with no `instance_type`**, whatever place read for it, since the
     `m9g.xlarge` of a missing file is implied on a read and never written (architect, 2026-11-23,
     for v0.09-T01, pin P31).
   - **When a run is due:** it sets the record's `last_attempt_at` to now and, only when the record
     is new or in state `creating` or `failed`, its `state` to `creating`, in the same write. **The
     same write removes the record's `last_success_at`** when it holds one (architect, 2026-11-24,
     for v0.10-T01, pin P24), so a start in the same second as the success of the run before it
     does not read as a run that has succeeded, since both times can fall in one second.
   - **When no run is due:** it writes `last_attempt_at` into no record, so an added record has
     none, and a record that existed is not written at all.
5. **It releases `hosts.lock`, and only then, when a run is due, starts it, detached** (below).
   When no run is due it starts none, and the lease is `pending` all the same.

- **A record in state `ready` never has its `state` changed by the driver** (architect, 2026-11-20,
  for v0.07-T01; D48). Starting a run for it writes only its `last_attempt_at`, removing its
  `last_success_at` in the same write (step 4), and a start that fails leaves it `ready`. A host that is not `ready` sends its project's `ready` leases to the box,
  where ssh's strict key check refuses them ([How it is reached](#how-it-is-reached)), so a `ready`
  host taken out of `ready` would cut every lease of its project. `creating` and `failed` apply
  only to a host that was never `ready`.
- **So a run is started for a host at most once in any 5 minutes, unless the last run succeeded**
  (D77, changing D48), by any number of `create`s and `get`s: the 5 minutes cover a failed run, a
  run that died with its record still `creating`, a run whose placing failed or left a lease
  (below), and the moment between the start and the run taking its lock. In each of these
  `last_success_at` is absent or before `last_attempt_at`, so the next start waits as before D77.
  **After a run that succeeded, the next start is due at once**, whatever the time since
  `last_attempt_at`, and sets `last_attempt_at` again and removes `last_success_at` as step 4 says,
  so the start after that waits 5 minutes unless that run succeeds in turn.
- **`last_attempt_at` is the start time of the last run**, UTC, in state.md's timestamp form, and
  "now" is the clock of the caller's machine. A run writes its own when it takes its lock, so a
  run by hand counts too ([operator.md, provision](operator.md#provision)).
- **`last_success_at` is the time a run that succeeded ended**, UTC, in the same form, written by
  the run alone, never by the driver, and only as "After the run", below, says. The
  driver reads it at step 2 and removes it at step 4, and writes it nowhere else
  ([state.md, The host registry](state.md#the-host-registry), which holds the host's record).
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
  provision -Host <project>`. **From v0.09 it is unchanged, for a resizing run as for every other**
  (D71, changing D68): the driver passes no `-InstanceType` and no size, and the run finds its type
  itself at its deploy step ([A resizing run](#a-resizing-run)), so a run started by `get` and one
  started by `create` agree. **A `.cmd` or `.bat` program**, its extension compared without regard
  to case, is run through Windows' command interpreter as a ProxyCommand line is:
  `<system folder>\cmd.exe /d /c "<program>" provision -Host <project>`, with `<system folder>` as
  [state.md, The alias file](state.md#the-alias-file) gives it, and a `SystemRoot` that is unset or
  empty makes the program one that cannot be found.
- **These two lines give the arguments, not the command line's bytes** (architect, 2026-11-21, for
  v0.07-T04, on #0155's question). The detached start quotes the program it starts, and quotes an
  argument only when it is empty or holds a space, a tab or a character outside printable ASCII
  (its rule, #0147), so `cmd.exe` is quoted and a `.cmd`'s path without a space is not. **What a
  program MUST receive is exactly the three arguments**, and a `.cmd` or `.bat` whose path holds a
  space MUST still run and receive them. A check asserts those, never the line's quoting.

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
- **It starts in a new process group, with a hidden console window, and its working folder is the
  home folder**, as the CLI has it, so that it never holds a worktree or a checkout open. The
  console is hidden, never absent, because the console programs the run starts (the AWS CLI, ssh)
  would otherwise each open a console window of their own; a new process group keeps a Ctrl+C
  in the CLI's console from reaching it (architect, 2026-11-20, for v0.07-T02, correcting "no
  window" after #0147's review observed it).
- **The driver gives the run none of the CLI's standard handles**, and so never the CLI's streams
  ([The boundary](#the-boundary); pin P14.2, architect, 2026-11-20, for v0.07-T01). The run's
  standard input, standard output and standard error are its hidden console's, and the run MUST
  NOT read its standard input, since nothing will ever write to it. A process created through
  `Win32_Process.Create` cannot be handed a file handle, so the driver MUST NOT open the host's
  `provision.log`, and passes no handle to the run.
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
- **A start that fails changes nothing more, and fails nothing** (architect, 2026-11-20, for v0.07-T01): the
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
  is due, it writes only `last_attempt_at`, removing `last_success_at` in the same write, and
  starts the run, which places the lease.
- **No record, and the project is on the project list:** it adds the record, as place does, and
  goes on by the rule. **No record, and not on the list** (a registry changed by hand since the
  `create`): it does nothing, and the lease stays `pending`. `get` never makes a lease
  `unsatisfiable`.
- **It runs no CA check**: [`place_pending`](#place_pending) checks the CA when it places the lease
  (pin P14.1, architect, 2026-11-20, for v0.07-T01).
- **It never writes the lease state file and never calls [`place_pending`](#place_pending)**, and
  a registry or list that cannot be read is a failure the CLI drops. **A pending lease whose
  driver is missing or unknown** starts nothing ([check_pending](#check_pending)).

**After the run.** A run whose steps before placing succeed records the host `ready`, or keeps it
`ready`, and then calls [`place_pending`](#place_pending) for the project (pin P3, architect,
2026-11-20, for v0.07-T01). A failure of that call is the run's to log, and leaves the host
`ready`; the leases it could not place stay `pending`, and the next run P2's rule allows, started
by a `get` of one of them, places them. From v0.09, `place_pending` also answers `unsatisfiable`,
with `no_capacity`, a `pending` lease that the host does not meet while a live `ready` lease of the
project holds it (D68; architect, 2026-11-23, for v0.09-T01, pin P1).
**A failed run leaves the leases `pending`, never `unsatisfiable`** (D48, D5). It records a
host that was never `ready` as `failed`, and on a host that was `ready` it logs the failed step and
leaves it `ready`. Once 5 minutes have passed since its last start, the run is started again: for a
host that is not `ready`, by a later `create` or `get` of the project, and for a `ready` one, by a
later `get` of a lease that is still `pending`. A caller that gives up releases the lease, and the
host is kept (D25).

**A run that placed everything records its success, and the next start is then at once** (D77;
architect, 2026-11-24, for v0.10-T01, pins P22 and P23):
- **A run has succeeded only when `place_pending` returned without error and placed or answered
  every `pending` lease it found**: none failed (`place failed lease=<id>`) and none was left for a
  later run (`place left lease=<id>`) ([operator.md, provision](operator.md#provision), step 9).
  Only then does the
  run write the host's `last_success_at`, the time it ended
  ([operator.md, provision](operator.md#provision), step 9's end), after `place_pending` returns
  and before the run's last step, with `provision.lock` still held, under `hosts.lock` alone. A
  run whose placing failed as a whole, or failed or left a lease, writes none and still exits 0,
  so the 5 minutes hold: counting it as a success would make every `get` of that stranded lease
  start a run at once, the loop D48's spacing exists to prevent.
- **A failed write of `last_success_at` changes nothing else**: the run logs it and exits as it
  would have, so the next start waits 5 minutes, as after a failed run.
- **A residual, accepted** (pin P22): a `pending` record of a size the driver does not know is
  invisible to `place_pending`, which neither places nor answers it, so a run that placed
  everything else still succeeds and that lease still waits 5 minutes for each start.
- **A lease created after `place_pending`'s last re-read of the lease state file, and before the
  run exits, finds `provision.lock` held, and waits for a `get`** (architect, 2026-11-24, for
  v0.10-T01, pin P31), as it did before v0.10. The run that has just succeeded does not place it,
  and no start is due while the run holds its lock; the `get` that comes after the run exits
  starts a run at once, since the success is recorded.

### A resizing run

**From v0.09 a resize is D48's provisioning run, `provision -Host <project>`, and nothing else**
(D68, as D71 changed it). There is no resizing command and no resizing argument: the run that
makes a host, or runs its steps again on a `ready` host, also gives the host its type. Its steps
are [operator.md, provision](operator.md#provision)'s, and this section says only what the EC2
driver's functions decide in it.

**At its deploy step, the run finds its type itself, from the lease state file, in five steps**
(D68's change notes, D71; architect, 2026-11-23, for v0.09-T01, pin P26, which replaces pin P13's
read of the file without a lock). The run holds its `provision.lock` throughout, and takes the
other two locks in [the lock order](#lock-order): the lease state lock, then `hosts.lock`
([state.md, The order of the locks](state.md#the-order-of-the-locks)). [operator.md,
provision](operator.md#provision) owns the step, its failures and its log lines; this section
states what the driver's functions decide in it, and what place sees of it:
1. **It takes the lease state lock, then `hosts.lock`**, waiting for each as a changing lease
   command does ([state.md, The lease state lock](state.md#the-lease-state-lock); [state.md, The
   host registry lock](state.md#the-host-registry-lock)). The lease state lock is the one of the
   state folder its environment names, as [`place_pending`](#place_pending)'s is
   ([The detached start](#starting-a-provisioning-run); a run by hand uses its own environment's).
2. **It removes the record's `instance_type` and writes the registry, then releases
   `hosts.lock`**, whether it will pass a type or none. From here until step 5 writes the type, a
   place finds the record with no type and answers `pending`
   ([On a `ready` host](#on-a-ready-host), row 2).
3. **It reads the lease state file, under the lease state lock still held, and decides its
   type**, writing nothing to the file:
   - **When a live `ready` lease of the project exists**, by the reading's time and the words of
     [On a `ready` host](#on-a-ready-host), **it passes no `InstanceType`**, whatever
     `resize_target` would give (D68; architect, 2026-11-23, for v0.09-T01, pin P2(a)). So the
     stack's change set holds no resize and the host is not stopped, and step 5 records the type
     it reads back. [`place_pending`](#place_pending) then places or refuses each `pending` lease
     by that type (pin P1). Such a lease can hold the host when the record had no type the map
     holds, as across v0.09's install ([On a `ready` host](#on-a-ready-host), row 2), and for a
     run started by hand.
   - **Otherwise it gives the type `resize_target(<the records>, <project>)` returns**, the type
     of the largest size among the project's `pending` leases that a run would place
     ([Sizes and instance types](#sizes-and-instance-types)), **or none when it returns `None`**.
     This holds for a run that makes a host as for one that runs again on a `ready` host: a host
     that is `creating` or `failed` is made at the size of the project's largest `pending` lease
     (D66's last row; D68), and has no live `ready` lease, since its record was never `ready`.
   - **A missing lease state file counts as no leases.**
   - **A lease state file that cannot be read or does not validate fails the step before any call
     to AWS, with the type already removed by step 2**, which costs one run: the leases stay
     `pending`, and the run fails as a failed deploy step does. The run MUST NOT change a host's
     type on a reading it could not make, since it could not tell whether a live lease holds the
     host, as the guard MUST NOT take such a file for "no live lease" ([state.md, Live leases and
     the lease horizon](state.md#live-leases-and-the-lease-horizon)).
4. **It releases the lease state lock, and only then calls AWS.** Giving a type means passing it
   to the stack's deploy as its `InstanceType`, and giving none means passing none, so that an
   existing stack keeps its type, as a run before v0.09 always did, and a stack the deploy creates
   takes what [operator.md, deploy](operator.md#deploy) gives a new host's stack. A type equal to
   the host's own changes nothing on the stack. The run's command line never carries it.
5. **Once the stack's deploy has succeeded, it writes the type `describe-stacks` reads back** as
   the record's `instance_type`, under `hosts.lock`, holding no lock during the AWS calls but
   the run's `provision.lock` (D67, D71; architect, 2026-11-23, for v0.09-T01, pins P11, P23 and
   P34; [state.md, The host
   registry](state.md#the-host-registry)). It does so on every run, whether it passed a type or
   none, so each host's first run after v0.09's install records its type, whether or not it
   changed it (D67).

**Why the order closes the race** (pin P26). A `create` holds the lease state lock for its whole
place ([Ordering and concurrency](#ordering-and-concurrency)), so it is either wholly before step 1
or wholly after it. **Wholly before**, its `ready` lease is in the file step 3 reads, and the run
does not resize the host under it. **Wholly after**, it finds the record with no type and answers
`pending` by row 2 of [On a `ready` host](#on-a-ready-host), until step 5 writes the type the
host then has. A read of the file without the lock, before the type's removal, would leave a gap
between them: a `pending` lease released and a new lease placed `ready` at the record's old type
inside it would be cut by the resize (D68's change notes).

- **A resize runs provision's steps entire** (D71, changing D68's probe): deploy, start and wait,
  `sync`, `bootstrap`, record, and `place_pending`, as D48's run on a `ready` host already does,
  each step safe to repeat. A faster path is not designed until a live check shows that the time
  matters; D70's checks record it.
- **A run never takes a `ready` host out of `ready`** (D48): a resize keeps the record `ready`
  throughout. **A run's deploy step that fails or is killed after step 2 leaves the record with no
  type, by design** (architect, 2026-11-23, for v0.09-T01, pin P30), whether it passed a type or
  none, and so does a read-back that fails or is malformed (pin P11). This changes D68's "a failed
  deploy leaves the type it had": a killed client's deploy goes on running on AWS's side, so a
  kept type could be larger than the host. The failure is logged in `provision.log`, **with the
  leases `pending`** for the next run (D68), which a `get` of one of them starts under the 5-minute
  rule; a record left with no type answers a new lease by row 2 of
  [On a `ready` host](#on-a-ready-host), and its next run records the type, which costs one run.
  A person's deploy that passes no `InstanceType` and whose stack deploy fails, before any
  read-back, keeps the type, since it is not a run's deploy step and removed nothing; one whose
  read-back fails or is malformed removes it, as every deploy's does
  ([Sizes and instance types](#sizes-and-instance-types); pin P30).
- **The type is read once, at the deploy step.** A `pending` lease that arrives after that read,
  and that the resized host does not meet, is not met by this run: its
  [`place_pending`](#place_pending) answers it `unsatisfiable` when a live `ready` lease then holds
  the host, and otherwise leaves it `pending` for the next run, which resizes the host to it (D68;
  pin P1).
- **The guard and the idle check meet a resize as they meet any provisioning run** (D68), by rules
  that are not this page's. **The guard MUST NOT stop a host while its run holds `provision.lock`**
  ([operator.md, guard](operator.md#guard); D56), which covers every step of a resize. Outside the
  lock, the `pending` lease that started the run explains the host, and the horizon of a project
  whose only live leases are `pending` is the reading's time
  ([state.md, Live leases and the lease horizon](state.md#live-leases-and-the-lease-horizon)), so
  neither the guard nor the horizon power-off stops a host that waits on its resize. **The stop and
  start a resize makes is the stack's, not the guard's**: a boot clears the host's lease horizon,
  the next guard run sends it again, and the idle check keeps D11's 60-minute rule until then
  ([operator.md, The idle check and the lease
  horizon](operator.md#the-idle-check-and-the-lease-horizon)).
  A host that a run leaves running and that no lease ends up holding is stopped by the guard a
  grace later, as any unexplained host is (D56).

### place_pending

**Places every `pending` lease of one project that its host meets, once the host is `ready`: the
transition lease-api.md names "`pending` to `ready`, by agentspace, when placement finishes"**
(D48; card #0143). **From v0.09 it places only the leases the host meets, and answers
`unsatisfiable`, with `no_capacity`, a lease the host does not meet while a live `ready` lease of
the project holds the host**, lease-api.md's transition from `pending` to `unsatisfiable` by the
same words (D68). It is a
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
   **A record that is not `ready`, or none, places nothing**, and the call succeeds. **From v0.09
   it keeps the record's `instance_type`**, or that there is none, for the whole call: the run
   that calls it holds `provision.lock` and has already recorded the type its deploy step set
   ([A resizing run](#a-resizing-run)).
2. **Places the project's `pending` leases that the host meets one at a time, each in a hold of its
   own of the lease state lock** (pin P14.4, architect, 2026-11-20, for v0.07-T01), so that no
   hold spans several placements, and a `create`, `renew` or `release` waits behind one placement
   at most. For each lease in turn:
   1. **Takes the lease state lock**, and waits for it as a changing lease command does, up to 30
      seconds ([state.md, The lease state lock](state.md#the-lease-state-lock)). One it cannot
      take fails the call, and every lease not yet placed stays `pending`.
   2. **Reads the lease state file again**, as every lease command does, so a lease released or
      changed since the last hold is seen as it now stands. One that cannot be read, or does not
      validate, fails the call: it releases the lock, and changes nothing more.
   3. **Picks the first record, in the file's order, whose stored state is `pending`, whose
      `project` is the project, whose `driver` is this driver's name, that this call has not
      tried yet, and, from v0.09, whose `size` the host meets**, `meets(<instance_type>, <size>)`
      with the type step 1 read ([Sizes and instance types](#sizes-and-instance-types)). When
      there is none, it releases the lock and goes on to step 3. A record with no
      `instance_type` in step 1 meets nothing, so step 2 then places nothing.
   4. **Removes that lease's leftover credential files first** (pin P14.5, architect, 2026-11-20, for v0.07-T01): a
      key, certificate or alias file for its ID, removed as release removes them
      ([Release a lease](#release-a-lease)), ending nothing and logging nothing
      ([The order of a release](#the-order-of-a-release), "For which leases"). A `pending` lease
      has none by contract, so any there can only be a placement's that was killed part way. A
      removal that fails, a foreign file at the alias's place among them, is a failure of this
      lease.
   5. **Issues the lease's credential and alias** by steps 2 to 5 of
      [How it places a lease](#how-it-places-a-lease): the CA check, the key, the certificate and
      the alias, with the record's lease ID and project, its `ttl_seconds` as the TTL, and the
      host's key lines ([Where it places a lease](#where-it-places-a-lease)). **A CA that fails its
      check fails this lease** (pin P14.1, architect, 2026-11-20, for v0.07-T01), and MUST NOT be created.
   6. **Writes the lease state file with that record `ready`**: it adds `expires_at`, the
      certificate's valid-before time, and `ssh`, the alias and the user `ubuntu`, and changes no
      other member, `created_at`, `ttl_seconds` and `driver` included. The file is checked against
      its schema before the write, and replaced atomically, as every write of it is
      ([state.md](state.md#the-lease-state-file)). One write per lease (architect, 2026-11-20, for v0.07-T01), so
      that a failure costs only that lease.
   7. **A failure at step 4, 5 or 6 leaves that lease `pending`**, with every file this call made
      for it removed as a failed place removes them ([Place a lease](#place-a-lease)), and is
      named in the call's result for the run's log. When the write fails, the driver removes the
      lease's files as release removes them, ending nothing and logging nothing
      ([The order of a release](#the-order-of-a-release), "For which leases").
   8. **Releases the lease state lock**, and goes on to the next lease.
3. **From v0.09, answers the project's `pending` leases that the host does not meet, one at a time,
   each in a hold of its own of the lease state lock** (D68; architect, 2026-11-23, for v0.09-T01,
   pin P1). `place_pending` answers them itself, at the run's end, after step 2 has placed every
   lease the host meets, so that whether a lease is refused never depends on its place in the
   file. For each lease in turn:
   1. **Takes the lease state lock**, as step 2.1 does. One it cannot take fails the call, and every
      lease not yet answered stays `pending`.
   2. **Reads the lease state file again**, as step 2.2 does, and fails the call the same way.
   3. **Picks the first record, in the file's order, whose stored state is `pending`, whose
      `project` is the project, whose `driver` is this driver's name, whose `size` is in `SIZES`,
      that the host does not meet, and that this step has not judged yet.** When there is none, it
      releases the lock, and the call is done. A record whose `size` is not in `SIZES`, which only
      a hand edit makes, is neither placed nor answered, and stays `pending` until it is released:
      answering it `no_capacity` would name capacity for a size no backend supports. It is not a
      lease a run would place, so it holds no new lease of the project `pending` and sets no type
      (D66; architect, 2026-11-23, for v0.09-T01, pin P20; [On a `ready` host](#on-a-ready-host),
      row 1).
   4. **Decides by the file it has just read, at a reading's time taken in this hold:**
      - **When a live `ready` lease of the project exists**, by the words of
        [On a `ready` host](#on-a-ready-host), **and the record step 1 read holds a type the map
        holds**, the lease is answered: it removes the lease's
        leftover credential files first, as step 2.4 does, ending nothing and logging nothing
        (architect, 2026-11-23, for v0.09-T01, pin P21), and then writes the record
        `unsatisfiable`, with `reasons` holding exactly one reason, code `no_capacity`, with the
        message of [On a `ready` host](#on-a-ready-host), byte for byte, for the lease's project
        and its `size`. It adds `reasons` and changes `state`, and no other member, `created_at`,
        `size`, `ttl_seconds` and `driver` included; the file is checked against its schema and
        replaced atomically, as every write of it is ([state.md, The lease state
        file](state.md#the-lease-state-file)). A removal or a write that fails leaves the lease
        `pending`, and is named in the call's result as step 2.7's failures are.
      - **Otherwise** the lease is left `pending`, written nothing, and named in the call's result
        as left for the next run, which, with no live `ready` lease holding the host, resizes it to
        the largest `pending` lease ([A resizing run](#a-resizing-run)).
      - **A record with no `instance_type`, or with one the map does not hold, is not known to be
        smaller than any lease, so its unmet leases are left, never answered**, whatever leases
        hold the host (architect, 2026-11-23, for v0.09-T03, pin P40; D66's change: no lease is
        refused only because its host's type was not recorded). It arises when a run's read-back
        failed, and the next run, which records the type, decides them.
      - **A lease step 2 has just placed `ready` holds the host for this step** (pin P41): it is a
        live `ready` lease of the project in the file this step reads, so an unmet lease is
        answered beside it.
   5. **Releases the lease state lock**, and goes on to the next lease.

- **A lease's TTL starts when it becomes `ready`** (D48): its `expires_at` is its certificate's
  valid-before time, so the time it spent `pending` costs its caller none of it.
- **Every other record is left as it was**: a lease released while it was `pending`, another
  project's, and another driver's.
- **It answers `unsatisfiable` only at step 3, only with `no_capacity`, and only for a lease its
  host, at a type the map holds, does not meet while a live `ready` lease of the project holds the
  host** (D68; pin P40; architect,
  2026-11-23, for v0.09-T01, pin P1). This is the one exception to v0.07's rule that it never
  answers `unsatisfiable`. It starts nothing, makes no call to AWS, and never writes the host
  registry. **A failed run never calls
  it**, so a failed run still leaves every lease `pending` (D48).
- **Its result, to the run that called it,** names each lease it placed by ID, each it could
  not by ID with the failure's message, and, from v0.09, each it answered `unsatisfiable` by ID
  and each it left `pending` for the next run by ID, for the run's log, whose lines are
  [operator.md, provision](operator.md#provision)'s. A refused lease's reason and its message are
  in the lease's record, and its log line carries no reason field (architect, 2026-11-23, for
  v0.09-T01, pin P37). Each message meets
  [Failure](#failure)'s rules. The call as a whole fails only at step 1, or at the lock or read of
  step 2 or step 3; a lease it placed or answered before such a failure stays as it was written.
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
  at once if it took it ([Starting a provisioning run](#starting-a-provisioning-run), step 2), only
  to decide whether a run is due: from v0.09 its choice of row on a `ready` host tries no lock
  ([On a `ready` host](#on-a-ready-host), row 3; architect, 2026-11-23, for v0.09-T01, pin P26),
  and a row that starts a run tries it only as step 2 does for any `pending` answer.
  `check_pending`, holding `hosts.lock` alone, MAY try it the same way.
- **From v0.09, place reads the lease state file while it holds the lease state lock and
  `hosts.lock`**, the first held by the CLI, its caller, and takes no lock for that read
  ([On a `ready` host](#on-a-ready-host)). Reading a file is not taking a lock, so the order is
  unchanged: the driver MUST NOT open `leases.lock` itself, which its own process already holds.
- **`hosts.lock` follows the lease state lock's rules** ([state.md, The lease state
  lock](state.md#the-lease-state-lock)): the open handle is the lock, the file is never deleted,
  and a lock held by another process is waited for up to 30 seconds. **The one exception is
  `check_pending`, which tries it once without waiting** (pin P12, architect, 2026-11-20, for
  v0.07-T01).
- **`provision.lock` is taken without waiting by every program**, and a run holds it for its life
  ([state.md, A host's folder](state.md#a-hosts-folder)). From v0.09 a person's `deploy` of the host
  holds it for its life too ([operator.md, deploy](operator.md#deploy); architect, 2026-11-23, for
  v0.09-T01, pin P34), so the driver finds it held then as it does during a run, and starts nothing.
- **A run ends in this order:** under `hosts.lock` alone it records the host `ready`, or keeps it
  `ready`, with the instance ID it learned, and releases that lock; it then calls
  [`place_pending`](#place_pending), which takes `hosts.lock` alone for its read of the host
  record, and then the lease state lock once per lease, in its step 2 and, from v0.09, its step 3;
  it holds `provision.lock` throughout. `place_pending` reads the lease state file again in each
  hold, so a lease released meanwhile is never placed or answered. **From v0.10, once
  `place_pending` has returned, a run that succeeded writes `last_success_at` under `hosts.lock`
  alone** ([After the run](#starting-a-provisioning-run), "A run that placed everything records its
  success"; architect, 2026-11-24, for v0.10-T01, pin P23), still holding `provision.lock` and no
  lease state lock, and then ends.
- **From v0.09, a run's deploy step takes the lease state lock, then `hosts.lock`, in this
  order** ([A resizing run](#a-resizing-run), steps 1 to 4; architect, 2026-11-23, for v0.09-T01,
  pin P26, which replaces pin P13's read without a lock). It removes the host's `instance_type`
  and writes the registry, releases `hosts.lock`, reads the lease state file under the lease state
  lock still held and decides its type, and releases the lease state lock before any call to AWS.
  It holds no lock of this page during the AWS calls, and its write of the type after the deploy
  is under `hosts.lock` alone (D67, D71; pin P11; [operator.md, provision](operator.md#provision);
  [state.md, The order of the locks](state.md#the-order-of-the-locks)), as its write of the
  instance ID is. Holding `provision.lock`, it waits for the two later locks only in the order,
  so the order is kept.
- **So a run, from v0.09, takes its locks in this order:** `provision.lock` for its life;
  `hosts.lock` alone to record its start; at its deploy step, the lease state lock, then
  `hosts.lock` within it to remove the type, released before the read, then the lease state lock
  alone for the read of the lease state file; none during the deploy and the read-back;
  `hosts.lock` alone to record the type, and the instance ID, after the deploy; `hosts.lock` alone
  to record `ready`; then `place_pending`'s `hosts.lock` alone and its lease state lock once per
  lease; and, from v0.10, `hosts.lock` alone to record the success. Besides `provision.lock`, it holds at most two locks of this page at a time, and both
  only at its deploy step's start. How [operator.md, provision](operator.md#provision) groups its
  own writes of the record under `hosts.lock` is that page's, within this order.

### Renew and release

**Renew** checks the CA, signs a copy of the lease's public key under a temporary name inside the
lease's folder, and then moves the new certificate over the old one, so the old certificate stays
until the new one exists. The temporary files are removed whatever happens. It does not touch the
alias.

**Release**, from v0.08, checks in its check step that the alias file, if there is one, belongs to
this lease. Its release step checks that again, ends the alias's clients on the laptop, removes
the alias file, ends any client of the alias again, from v0.10 ends them again while the walk
before ended a client (D59, as changed for v0.10-T03), logs all its walks' counts in one line, and
then removes the lease's whole folder
([Ending a lease's connections](#ending-a-leases-connections)). If the alias file belongs to
another lease or to none, either step fails, ends nothing, logs nothing and deletes nothing. If it
is absent, release goes on, and still walks for the alias's clients at least twice.
It runs no CA check and starts no program. It does not touch the OpenSSH client configuration, and
it tells the lease's host nothing (D59).

**Both are the same for a lease on any host** (D48): neither reads the host registry or the project
list, neither starts a run, and a lease that [`place_pending`](#place_pending) placed is renewed
and released as one place made `ready`.

### Ending a lease's connections

**From v0.08, the EC2 driver's release ends the alias's clients on the laptop before it deletes the
alias, and again after, once or, from v0.10, more times** (D59 and its changes, among them D59 as
changed for v0.10-T03; D63; card #0064; [The order of a release](#the-order-of-a-release)). Each
time it calls `end_alias_clients(<alias>)` of the crate `agentspace-connections` (D63), which
returns `Result<usize>`, and it logs what the calls returned in one line. `<alias>`
is the lease's alias, `agentspace-<project>-<first 8 hex digits of the lease ID>`
([lease-api.md, Fields](lease-api.md#fields)), built from the request's project and normalized
lease ID, as release builds it to find the alias file.

**Why it exists** (D59): until v0.08 a released lease's connections stayed open until the host
stopped or its sshd dropped them (#0064), so a lease given back still held sessions on its host.
Every client of a lease is on the laptop today (D8): the Claude desktop app's connections, a
person's `ssh`, and the keeper's holder. Ending them there needs no call to the host, so a release
stays laptop-local and works on a stopped host (D59). **The host's sshd drops the far end of each
ended connection on its own**, within about 90 seconds, by the `ClientAlive` bound every host has
(D59), since the Session Manager tunnel never passes a client's close on.

#### The order of a release

**All of it under the lease state lock, in this order** (D59 and its change for v0.08-T01;
architect, 2026-11-21, for v0.08-T01, pin P23):
1. **The CLI asks the driver's check step**, which checks the alias file's owner mark
   ([Release a lease](#release-a-lease);
   [state.md, Release, and what it leaves](state.md#release-and-what-it-leaves), step 1). An
   alias file of another lease or of none fails the release with `internal`, as before v0.08, and
   **the release MUST NOT end, log or delete anything**.
2. **The CLI writes the record `released`**, unless it is already.
3. **The CLI asks the driver's release step.** It checks the owner mark again, as step 1 did, and
   a mark that now fails fails the release with `internal`, with nothing ended, logged or deleted,
   and the record left `released`. Otherwise, in this order:
   1. **The first walk:** it ends the alias's clients, by the rules below, with one call of
      `end_alias_clients`.
   2. **It deletes the alias file**, as before v0.08 ([Release a lease](#release-a-lease);
      [state.md](state.md#release-and-what-it-leaves)). An alias file that is absent is a success.
   3. **The second walk:** it ends the alias's clients again, by the same rules, with a second
      call of `end_alias_clients`. It runs whether or not the alias file's deletion succeeded.
   4. **Further walks, from v0.10:** while the walk before ended at least one client, it MUST walk
      again, one call of `end_alias_clients` each, and stop at the first walk that ends none or
      cannot read the process list, at six walks in all, or once 10 seconds have passed since the
      first walk started (D59, as changed for v0.10-T03;
      [Release a lease](#release-a-lease), #0196). A release whose second walk ends none makes two
      walks, as before v0.10.
   5. **It appends one line to `release.log`**, which counts all its walks
      ([Where the count is logged](#where-the-count-is-logged)).
4. **The release step then deletes the lease's folder**, as before v0.08, when the alias file's
   deletion succeeded. The alias file MUST NOT be deleted after the folder, so that an alias never
   points at a key that is gone: a release whose alias file could not be deleted deletes nothing
   more.

- **A failure at step 2 fails the release with `internal`**, as any failed write of the records
  does: no release step is asked, so nothing is ended, logged or deleted.
- **A failed deletion, of the alias file at step 3 or of the folder at step 4, fails the release
  with `internal`, as before, and the clients the walks ended stay ended**
  ([lease-api.md, Concurrency and atomicity](lease-api.md#concurrency-and-atomicity)). A failed
  alias deletion still has its second walk, its further walks and its line. A retried release runs
  step 3 again, and ends any client that has appeared since.
- **Ending the clients never fails the release**, whatever any call of `end_alias_clients`
  returns ([What cannot be ended](#what-cannot-be-ended)); only the checks of the owner mark and
  the deletions can.
- **The release's document is unchanged** (D59; contract v0): the count and every failure of
  any walk are logged, and reach neither its standard output, nor its standard error, nor its
  exit code.

**Why the clients go first, why there is a second walk, and why there can be more** (D59's change
for v0.08-T01; architect, 2026-11-21, for v0.08-T01, pin P23; D59, as changed for v0.10-T03). The
clients are ended before the alias file is deleted so that the release reaches every connection
the alias had before its credential goes: a client is found by the alias in its command line, and
the alias and key are what let it connect. **The first walk alone would leave a window**, from a
client's end to the alias file's deletion, up to the first walk's 10 seconds, in which a client
that reconnects by itself, the desktop app's most likely, can come back through the alias while
the credential still exists. **The second walk narrows it:** it runs once the alias file is gone,
so it ends any client that came back through the alias while the first walk ran. **It does not
close it:** deleting the alias file does not stop every client from coming back, since the
desktop app can start the alias's `agentspace-ssh-proxy.exe` from its own `proxy-command.txt`
after the file is gone, and a client started after the last walk read the process list is outside
what a release claims ([Release a lease](#release-a-lease), #0229). **The further walks** end a
client that no walk before could reach: one below a program that has exited while the client above
it still holds that program, which is in no client's tree and is not counted on its own while the
client above runs ([Each with its tree](#each-with-its-tree), #0196). E0.08-T06-S02's live check,
which looks for the alias's clients after the release, shows the result with the desktop app.

**For which leases.** Step 3 runs in every release the driver is asked for, whatever files remain:
a `ready` lease, an `expired` one, a `released` one released again, a `pending` one that carries a
`driver`, and one that a failed `create` just placed ([Release a lease](#release-a-lease), "When
the CLI asks"). So a release repeated after an earlier one left a client running ends it, as
[lease-api.md's `released` row](lease-api.md#agentspace-lease-release) says.
- **A `pending` lease's release ends nothing** (architect, 2026-11-21, for v0.08-T01). Its alias was
  never handed to a caller: place answered `pending` with no alias, and a placement killed part way
  returned nothing, so no client of that alias exists. Step 3 runs all the same, its first two
  walks find no client, so it makes no further walk, and it logs that it ended none. The driver does not tell a `pending` lease from
  a `ready` one: the request carries no state ([Release a lease](#release-a-lease)).
- **A lease that a failed `create` just placed** ends nothing the same way: its alias was never
  printed.
- **A release the CLI asks no driver for ends nothing and logs nothing**: an `unsatisfiable` lease,
  a `pending` one with no `driver`, and a `pending` one released with no home folder.
- **Three removals of a lease's files are not a release, and remove files only: they end nothing
  and log nothing** (architect, 2026-11-21, for v0.08-T01, pin P23). They are place's removal of
  the credential when its alias step fails ([How it places a lease](#how-it-places-a-lease)), and
  [`place_pending`](#place_pending)'s steps 2.4 and 2.7. Each removes the lease's files as the
  release step removes them, walks for no client, and appends no line to `release.log`. No client
  of those aliases can exist, since none of them was ever handed to a caller. **From v0.09
  `place_pending`'s step 3.4 is a fourth, of the same kind** (architect, 2026-11-23, for
  v0.09-T01, pin P21): its removal of
  a lease's leftover files before it answers the lease `unsatisfiable` ends nothing and logs
  nothing, for the same reason.

#### Which processes are the alias's clients

**A process on the laptop is one of the alias's clients when every one of these holds, and only
then** (D63's pin; the comparison is architect, 2026-11-21, for v0.08-T01, and the grammar
architect, 2026-11-21, for v0.08-T01, S03 marks):
1. **It runs as the user the release runs as** (architect, 2026-11-21, for v0.08-T01): its token's
   user is the release process's own. A process whose user cannot be read is not a client, and is
   not reported. A process of another user is never one.
2. **Its image file name is `ssh.exe` or `agentspace-ssh-proxy.exe`**, compared without regard to
   ASCII case. The image file name is the last part of the full path of the executable the process
   runs, never its first command-line token, and the folder it is in does not matter: Windows'
   OpenSSH, another build of `ssh.exe`, and an install folder of agentspace's all count. A process
   whose image file name cannot be read is not a client, and is not reported.
3. **Its command line names the alias**, read from the process and split into tokens as Windows'
   `CommandLineToArgvW` splits it, by rule 1 of
   [keeper.md, The holder command](keeper.md#the-holder-command). The first token is the program.
   - **An `ssh.exe` is a client when its destination's host is the alias**, by
     [the grammar below](#an-ssh-clients-destination).
   - **An `agentspace-ssh-proxy.exe` is a client when exactly one token follows the program, and
     that token is the alias.** That is the form the alias's ProxyCommand line gives it
     ([state.md, The alias file](state.md#the-alias-file);
     [operator.md, The ProxyCommand](operator.md#the-proxycommand)).
   - **A command line that cannot be read**, because the process cannot be opened to read it or
     has exited, is not a client, and is not reported.

**A client that is the release's own process or one of its ancestors MUST NOT be ended**
(architect, 2026-11-21, for v0.08-T01, pins P23 and P26). It is still a client by the rules above,
and the release counts it once, as a client that could not be ended
([What cannot be ended](#what-cannot-be-ended)): the first walk counts it, and a later walk,
which finds the same process again, does not count it again ([The count](#the-count)). The
release's line therefore says `1 could not be ended` for it
([Where the count is logged](#where-the-count-is-logged)).
- **The ancestors are found by parent process ID**, walked up from the release's own process, each
  opened and checked to have been created no later than the process below it, as the trees below
  are checked. **A process on that walk that cannot be checked**, because it cannot be opened or
  its creation time cannot be read, **is treated as an ancestor**, and the walk stops there.
- **A spared client's tree is left whole: a descendant of it MUST NOT be ended** (architect,
  2026-11-21, for v0.08-T01, pin P23). A spared client is a client that is the release's own
  process or one of its ancestors, and only such a client's tree is spared: an ancestor that is
  not a client, such as the desktop app or the shell, spares nothing, so the desktop app's own ssh
  clients are still ended (architect, 2026-11-22, for v0.08-T02, #0174 Q1). So the release's own
  process, and any process beside it under a spared client, are never ended, nor is a client
  inside that tree, which is not counted on its own ([Each with its tree](#each-with-its-tree)).
- **A client whose tree holds the release's own process or a spared client is spared too**
  (architect, 2026-11-22, for v0.08-T02, #0174 Q2): it is an ancestor that the walk up could not
  reach, and is counted once as a client that could not be ended. **An ancestor's process ID that
  no process holds ends the walk up there**, and that ID is spared nowhere, so a client that later
  reuses it is not taken for an ancestor.

**The alias is compared whole, without regard to ASCII case** (architect, 2026-11-21, for
v0.08-T01). Whole: a longer or shorter text never matches, so `agentspace-foo-1234abcd` matches
neither `agentspace-foo-1234abcd0` nor `agentspace-foo-1234`, and another lease's alias never
matches. Without regard to ASCII case, so that any spelling `ssh` could have resolved to the
alias's `Host` block is ended; no two kept leases' aliases differ in case alone, since the
project is normalized and the ID's hex digits are lowercase
([lease-api.md, Fields](lease-api.md#fields)).

**Nothing else is a client.** In particular:
- **A program of another name is not one by itself**, even when the alias is in its command line:
  `scp.exe`, `sftp.exe`, the Claude desktop app's own `claude-ssh-proxy.exe`, and a `cmd.exe` that
  runs `agentspace-ssh-proxy.cmd`. Each reaches a host only through an `ssh.exe` or ProxyCommand
  that is a client, or is a descendant of one, and is ended in that client's tree.
- **The keeper's holder is a client** (D59's change): it is an `ssh.exe` whose destination is the
  alias, `<SshExe> -N -o ExitOnForwardFailure=yes <alias>`
  ([keeper.md, The holder command](keeper.md#the-holder-command)). The release ends it with the
  rest, reads and writes none of the keeper's files, and leaves the keeper to find its holder gone
  at its next pass, with its release grace
  ([keeper.md, Release grace](keeper.md#release-grace)). A holder that the keeper started through
  `cmd.exe` for a `.cmd` `-SshExe`, which only a check does, is not an `ssh.exe`, and is not a
  client.
- **The Claude desktop app's connections are clients**: each is an `ssh.exe` whose destination is
  the alias, with its own ProxyCommand that runs the alias's line, and so the alias's
  `agentspace-ssh-proxy.exe`, as a descendant (D9's findings, E0.02-T03-S01).

#### An ssh client's destination

**The destination is read as OpenSSH's `ssh` reads its arguments, so that an option's value is
never taken for the destination** (architect, 2026-11-21, for v0.08-T01, S03 marks). The tokens
after the program are read in order:
1. **The option letters that take a value are** `B`, `b`, `c`, `D`, `E`, `e`, `F`, `I`, `i`, `J`,
   `L`, `l`, `m`, `O`, `o`, `P`, `p`, `Q`, `R`, `S`, `W` and `w`. **Those that take none are** `1`,
   `2`, `4`, `6`, `A`, `a`, `C`, `f`, `G`, `g`, `K`, `k`, `M`, `N`, `n`, `q`, `s`, `T`, `t`, `V`,
   `v`, `X`, `x`, `Y` and `y`. Letters are compared with their case. **A letter in neither list is
   read as one that takes no value**, so a later `ssh` with a new flag before the destination is
   still read. `P` takes a value only from OpenSSH 9.4 on (`-P tag`); an older `ssh.exe` refuses
   it, so `ssh -P <alias>` read as having no destination is a miss on the safe side.
2. **A token that is `-` followed by at least one character, and is not `--`, is a run of option
   letters**, read from left to right. A letter that takes no value is read and the next is read.
   A letter that takes a value takes the rest of the token as its value when any of it is left
   (`-p22`, `-oPort=22`, `-vp22`), and otherwise the whole next token, whatever it holds, a
   leading `-` and the alias included (`-J <alias>`, `-o <value>`, `-F <file>`). A letter that
   takes a value with no token left after it leaves the line with no destination.
3. **`--` ends the options**: the token after it is the destination.
4. **Otherwise the first token that is neither a run of option letters nor an option's value is
   the destination.** A lone `-` is such a token. **No token after the destination is compared**,
   whatever it holds: `ssh` reads options after the destination too, and then the remote command,
   and neither changes the destination.
5. **The host is taken from the destination:**
   - **When it starts with `ssh://`**, compared with its case: the text after `ssh://` and up to
     the first `/`, or to the end when there is none, is the authority, and what follows it is
     either nothing or a single `/`; anything else leaves the line with no host. In the
     authority, the text up to and including its last `@` is the user, and is dropped. What is
     left is the host and an optional port: when it starts with `[`, the host is the text inside
     the brackets; otherwise it is the text up to the first `:`, or all of it.
   - **Otherwise** the host is the text after the destination's last `@`, or the whole
     destination when it holds none.
6. **A line with no destination, or no host, is not a client.**

**Examples, for the alias `agentspace-foo-1234abcd`, where `<a>` stands for it:**

| Command line | A client |
|---|---|
| `ssh.exe <a>` | Yes |
| `"C:\Windows\System32\OpenSSH\ssh.exe" -T -o ProxyCommand="x y" -o ClearAllForwardings=yes <a>` | Yes |
| `ssh.exe -N -o ExitOnForwardFailure=yes <a>`, the keeper's holder | Yes |
| `ssh.exe ubuntu@<a>`, and `ssh.exe ssh://ubuntu@<a>:22` | Yes |
| `ssh.exe -l ubuntu -p 22 <a> uname -a`, and `ssh.exe -s <a> sftp` | Yes |
| `ssh.exe -vvv -oPort=22 AGENTSPACE-FOO-1234ABCD` | Yes |
| `ssh.exe -- <a>` | Yes |
| `ssh.exe <a> -v`, an option after the destination | Yes |
| `ssh.exe -J <a> other`, `ssh.exe -F <a> other`, and `ssh.exe -i<a> other` | No: the alias is an option's value |
| `ssh.exe other <a>`, and `ssh.exe other -- <a>` | No: the alias is in the remote command |
| `ssh.exe <a>0`, `ssh.exe agentspace-foo-1234`, and another lease's alias | No |
| `ssh.exe ssh://<a>/path` | No: no host |
| `ssh.exe -p` | No: no destination |
| `scp.exe <a>:file .` | No: not an `ssh.exe` |
| `agentspace-ssh-proxy.exe <a>` | Yes |
| `agentspace-ssh-proxy.exe <a> extra`, and `agentspace-ssh-proxy.exe` alone | No |

#### Each with its tree

**Each client is ended with its process tree, and the call MUST NOT end any other process** (D30;
architect, 2026-11-21, for v0.08-T01, S03 marks).

**What carries over from [keeper.md, Stopping a holder](keeper.md#stopping-a-holder)**, a client
taking a holder's place:
- **The descendant walk and its checks**: the descendants are found by parent process ID, each
  opened and checked on its own handle, its parent process ID its parent's and its creation no
  earlier than its parent's. A descendant that cannot be opened, or fails its checks, MUST NOT be
  ended, and is not reported.
  - **A descendant that has exited is not taken into the tree either, and the walk does not go
    below it**, so nothing below it is in the tree, even while the process above it still holds it
    open. A client below it is then in no client's tree, and is reached only on its own, by a later
    walk of the release ([Release a lease](#release-a-lease), #0196).
- **The handles held**: every handle is held until the call ends, so no process ID in a tree is
  reused meanwhile.
- **Leaves first**: each tree is ended leaves first, the client last.
- **Exit code 1**: a process ended is given exit code 1.

**What does not carry over:**
- **"Nothing is ever matched by command line alone."** A client here is found by its command line,
  by the rules above; its descendants are still found only by the walk.
- **The keeper's wait of 2 seconds per process, for up to 5 rounds.** This call's waits are
  bounded as the last bullet below says.

**And for this call:**
- **A client is opened by its process ID, and checked again on that handle** by rules 1 to 3
  above, before anything is ended, so the process checked is the process ended.
- **The tree takes in the ProxyCommand and what it runs**, among them the AWS CLI, the Session
  Manager plugin and the nested ssh of `up`'s steps
  ([operator.md, The ProxyCommand](operator.md#the-proxycommand)), and the desktop app's own proxy.
- **A client inside another client's tree** (an `agentspace-ssh-proxy.exe` that an `ssh.exe`
  client started, most often) **is ended as part of that tree, once, and is not counted on its
  own.** The clients are counted by the processes that are a client and have no client among their
  ancestors.
- **The call MUST NOT end the release's own process, or any of its ancestors**, wherever they sit,
  nor a descendant of a spared ancestor
  ([Which processes are the alias's clients](#which-processes-are-the-aliass-clients)).
- **After the ends, it waits for the processes to exit, and walks each tree again for a child
  made meanwhile, at most 3 rounds in all. Every wait shares one deadline, 10 seconds from the
  call's start, and the call MUST return by it.** **Each of a release's walks is its own call,
  with its own 10 seconds** (architect, 2026-11-21, for v0.08-T01, pin P23): a later walk's
  deadline counts from its own start, never from the first's. No walk after the second starts
  once 10 seconds have passed since the first started (D59, as changed for v0.10-T03;
  [Release a lease](#release-a-lease)), so a release's walks take at most 20 seconds between them,
  as its two did before v0.10, and a release stays inside the 30-second wait of every other
  changing command ([Ordering and concurrency](#ordering-and-concurrency)). A client still running
  at its walk's deadline has not been ended by that walk.

#### The count

**`end_alias_clients` returns `Ok(<n>)` when every client it found was ended, or had exited, and
`<n>` is how many it ended** (D63; the rest of this section is architect, 2026-11-21, for
v0.08-T01, S03 marks).
- **A client is ended** when the call terminated it and it exited within the deadline.
- **A client gone already is not a failure, and is not counted**: one that exits before the call
  opens it, that has exited when it is opened, or whose termination is refused because it is
  exiting and that has then exited. A client that exits during the walk is the same.
- **Descendants are never counted**, ended or not, and one left running is not reported.
- **`Ok(0)` is the common answer** for a release with no client: a lease that was never connected,
  a `pending` lease, and a repeat release. It is also the second walk's common answer, since the
  first has ended every client it found, and the last walk's in a release of more than two walks,
  unless the bounds stopped it or that walk could not read the list: a release stops walking at
  the first walk that ends none (D59, as changed for v0.10-T03; [Release a lease](#release-a-lease)).
- **A release's count is all its walks' together** (architect, 2026-11-21, for v0.08-T01, pins P23
  and P26, for two walks; D59, as changed for v0.10-T03, for more): the clients ended are every
  call's ended counts summed, and the clients that could not be ended are counted once each in the
  release.
  - **A process counted as could-not-be-ended in one walk is not counted again in a later one.**
    The walks tell it by process ID and creation time together, both equal. A spared ancestor, and
    a client that no walk could end, are found by each walk and counted once.
  - **A client an earlier walk could not end and a later walk ends, or finds gone, is counted as
    ended by the walk that ended it, or not at all when it was gone, and no longer as
    could-not-be-ended**: a release's `<k>` holds only the clients that the last walk that read
    the process list could not end (the dev lead, 2026-11-22, for v0.08-T01, under pin P26, for
    two walks; E0.10-T03-S02 for more).
  - **The release holds those identities, the process ID and creation time of each client a walk
    could not end, in memory until its line is written, and never logs, prints or stores
    them** ([What cannot be ended](#what-cannot-be-ended)).
  - **The release logs the sums in one line**
    ([Where the count is logged](#where-the-count-is-logged)).

#### What cannot be ended

**`end_alias_clients` returns an error in two cases, and the release goes on in both**
(architect, 2026-11-21, for v0.08-T01):
- **The process list cannot be read at all.** Nothing is ended, and the error says why, with
  Windows' error number and never its localized text.
- **At least one client could not be ended**: its termination was refused, by access denied among
  others, it was still running at the deadline, or it is the release's own process or ancestor.
  Every other client is still ended. The error carries two counts, the clients ended and the
  clients that could not be, and the process ID and creation time of each client that could not be
  ended, which the release uses to count that process once across its walks
  ([The count](#the-count); architect, 2026-11-21, for v0.08-T01, pin P26).

**Each of those errors, and an `Ok`, is a walk that read the process list or one that could not**
(architect, 2026-11-21, for v0.08-T01, pin P26). An `Ok`, and an error with counts, are walks that
read it. An error with no list read is a walk that could not. **A walk that could not read the
list never makes the release throw away another walk's counts**
([Where the count is logged](#where-the-count-is-logged)).

**A client that cannot be ended, or a list that cannot be read, MUST NOT fail the release, change
its document, or change its exit code** (D59: the release's document is unchanged; lease-api.md's
best effort). The driver goes on with the release in its order: after the first walk it deletes
the alias and walks again, after the second it walks again while the walk before ended a client,
and after its last walk it logs all its walks and deletes the key and the certificate
([The order of a release](#the-order-of-a-release)).
- **Why not fail the release:** a release that failed on a client it could not end would leave
  the lease's credential in place, which is what a release exists to remove, and a person could
  not release the lease until the client went. A client missed costs at most a grace of running
  time, since the guard stops a host whatever sessions it holds (D56, D59).
- **What the call MUST NOT reveal:** neither its result nor its error, nor anything it logs, holds
  a process's command line, image path, user or environment. Only counts, the lease ID and a
  Windows error number leave it, bar the process IDs and creation times of the clients it could
  not end, which go to the release and stop there. A command line holds a ProxyCommand's paths and, for the
  operator's own calls, AWS parameters (AGENTS.md: a live run MUST mask what it reads).
- **It writes nothing to the CLI's standard output or standard error**, as no driver step does
  ([The boundary](#the-boundary)).

#### Where the count is logged

**In `<state folder>\release.log`, one line per release step, which counts all of its walks**
(D59 leaves the place to lease-api.md, which names this section; the file, its name and its lines
are architect, 2026-11-21, for v0.08-T01; one line for both walks is architect, 2026-11-21, for
v0.08-T01, pin P23, and one line for all of them, from v0.10, D59, as changed for v0.10-T03). `<state folder>` is the lease CLI's state folder, which the driver's context
holds ([lease-api.md, Environment](lease-api.md#environment); [state.md](state.md)), so a check's
throwaway state folder holds its own log. **Every driver's release step appends its line to this
file, in this form** (architect, 2026-11-21, for v0.08-T01, pin P23), so the log is the CLI's
whichever driver placed the lease. A driver that does not call `end_alias_clients` writes the
text of the row its own outcome matches.
- **Its form is `guard.log`'s and `provision.log`'s:** UTF-8 without a byte order mark, each line
  ending in one LF, created when missing, opened for append sharing read, write and delete, and
  never rotated or removed by agentspace.
- **Each line is a UTC time, `YYYY-MM-DDTHH:MM:SSZ`, one space, `lease=<id>` with the normalized
  lease ID, one space, then one of two forms** (architect, 2026-11-21, for v0.08-T01, pin P26;
  the line's earlier rule, that either walk failing made the whole line "could not be looked for",
  is replaced by it, so that no count is thrown away):
  - **When at least one walk read the process list**, the text is `Ended <n> of the lease's
    connections`, then `; <k> could not be ended` when `<k>` is above 0, then `; a look failed:
    Windows error <e>` when a walk could not read the list, and last a `.`. **The `Ended` part is
    always there, `Ended 0` included.**
  - **When no walk could read the process list**, the text is
    `The lease's connections could not be looked for: Windows error <e>.` Only a release of two
    walks can give it, since a release makes no walk after a second or later walk that cannot
    read the list ([Release a lease](#release-a-lease)).

  | What the release's calls of `end_alias_clients` returned | The text |
  |---|---|
  | Every walk read the list, and no client could not be ended | `Ended <n> of the lease's connections.` |
  | Every walk read the list, and `<k>` clients could not be ended | `Ended <n> of the lease's connections; <k> could not be ended.` |
  | At least one walk read the list, at least one could not, and no client could not be ended | `Ended <n> of the lease's connections; a look failed: Windows error <e>.` |
  | At least one walk read the list, at least one could not, and `<k>` clients could not be ended | `Ended <n> of the lease's connections; <k> could not be ended; a look failed: Windows error <e>.` |
  | No walk could read the list | `The lease's connections could not be looked for: Windows error <e>.` |

  - **A walk that read the list** is an `Ok` or an error with counts. **`<n>` is the clients the
    walks ended, summed, and `<k>` the clients that could not be ended, each counted once
    in the release** ([The count](#the-count)): those that the last walk that read the list could
    not end. A walk that could not read the list adds nothing to either. Both are decimal counts,
    `<n>` 0 included.
  - **`<e>` is Windows' error number alone, and the first failing walk's** when more than one
    failed. A walk that failed never throws away another walk's counts: the first walk's ended
    clients stay in `<n>` when the second walk could not read the list, the second's stay when the
    first could not, and every earlier walk's stay when a further walk could not (D59, as changed
    for v0.10-T03).
  - **For example, with a made-up time and ID,** a release whose second walk could not read the
    list, after a first that ended 3 clients and could not end 1, and the same one whose two walks
    both could not read it, with an invented error number:
    `2000-01-01T00:00:00Z lease=00000000-0000-4000-8000-000000000000 Ended 3 of the lease's connections; 1 could not be ended; a look failed: Windows error 5.` and
    `2000-01-01T00:00:00Z lease=00000000-0000-4000-8000-000000000000 The lease's connections could not be looked for: Windows error 5.`
- **The line is written once, after the last walk returns and before the lease's folder is
  deleted** ([The order of a release](#the-order-of-a-release)), under the lease state lock, so two
  releases of one state folder never write at once.
- **A log that cannot be opened or written is not a failure**: the release goes on, its document
  and exit code are as they would be, and nothing reaches its standard error.
- **Why a file of its own:** `provision.log` is a host's run's, and a release MUST NOT need a host's
  folder or make one; `board-holder.log` is the keeper's, and `guard.log` the guard's, whose lines
  are operator.md's. The lease CLI writes no log before v0.08
  ([lease-api.md, Open questions](lease-api.md#open-questions), Q10, is the causes' log, still
  later). state.md lists the file with the lease CLI's others.
- **It carries the lease ID and never the alias**, as the keeper's log carries lease IDs. A live run
  that prints a line of it masks the ID, as every lease ID is masked (AGENTS.md).

#### How it is checked

- **Through the lease CLI with stand-ins**:
  [Conformance cases for v0.08](#conformance-cases-for-v008), E0.08-T04-S02.
- **Through the crate's own tests** for what no check can set up on the laptop: another user's
  process, a process whose termination is refused, a client that exits during the walk, and a
  process list that cannot be read. They run `end_alias_clients`'s logic over a stand-in process
  list, built with the crate (D63, T02). **A crate test that calls the real walk**, over the
  laptop's own process list, **MUST use an alias built from a lease ID that is a UUIDv4 drawn at
  random for that run, never a fixed ID**, so that no process of Terry's can match it, and MUST
  NOT print, log or format any process's command line it reads.
- **Through the crate's own tests for the walks' line**: the line each pair of the walks'
  results gives, in the forms of [Where the count is logged](#where-the-count-is-logged), among
  them a walk that could not read the list beside one that ended clients, and a process counted
  once whichever walk found it (architect, 2026-11-21, for v0.08-T01, pin P26); and, from v0.10,
  the line of three walks or more, summed and counted as that section says (E0.10-T03-S02).
- **Through E0.08-T03-S04 (#0180)'s crate test for the second walk itself** (architect, 2026-11-21,
  for v0.08-T01, pin P26): a test-only pause between the first walk and the alias file's deletion,
  which exists only under `cfg(test)`, lets the test start a client in that gap and show that the
  second walk ends it. No check through the lease CLI can show which walk ended a client, so
  R15 does not ([Conformance cases for v0.08](#conformance-cases-for-v008)).
- **Through E0.10-T03-S02's crate tests for the further walks** (D59, as changed for v0.10-T03;
  #0196): the bounds, six walks and 10 seconds, and the stop at a walk that ends none or cannot
  read the list, over stand-in results; and a chain of clients, each below a stand-in program that
  has exited while the client above still holds it, released over stand-in processes, with every
  client ended when the release returns and its line counting each. Two walks leave the last of a
  chain two deep running, which that test shows on the code before E0.10-T03-S02.
- **Live, with Terry's desktop app**, D62's "A release ends its connections" (#0064), in
  E0.08-T06-S02 on agentspace's host: a desktop-app session left open on a lease, the lease
  released, then no `ssh.exe` or `agentspace-ssh-proxy.exe` client of that alias left on the laptop,
  by the rules above, reported as a count and never as command lines; `release.log`'s line for the
  lease; and no session of that lease on the host 2 minutes later.

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
never the `state` of a record that is `ready` (D48). **It never writes a record's `instance_type`**,
which every `deploy` that names or implies a host writes, `provision`'s deploy step included (D67,
D71); place and `place_pending` only read it. **Its writes of the host registry keep every record's
`instance_type` exactly as they read it from the file, and a record it adds has none** (architect,
2026-11-23, for v0.09-T01, pin P12; [state.md, The host registry](state.md#the-host-registry)).
**An implied record's type is never written:** with no `hosts.json`, the box's record it writes is
the seed's, with no `instance_type` (pin P31). From v0.09, place's start of a resizing run for a
`ready` record writes that record's `last_attempt_at` alone, as `check_pending`'s start does. The
lease state file is the CLI's, but for [`place_pending`](#place_pending)'s writes of the leases it
places and, from v0.09, of those it answers `unsatisfiable` (D68).

**From v0.08, its release also appends one line to `<state folder>\release.log`**, how many of the
lease's connections its walks ended ([Where the count is logged](#where-the-count-is-logged);
architect, 2026-11-21, for v0.08-T01, and for the two walks, pin P23; from v0.10, for all of a
release's walks, D59, as changed for v0.10-T03). It writes no other log.

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
reads a host record's `state` and, from v0.09, its `instance_type`, and nothing more of it
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
- **Each host stops itself when idle,** and the transport starts it (D11, D51). From v0.08 the
  laptop's guard also stops a host once no live lease of its project explains it, and a host powers
  itself off past its lease horizon (D56, D57); a release's part in that is to end the lease's
  connections on the laptop ([Ending a lease's connections](#ending-a-leases-connections)).
- **Each host's size is its stack's `InstanceType`, one of the four types of
  [Sizes and instance types](#sizes-and-instance-types) from v0.09** (D65, D67). Before v0.09 both
  hosts were `m9g.xlarge`, sized by hand (D39); from v0.09 a lease's size resizes its project's host
  through a provisioning run, while no live `ready` lease holds it
  ([A resizing run](#a-resizing-run)), the box included (D67). The box is not resized in v0.09's
  live checks (D70). With no `hosts.json`, the box's record reads as having `instance_type`
  `m9g.xlarge`, and a registry written from a missing file holds the box's record with no type
  (pins P2c and P31, [Where it places a lease](#where-it-places-a-lease)).

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

### Conformance cases for v0.08

**Each behavior below can be shown through the lease CLI with stand-ins, and MUST be a conformance
case of E0.08-T04-S02** (D59, D62, D63). Every case shows a rule that is architect, 2026-11-21,
for v0.08-T01, R1, R3 and R4 by its S03 marks, and R7's spared ancestor's child, R15's second walk
and every line that counts two walks by pin P23, and R7's count of one and R15's lack of any
timing assumption by pin P26. E0.08-T03-S04 (#0180) builds what they show.
Each runs in the harness's throwaway home ([conformance.md](conformance.md)).
- **The stand-in clients are programs the harness builds**, each of which waits until it is ended,
  whatever its arguments, and starts any child its check asks for. A check copies one into a folder
  of its own in its throwaway home, under the image file name it needs, `ssh.exe` or
  `agentspace-ssh-proxy.exe`, and starts it with the command line the case gives. **A check MUST
  NOT copy, open or start the installed programs as stand-ins**, nor any `ssh.exe` of Windows'
  (AGENTS.md, Terry's real files).
- **A stand-in is told what to do by a file beside its program**, which the check writes before it
  starts it and the stand-in reads once, at its start: which children to start, each with its
  image file name and command line, for R2, the file to write and what to watch, and for R15, the
  one child to start once its first is ended. Its command
  line is the case's alone, and tells it nothing; the file's form is the harness's.
- **A stand-in's command line MAY name another program than its image**: a check starts it with
  its own image and the case's command line, first token included. So the quoted row of R3, whose
  first token is `C:\Windows\System32\OpenSSH\ssh.exe`, runs the stand-in, never Windows' own
  `ssh.exe`, and shows that the image file name is read from the process and never from the first
  token.
- **Every stand-in a check starts, the check ends at its close**, so a failed case leaves none
  running.
- **A check asserts on the stand-ins it started, by process ID, and on nothing else of the laptop's
  process list.** It MUST NOT list, print or log any other process's command line.
- **A case that starts a stand-in MUST make its leases with `create`, or seed a lease ID that is a
  UUIDv4 drawn at random for that run, and MUST NOT use a fixed ID**, so that no process of
  Terry's can match the case's aliases.
- **`<a>` is the released lease's alias**, and `<b>` the alias of another `ready` lease in the same
  state folder. Where a case uses the examples table, `<a>` stands in for its example alias: in
  capitals for the row that writes it so, and cut short by its last four characters for the row
  whose alias is shorter.

| Case | What it shows | Where the rule is |
|---|---|---|
| R1 | `release` of a `ready` lease with a stand-in `ssh.exe <a>` running, which has a stand-in `agentspace-ssh-proxy.exe <a>` as its child and a grandchild of any name: all three are ended; exit 0; the document is the one v0.07's `release` prints for the lease, and standard error is empty; `release.log` gains exactly one line, `lease=<id>` and `Ended 1 of the lease's connections.` | [Each with its tree](#each-with-its-tree), [The count](#the-count) |
| R2 | The order: a stand-in `ssh.exe <a>` that writes, every 100 milliseconds, whether the alias file exists, has written that it does in its last line before it was ended; the alias file and the lease folder are gone after the `release` | [The order of a release](#the-order-of-a-release) |
| R3 | Every `ssh.exe` command line of the table's "Yes" rows in [An ssh client's destination](#an-ssh-clients-destination), each a stand-in of its own, is ended by one `release`, and the line logs their count | [An ssh client's destination](#an-ssh-clients-destination) |
| R4 | Every `ssh.exe` command line of the table's "No" rows, each a stand-in of its own, is still running after the `release`, and so are `ssh.exe <b>`, `agentspace-ssh-proxy.exe <b>`, and a stand-in under another image file name whose only argument is `<a>` | [Which processes are the alias's clients](#which-processes-are-the-aliass-clients) |
| R5 | A stand-in `agentspace-ssh-proxy.exe <a>` with no `ssh.exe` above it is ended, and counted; `agentspace-ssh-proxy.exe <a> extra` is not | [Which processes are the alias's clients](#which-processes-are-the-aliass-clients) |
| R6 | A stand-in in the keeper's holder form, `ssh.exe -N -o ExitOnForwardFailure=yes <a>`, is ended, and the keeper's files in the throwaway state folder are byte for byte as they were | [Which processes are the alias's clients](#which-processes-are-the-aliass-clients) |
| R7 | A `release` started as the child of a stand-in `ssh.exe <a>`, which has a second stand-in child of any name beside it, leaves that stand-in and that second child running, succeeds with exit 0 and its usual document, deletes the alias file and the lease folder, and logs `Ended 0 of the lease's connections; 1 could not be ended.`, since the spared stand-in is counted once in a release, though both walks find it (architect, 2026-11-21, for v0.08-T01, pin P26); the check's own process is never ended | [Which processes are the alias's clients](#which-processes-are-the-aliass-clients), the release's own process and its ancestors |
| R8 | `release` of a `released` lease, with a stand-in `ssh.exe <a>` started after the first `release`, ends it, exit 0, prints the lease with its first `released_at`, and logs a line | [The order of a release](#the-order-of-a-release), for which leases |
| R9 | `release` of a `pending` lease that carries a `driver` ends nothing (a stand-in `ssh.exe <b>` keeps running), exit 0, its document as v0.07's, and logs `Ended 0 of the lease's connections.` | [The order of a release](#the-order-of-a-release) |
| R10 | `release` whose alias file belongs to another lease fails with `internal` as before v0.08, a stand-in `ssh.exe <a>` keeps running, and `release.log` gains no line | [The order of a release](#the-order-of-a-release), step 1 |
| R11 | With `release.log` made a folder by the check, `release` still ends the stand-in, succeeds with exit 0 and the same document, and writes nothing to standard error | [Where the count is logged](#where-the-count-is-logged) |
| R12 | `release` with no client running succeeds as v0.07's did and logs `Ended 0 of the lease's connections.`, and an `unsatisfiable` lease's `release` logs no line | [The order of a release](#the-order-of-a-release) |
| R13 | With ten stand-in clients of `<a>`, `release` returns within 15 seconds of its start, and ends all ten | [Each with its tree](#each-with-its-tree) |
| R14 | `release` of an `expired` lease, one made with the shortest TTL, 60 seconds, and released once its `expires_at` has passed, with a stand-in `ssh.exe <a>` running: the stand-in is ended, exit 0, the lease is printed `released` with its `expires_at`, the alias file and the lease folder are gone, and `release.log` gains `Ended 1 of the lease's connections.` | [The order of a release](#the-order-of-a-release), for which leases |
| R15 | The second walk (#0183), with no timing assumed (architect, 2026-11-21, for v0.08-T01, pin P26): a stand-in under another image file name, standing for the desktop app, which is itself no match, starts a stand-in `ssh.exe <a>` as its child and, once that child is ended, starts exactly one new matching client, a second `ssh.exe <a>`, outside the first one's tree, so that nothing respawns after the release. After one `release`: no stand-in `ssh.exe <a>` is running, the parent stand-in still is, exit 0, and `release.log` gains one line that is `Ended <n> of the lease's connections.` with `<n>` at least 2. The case never asserts which walk ended the second client, and never repeats a run to wait for a timing. **It holds only when the parent stand-in starts its second client within the release's run**, which a whole release, at about 150 milliseconds, makes a matter of milliseconds: a second client found started after `release` returned is reported SKIPPED with that reason, never as a failure, and the stand-in SHOULD start it with a direct process creation, warmed, not through a script (architect, 2026-11-22, for v0.08-T04, on #0183's finding). E0.08-T03-S04 (#0180)'s crate test proves the second walk itself, with a test-only pause between the first walk and the alias file's deletion that exists only under `cfg(test)` ([How it is checked](#how-it-is-checked)) | [The order of a release](#the-order-of-a-release), the second walk; [Where the count is logged](#where-the-count-is-logged) |

**What no stand-in shows,** and so the crate's own tests show
([How it is checked](#how-it-is-checked)): another user's process, a termination refused, a client
that exits during the walk, a process list that cannot be read, the line's forms that say a look
failed, and the second walk itself, which only a pause between the walks can separate from the first
(architect, 2026-11-21, for v0.08-T01, pin P26). **The desktop app's own
connections** are shown only live, in E0.08-T06-S02 (D62).

### Conformance cases for v0.09

**Each behavior below can be shown through the lease CLI with stand-ins, and MUST be a conformance
case of E0.09-T04-S01** (card #0205; written by #0197; D70, D71). Each runs in the harness's throwaway home
([conformance.md](conformance.md)), and writes there itself the `hosts.json` it needs, each `ready`
record with the `instance_type` the case names or with none, the `projects.json` it needs, a held
`provision.lock` where it needs one, and any lease record it needs a lease of the project to be
(a `pending`, a `released`, an `unsatisfiable`, or a stored `ready` record whose `expires_at` is
past or to come), into `leases.json`, as C26 writes its records.
- **A check whose `create` or `get` could start a run MUST set `AGENTSPACE_PROVISION_CMD` to v0.07's
  recording stand-in** (pin P12, architect, 2026-11-20, for v0.07-T01). From v0.09 that is every
  `create` of a supported request on a `ready` record that rows 1, 2 or 6 of
  [On a `ready` host](#on-a-ready-host) answer `pending`, the box's included.
- **A live `ready` lease of the project is made with `create`, at the host's exact size, or written
  as a stored `ready` record with an `expires_at` after the case's run**; a written one needs no
  lease folder or alias, since place reads only the record. **A size no backend has is `xxl`**, and
  `XXL` where a case needs upper case (D71).
- **A `pending` lease of the project is written as a stored `pending` record**, with the EC2
  driver's name as its `driver` and a size in `SIZES` unless the case says otherwise.
- **`<project>` is a project of the case's own with a record** (in Z16, one with none until its
  first `create`), other than `swimcoach` unless the
  case says the box; `<t:s>`, `<t:m>`, `<t:l>` and `<t:xl>` stand for `m9g.large`, `m9g.xlarge`,
  `m9g.2xlarge` and `m9g.4xlarge` ([Sizes and instance types](#sizes-and-instance-types)).
- **"Starts nothing"** means the stand-in is not started; **"the registry as it was"** means
  `hosts.json` byte for byte as the case wrote it.

| Case | What it shows | Where the rule is |
|---|---|---|
| Z1 | For each of `s`, `m`, `l` and `xl`, with no other lease of the project: a `create --size <size>` for a record `ready` at that size's type answers `ready`, exit 0, a document that validates, whose `size` is the size asked for and that names no type; it starts nothing and leaves the registry as it was | [On a `ready` host](#on-a-ready-host), row 3 |
| Z2 | `create --size xxl`, and `create --size XXL`, answer `unsatisfiable` with exactly one reason, `unsupported_size`, whose message is byte for byte `Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.`; neither needs a CA, reads `hosts.json` (one that does not validate gives the same answer), writes the registry or starts anything. With `--platform windows/amd64` as well, the reasons are `unsupported_platform` then `unsupported_size`, in the contract's order; with `--platform windows/amd64 --size l`, the one reason is `unsupported_platform` (pin P9) | [What it supports](#what-it-supports) |
| Z3 | With a live `ready` lease of the project, a record `ready` at `<t:l>`: `create --size m` and `create --size s` each answer `ready`, with the alias of the project's host and the document's `size` the size asked for; each starts nothing and leaves the registry as it was. With a record at `<t:m>`, `create` with the default size answers `ready` the same way | [On a `ready` host](#on-a-ready-host), rows 3 and 4 |
| Z4 | With a live `ready` lease of the project, a record `ready` at `<t:m>`: `create --size l` and `create --size xl` each answer `unsatisfiable`, exit 0, with exactly one reason, `no_capacity`, whose message is byte for byte D66's with the project and the size asked for; each makes no lease folder and no alias file, starts nothing, leaves the registry as it was, and leaves every other record of `leases.json` as it was | [On a `ready` host](#on-a-ready-host), row 5 and the message |
| Z5 | With no live `ready` lease of the project and a record `ready` at `<t:m>` whose `last_attempt_at` is absent: `create --size l` answers `pending`, exit 0, a document that validates with no `expires_at`, `ssh` or `reasons`, and no lease folder or alias file; the stand-in is started once, with exactly `provision -Host <project>`, three arguments and no `-InstanceType`; the record keeps `state` `ready`, its `instance_type` and its `instance_id`, and gains `last_attempt_at` within the command's run. The same for a record at `<t:xl>` and `create --size m`, the host to be made smaller | [On a `ready` host](#on-a-ready-host), row 6; [Starting a provisioning run](#starting-a-provisioning-run), the program |
| Z6 | None of these holds a host's size: a stored `ready` lease of the project whose `expires_at` is past, a `released` one and an `unsatisfiable` one, and a live `ready` lease of another project. With all of them and a record `ready` at `<t:m>`, `create --size xl` answers `pending` and starts the stand-in, as Z5, never `unsatisfiable` | [On a `ready` host](#on-a-ready-host), the live `ready` leases |
| Z7 | Expiry is decided by the reading's time: a lease made with `--ttl 1m` at the host's exact size holds it, so `create --size xl` answers `unsatisfiable` with `no_capacity`; once its `expires_at` has passed, the same `create` answers `pending` and starts the stand-in | [On a `ready` host](#on-a-ready-host), the live `ready` leases |
| Z8 | A record `ready` with no `instance_type` (D67): with no live `ready` lease of the project, `create` with the default size answers `pending` and starts the stand-in, the record staying `ready` with no `instance_type`; with a live `ready` lease of the project, the same `create`, and a `create --size s`, answer `pending` too, never `unsatisfiable`, and start the stand-in when a run is due. The same answers for a record whose `instance_type` is `m9g.8xlarge`, which the map does not hold and which meets no size (architect, 2026-11-23, for v0.09-T01, pins P2 and P14) | [On a `ready` host](#on-a-ready-host), row 2 (pins P2 and P14) |
| Z9 | Place's side of a run's deploy step (architect, 2026-11-23, for v0.09-T01, pin P26), with the project's `provision.lock` held by the check and no live `ready` lease of the project. With the record `ready` and no `instance_type`, as a run's deploy step leaves it from its step 2 until its read-back: `create` with the default size answers `pending`, starts nothing, since no run is due while the lock is held, and leaves the registry as it was. With the record `ready` at `<t:m>`, as the read-back leaves it: the same `create` answers `ready`, starts nothing and leaves the registry as it was, the held lock changing nothing | [On a `ready` host](#on-a-ready-host), rows 2 and 3; [A resizing run](#a-resizing-run) |
| Z10 | With no live `ready` lease of the project, a record `ready` at `<t:m>` whose `last_attempt_at` is under 5 minutes before now: `create --size l` answers `pending`, starts nothing and leaves the registry as it was; a `get` of that lease after the check sets `last_attempt_at` 5 minutes or more before now starts the stand-in once, with exactly `provision -Host <project>` | [Starting a provisioning run](#starting-a-provisioning-run), steps 2 and 4, and from `check_pending` |
| Z11 | With no CA, a `create` that would answer `pending` on a `ready` record (Z5's) fails with `internal`, records no lease, leaves the registry as it was and starts nothing; one that would answer `unsatisfiable` with `no_capacity` (Z4's) answers it, with no CA | [On a `ready` host](#on-a-ready-host), the CA bullet |
| Z12 | The box: with `hosts.json` holding the box's record `ready` at `<t:m>`, a `create` for `swimcoach` with the default size and no other lease answers `ready` with v0.06's alias bytes, and starts nothing; with `--size l` it answers `pending` and starts the stand-in once, with exactly `provision -Host swimcoach` | [On a `ready` host](#on-a-ready-host); [What is true of today's box](#what-is-true-of-todays-box) |
| Z13 | A live `ready` lease recorded under another `AGENTSPACE_STATE_DIR` does not hold the host: with one in a second state folder, and none in the case's own, a `create --size xl` in the case's own state folder, for a record `ready` at `<t:m>`, answers `pending` | [On a `ready` host](#on-a-ready-host), the state folder |
| Z14 | While the project has a `pending` lease of the EC2 driver at a size in `SIZES`, every new lease of it is `pending` (pin P2): with a record `ready` at `<t:m>`, a stored `pending` lease of the project at `l`, and with and without a live `ready` lease of the project, `create` with the default size and `create --size s` each answer `pending`, never `ready` or `unsatisfiable`, and leave the `pending` record as it was. The stand-in is started once when the record's `last_attempt_at` is absent or 5 minutes or more before now, and not at all when it is under 5 minutes before now (architect, 2026-11-23, for v0.09-T01, pin P27). A stored `pending` record of the project with a size outside `SIZES`, or with another `driver`, does not hold the project's leases: with it alone, `create` with the default size answers `ready` (pin P20) | [On a `ready` host](#on-a-ready-host), row 1 |
| Z15 | With no `hosts.json`, the box's implied record has `instance_type` `m9g.xlarge` (pin P2c): a `create` for `swimcoach` with the default size answers `ready` with v0.06's alias bytes, starts nothing and leaves `hosts.json` absent; a `create --size l` for it answers `pending` and starts the stand-in once, with exactly `provision -Host swimcoach` | [Where it places a lease](#where-it-places-a-lease) |
| Z16 | The implied type is never written (architect, 2026-11-23, for v0.09-T01, pin P31): with no `hosts.json` and a `projects.json` listing `<project>`, a `create` for `<project>` with the default size answers `pending`, starts the stand-in once, with exactly `provision -Host <project>`, as C5, and leaves a `hosts.json` that validates, whose first record is the box's, project `swimcoach`, stack `agentbox` and state `ready`, with no `instance_type`, beside `<project>`'s new `creating` record. Then a `create` for `swimcoach` with the default size and no other lease of it answers `pending` by row 2, never `ready`, makes no lease folder or alias file, starts the stand-in once more, with exactly `provision -Host swimcoach`, and leaves the box's record `ready` with no `instance_type` | [Where it places a lease](#where-it-places-a-lease); [Starting a provisioning run](#starting-a-provisioning-run), step 4; [On a `ready` host](#on-a-ready-host), row 2 |

**Cases of earlier releases that change with v0.09** (architect, 2026-11-23, for v0.09-T01, pin
P3). Each seeds a `ready` record with no `instance_type`, which row 2 answers `pending` on T03's
build, or depends on a rule that row 1 changes:
- **C3, its second case, and both cases of C4** write a `hosts.json` whose `ready` records have no
  `instance_type`, and expect `ready` and no run. On T03's build each answers `pending` and starts
  the stand-in, until its records are seeded with `instance_type` `m9g.xlarge`.
- **C20** expects `ready` and no run for a `create` on a `ready` record while another lease of the
  project is `pending`. Row 1 answers it `pending` whatever the record's `instance_type`, so C20
  changes by the rule, not only by its seeding, and its new form is Z14's.
- **C3's first case and C21's `swimcoach` case** have no `hosts.json`, so with pin P2c they are
  unchanged. Every other case of C1 to C32 either writes no `ready` record, or reaches one only
  through `get`, `list` or `release`, which v0.09 does not change, or expects an answer that comes
  before the record's type is read (`no_capacity` for a project with no record, `internal` for a
  registry that does not validate).
- **Every earlier lease CLI case that seeds a `ready` host through the harness**, beyond C1 to
  C32, expects `ready` for a default `create` and so changes the same way.
- **The seeding lands in T03** (pin P3): E0.09-T03-S01 (#0203) makes
  `conformance/lease-cli/harness.ps1`'s `Add-ConformanceReadyHost` write `instance_type`
  `m9g.xlarge` on every `ready` record it seeds, by default, with an opt-out for the cases of a
  missing type; E0.09-T03-S02 (#0204) gives the same default to any operator helper that seeds a
  `ready` record for a place or provision case. **C3's second case and C4 are seeded by
  `New-PgHostRecord` in `conformance/lease-cli/test-unmet-host-alias-bytes.ps1`, not by
  `Add-ConformanceReadyHost`**, and E0.09-T03-S01 (#0203) owns that helper too, with the same
  default (architect, 2026-11-23, for v0.09-T01, pin P29).

**What no stand-in for the lease CLI shows,** and where each is shown instead:
- **`SIZES`, `instance_type_for`, `meets` and `resize_target`, each over its whole domain**, among
  them a type and a size given in upper case, an empty text, a type the map does not hold, and
  `pending` records of another project, of another state and with a size outside `SIZES`: the crate
  `agentspace-driver-ec2`'s own tests, E0.09-T02-S02.
- **Place's decision over stand-in files, row by row, and `place_pending`'s two passes**: that it
  places only the leases the host meets, answers `unsatisfiable` with `no_capacity` a lease it does
  not meet while a live `ready` lease holds the host, whatever the lease's place in the file, leaves
  such a lease `pending` when none does, and changes no other member of the record: the crate's own
  tests, E0.09-T03-S01. **A run's `place_pending` through `provision` and operator.md's test seam**,
  and **the type a run's deploy step passes**, `resize_target`'s or none when a live `ready` lease
  holds the host, its removal and read-back of the record's `instance_type`, and its failure
  before any AWS call, with the type removed, when the lease state file cannot be read or does
  not validate: the operator's cases, E0.09-T04-S02
  ([operator.md, provision](operator.md#provision)).
  **Pin P26's order as the stack's deploy sees it**, where operator.md's test seam can show it: the
  AWS stand-in, called for the stack's deploy, finds the record with no `instance_type` and the
  lease state lock free, the same cases.
- **Place's `internal` answer when the lease state file cannot be read under its caller's hold**,
  with `<why>` `agentspace's lease records could not be read` (architect, 2026-11-23, for
  v0.09-T01, pin P19), which only a file changed or lost under the lock gives, and **step 3.4 of
  [`place_pending`](#place_pending), its removal of a lease's leftover credential files before it
  answers the lease `unsatisfiable`**, ending nothing and logging nothing (pin P21): the crate's own
  tests, E0.09-T03-S01.
- **Pin P26's order against a `create` that runs at the same time**, wholly before the deploy
  step's first lock or wholly after it, which no stand-in through the lease CLI can time:
  `agentspace-sync`'s own tests of provision's deploy step, E0.09-T03-S02 (#0204), which can hold
  the lease state lock as a `create` does.
- **A host's type changing, and everything that needs a real host**: D70's live checks on
  agentspace's host, E0.09-T05-S01, one for each of these:
  - **Bigger:** with the host stopped and no live lease, `create --size l` answers `pending`,
    becomes `ready`, and `nproc` over its alias prints 8. The lease document names only the size.
  - **Met at once:** with that lease live, `create --size m` answers `ready`, on the same host.
  - **Refused:** with that lease live, `create --size xl` answers `unsatisfiable` with `no_capacity`
    and D66's message, and the host's type and state are unchanged.
  - **Smaller:** once every lease is released, `create` with the default size answers `pending`,
    and the host comes back as `m`, four vCPUs.
  - **Unsupported:** `create --size xxl` answers `unsatisfiable` with `unsupported_size`, makes no
    AWS call and starts no run.
  - **The first lease after the install:** the host's record gains `instance_type` by its run
    (D67).
  - **The guard:** `guard.log` and `status -All` show no stop of the host during a resize, and the
    host is `stopped` within D56's bound after the last release.
- **The box is not resized live** (D70): its first lease after the install records its type at `m`,
  which changes nothing on it, shown by its registry record and `status -Host swimcoach`.

### Conformance cases for v0.10

**Each behavior below can be shown through the lease CLI with stand-ins, and MUST be a conformance
case of E0.10-T04-S01** (card #0221; D77, with pins P22, P24 and P31, architect, 2026-11-24, for
v0.10-T01). **They are the cases of the v0.09 section's kind**, with its seeding rules: each runs
in the harness's throwaway home, writes there the `hosts.json` it needs, a `ready` record with
`instance_type` `m9g.xlarge` unless the case says otherwise, and `AGENTSPACE_PROVISION_CMD` set to
v0.07's recording stand-in.
- **The lease CLI's stand-in provision writes nothing to `hosts.json`**, so it never records a
  success. **A case that needs one seeds `last_success_at` itself**, in the record it writes, in
  state.md's timestamp form.
- **`C8`, `C12`, `Z10` and `Z14` stay as written**: their records carry no `last_success_at`, so
  they show the 5 minutes unchanged.
- **`<project>` is a project of the case's own** with a `ready` record at `<t:m>` and no live
  `ready` lease, and "starts nothing" and "the registry as it was" mean what the v0.09 section says.

| Case | What it shows | Where the rule is |
|---|---|---|
| Y1 | A success at or after `last_attempt_at` makes the next start at once: with `last_attempt_at` 1 minute before now and `last_success_at` 30 seconds before now, a `get` of a stored `pending` lease of the project starts the stand-in once, with exactly `provision -Host <project>`, sets `last_attempt_at` to now, and leaves the record with no `last_success_at` and every other member as it was; a `create --size l` on the same seed, in a case of its own, starts it once the same way and answers `pending` | [Starting a provisioning run](#starting-a-provisioning-run), steps 2 and 4 |
| Y2 | An older or absent success waits 5 minutes: with `last_attempt_at` 1 minute before now, and `last_success_at` 2 minutes before it (in one case) or absent (in another), a `get` of the `pending` lease and a `create --size l` each start nothing and leave the registry as it was; with `last_attempt_at` 5 minutes or more before now, in either case, each starts the stand-in once | [Starting a provisioning run](#starting-a-provisioning-run), step 2 |
| Y3 | Equal seconds follow pin P24: with `last_success_at` equal to `last_attempt_at`, 1 minute before now, a `get` starts the stand-in once, and removes `last_success_at` in the write that sets `last_attempt_at`; a second `get` straight after it starts nothing and leaves the registry as that write left it | [Starting a provisioning run](#starting-a-provisioning-run), steps 2 and 4 |
| Y4 | A run whose placing failed or left a lease writes no success (pin P22): a record as such a run leaves it, `last_attempt_at` 1 minute before now and no `last_success_at`, beside a stored `pending` lease of the project, answers a `get` and a `create --size l` as Y2's absent case does, and the next start comes once `last_attempt_at` is 5 minutes old | [Starting a provisioning run](#starting-a-provisioning-run), step 2, and "After the run" there |
| Y5 | A held lock still stops a start that a success would make due: with `last_success_at` at or after `last_attempt_at` and the project's `provision.lock` held by the check, a `get` and a `create --size l` start nothing and leave the registry as it was | [Starting a provisioning run](#starting-a-provisioning-run), step 2 |

**What no stand-in for the lease CLI shows,** and where each is shown instead:
- **A run's own writing of `last_success_at`**, after step 9's placing returned without error and no
  lease was failed or left, and under `hosts.lock` alone with `provision.lock` still held; its
  failed write logged as `success failed reason=<why>` and changing nothing else; and a run
  whose placing failed, or failed or left a lease, writing none: the operator's cases through
  `provision` and its test seam, E0.10-T04-S02 (#0222)
  ([operator.md, provision](operator.md#provision), step 9's end).
- **The run's start record removing `last_success_at`** (pin P24): the same cases.
- **A lease created after placing's last re-read and before the run exits, which waits for a
  `get`** (pin P31): not shown, since no stand-in can time it; it is the order of the locks.

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
9. **From v0.08, end the lease's connections on the caller's machine at release, best effort, before
   it removes the alias, and again after** ([Release a lease](#release-a-lease); D59 and its
   change for v0.08-T01). The second ending catches a client that came back through the alias
   while the first ran. A driver whose aliases are reached through `ssh` MAY call the same
   `end_alias_clients` the EC2 driver calls, once for each
   ([Ending a lease's connections](#ending-a-leases-connections)). Its release step appends one
   line counting both to `release.log` in the CLI's state folder, in the line form of
   [Where the count is logged](#where-the-count-is-logged) (architect, 2026-11-21, for v0.08-T01,
   pin P23).
10. **From v0.08, supply release in its two steps** ([Release a lease](#release-a-lease); architect,
    2026-11-21, for v0.08-T01, pin P23): a check
    step that checks the file at the alias's place and changes nothing, and a release step that
    checks it again before it ends or deletes anything. A check step that fails MUST leave every
    connection, file and log as it found them.

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
   v0.07-T01; D48), so no gap is left for the EC2 driver. A failed run leaves a `pending` lease of
   this driver `pending`, never `unsatisfiable` (D48, D5); **from v0.09 a run that succeeds answers
   one `unsatisfiable`, with `no_capacity`, when its resized host does not meet it while a live
   `ready` lease of the project holds the host** ([`place_pending`](#place_pending), step 3; D68;
   architect, 2026-11-23, for v0.09-T01, pin P1).
   It is given up by
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
   **Q4: How does the boundary let the CLI undo a renew, or record before it takes effect?**
   lease-api.md requires a failed renew to leave the previous certificate (#0041). The
   prototype's driver changes the files first, and the CLI records afterwards, so a recording
   failure leaves them changed. Either a two-step operation (prepare, then commit), an undo, or
   recording first would meet it. **Release's half of Q4 is closed from v0.08** (architect,
   2026-11-21, for v0.08-T01): release is a check step and a release step, with the record written
   `released` between them ([Release a lease](#release-a-lease)), so a release never leaves a
   `ready` lease with part of its credential gone. The prototype's release, which removed the
   alias and could then fail on the folder, leaving a `ready` lease with no alias, is not behavior
   to copy.
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
