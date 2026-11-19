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
  that today's driver hands back: [docs/operator.md](operator.md).

**Where this page is the owner:** the operations and their rules; the shape of a failure at the
boundary; the rule for what is `unsatisfiable`; what reachability is, and who supplies it; and the
EC2 driver as it stands.

**Hardening carried in.** Card #0041, first point (credential errors reach the caller unwrapped),
is carried in here as a requirement on every failure that crosses the boundary
([Failure](#failure), [Hardening carried in](#hardening-carried-in)). The rest of #0041, and
#0029, are [lease-api.md](lease-api.md#hardening-carried-in)'s.

**A requirement the prototype does not meet is written as a requirement, and the prototype's
behavior follows as a note.** Where a note says "not behavior to copy", the rewrite MUST meet the
requirement and MUST NOT repeat the note.

## Contents

- [Conventions](#conventions): [the boundary](#the-boundary), [failure](#failure),
  [what a driver never exposes](#what-a-driver-never-exposes),
  [ordering and concurrency](#ordering-and-concurrency),
  [the driver's name](#the-drivers-name)
- [The operations](#the-operations): [place a lease](#place-a-lease),
  [renew a lease](#renew-a-lease), [release a lease](#release-a-lease)
- [Unsatisfiable requests](#unsatisfiable-requests)
- [Reachability](#reachability)
- [The EC2 driver](#the-ec2-driver)
- [Writing another driver](#writing-another-driver)
- [Hardening carried in](#hardening-carried-in)
- [Open questions](#open-questions)

## Conventions

### The boundary

**The boundary is three operations: place a lease, renew a lease and release a lease.** The CLI
asks nothing else of a driver. `get` and `list` never call one, and neither does any command on a
lease that is `unsatisfiable`. Which commands ask a driver for a lease in each state is in
[Release a lease](#release-a-lease).
- In the prototype, no command reaches a driver for a lease that is `pending`, because the
  prototype's driver never answers `pending`. How a placement that is `pending` would reach one is
  [Q3](#open-questions)'s.

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
  `pending`, instead of waiting inside the operation** ([Place a lease](#place-a-lease)). Today's
  driver starts nothing at place time ([The EC2 driver](#the-ec2-driver)).

**What the CLI does, in order, around each operation:**

| Command | In the lock | The driver is asked | Then the CLI |
|---|---|---|---|
| `create` | Reads the records; picks a new lease ID whose alias no kept lease uses | To place the lease | Records the lease, validates the records, writes them |
| `renew` | Finds the lease; checks that it is `ready`, so an expired lease is refused before any driver is asked ([lease-api.md](lease-api.md#agentspace-lease-renew)) | To renew the lease's certificate | Records the new `expires_at` and `ttl_seconds`, writes |
| `release` | Finds the lease | To release it, when its stored state is `ready` or `released` ([Release a lease](#release-a-lease)) | Records `released`, drops `ssh`, sets `released_at`, writes |

**A failure of the driver ends the command at once, with the records untouched.** A failure after
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
([lease-api.md](lease-api.md#environment)) is the CLI's alone.

**The result, always one of these states:**

| State | Other members | Meaning |
|---|---|---|
| `ready` | Host alias, user, expires at | The lease's credential and alias are in place, and a connection through the alias is expected to succeed until the expiry |
| `unsatisfiable` | Reasons, 1 to 4 | The request cannot be met as asked ([Unsatisfiable requests](#unsatisfiable-requests)). Nothing was issued |
| `pending` | None | Accepted, not yet ready. No credential has been issued. The prototype's driver never answers it |

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
| `pending` | Would record the lease `pending` with the driver's name, and print it. How the lease then leaves `pending` is not in the boundary ([Q3](#open-questions)) |
| A failure | Records nothing, and ends the command with `internal`, exit 1 |

**A failed place MUST remove every file and folder it made, and MUST NOT disturb anything that
was there before it ran. When a removal fails, what it MUST do is
[lease-api.md](lease-api.md#concurrency-and-atomicity)'s** (#0029). A driver that fails after issuing
the credential MUST remove it before it fails, and MUST remove only what it made itself. The table
says what today's driver leaves, which is nothing except in the alias row.

| Failure | When | Left behind |
|---|---|---|
| The CA is missing or unusable | The CA check fails: no CA, a half-made one, or a mismatched or loosened one, by the five checks of [state.md](state.md#its-checks) and no others. The CA folder's place is `ca`'s to check, not the driver's. A driver MUST NOT create the CA ([lease-api.md](lease-api.md#the-credential), #0029) | Nothing |
| The credential could not be issued | Making the key, checking that it has no passphrase, or signing fails | Nothing from the credential: every file this call made is removed, and the lease's folder too if this call made it. A folder that already existed, and held neither key nor certificate, stays (gap FA-01) |
| The lease ID already has a credential | A key or certificate for this ID exists. A driver MUST refuse, and MUST NOT overwrite or remove it | The existing files, untouched |
| The alias could not be set up | The caller's OpenSSH configuration cannot take it, an alias file for this alias belongs to another lease or to no lease, the transport cannot be found, the transport or a path holds a line break, the alias is not a valid name, or a write fails | The credential is removed, and a foreign file at the alias's place is kept. See below for what the alias step itself can leave |
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
- **`pending`:** the prototype never has one, and records `released` without asking a driver. A
  driver that answers `pending` has to be told to give up the request ([Q3](#open-questions)).
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
  The prototype's driver never has a reason to answer it.
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

**The one driver v0 has: it places every lease on the one shared box, an EC2 instance.** It is the
only part meant to know the backend, and what it names stays in the files state.md owns and in the
operator's messages, never in the CLI's output. Today the shared credential and alias steps also
hold its user, transport and host-key settings ([Q5](#open-questions), [Q7](#open-questions)).

### What it supports

**Exactly three values, one of each (D5):**

| Request | Supported | Anything else |
|---|---|---|
| Platform | `linux/arm64` | `unsupported_platform` |
| Profile | `agent-linux`, with no version | `unsupported_profile`; `agent-linux@1` is one |
| Size | `m` | `unsupported_size` |

**Matching is exact on the normalized values.** `windows/amd64`, `linux/amd64`, and any
profile with a version are `unsatisfiable`. The reasons use lease-api.md's three messages, and the
driver never answers `no_capacity` or `pending`.

### How it places a lease

**Place makes no call to AWS and starts no host.** Every step is on the caller's machine:
1. **Decides** whether the request is supported, and answers `unsatisfiable` with every reason
   when it is not. Nothing has been issued.
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
   transport the next section describes. If the configuration cannot take the alias, or the
   default transport's program cannot be found, place fails here, after the key is made and
   signed, and the credential is removed as for any alias failure. **Place MUST NOT check for the
   instance-ID cache** ([How it is reached](#how-it-is-reached)).
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

### Renew and release

**Renew** checks the CA, signs a copy of the lease's public key under a temporary name inside the
lease's folder, and then moves the new certificate over the old one, so the old certificate stays
until the new one exists. The temporary files are removed whatever happens. It does not touch the
alias.

**Release** removes the alias file, if it belongs to this lease, and then the lease's whole folder.
If the alias file belongs to another lease or to none, release fails and deletes nothing. If it is
absent, release goes on. It runs no CA check and starts no program. It does not touch the OpenSSH
client configuration, and it tells the box nothing.

### What it records

**On the caller's machine only, and only the files [state.md](state.md#a-leases-credential)
describes:** the lease's key pair, its certificate, and its alias file, and the `Include` line in
the OpenSSH client configuration that pulls aliases in (place adds it when it is missing, and
release leaves it; how is state.md's).

**The driver keeps no other record.** Everything it needs to renew or release a lease comes from
the lease's ID and project, the OpenSSH client folder, and those files. **Nothing is recorded on
the box and nothing in AWS.** The box learns of a lease only when a connection arrives, through the
certificate's key ID in its SSH log (D7).

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
([How it places a lease](#how-it-places-a-lease)).

**The default transport reads the operator's instance-ID cache when a connection is made, and
place never reads it** (D21). The cache is `%USERPROFILE%\.agentspace\instance-id.json` (D40)
([state.md](state.md#files-in-the-checkout)), and what the ProxyCommand does when it is missing or
stale is [operator.md](operator.md#laptop-files)'s. **`create` MUST NOT read it, check for it or
depend on it:** a missing or stale cache fails the connection, never the `create`. So a check, or a
worktree, needs no cache to create a lease, with or without the override.
- **If the default transport's program cannot be found, place fails** (step 5 above), with a
  message in agentspace's own words and no absolute path
  ([lease-api.md](lease-api.md#messages)).
- Note on where this came from: the prototype's default line runs
  `<checkout>\scripts\ssh-proxy.cmd`, beside the CLI's own script, and its place checks for the
  checkout's `.instance-id`, its copy of the cache, which a git worktree does not have. That check
  is a difference the spec drops (D21).

**What place cannot know:** whether the box trusts the CA, or is running, or can be reached. The
operator sets up the CA, and `sync` installs its public half on the box (operator.md,
[The box as the laptop sees it](operator.md#the-box-as-the-laptop-sees-it)). A lease placed before
that is `ready` and refuses every connection. That is why place MUST NOT create the CA itself.

### What is true of today's box

**These are properties of this driver and its one host, never of the contract (D4):**
- **The host runs Linux on arm64 as Ubuntu,** with `sshd` trusting the CA, `systemd`, and a login
  user named `ubuntu` with a home and `sudo`.
- **Every lease logs in as `ubuntu`** (D3). The driver says so in three places (the certificate's
  principal, the returned user, the alias's login user), and every driver MUST keep all three the
  same. D25 replaced D3's isolation by a user per project with a host per project, which arrives
  in v0.07.
- **Every project's leases land on the same host** until v0.07 (D25).
- **The host stops itself when idle,** and the transport starts it (D11).

## Writing another driver

**To be driven by the CLI without change, a driver MUST:**
1. **Supply the three operations,** with the inputs, results and failures above, over whatever
   transport the rewrite chooses ([Q1](#open-questions)), and an opaque, stable name.
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
3. **Later, not blocking:** v0's one driver never answers `pending`
   ([What it supports](#what-it-supports)), so a v0 lease never needs a way out of it.
   **Q3: How does a `pending` lease become `ready` or `unsatisfiable`, and how is one given up?**
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
   rule, and no decision says who supplies it.
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
