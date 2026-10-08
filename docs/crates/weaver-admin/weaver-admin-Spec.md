# weaver-admin - Spec

**Status:** MERGED. Cut 2026-08-02, fifth of the Spec pass and the first outside the
agent. Code is written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-02
**Document ID:** `weaver-admin-Spec`
**Parent:** `weaver-admin-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #72

---

## 0. What this document is

Build instructions for `weaver-admin`: the binary's layout, the invocation's
interface, the verb sequencing, the sink openings, the start step, the
coordination channel's dial, and the elections a builder would otherwise invent. It is
derived from `weaver-admin-PRD` and from the two contracts this crate is party to,
`weaver-admin-harness-contract` and `weaver-admin-operator-contract`,
`weaver-admin-systemd-contract` having retired with systemd on 2026-10-03 (#50),
together with `weaver-organ-channel`, the drawn material the first
of them draws in part.

Level discipline. The charter says what the crate needs and why. This document
says how it is represented, and per gate G2 it elects against grounds the charter
and the contracts state rather than developing grounds of its own. Where this
document and the charter disagree the charter yields nothing.

**This document declares its crate's assertion records and no other record,**
per Document Format sections 3 and 4 as of the notation of 2026-08-03. The
charter stays the source of this crate's node, its parent edge, its one floor
link, its one declared seam, and its artifact edges, and a Spec that restated
any of them would give the mapper two sources for one record, per that
format's section 1. What this document sources is the claims code must
conform to, declared at the clauses that argue them rather than gathered in
one place, per that format's section 6, and `asserts` runs from the crate
rather than from this document, which is why the document needs no node of
its own.

**A claim this Spec cites and another Spec argues carries no record here,**
and there are ten of them, named in section 10 with the crate that declares
each. The pattern behind them is this crate's own: admin authorizes and does
not execute, so a claim about what a run does once the enter directive lands
is argued where the run happens, and a floor definition admin consumes is
argued where it is defined. Copying either would be the duplication gate G5
makes someone adjudicate, with an authority owed against every copy.

**It is written from the merged corpus alone,** per the ruling of 2026-08-01
that keeps the old tree's Specs out of the Spec pass.

**This crate is fully chartered, so this Spec's bound is the charter's own.**
The lifecycle workflow is the whole of admin's job, and nothing here defers to
the token or tool workflows: what stays open is what the charter's section 10
holds open, the session-close cue and the enter question, each already carrying
its settler.

## 1. The crate

**Two binaries, and still no library surface.** The crate builds `weaver-admin`, and
since 2026-10-03 (#50) `weaver-trace-relay`, the trace door's relay of section 6, which
belongs here because admin holds the trace's custody and its start step is the relay's
only starter, and no other crate's layout admits it: the harness builds its two worker
targets and `weaver-trace` is a library. The relay links nothing of admin's root code,
being a separate `src/bin/weaver-trace-relay.rs` that shares only the floor's types, and
it runs under the relay account and never as root. No library surface is published:
nothing links admin, per the charter's section 7, and a `lib.rs` would be an API for a
consumer the topology forbids. The instrument reads Cargo's target inventory, which
includes both explicit declarations and targets found by convention.
`one_binary_and_no_library_surface` of `crates/weaver-admin/tests/manifest.rs` becomes
`two_binaries_and_no_library_surface` in the code act: it requires exactly the binaries
`weaver-admin` and `weaver-trace-relay`, permits integration test targets, and refuses
every other target, including a library, build script, example or bench.
A comment or alternate TOML spacing cannot change the inventory. The reverse
relation, that nothing links admin, remains review's.

```graph
node: admin-no-library-surface
kind: assertion
tag: manifest

edge: asserts
from: weaver-admin
to: admin-no-library-surface

edge: grounds
from: admin-no-library-surface
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**Layout.** One module per obligation.

    src/main.rs       entry, argument parsing, and wiring, and nothing else
    src/surface.rs    the invocation's interface and the OS calls, section 2
    src/verbs.rs      load, unload, validate, stop, and rollback, section 3
    src/inventory.rs  config validation and boundary verification, section 4
    src/sink.rs       sink resolution and opening, section 5
    src/start.rs      the start step, the run lock and the trace relay, section 6
    src/channel.rs    the coordination channel's dial, section 7
    src/log.rs        the operations log, section 8
    src/bin/weaver-trace-relay.rs   the trace relay, section 6, its own binary

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly
feature used.

**The dependency set is one internal crate and three external ones, and each is
argued.** `weaver-types` is the charter's one floor link, taken **with its `config`
feature on**, because admin is the crate that parses the operator's file, per that
Spec's section 1, and the parser's whole audience is this module's inventory.
`weaver-traits` is deliberately not a direct dependency, per charter section 3: it
arrives transitively through `weaver-types` and nothing here draws it by name, which the
manifest states by carrying no line for it. `serde_json` renders the invocation's
answers and encodes and decodes the coordination channel's envelopes. `nix` is the OS
surface, on the grounds `weaver-harness-Spec` section 2.4 argued and this crate inherits
rather than re-argues: descriptor custody is central, the io-safe owned types make it a
compile property, and the needed calls, `socket`, `bind`, `listen`, `accept`,
`getsockopt` for the peer credential, `sendmsg` with control messages, `open`, `mkfifo`,
and `stat`, are all covered. That crate asserts the election where it argues it, so this
one cites it and adds no second record for one decision. `sha2` joins on 2026-09-04 for
one purpose, the digests of the declaration file and, since 2026-10-02, of the prompt
file it names, which this crate reads at the inventory and supplies on the enter, per
`weaver-admin-harness-contract` section 5, so the run and the record name what they were
built from without a second reader of either file.

```graph
node: admin-one-floor-link-types-config
kind: assertion
tag: manifest

edge: asserts
from: weaver-admin
to: admin-one-floor-link-types-config

edge: grounds
from: admin-one-floor-link-types-config
to: axiom-floor-is-vocabulary-behavior-is-socket

node: admin-no-direct-traits-line
kind: assertion
tag: manifest

edge: asserts
from: weaver-admin
to: admin-no-direct-traits-line
```

**What the compiler holds of that custody is the ownership and no more.** A
descriptor is an owned type end to end, so a leak is a move the borrow
checker sees, and that half is a type property. That every creating call sets
the close-on-exec flag atomically is a behaviour rather than a type property,
argued at section 6 and tested by the third walk of section 10, so the two
halves take separate records and neither claims the other's instrument.

```graph
node: admin-descriptors-owned-types
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-admin
to: admin-descriptors-owned-types
```

**No async runtime, no D-Bus crate, no logging crate.** The surface's traffic
is operator-paced and the coordination traffic is per-load, so nothing here
needs an executor, and threads from the standard library carry the concurrency
section 2 needs. No init system is reached at all, per section 6, so no bus library
enters the tree. The operations log of section 8
is this crate's own file with this crate's own writer, and a logging framework
would be a second account with its own schema, which is the arrangement charter
section 2 rules out for the trace and this Spec declines for the log. The
absences are checked by the build-time `cargo tree` assertion the floor Specs
share.

```graph
node: admin-no-runtime-no-bus-no-logging
kind: assertion
tag: manifest

edge: asserts
from: weaver-admin
to: admin-no-runtime-no-bus-no-logging
```

## 2. The invocation's interface

The interface of charter section 8: the operator runs the binary with root, one
verb per run. The socket this section carried until 2026-08-05 retired with the
service account, and what replaces it is the process boundary the operating
system already draws around an executed program.

**The verb and its agent arrive as arguments.** One verb per invocation, `load`,
`unload`, `validate`, `stop`, `show`, and, since A3.2 on the operator's rulings of 2026-10-06 on #1 (the A3.0 items), `save-point`,
`restore` and `force-unload`, with the agent name as the one further argument, which
every verb takes. **The verbs are the application layer's primitives,
and their orchestration is interior**, on the operator's ruling of 2026-10-03 on #72:
each verb keeps one promise and does inside the agent only what that promise needs, and
a choice between outcomes that its caller should make is left to the caller, admin-con,
which reads `show` and issues the next verb. **Three verbs joined with A3.2**, on the operator's rulings of 2026-10-06 on #1 (the A3.0 items): `save-point`, asking the running
worker for a save point on demand over section 7's channel and publishing it at once,
per section 6; `restore`, naming the save point the next load restores, which is the one
the declaration's `[restore]` names, judged loadable now and, where no line names it,
entered in the manifest as named at a restore, per section 4, so a file that arrived by
no publication becomes loadable only by this verb and `RestoreNamed` means named and
judged loadable now; and `force-unload`, the unload that completes without its leave save point,
per section 3. Each is one fixed command line with no argument: the save point `restore`
names is the declaration's and never the caller's, which is why the verb takes none, and
the operator's rule grants all three, the observer's none. **The live restore of a
running agent waits on the decode seam's `Reopen`**, the loop act's (A5): a restore is a
reload of the agent's state at a turn boundary, the decode cache flushed and the prefix
re-presented from the restored holdings, on the operator's ruling of 2026-10-06 on #1,
and until A5 lands a restore is `restore`, then `load`, which restores what was named. Arguments rather than a parsed request line, because the kernel
already delivered them as a vector and re-encoding them into a wire format would be
inventing a wire where no seam crosses. The base the agent's root stands under is the
one environment variable read, per section 9.

**A new interior verb has one recipe, so the state-management acts can add them
cheaply**, on the operator's direction of 2026-10-03 on #50. Start and stop need no
socket, and interior control exists only while the agent runs, over the coordination
channel. An interior verb is one fixed `weaver-admin <verb> <agent>` command line with
no argument, which a role's sudo rule grants. It is one directive and one answer on
section 7's coordination channel, valid only while the worker holds the run lock. Its
case is added to `LifecycleAnswer` and to the operator contract's case list, so it
breaks a consumer loudly under `weaver-types-Spec`'s exhaustive-enum rule. Its outcome
is written to `admin.log`, and any change it makes to the agent is recorded on the trace
with its cause. Any value it needs is taken from the declaration, recorded or pinned
under #71, and never from the caller. `save-point`, `restore` and `force-unload` followed it in A3.2.

**Authorization is the kernel's, and what this crate checks is the name.** The
invocation runs as root or performs nothing, so no predicate, no allow set, and
no deny set exist here: the earlier form's `authorized` call, its group allow
set, and its agent-uid deny set retired with the socket, the operator being the
party the kernel already admitted. What survives is the name check of section 4
and the agent's root of section 9, which are about which agent may be named rather
than about who may name it. The refusal is enacted before any verb touches anything, and the
instrument is a test running the binary as a non-root uid and finding it
refuses, watched to fail when the check is removed.

**A caller that is not the operator at a root shell reaches the verbs through a sudo
rule, and the rule is strict**, on the operator's rulings of 2026-10-03 on #50, which
revise the ruling of 2026-10-02 that WeaverWeb carries no sudo. The operator is assumed
to hold sudo and starts the agent as root so that its permissions can be established:
its user, its runtime directory, its sink. Where WeaverWeb's admin-con issues the
lifecycle for the operator, it does so through a rule the operator installs as the box's
own boundary, which names this agent's admin-con user, names the exact `weaver-admin
<verb> <agent>` command lines for this agent, and allows no caller-chosen argument.
**The role split of #50 is which command lines a role's rule grants**: an observer's
grants `show`, and an operator's adds `validate`, `load`, `unload` and `stop`. **Nothing
is read from the caller**: no argument the caller chose, which the rule forbids, and
nothing on standard input, which this crate never reads. **The cause is the uid sudo
reports and nothing else**: where the invocation runs as root under sudo, this crate
reads sudo's own `SUDO_UID`, parsed strictly as a decimal uid, a malformed value
refusing `Malformed` and never falling back, and at a root shell where sudo set no
`SUDO_UID` it takes uid 0, and it hands that uid to the harness as the `Cause` of a verb
that changes the agent, per `weaver-types-Spec` section 3.1. Which person asked is
WeaverWeb's record and never the agent's. **The rule carries no `SETENV` and no
`env_keep`**, so sudo's environment reset leaves the invocation only what sudo itself
sets, and the worker's environment is the start step's own, per section 6, never a
caller's. **The command never crosses a wire**: WeaverWeb sends admin-con an abstract
verb, and admin-con maps it locally to one of the fixed command lines, so the wire
between WeaverWeb and admin-con is WeaverWeb's, and this crate's interface is the
granted command lines and the one answer object each prints. **The ceiling is the rule's
lines**: the `grants` verb #50 once drafted does not return, admin holding no view of
what a sudo rule grants, so admin-con reads its ceiling from the rule itself on the box,
with `sudo -n -l` on each exact command line. This supersedes toddwbucy/WeaverTools#6
point 5, on WeaverWeb's own proposal on toddwbucy/WeaverWeb#14. **What a compromised
admin-con can do is start and stop this agent**, and the threat that matters, a model,
binary, configuration or state swapped and then loaded, is
#71's to answer, independently of this mechanism, the declaration pin being required
wherever the lifecycle is delegated. **Every verb is a command line, `show` included,
and the running-agent verbs too once A3 grants them**, so a connector never speaks on
the coordination channel: that channel sits in a directory only root enters and checks
its peer at accept, per section 7, so only a root-run `weaver-admin` dials it, and the
connector's one direct contact with the agent is the trace door. **The only lines
granted without a password are the agent's fixed command lines**: the connector runs
`sudo -n`, so it can run those and nothing else, any wider sudo the operator holds still
asks for a password, and no cached credential is within the connector's reach.

**The rule names the connector's own service user, never the operator's account**, on
the operator's ruling of 2026-10-03 on #50, the toddwbucy/WeaverTools#6 addendum
standing. It grants that user exactly the agent's fixed command lines, as root, without
a password, used with `sudo -n`. The operator's human account is never used by the
machine, and the operator's own sudo keeps asking for a password. The reasoning is three
rules at once. **Human and machine accounts stay separate**, so nothing the machine does
runs as the person. **Least privilege**: the connector holds the fixed lines and nothing
else, and since the operator's account owns the declaration, the prompt and the
published save points, a compromised connector can write none of them. **Two audit
records each answer their own question**: WeaverWeb's audit names the operator as the
person who asked, and the box's record carries the service uid sudo reports as the
account that ran it. The same service user is the boundary file's `trace-reader`, per
section 9, so the connector's one other contact with the agent is that same account.

```graph
node: admin-runs-as-root-or-performs-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-runs-as-root-or-performs-nothing

edge: grounds
from: admin-runs-as-root-or-performs-nothing
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The answer is one JSON object on standard output and the exit status agrees
with it.** One `lifecycle-answer` or one `lifecycle-refusal` in the floor's
internally tagged rendering, the discriminant being the tag itself because the
case sets are disjoint at the floor. Zero exits an answer and a non-zero status
exits a refusal, so a shell reads the status and a tool reads the object and the
two never disagree. No organ envelope appears here, because this is not an organ
channel and no contract draws one across it. What the earlier form asserted of
the wire, one answer per request in request order, is structural now: one
invocation carries one verb and emits one object.

**The answer's bounds are named, so anything outside them is a fault.** The object on
standard output is at most 64 KiB, the largest being `show`'s with its load facts, well
inside it. Exit status 0 is an answer and 1 is a refusal, as `surface.rs` already pins.
Any other status, or a status with no object, is a fault of this crate's, and a caller
reads the agent's state with the next `show`. Standard error carries human-readable
diagnostics and nothing a caller parses, its contents being no part of this interface.
**A load answers once the agent is up**: the worker has answered ready, or the load has
refused, and the worker stays running, detached. Admitting a large model can take
minutes, so this crate's own bound on the enter's answer is 900 seconds, the agent's
root naming another as `load-bound-seconds` where a model needs it, a positive count of
seconds that must form a deadline on the box's clock or the read fails, and a caller's
bound must exceed it, a caller giving up first reading the outcome from the next `show`.

**An invocation finishes even when its caller disappears**, on the operator's ruling of
2026-10-03 on #50, which folds in #60. **This crate ignores every catchable signal
whose default ends a process and which carries no fault** from its first instruction:
`SIGHUP`, `SIGINT`, `SIGQUIT`, `SIGPIPE`, `SIGTERM`, `SIGUSR1`, `SIGUSR2`, `SIGALRM`,
`SIGVTALRM`, `SIGPROF`, `SIGIO`, `SIGPWR`, `SIGSTKFLT`, `SIGXCPU`, `SIGXFSZ` and
`SIGABRT`, on #73's third item. Sudo relays a caller's interrupt and termination to the
command it runs, and a terminal sends `SIGQUIT`, so a caller cancelling, timing out or
quitting cannot end the verb part way. The synchronous faults, `SIGSEGV`, `SIGBUS`,
`SIGFPE`, `SIGILL`, `SIGTRAP` and `SIGSYS`, stay at their defaults, an ignored fault
being a loop, and `SIGKILL` and `SIGSTOP` cannot be caught. Only `SIGKILL` can end a
verb, and `unload` ends what it leaves,
per section 3, so a caller that hangs up, closes the pipe or is killed
does not end the verb. A write of the answer to a closed standard output fails and is
ignored, and the verb runs to completion all the same. Its outcome is recorded in
`admin.log` whatever became of the caller, and the next `show` reads the agent's state.
A caller that times out therefore leaves the process running, answers that the outcome
is unknown, and reads it with the next `show`.

```graph
node: admin-answer-and-exit-status-agree
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-answer-and-exit-status-agree

edge: grounds
from: admin-answer-and-exit-status-agree
to: axiom-contract-is-a-complete-interface
```

**Concurrency left this crate with the surface that held it.** One invocation
runs one verb and exits, so no threads, no accept loop, and no cross-connection
synchronization remain. What kept two transitions for one agent from
overlapping was the in-flight flag of the map this crate once held across agents, and
section 3 states where that obligation lands now.

## 3. The verbs, the agent's state, and rollback

**Two locks the kernel holds answer the two questions a per-invocation crate cannot keep
across verbs**, per the operator's ruling of 2026-10-03 on #50. Both stand in the
agent's **run directory**, `<coordination-root>/weaver.run/<agent>/`, which every verb
makes, before it takes a lock, owned by root, mode `0755` and writable by no one else,
under a parent `weaver.run/` that is root's too. **The run directory has its own
namespace**, which no agent name can produce: an agent's runtime directory is
`weaver-<agent>/` and a name carries no `.`, so `weaver.run/` is never any agent's,
where `weaver-run/` would be the runtime directory of an agent named `run`, and
`weaver-<agent>-run/` that of an agent named `<agent>-run`, each owned by that agent.
Each is a record lock the kernel holds and releases when its last holder is gone, so
neither can go stale the way a pidfile does, and they are of two kinds because they
answer for two kinds of holder: the invocation lock is a classic POSIX record lock,
owned by one process, never inherited by a fork, and named by `F_GETLK` with its
holder's pid, and the run lock is an open file description lock (`F_OFD_SETLK`), owned
by one open file description that every process of the run shares. **They stand in a
root-owned directory and not the agent's runtime directory** because the agent owns that
one: a process of the agent's could unlink a lock file there and leave the next `load`
finding a free lock on a new file while the worker still held the old, starting a second
worker of one agent.

- **The invocation lock**, on `admin.lock`, says whether an invocation is changing this
  agent now. **The run directory is made before either lock is taken**: every verb's
  first act after the name check and the root's admission, `show` and `validate`
  included, makes `<coordination-root>/weaver.run/` and `weaver.run/<agent>/` beneath it
  where they are absent, which they are at an agent's first invocation and after every
  reboot clears `/run`, and judges them where they stand, opened without following a
  link and set to root's ownership and `0755` as the start step sets the runtime
  directory. **`coordination-root` and every directory above it are held closed first**,
  by section 9's ancestor rule, or the verb refuses `BoundaryUnverified` naming the
  directory, since a principal that could write the coordination root could rename
  `weaver.run/` away and leave the next `load` a fresh `run.lock` while a run still
  holds the old one. Every verb but `show` then takes the invocation lock exclusively
  and holds it until it exits, and an invocation that finds it held exclusively refuses
  `InvocationInFlight` before touching anything. **`show` holds it shared for the length
  of its observation**, taking a shared lock without waiting: where an exclusive holder
  stands, the shared lock is refused and `show` answers `InTransition` at once, without
  dialing, because before the worker exists there is no socket and once it exists the
  harness serves one connection at a time and the load's enter holds it. Where the
  shared lock is granted, no load or unload can begin until `show` has read the run lock
  and observed the worker, so its answer cannot straddle a transition. **A verb that
  wants the lock exclusively tries it without waiting and, refused, reads the holder's
  kind from `F_GETLK`'s `l_type`**: an exclusive holder is another transition and the
  verb refuses `InvocationInFlight` at once, and a shared holder is a `show` and the
  verb retries, re-reading `l_type` before every attempt, for at most `show`'s own
  bound, the dial's, a read being no transition. A holder that changes between the read
  and the next attempt is judged afresh on that attempt, so a shared hold that gives way
  to an exclusive one turns the wait into the refusal. **A poller can starve the
  exclusive taker, and the behaviour is stated**: Linux record locks give a waiting
  writer no priority, so back-to-back `show` calls can keep a shared hold standing past
  the bound, and the verb then refuses `InvocationInFlight` with the cause named as
  readers holding the lock, the caller retrying after its poller pauses. A caller
  polling `show` during a long load is therefore answered that a transition is in
  flight, and never refused or left to time out. **No child of the invocation holds
  it**: its descriptor is opened close-on-exec and a classic lock is not inherited by a
  fork, so it is released the instant the invocation exits, whatever its children do.
- **The run lock**, on `run.lock`, says whether the run stands, and **the agent is
  running while any of its constituent processes, the worker, the state member or the
  trace relay, holds the run lock's open file description.** The start step opens
  `run.lock`, root-owned and mode `0600`, for writing and takes an exclusive open file
  description lock on it, without waiting, before it forks anything. Each of the three
  children inherits that description at its fork and carries it across its exec at a
  fixed descriptor number, 9 for all three, so the lock is held from before the first
  fork until the last of the three exits, and the kernel releases it then, however each
  one ended. **Nothing waits for a child to take it**: a child exists only after the
  lock is taken and holds it from its first instruction, so no interval leaves a
  constituent alive with the lock free, and a `SIGKILL` of the invocation at any point
  leaves whatever it has already started holding the lock, for a later `unload` to end,
  per below. The invocation's own copy closes when it exits, which releases nothing
  while a child holds the description. **Each constituent marks its descriptor
  close-on-exec as its first act** and never passes it on, so no organ the worker forks
  inherits it, the organs being bound to the worker by their death signal instead, per
  `weaver-harness-Spec` section 2.2, and no process outside the run holds a writable
  descriptor to root's file. An organ outliving the worker by the instant its `SIGKILL`
  takes is not a run. **A description lock is dropped only when the last descriptor
  referring to that description closes**, so a constituent that opens `run.lock` again,
  or closes a duplicate, releases nothing, where a classic lock is dropped by its
  holder's close of any descriptor of the file.

**The order within a load is stated, because the setup happens before the worker
exists.** The invocation lock is taken first. The run lock is then taken by the
invocation itself, without waiting, and where it is held the load refuses
`AgentRunning`, or `Unanswered` where the worker is silent, and touches nothing: a load
never ends an existing run. Everything the start step does next, repairing the runtime
directory, clearing a dead worker's names and a dead relay's `trace.sock`, standing the
member and the relay, and forking the worker, happens holding both locks, which is what
makes clearing safe: no other invocation can start a constituent while this one holds
the invocation lock, and no constituent of an earlier run holds the run lock this one
took. Taking the run lock, rather than reading it free, leaves no interval between the
read and the first fork.

**A held lock says the run stands and nothing about its lifecycle state.** A held lock
may be a worker that has not yet answered enter, one serving a turn, or one unwinding
after leave, and apex section 6's states distinguish exactly those, so reading it as
loaded-and-idle would be this crate inventing a fact, and would contradict the charter's
rule that the state publishes only on a ready aggregate.

**So `show` answers through the observation exchange, as of 2026-09-04, and residency is
read only where no worker answers.** `show` dials the agent's coordination socket and
opens `Observe`, per `weaver-admin-harness-contract` section 3, and what returns is the
harness's own word: `Unloaded` before any enter or after a leave, `Idle` or `Active`
with the load's facts beside it where a run stands, read from the run and never from the
record. **Those facts are `LoadFacts`, which overlaps the `load` event and is not its
shape**, per `weaver-types-Spec` section 4.2: it carries the session, run and artifact
the event carries in its envelope or not at all, and lacks the event's stack, lineage,
reset and prompt digest, so a consumer that stores both stores two shapes. Where the run
lock is free there is no worker and no exchange to open, and this crate reports
`Unloaded` from that absence without dialing, which is the one place the lock is read
and it is read as the absence of a worker and not as a state. **Where the lock is
held, `show` names the run's constituents beside the harness's word**, on
toddwbucy/WeaverWeb#15: the pid of every process holding the run lock's description, the
worker, the member and the relay, from the holder scan the escalation runs, sorted and
each named once, so a caller can check that every one sits in its own containment. A
held lock with no worker listening answers `Unloaded` with its constituents, a run that
never entered, for the caller to end with `unload`. A name with no root
refuses `NoSuchAgent` as every verb does, and whether a declaration validates stays
`validate`'s own answer, since no verb chains another. No verb answers for more than the
one agent named, admin being one agent's organ on the operator's ruling of 2026-10-01,
and managing several WeaverWeb's or a separate application's.

**A load never ends an existing run, and recovery from a killed invocation is the
caller's, through `show` and `unload`**, the second half of #60, on the operator's
ruling of 2026-10-03 on #72. A load's promise is a run started by this load or a refusal
with nothing changed, so a held run lock refuses it `AgentRunning`, or `Unanswered`
where the worker accepts the observation and answers nothing inside its bound, and it
touches nothing either way. An invocation that ended between taking the run lock and the
enter, by `SIGKILL` now that this crate ignores the catchable signals, leaves whatever
it had started holding the run lock, the member alone, the member and the relay, or
those and a worker whose observation answers `Unloaded`, no enter having reached it.
**`unload` ends it, because ending whatever holds the run lock is `unload`'s own
promise**: it acts on the lock and not on a classification, and a held invocation lock
refuses it `InvocationInFlight`, so it never meets a load still admitting its weights.
Where the observation answers `Idle` it directs leave per below. Where it answers
`Unloaded`, or no worker listens at all, the dial finding no name bound or its
connection refused through the dial's whole bound, no run was entered and there is
nothing to leave, so it goes straight to the escalation below. Where the worker is
silent it directs leave all the same, the leave's own bound and the escalation ending a
worker that is truly wedged and a healthy one answering `ActivityNotAtRest`. **The
recovery path is admin-con's**: it reads `show`'s facts and issues `unload`, then
`load`, the choice between leaving a run standing and ending it being the caller's and
never a load's.

**The record's instrument stays a test.** `show` on an admitted agent whose run lock is
held by a worker that has not yet answered enter answers through the exchange and
constructs no state from the lock, watched to fail when the verb is made to read a held
lock as `Idle`, which is the invention this clause forbids. Review could confirm the
absence of a mapping and could not confirm that the verb answers the harness's word
rather than something plausible.

```graph
node: admin-residency-is-not-lifecycle-state
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-residency-is-not-lifecycle-state

edge: grounds
from: admin-residency-is-not-lifecycle-state
to: axiom-harness-integrates-by-the-loop
```

**Two invocations for one agent are ordered by the invocation lock, and a worker's
presence by the run lock.** The in-flight flag that held one transition per agent went
with the map, and the unit-name uniqueness that replaced it went with the init system.
What holds now is the invocation lock: a second `load`, `unload` or `stop` arriving
while one runs refuses `InvocationInFlight` before it touches anything, across the
pre-enter window included, so two loads cannot both start a worker and an unload cannot
end a load that is still admitting its model. The run lock then answers what a later
invocation finds: a worker present or not.

```graph
node: admin-publishes-only-on-ready
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-publishes-only-on-ready
```

**`load` runs the charter's seven steps in order and the sequence is code
rather than convention.** Authorize the name, validate through section 4's one
inventory, verify the boundary in the same inventory, resolve the session and
open the sink per section 5, run the start step per section 6, dial the worker's
socket and direct enter per section 7, publish. Seven actions, the charter's
own, the bind-and-listen act the earlier form interleaved here having moved to
the worker with the inversion. **The run lock is taken before `validate`, and the save-point publication of section
6 runs under it, after the inventory and before the selection** of the save point to
restore, on the operator's rulings of 2026-10-06 on #1 (the A3.0 items) (item 7): a save point an unclean stop left in the member's
room is published and selectable by the load that follows it, and a concurrent `load`
against an active or a stopped-unload agent refuses at the run lock with `AgentRunning`,
or `Unanswered` where the worker is silent, and touches nothing. The publication adds no step to the seven,
being the inventory's first read of what the load restores. Each step's failure returns a typed
`lifecycle-refusal` and enters the rollback below carrying the step's name.

**`validate` stays the load's front half, and it is left room to grow.** WeaverWeb's
registration is where proper setup is confirmed, and `validate` is the natural command
line for it to call. This act does not extend it: what it answers is section 4's
inventory and nothing more. It closes nothing off either, its answer being a
`lifecycle-answer` case a later act may widen with the setup facts registration needs.

**`validate` is the load's front half, and the pin is one function.** The
inventory of section 4 is a single function that both the verb and the load
call, so the two cannot drift, which is the charter's one-code-path-entered-
two-ways rule made structural and the call graph the compiler checks.

```graph
node: admin-inventory-one-function
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-admin
to: admin-inventory-one-function
```

**What the call graph does not hold is where the verb stops.** Invoked as a
verb the inventory run ends at the report and answers `Validated`, having asked
the store where the declaration elects one and touched no other seam, and
starting no process, which is a behaviour no signature states and no count of
callers reaches. It is review's and takes its own record, because
a single record tagged for the mechanical half would claim the compiler for
the whole.

```graph
node: admin-validate-starts-no-process
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-validate-starts-no-process
```

**`unload` runs the charter's three steps, and the third waits on the second.** Direct
leave and await the aggregate, then await the run's exit, the worker's after it answers
left and the member's and the relay's with it, read as the run lock's release,
publishing the member's finished save points per section 6 once the member has stopped,
the leave's own among them, which the harness's `Left` answer names, and answer
provisioned-and-unloaded **only once the lock is free**. The publication adds no step to
the three, being the second step's tail. A refusal on leave, `ActivityNotAtRest` above
all, returns to the operator unchanged and answers nothing further; `ActivityNotAtRest`
covers a frame the gate admitted that the harness's loop has not yet taken, per
`weaver-harness-Spec` section 6, so an unload that meets one is retried after the turn.

**An unload whose leave save point is not finished does not complete**, on the operator's rulings of 2026-10-06 on #1 (the A3.0 items)
(item 6). The harness takes the leave's save point before it authors `unload`, per
`weaver-harness-Spec` section 6, and where that save point is not finished, the write
having failed, the answer or the acknowledgement having missed its bound, or the member
being dead, it answers `SavePointNotTaken` naming which, authors no `unload`, and stays
entered at rest: the run stays open with its gate lowered, since the harness lowers the
gate before it takes the leave's save point, the constituents keep the run lock, this verb prints
the refusal and exits non-zero, and nothing silent happens. If the member is alive the
operator retries, `save-point` and then `unload`; if it is dead the operator issues
`force-unload`, which directs the leave with `forced` set, so the harness leaves without
the save point and records on the `unload` event that the leave's save point was not
taken, and this crate leaves the clean-unload marker open under `ForcedUnload`, so the
next load restores the latest published save point with that reset recorded. The loss is
the operator's recorded choice, never this crate's. **Nor does an unload complete whose
leave save point did not publish** (Codex on #94, round 9), publication being part of
taking it: the leave's reported digest must be among the lines section 6's publication
appended at the unload, and where it is not, a room file past the bound or any
publication that does not land, this verb refuses `SavePointNotTaken` naming the
publication, the fifth leg and this crate's own, and leaves the marker open, so the next
load records `NoCleanUnload` and recovers the room's file or names it as unpublishable,
never restoring an older save point in silence behind an `Unloaded`. A forced unload
reports no save point and is unchanged. **A leave whose lock outlives the after-left
wait keeps its report** (the #94 survey's S7): the escalation ends the holders and the
reported save point is published as on a lock that freed, the same refusal following
where it does not land, so `Unloaded` is never answered over a leave save point left in
the room. **The marker closes clean only for this run's own save point** (the push review
of 197e80b): a report whose `event_run` is not the run the marker stands open on, the
reference this crate minted, publishes and leaves the marker open, so the next load
records the reset. The report's covered `run` is not compared, a run restored and left
with no turn covering the prior run's position. **A verb that finds the run already
ended, the lock free, publishes the room first** (the #94 survey's S11): a publication
that refuses or leaves a file refuses `SavePointNotTaken` naming the publication again.
**It never closes the marker clean** (Codex on #94 at 197e80b), on the rule that nothing
is lost silently, a `Closed` marker meaning a save point of this run's state published:
with the
run gone, nothing tells a leave whose publication failed from a run that crashed with
an on-demand save point in its room, and a published file proves no leave. So a forced
verb closes the marker as forced where it stands open, the operator's choice, and an
unforced one closes nothing, the next load recording `NoCleanUnload`. A conservative
label is never a false one. The survey's aim that a force record no `ForcedUnload` over
a leave's save point it published yields to that rule: a retry after `published` records a
reset, forced or not, over a leave that did take its save point, until the leave's
provenance in the marker, the lifecycle act's, can tell the two apart. A
forced leave the harness refuses past its `Left`, its organs going down behind the
refusal, closes it the same inside the after-left wait before the refusal returns, so
the next load records `ForcedUnload` and never `NoCleanUnload` for a run the operator
forced. **A force does not depend on the territory** (the operator's ruling of
2026-10-08 on #99): where the territory does not judge, or its groups do not resolve,
`force-unload` still ends the run, through the same leave, waits and escalation, but
writes nothing into the unjudged territory and reads nothing from it: no `admin.log`
line, which goes to standard error alone, and no publication, the room's files waiting
for a load, which judges the territory first. The marker, in the root, closes `Forced`,
so the next load records the loss. Force always works. `force-unload` is `unload` in
every other respect, the same waits and the same escalation.

**The leave has a bound of its own, 150 seconds from the verb's start**, once the
invocation lock is held: past the harness's 120 seconds for the save point's answer leg
and its two-second legs (the operator's ruling of 2026-10-08 on #1, `weaver-harness-Spec`
section 6), so admin never abandons a save point the harness still awaits. The
observation and both dials spend it, so `unload` holds the invocation lock at most those
150 seconds and the escalation's forty-five, 195 in all, **before the publication that
follows `Left`** (the #99 area 1 review): copying the leave's save point and any recovered
file the room holds, at most section 6's 32 of up to a gibibyte each, is bounded by that
cap and the copy's own speed and not by time, so a caller builds against 195 seconds plus
the copy of what the room holds, never against 195 seconds alone, beside `show`'s short
wait for the lock. A worker
that accepts leave and answers nothing inside it is a worker that would not exit: the
verb goes to the escalation below without the aggregate, answers
provisioned-and-unloaded once the lock is free, the run having ended with no leave
answered, which `admin.log` records and the next load's reset reads, and refuses
`WorkerWouldNotExit` where the lock still stands after it. Without the bound a wedged
worker would hold the verb, and with it the invocation lock, for ever, and since the
invocation ignores the catchable signals no later verb could recover the agent.

**The wait has a bound and an escalation, and the report never runs ahead of the lock.**
A run whose lock is still held thirty seconds after left, or past the leave's own bound,
is ended by the escalation: every holder is sent `SIGTERM`, then every holder still
standing ten seconds later `SIGKILL`, and the lock is read a last time five seconds
after that. **The three waits are fixed**, as the leave's and the stop's are, so
`unload` holds the invocation lock at most the leave's 150 seconds and these
forty-five past it. **The holders are found from the kernel's descriptor tables, because
a description lock names no pid**: `F_OFD_GETLK` reports a held lock with an `l_pid` of
`-1`, so this crate stats `run.lock` for its device and inode and scans `/proc/<pid>/fd`
of every process for a descriptor referring to that file, root reading every table, its
own process excluded, since it holds a descriptor of its own to read the lock. **A
descriptor is a holder only where its description holds the lock**:
`/proc/<pid>/fdinfo/<n>` lists a description lock (`OFDLCK`) only on the descriptors
sharing the description that took it, so a process with the file merely open, such as a
concurrent `show` reading the lock, is never named a constituent. **Each target is
pinned before it is signalled**: for each holder found it opens a pidfd (`pidfd_open`),
confirms through `/proc/<pid>/fd/<n>` and `/proc/<pid>/fdinfo/<n>` that the descriptor
it found still refers to the same device and inode and still holds the lock, and only
then signals through the pidfd (`pidfd_send_signal`), so a holder that exited between
the scan and the signal, and a pid reused since, is never signalled, a reused pid
holding no descriptor to root's file. **Where the lock is held and the scan finds no
holder** the verb refuses `LockHolderUnknown` and signals nothing, the description
having been carried where no table shows it, such as a descriptor in flight on a socket.
**An agent reported unloaded while any constituent still runs is the one report this
verb must never produce**: where the lock is still held after the escalation, the verb
refuses `WorkerWouldNotExit` and answers no state, which the rollback of this section
records as an act it could not undo, per charter section 5.

**A held lock after a clean leave is the case this clause exists for, and a free lock is
the ordinary end.** The leave the previous step confirmed causes the worker to exit, the
kernel releases the lock, and the verb answers. The defect an earlier form of this
section carried, every unload of a live agent answering `bind_failed` while the worker
was gone, came from reading an init system's ambiguous stop status, and the lock leaves
nothing ambiguous to read.

```graph
node: admin-unload-answers-after-confirmed-stop
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-unload-answers-after-confirmed-stop
```

**`stop` is a conveyance and its answer is a relay.** The operator runs the
verb, admin dials and opens the stop exchange on the coordination channel, and
the harness's answer, `TurnAborted` or `AtRest`, returns to the operator as
received. Admin holds no opinion about which, per charter section
3. This is the crate's own rule read at a verb: authorizing a stop and
deciding what a stop found are different acts, the second is the harness's,
and a relay that translated the answer would be admin ruling on a run it does
not conduct.

**Every answer a verb waits for under its invocation lock has a bound, and the stop's is
sixty seconds from the directive.** The enter's is section 2's 900 seconds, the leave's
150 above, and the observation's five seconds from the `Observe`, after the dial's
bound, which covers only the connect. A worker that accepts stop and answers nothing
inside its bound is not ended, a stop being no unload: the verb refuses `Unanswered`,
exits, and releases the invocation lock with the run as it stands, so `show` and
`unload` reach the agent next, and `unload`'s own bounds and escalation are the
recovery. **An observation unanswered inside its bound** refuses `show` with
`Unanswered` too, releasing the shared hold, and claims no state, and inside `load` it
refuses `Unanswered` and inside `unload` it is the silence section 3 meets with a
bounded leave. **No verb holds the invocation lock past a bound it states**, since the
invocation ignores the catchable signals and a wait without end would leave every later
verb refusing `InvocationInFlight` and `show` answering `InTransition`. The interior
verbs of section 2 take the same rule by the recipe.

**This record's edge moves to the integration invariant.** The labelling pass
placed it at `axiom-organ-and-submodule`, that being the nearest thing the apex
then held to a statement about domains, and apex section 5.4 settles what an
organ is rather than what an organ is answerable for. This claim turns on the
second question. An organ is answerable for its own domain and for nothing
outside it, per apex section 5.5, and what a stop found is a fact about a run
the harness conducts. A translated answer is the organ starting to reason about
a domain that is not its own, which is the harm that section states as its own
reason for existing.

```graph
node: admin-stop-answer-relayed-unchanged
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-stop-answer-relayed-unchanged

edge: grounds
from: admin-stop-answer-relayed-unchanged
to: axiom-harness-integrates-by-the-loop
```

**Rollback is the reap plus one directive, as data.** What a failed load can leave is a
running worker, a connected sink, and a device the SPU took, per charter section 5, and
the rollback walks what stands: direct a forced leave where a run was entered, a run
being undone keeping no save point (the #94 survey's S10), and, unless the leave answers
`OutOfOrder` (no run entered), leave the marker open on that run rather than restore the
one found, since the trace holds its `load` and the next load must record the reset
(section 4); end every
constituent the start step started by the escalation above, which never signals this
invocation, then close the invocation's own copy of the run lock's description, which
frees the lock, and close the sink where one opened.
Each act's failure is logged per section 8, the rollback reports what it could not undo,
and no state is published on any partial outcome, which is the same rule as the partial
load and not a second one.

**No failed unit is left to clear.** The clear the rollback asked of the manager, on
the operator's ruling of 2026-10-01, retired with the manager on 2026-10-03: a worker
that died leaves no name held and no lock behind, so the next load proceeds, and a
constituent still holding the lock is ended by the escalation above.

```graph
node: admin-rollback-logs-its-account
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-rollback-logs-its-account
```

## 4. The inventory

One function, called by `validate` and by `load` step 2 and 3, refusing at
the first failure with the field or check named.

**The parse is the floor's.** `weaver_types::parse` yields a whole
`AgentConfig` or a typed error, per that Spec's section 2, and this crate
adds no partial reader. A parse error maps to `ConfigInvalid` with the field
carried. That the parse is total and exposes no partial value is
`weaver-types-Spec` section 2's claim and asserted there, so what this crate
adds is the mapping and not a second statement of the parse.

**This crate reads no prompt**, on the operator's ruling of 2026-10-06 (#1), which
supersedes #57's reader of 2026-10-02: the system prompt is state, entered through the
gate by the seeding step as the agent's first turn and held by the member, so no root
is in the prompt's path, the parse takes the declaration's text alone, and the enter
carries no prompt in bytes or by digest. The one digest this crate computes is the
declaration's, `declaration` on the enter, per `weaver-admin-harness-contract` section
5. **The enter carries the operator's uid**, `operator`, the value the root's `operator`
key names per section 9, so the harness admits a seeding line from the operator's peer
identity alone, per `weaver-gate-world-contract` section 2; this crate names the
operator and judges no line. `system-prompt.md` beside the declaration is the
operator's draft, which the seeding step of the deploy runbook reads as the operator
and sends through the gate after this crate's load, and which this crate never opens.

**The existence checks are admin's where admin holds custody, and an ask where
another organ does, per charter section 4.3 as ruled 2026-09-05 on issue #456.**
The sink exists, or its creation flag is set, per the discriminated cases of
section 5. The agent's uid resolves and its home directory exists with the
expected ownership and modes. Any failure refuses with `BoundaryUnverified`,
nothing is repaired, and nothing is built, per charter section 2. **The model
binding's artifact is checked for presence and never resolved here.** An empty
member refuses `ConfigInvalid` naming
`spu-instruction.decoder.model-binding.artifact`, the omission being the
declaration's, and whether the named artifact resolves is the SPU's at
admission under the agent's identity, per `weaver-spu-Spec` section 3: a look
from here runs as root and cannot see a directory the agent uid is denied, so
`ArtifactUnresolvable` is never this crate's refusal and the walk raises it at
no site. The checks are a list a reviewer reads against the charter's boundary,
and review is the instrument that holds the set, stated rather than left to look
like an omission. **One member of the set has a test and has had one since the
crate landed**, as of 2026-09-07 per issue #481: a home that does not exist refuses
`BoundaryUnverified` and the walk builds nothing, the absent directory still absent
after the refusal. That claim is bought by perturbation, the walk creating the home
on the miss and the test failing on the directory it finds, and it takes its own
record below so that the tag on each record names the instrument that holds it.

```graph
node: admin-existence-checks-repair-nothing
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-existence-checks-repair-nothing

node: admin-missing-home-refuses-and-builds-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-missing-home-refuses-and-builds-nothing
```

**The store election is judged here too, as of 2026-09-04.** Every election but
`none` stands a member, so every election but `none` requires the member's
binary beside the worker's, and a box that lacks it refuses with
`BoundaryUnverified` rather than running without a leg the declaration never
declined: the leg's standing is the declaration's fact and not the directory's,
per `weaver-state-PRD` section 4 and issue #381, and a missing binary is never
read as an absent member. Absent, the election resolves to the embedded engine
under that same requirement. `none` declines the member and requires nothing,
and refuses a declaration that elects a state election beside it,
`ConfigInvalid` naming `state-election`, per `weaver-types-Spec` section 2: an
election with no member to receive it is malformed, not surplus. An engine
this build does not provide refuses at the declaration's parse, `ConfigInvalid`
naming `state-store.engine`, per `weaver-types-Spec` section 2, and the walk's
two questions to the service engine's gates retired with that engine on the
operator's ruling of 2026-10-02 on #1. The grant surface is not judged here, only read, at the
enter and again at the leave, per `weaver-trace-PRD` section 3.1.

**Every election but `none` requires the member's own account too, as of
2026-09-15**, per issue #545. The account is `weaver-<name>-state`, derived
from the same validated name section 1's identity is, and read from the
account database rather than declared, for the reason the agent's home is read
rather than composed: the uid the spawn drops to, the uid that owns the
territory are one kernel fact, and a declaration naming it would be a second place that fact is stated
and a value the operator could move under a running agent. A box lacking the
account refuses `BoundaryUnverified` naming the account, exactly as a box
lacking the member's binary does and for the same reason, the provisioning
being what is absent. **The account's primary group is the state group** of the
same derived name, resolved by name, and is not gid 0: the member passes the
territory by its primary group and section 9 judges the territory against the state
group, so a box where the two differ, or where the state group does not resolve,
refuses `BoundaryUnverified` at the inventory rather than at every load.

```graph
node: admin-member-account-required-at-inventory
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-member-account-required-at-inventory
```

**A restore is judged here too, and it names a save point**, on the operator's ruling of
2026-10-02 on #58, which retires the record restore of 2026-09-04 (issue #432), and on
the operator's rulings of 2026-10-06 on #1 (the A3.0 items) (items 1, 3 and 5), which shape the selection. **It is judged only where a
state member stands**: under the `none` store engine the inventory selects no save point
and resolves no lineage, and a declaration naming `restore` beside that engine refuses
`ConfigInvalid` naming `restore`, per `weaver-types-Spec` section 2. **The manifest of
section 6 is the record of what is loadable**: the inventory reads it, root-owned in
the territory's `save-points/` of section 9, one line per published or named save point, and
selects either the save point the declaration's `[restore]` names, by its published name
or its digest, or the latest, the line of highest ordinal, whose file must stand under
its name and digest to the line's digest. **The latest is never passed over** (the
Planner's ruling of 2026-10-08 on #94, beyond the survey's S9): its file gone, of other
bytes, or not reached or judged (an open or a read that fails, an owner, group, mode or
size not as published) refuses the load `BoundaryUnverified`, naming the line and what
was found, and no older save point is restored in its place. The operator names an older
one with `restore`, a deliberate act the manifest records; a save point so named, or
named by `[restore]`, is judged as before. **No state is restored older than the latest
unasked**: the publication defers on any entry it declines and a load refuses on any
deferral (section 6), and the selection refuses where the latest does not stand, so
older holdings return only by the operator's name. **A member stands by the
declaration's election, never by the box's account** (the #94 survey's S2): an agent
electing `none` restores, publishes and waits on no member, though the box carries the
member's account. **A file no line names is not
loadable**, whatever stands in the directory: a declaration naming one refuses
`ConfigInvalid` naming `restore`, and the way such a file becomes loadable is the
`restore` verb, which judges it as below and appends the line that names it, marked as
named at a restore, so the manifest records what this crate published and what the
operator named and nothing else; there is no operator-supplied save point, only a save
point named at a restore, on item 3. **The verb judges a listed name too**: the file is
opened and judged as the load judges it below before the verb answers, gone refusing
`BoundaryUnverified` and not digesting to its line refusing `ConfigInvalid` naming
`restore`, as the load would, so `RestoreNamed` means named and judged loadable now, never
a line whose file has since gone or changed. **The verb makes the named save point the
latest** (the operator's ruling of 2026-10-08 on #99): a listed one that a newer line
outranks is named again under the next ordinal, marked as named at a restore, so the load
that follows continues from it. **A `[restore]` is honoured only while the save point it
names is the manifest's latest**: where a newer save point has been published since, the
load refuses `ConfigInvalid` naming `restore` and names both, the operator then removing
`[restore]` to load the latest or running `restore` again to continue from the named one,
so a key left behind never restores older state. A manifest that stands beside published
files and does not read, or a named line whose file is absent, refuses `BoundaryUnverified` naming
the manifest, and no manifest and no file is an agent's first load; a listing of
`save-points/` that fails refuses the same way and is never read as an empty directory
(the custody audit's G9). **The selected save
point's bytes are judged here**, before any process exists, under this crate's custody
through the descriptor it opens and never by path: the entry judgment of section 9 (a
regular file, not a link, root's, mode 0640, grouped to the access group (judged by
name, section 9)), a stamp line
of the format's seven members, a check over the bytes that holds, a digest equal to the
manifest's and a published name computable from the stamp per `weaver-state-Spec`
section 3, and any of these failing refuses `ConfigInvalid` naming the save point, so a
damaged save point never reaches the member and the operator names another. **The judgment is the member's format rule read twice, and held equal by one corpus**:
this crate links no interior crate, so its reader of the format and the member's parse
are two hand-written readers, and the workspace's save-point corpus
(`crates/weaver-types/tests/fixtures/save-points/`, a sound file and one malformed each
way with a verdict per file) is read by both crates' tests, so a reader that admits what
the other refuses fails its own test. **The
schema is the member's to judge**, at the opener and answered on the enter's `restored`
ask of `weaver-harness-state-contract` section 2: this crate links no engine and the
member's vector carries no flag, so the one party that can read the schema a save point
stands under against the one it runs is the member, and a mismatch refuses the enter
there, before `load` is authored. **The lineage that crosses the enter is resolved here
from the stamp and the manifest line**: the digest, the run, the sequence and the last
turn the stamp covers, and `named_at_restore`, true where the line arrived by the
`restore` verb; `built_from` stays absent until the offline builder's runner lands, which
is `weaver-state-Spec` section 6's cell and not this act's. **The descriptor is opened
at the selection and placed at the spawn**: the save point is opened for reading,
close-on-exec, at the inventory, and placed at its fixed number in the member's process
at the spawn alone, per section 6 and `weaver-state-Spec` section 2, so no path rides
the vector and the agent, whose worker never holds the descriptor, never reaches the
save point. No save point selected is an absent descriptor and no refusal, the member
standing empty.

**The clean-unload marker is written at both ends, and a write that fails refuses the
verb at that end**: at the load after the enter answers, where a failed write rolls the
load back, and at the unload after the member has stopped, where a failed write refuses
the unload as unrecorded, the run being gone either way, so no run stands or ends with a
reset the marker could not say. **The marker is this crate's, and it is written only
once a run stands**,
on the operator's rulings of 2026-10-06 on #1 (the A3.0 items) (item 5): at every load, after the enter answers `Ready`, this crate
writes, in the agent's config root under its own custody, a marker naming the run it
minted as open, and a clean unload marks that run closed. **A load that rolls back after
its `load` event is on the trace leaves the marker open on that run** (the operator's
ruling of 2026-10-08 on #99), whether or not this crate read its `Ready`: an enter refused
after `load`, or a `Ready` past the load bound, authored `load` all the same, so the next
load records the reset. The rollback's forced leave says which it was: `OutOfOrder` is a
harness never entered, so no `load` is on the trace and the marker stays as it was found;
`Left`, or no answer, is a run that may have authored `load`, and the marker names it, a
conservative reset rather than a false clean one. A load that finds a
marker naming a run still open resolves the enter's `reset`, which rides apart from the
lineage and whether or not a save point stands, to that run with `NoCleanUnload`, or
with `ForcedUnload` where `force-unload` closed that run without its save point, per
`weaver-types-Spec` section 4. An agent's first load finds no marker and resolves no
reset. **A marker that stands and does not read refuses** (the custody audit's G8): one
that is a link, torn, foreign or past 4096 bytes refuses the load before any
process stands and the unload before it writes, `BoundaryUnverified` naming the marker,
never read as no marker, since an absent marker resolves no reset and a run left open
would load without its `NoCleanUnload`. The marker's temporary is created `0600`
without following a link, root and gid 0 set on it, and only then given its `0644`. Branching from a record is the builder's, outside the agent, and this crate never
reads a record for a load. **The judgment is this crate's because the save point is**,
per charter section 4.3's custody rule: a look at a thing admin holds, taken at the
load's cheapest moment before any process exists, and an ask of nobody. **The
instrument is perturbation**: a `restore` naming an absent or unlisted save point
refuses naming `restore`, watched to fail when the manifest check is dropped; a file
whose digest is not its line's is not the latest, watched to fail when the digest
comparison is dropped; a damaged save point refuses naming it, watched to fail when the
check is dropped; and a load failing after the enter leaves the marker as it found it,
watched to fail when the rollback's restore is dropped.

```graph
node: admin-restore-save-point-judged-at-the-inventory
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-restore-save-point-judged-at-the-inventory
```

**A reachable organ's boundary is judged here when one exists, per charter section 4.3
as ruled 2026-09-05 on issue #438.** The refusal's shape is fixed now so the act that
charters the first loopback or off-host organ lands it rather than electing it: a
declaration whose reachable organ carries no boundary refuses `ConfigInvalid` naming
that organ's `boundary` member, ahead of every look at the box, because the omission is
the declaration's and not the provisioning's, and admin grades nothing about the
boundary it finds, a declared one being accepted whatever it is. Today no organ in the
base is reachable other than by kernel peer identity, no such member exists on the
declaration, and the floor's parse refuses a binding to a loopback port or an off-host
endpoint as an unknown field, so this clause binds the future act and judges nothing on
this date. The load event's carrying of what was declared is `weaver-trace-PRD` section
3.1's to state with that act.

**One check in that list is the second walk's mechanism and is held
mechanically instead.** The sink path's containing directory is root-owned
and not searchable by the agent uid, whatever the sink's kind: the agent uid
is not the owner, holds no group search bit through any membership, and the
other bits carry no search. Section 10's second walk derives a
perturbation-verified test from that one check, so it takes a record of its
own rather than riding the review the rest of the list takes, a single record
for the whole list having claimed the test for checks no test touches.

```graph
node: admin-boundary-denies-agent-traversal
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-boundary-denies-agent-traversal
```

**"Through any membership" is the whole of the requirement, and the gid set the walk
reads is where it is met.** The start step drops the worker to the agent's own group by
name, per section 6, so the worker's runtime egid is the agent's own group whatever
primary group passwd records. A walk reading the passwd gid alone therefore asks about a
credential the worker does not run under, and where the operator provisioned a shared
primary - which is the case section 6's own argument for naming the group cites - the
two disagree: a sink directory at `root:weaver-<agent>` mode `0710` passes a walk that
sees only `users`, and the running worker traverses it. So the walk reads the group the
start step sets, the passwd primary, and the user's supplementary memberships, and it
**over-approximates on purpose** - the question is what the agent could reach, so a gid
too many refuses a boundary that might have held and a gid too few admits one that does
not.

```graph
node: admin-boundary-reads-every-gid-the-worker-holds
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-boundary-reads-every-gid-the-worker-holds
```

**The devices the binding assigns are not checked here, and the absence is
stated rather than left to be inferred.** The parse has already answered that
the assignment is present and well-formed, per `weaver-types-Spec` section 2,
which is the whole of what this crate needs to know about it. Whether those
devices exist on the host, whether they have room, and whether they can reach
each other are questions about hardware, and admin reasons about the device at
no point, per ruling C of 2026-07-31, so they belong to the one authority on
the device and are answered at admission. The check would be easy to write
here and that is what makes stating its absence worth the sentence: an admin
that verified the GPU would be the second arbitrator ruling C removed,
reintroduced as a convenience.

**This record grounds in the integration invariant as well, and the two edges
are separate reasons.** That the device has one authority is the domain
partition `axiom-organ-and-submodule` draws, and it is what gives the question
somewhere else to belong. That admin forms no view of it anyway is apex section
5.5's bound, an organ being answerable for its own domain and for nothing
outside it, with what the device can carry reaching a load through the harness
rather than through a second check here.

```graph
node: admin-checks-no-device
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-checks-no-device

edge: grounds
from: admin-checks-no-device
to: axiom-organ-and-submodule

edge: grounds
from: admin-checks-no-device
to: axiom-harness-integrates-by-the-loop
```

**The name is checked before anything else is touched.** The agent name is
validated as a name, non-empty ASCII letters, digits, `-` and `_`, its root of
section 9 is admitted, and the constructed identity is `weaver-<name>` from the
validated name, never from a caller-supplied string, which is the name-validation discipline of charter
section 7 landing at the one site that constructs. It is the one site because
the same validated name is what section 6's start step interpolates,
so a name reaching a path or a process has one origin and review reads that origin
rather than every use. **A name ending in a reserved suffix refuses**: `-state`,
`-trace`, `-relay`, `-admin` and `-admincon`, the last being the connector's service
user, on the operator's ruling of 2026-10-03 on #50. The
accounts and groups the box provisions for an agent append those suffixes to its name,
`weaver-<name>-state`, `weaver-<name>-relay` and the rest, so an agent named `x-relay`
would own `weaver-x-relay`, which is agent `x`'s relay account, and the collision would
hand one agent another's identity. The suffixes are reserved rather than the derivation
changed because the derived names are already provisioned on running boxes, and the run
directory leaves the collision behind by its own namespace, per section 3.

```graph
node: admin-identity-from-validated-name
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-identity-from-validated-name
```

## 5. The sink

Opened by the discriminant the config carries, under root, the role's
principal, every descriptor close-on-exec in the opening call itself.

**This section runs for every binding and does not read the kind.** Both kinds
declare a sink and both author a record into it, the kind selecting which
mechanism the harness authors through rather than whether it authors, per
`weaver-agent-PRD` section 6 as ruled 2026-08-24. So the discriminant is
opened the same way and the descriptor is sent the same way, and nothing here
branches. An act earlier that date scoped this section to a serving binding on
a reading the same day's ruling replaced, and the scoping is withdrawn rather
than narrowed: a diagnostic sink is a sink.

**`File { path, create }`.** Judged before it is opened and after, on #62: a trace the
member could replace, or a link root would follow, would hand the worker a file other
than the record, and since #80 the relay would read that file back out to the trace
reader. **The trace's directory is judged first**: resolved once, owned by root or the
admin principal and writable by no group or other, the territory's 0710 passage
being no write, and every directory above it held by section 9's
rule. **The open goes through that directory** write-only with `O_APPEND`, `O_CLOEXEC`,
`O_NOFOLLOW` and `O_NONBLOCK`, so a link at the trace's name refuses and a FIFO there
never blocks, and the descriptor must be a regular file. **An existing trace stands as
the territory lays it out** (#56): owned by root or the admin principal, grouped to
`weaver-<agent>-trace`, mode 0640, and any other is a trace replaced or never
provisioned, which refuses rather than being appended to. Where the flag is set and no
trace stands, admin creates it exclusively (`O_CREAT|O_EXCL`) and gives it that layout,
so the next load finds it as provisioning would have. Every refusal here is
`BoundaryUnverified`, naming the path on stderr. An absent trace without the flag is
refused earlier, by section 4's inventory, as `BoundaryUnverified` before the open is
reached, and only a trace that vanishes between the inventory and the open reads
`DescriptorsUnusable` here. This is the custody of charter section 7. The relay's
read-only reopen goes through `/proc/self/fd` of this judged descriptor, per section 6,
and never through the path. Append-only rides the open file description, verified: a
duplicate of the descriptor carries the flag, so the worker's copy appends wherever it
writes, which is what `weaver-trace-Spec` section 7 relies on from the far side. The
instrument is review, the verification being a fact about the kernel read once rather
than a property this crate's own suite can perturb.

```graph
node: admin-sink-file-append-only
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-sink-file-append-only
```

**`Pipe { path }`.** Created with `mkfifo` at mode 0640 when the creation
flag is set. Opened write-only with `O_NONBLOCK` and `O_CLOEXEC`, because a
blocking open of a reader-less FIFO hangs the load, and the nonblocking form
fails loudly instead. Verified: the open returns `ENXIO` when nothing holds
the read end, which maps to `DescriptorsUnusable` and refuses the load with
the truth, that the operator's tooling is not listening. On success the
nonblocking flag is cleared, verified clearable, so the worker's writer sees
ordinary blocking semantics.

```graph
node: admin-fifo-open-nonblocking-refuses
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-fifo-open-nonblocking-refuses
```

**`Socket { path }`.** A stream connection to the operator's listener,
close-on-exec at the socket call. There is no creation flag, per
`weaver-types-Spec` section 2: something of the operator's must already be
listening, and a connection refused refuses the load. The discriminated shape
this section opens by, and the asymmetry that leaves this case without a
flag, are that Spec's claims and asserted there.

**One open site, and the path dies at it.** The sink is resolved and opened
in this module and the resulting `OwnedFd` is what travels, so no other
module of this crate holds a sink path, and the worker never sees one at
all, per the descriptor discipline the contracts fix. What this clause
asserts is the site inside this crate. That the worker holds no path is a
pin on the worker's side of the seam and is declared by the crates that would
have held it, per section 10.

```graph
node: admin-sink-path-dies-at-open-site
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-sink-path-dies-at-open-site
```

## 6. The start step

**The agent starts the way an appliance does, and the start step is this crate's root
act**, on the operator's ruling of 2026-10-03 on #50 that the agent leaves systemd. No
unit is asked of an init system and nothing is installed to wait for the agent: restart
on a crash or a reboot, sandbox hardening and log collection belong to whoever packages
and deploys the agent, with systemd, a container or anything else, and are not this
framework's. What the transient unit did for the agent, the start step does itself, as
root, in the `load` verb, after section 4's inventory has passed and section 5 has
opened the sink, and holding the invocation lock and the run lock it took, per section
3's order: it prepares the runtime directory, stands the trace relay, starts the state
member, and starts the worker under the agent's own account, each child inheriting the
run lock's description at its fork, per section 3. Each is below, and
`weaver-admin-systemd-contract` retired with the unit. The code act of #50 removed
`crates/weaver-admin/src/unit.rs`, and its deploy act removes the units from the deploy
scripts, which name `weaver-worker@<agent>` only to refuse a box whose agents still run
under one, `update-stack.sh` before an install and `decommission.sh` before an archive.

**The runtime directory is made by the start step, with its owner and mode stated**:
`<coordination-root>/weaver-<agent>/`, `/run/weaver-<agent>/` by default, owned by the
agent's account and the agent's own group, `weaver-<agent>`, mode `0750`, so the agent
binds its coordination socket and its gate socket there, the operator reaches it through
membership in the agent's group, and no one else reaches it. Systemd made it at `0755`
absent an instruction, which is why the mode was always stated rather than inherited, a
boundary whose permissions come from an ambient default being a boundary nobody elected,
per `weaver-gate-Spec` section 3. **The directory outlives the worker now**, where the
unit's cleanup removed it at every stop, so the start step finds one standing and
repairs it rather than trusting it: it is opened without following a link, judged for
its owner, group and mode, and set right, and a socket name left in it by a dead worker
is removed, which is safe only because this happens holding the invocation lock and
the run lock this load took, per section 3's order, so no constituent of an earlier run
stands and no other invocation can start one.

**The group is named rather than inherited, and the mode rests on that.** The worker is
dropped to the agent's uid and to `weaver-<agent>` as its group by name, never to
whatever primary group the agent user was provisioned with, so where that is shared,
`users` or `nogroup`, `0750` would grant traversal to every member of it and the
sentence above would be false. Naming it also fixes the group the access rule is
checked against at section 4's inventory, so the two locks narrow one set rather than
two. **It carries a provisioning requirement**: an agent user created without a group of
its own cannot be started, and validate catches that at the reachability check below. A
box carrying the agent user and not its group is refused there by name, the missing
group being a fact about a provisioned agent. **A box carrying neither is not refused**,
on the ground that unresolvable is not unreachable: a declaration may not be refused on
a fact never established, and a bare checkout carries no agent group at all.

**The access rule is checked against the mode that will carry it**, and the
check is this section's. The gate binds its socket `0770` owned by the agent's
group, per `weaver-gate-Spec` section 3, so a peer reaches `accept` only
through that group. A rule admitting a uid outside it names a peer the
filesystem turns away at
`connect(2)`, before the credential check runs: the dialer sees a permission
refusal, the driver reports the socket never stood, and the gate records
nothing because the peer never arrived. That diagnosis cost two runs on
2026-08-27 while it was an unprovisioned box, and the mode makes it the
designed behaviour unless the two are made to agree.

**Two locks on one door may narrow the same set and may not contradict.** So
the inventory refuses a declaration whose `allowed-uids` name a uid outside
the agent's group, before the start step runs.

**It refuses as `BoundaryUnverified` and not as `ConfigInvalid`.** The declaration is
well formed and the fault is the box's: the operator wrote a uid that ought to reach the
socket and the provisioning has not put it in the agent's group. `ConfigInvalid` names
the declaration's TOML, the format `weaver-types-Spec` section 2 elects, and a deployer
reading it that way deletes the uid - which makes validate pass and breaks the connector
for good, the credential check then denying it at `accept` with nothing saying why. This
is the same fault an unprovisioned home or sink is, and it answers as they do, naming
the group on the way out. 

**A user without its group is named too.** Starting the worker under its
group is a hard requirement, so a box that provisioned the agent user and not its group
is checked and refused by name rather than failing the start opaquely. A box carrying
neither has provisioned no agent and is not refused on a fact about an agent it does not
have, which is what lets a bare checkout and CI run this at all.

**`allowed-gids` is not judged and uid 0 is skipped.** A gid names no
particular peer, and whether one holding it reaches the socket turns on that
peer's own memberships rather than on the gid - a rule admitting an operator's
primary gid works where that operator is a supplementary member of the agent's
group, and an arm refusing every gid but the agent's own refused it. Root is
skipped for a different reason: `CAP_DAC_OVERRIDE` reaches a `0770` socket
whatever group it holds. Both halves rest on the credential check, which is
where they were always decided.

**The check runs last of the inventory's walks**, so it preempts none of them:
an unverified boundary is the older and narrower fact, and a check running
ahead of it made three inventory tests depend on whether the box carried the
agent's group. **Unresolvable is not unreachable**:
where the group cannot be read this asserts nothing and the load fails later
and loudly, rather than refusing on a fact it never established.

```graph
node: admin-access-rule-reaches-the-socket
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-access-rule-reaches-the-socket
```

```graph
node: admin-runtime-directory-mode-is-stated
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-runtime-directory-mode-is-stated
```

**The worker is started bare, under its own account, detached, and owning its organs.**
The start step forks, the child inheriting the run lock's description, and the child
does six things before it executes the worker with the vector below. It takes a new
session (`setsid`), so the worker belongs to no terminal and outlives the invocation
that started it, though never its invoker's containment, below. It points its standard
input at `/dev/null` and its standard output and error at the agent's worker log, a file
of its own beside the operations log and never that log, since an agent holding a
writable handle to the boundary's record could write into it. It replaces its
environment with a fixed one, `PATH=/usr/bin:/bin`, `HOME` the agent's home,
`LANG=C.UTF-8`, and `LD_LIBRARY_PATH` where the root's optional `library-path` names the
engine libraries, the value `unit-properties` carried before it retired, and nothing
else, so no variable of the caller's or of sudo's reaches the worker or the organs that
inherit its environment. **`library-path` is judged and recorded**: the directory it
names is held to the root's own judgment, root-owned and writable by no group or other,
its ancestors closed as the root's are, because whatever it holds is loaded into the
worker and its organs, and its value rides the enter for the harness to record on the
`load` event beside the stack, so the record names the libraries a run loaded, pinning
their digests being #71's. **It resets every signal the invocation ignores to its
default disposition and clears its signal mask**, the set of section 2, and an ignored
disposition survives a fork and an exec, so without the reset the worker, and through it
every organ, would ignore the unload's `SIGTERM` and a packager's stop alike. **The
relay's and the member's children take their own new session first and reset the same
set the same way after it**, before their own execs, so neither belongs to the invoking
terminal's process group: a caller's hangup or interrupt reaching that group then
reaches only the invocation, which ignores both, and never a child whose dispositions
are already back at their defaults. **It moves to `/` and sets its file-creation mask to
`027`**, since the caller's working directory and umask are not the agent's. **It marks
every descriptor above its standard streams but its gifts close-on-exec**, so whatever
the caller left open, a root shell's descriptors among them, never crosses the exec, per
section 10's allowlist. It sets `PR_SET_NO_NEW_PRIVS`. It narrows its supplementary
groups to `weaver-<agent>`, then its gid, then its uid, in that order, because the
narrowings need the privilege the last one gives away. Then it executes. It carries
across the exec exactly two descriptors besides its standard streams: the run lock's
description, which section 3 needs held by every constituent of the run, and the write
end of the relay's lifetime pipe, below. **The sink does not cross at the exec.** It
crosses in the enter directive as ancillary data over the coordination channel, per
section 5 and section 7, the route the harness contract already holds and tests, so
nothing of the record rides the worker's start and the organs the worker forks inherit
no handle to it. The worker binds its own coordination socket in the runtime directory,
per `weaver-harness-Spec` section 2.3, and this crate dials it per section 7 as before.
The worker kills its organs when it dies, per `weaver-harness-Spec` section 2, so an
orphaned SPU never holds the device.

**The agent's lifetime is bound to its invoker's containment, and this crate depends on
no supervisor**, on the operator's ruling of 2026-10-03 on #72. The new session leaves
the terminal and nothing else: every process the start step starts stays in whatever
containment the invocation ran in, so where admin-con runs as a service the worker, the
member and the relay share its group, and stopping admin-con stops its agent and only
its agent, each agent having its own. **That is the intended behaviour and not a leak**:
the management plane going down may mean it was taken over, so the agent fails closed
with it. This crate moves no process into a group of its own and asks no supervisor to,
so the binding is the invoker's containment's and never this crate's code. The
consequences are stated in `weaver-admin-operator-contract` section 3 and carried to
toddwbucy/WeaverWeb#15: an orderly stop
of admin-con unloads first, a kill is an unclean stop that resets the next load to the
latest save point, and the invoker's resource limits contain the agent.

**A failed dial is answered from the worker's own exit, which the start step can read.**
The invocation is the worker's parent until it exits, so where the dial's bound expires
it reads the child's status without waiting and the run lock's state. A worker that
exited refuses naming its exit status or the signal that ended it, which says more than
the manager's three values ever did. A worker still running with no socket is a bind
that failed, `BindFailed`, and the rollback of section 3 ends it. **The instrument is a
test whose watch turns on the status it names**: a worker binary that exits non-zero
refuses naming that status, watched to fail when the status read is removed and the
refusal falls back to the absent residency.

```graph
node: admin-failed-dial-reads-the-worker-exit
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-failed-dial-reads-the-worker-exit
```

**The subprocess inherits nothing it was not deliberately given, because every
descriptor this crate holds is close-on-exec atomically at creation,** no descriptor
existing for an instant between its creating call and its flag. The deliberate gifts
are the member's own end of the first door's pair, per the operator's ruling of
2026-08-26, the save point the member restores, per section 6, and the run lock's
description to each of the worker, the member and the relay, per section 3: each is
created atomically flagged like everything else and re-armed onto its child's fixed
number in the spawn path itself, so each inheritance is an act at one site and never a
default anywhere. This is the behavioural half of the custody section 1
opens, and section 10's third walk makes it a test, where section 1's half is the
ownership the compiler holds. The two halves carry separate records because a test
cannot demonstrate ownership and the borrow checker cannot see a flag. **The behavioural
half grounds in apex section 5.1 where `weaver-harness-Spec` section 2.2 grounds the
same claim**, a pair with no name being authenticated by possession of the descriptor
and by nothing else, so an end that crosses an exec is a credential handed to whatever
runs next and the window a later `fcntl` opens is where it is handed over. The ownership
half at section 1 grounds in nothing, being this crate's representation of a descriptor
rather than a claim about who can reach one.

```graph
node: admin-cloexec-atomic-at-creation
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-cloexec-atomic-at-creation

edge: grounds
from: admin-cloexec-atomic-at-creation
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The worker's argument vector takes no value the invocation's own input composes.** The
only value the start step interpolates is the validated agent name of section 4, so the
authority stays bounded by the agent's root exactly as charter section 7 requires. Its
values are the coordination socket path of section 7, which this crate already derives
from that same validated name, the two organ binary paths section 9 holds among the
operator's installed values, the SPU's being the agent's own, and, where the declaration
carries one, the agent's loop file as the named flag `--loop-file`, one token on both
sides of the vector, composed here and parsed by the worker under the same spelling, per
`weaver-types-Spec` section 2 and the operator's ruling of 2026-08-20 on issue #243. A
worker that holds no file-read loop refuses the flag at its own argument parse, named
rather than ignored, so a declaration the installation cannot honor fails the load
loudly instead of standing as a fact with no effect. The loop file is the vector's one
declaration-sourced value and it widens nothing: the declaration is the operator's file,
validated at section 4's inventory before the start step runs, and the worker resolves
the path under the agent's own identity, so the value grants nothing the agent uid did
not already have, per the bare clause below. An absent member puts no flag on the
vector, the worker's own default standing, per `weaver-harness-PRD` section 2. A builder
who let any of these values be composed from the invocation's own input would be
widening the delegated authority by the route the name check closes, so the shape to
hold is that the vector reads the checked name and the agent's root, the installed
values and the validated declaration, and reads nothing else. An earlier form of this
clause counted three values and named the name the one variable, written before any
declaration member rode the vector.

**The classify arm's binary rides the vector too, where it stands, as
`--classify-binary`.** It is the third organ path the vector carries and the
first that section 9 does not hold among the operator's installed values,
because it is derived rather than placed: admin joins `weaver-spu-classify`
to the directory of the worker binary the operator did place, exactly as the
state member's binary is found, per section 9. **Where the file is absent
the flag is absent**, and no load is refused here on that account. A
declaration electing the arm on a box that has no binary is refused by the
harness at admit, `ConfigInvalid` naming `classify`, per
`weaver-spu-PRD` section 15.3, and **that refusal is not repeated here**:
admin provisions what it holds and the organ judges what it can stand, per
the custody rule of issue #456. Admin passing a flag for a file that is
there, and staying silent about a file that is not, is the whole of its
part.

**The state member is started here too, as it always was, a direct child of this
crate.** The custodian runs under its own account over a territory this crate prepares,
per `weaver-admin-PRD` section 2, and this crate holds no channel to it once it runs.

**The member runs under its own account, and the charter's sentence is met as
of 2026-09-15**, per issue #545 and the operator's ruling of that date.
`weaver-state-PRD` section 4 has the member holding "a uid of its own over one
subdirectory the agent's uid cannot enter", and it now does: the account is
`weaver-<name>-state`, resolved and required at section 4's walk, the territory
is made `0700` and chowned to it, and the spawn drops to it. The G5 marking
this clause carried from 2026-08-25 to that date retires with the gap it
described, there being no divergence left for an authority to settle. **The
three parts are one act and none of them holds alone**: an account nothing runs
as is a passwd entry, a chown under a spawn that keeps root is a room its
occupant does not need, and a privilege drop into a room the member cannot
write is a member that dies at its first open.

```graph
node: admin-member-spawn-drops-to-its-account
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-member-spawn-drops-to-its-account
```

**The drop's record is perturbation, as of 2026-09-26** (#673 item 6). What it asserts
is that the spawned process runs as the member and holds no group root left it - the
supplementary set narrowed to the member's own group alone, then the gids, then the
uids, in that order, because the two narrowings need the privilege the last one gives
away. **The member holds its own group alone**, on the operator's ruling of
2026-10-08 on #1 (the custody audit's G11): the territory is `0710` under the state
group, the member's own primary group, so the member passes to its own room by that
group, and the drop sets the supplementary set
from this crate's slice and never from the account database, so no membership the
account carries reaches the member. Neither the access group, which reads `admin.log`,
`worker.log` and the published save points, nor the trace's group is among them, so the
member reads none of those. The
instrument drives `stand_state_member` itself inside a user namespace, where the test
runs as root over the invoking user's subordinate ids: a stand-in member beside the
worker records the identity it runs under into the territory the real path prepared, and
the kernel's status after exec must read every uid the member's, every gid its group's,
and the supplementary set that group alone. **It stood at review
until then** on the
reading that an unprivileged suite could only assert the three calls' refusal. The store
probe's namespaced watch of 2026-09-24 disproved that reading, the same calls succeeding
inside the namespace, and this instrument is that watch pointed at the member's spawn. A
box where the namespace cannot be entered prints a skip naming why and has no watch.

**The territory is the member's own room and a load closes it rather than
opening it.** It is one subdirectory of the operator-side directory the sink
already stands in, made if absent and repaired if present, `0700` and owned by
the member's account. **The repair is unconditional and that is the half
issue #545 found second**: the preparation ran on every load and rewrote the room to
this crate's uid, the parent's group, and `0750`, so a member-owned room did not
survive one load and the ownership could not be held by provisioning alone. The
agent's uid is walled out twice over and neither wall rests on the other, the
containing directory denying it the search bit, which section 4 verified before
this runs, and this directory granting it nothing through owner, group, or
other.

```graph
node: admin-member-territory-is-the-members-own
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-member-territory-is-the-members-own
```

**Its vector is the territory and, under a diagnostic binding, the preload socket path,
with the first door's end inherited beside it rather than named on it.** Per the
operator's ruling of 2026-08-26 the first door is a socketpair this crate creates at the
spawn: the member's end crosses by inheritance, its number the code act's one remaining
election, and the harness's end crosses inside the enter directive, so no socket path
for that door rides the vector and no name exists to ride it. The agent's uid retires
from the vector with both of its uses, the first door judging no credential under
possession and the preload door admitting the operator principal and refusing the rest
without knowing the agent by number. The territory is what the member needs to open the
embedded store, and **the vector carries no flag**, per `weaver-state-Spec` section 2:
the territory first and the preload path behind it, the `--engine` flag and the service
engine's three having retired with that engine on the operator's ruling of 2026-10-02
on #1, so one engine stands and the member needs no word to choose it. **The preload path
is present where the resolved kind is diagnostic and nowhere else**, per
`weaver-agent-PRD` section 6 as ruled 2026-08-24, the serving load that elected a record
restore having retired on 2026-10-02 (#58), and its absence is a serving load rather
than a defect: the member binds the preload name only where this vector carries one, so
a serving binding stands no named door by the value not being there. The kind is section
4's inventory's, resolved once and read here, which is the same single-resolution rule
the enter payload's `EnterBinding` follows in section 7, so the verb and the load cannot
resolve differently because only one site resolves.

**The instrument is perturbation, and the claim is this site's half of a two-sided
one.** **What is watched is the vector this crate composes, in both directions, and each
names its removal.** A serving inventory puts one value on it, watched to fail when the
arm that appends the preload name is made unconditional and such a load carries two. A
diagnostic inventory puts two, watched to fail when that same arm is removed and a
diagnostic load carries one. **Two watches rather than one, because one would not fail
on both directions**, and the pair is what apex section 11 asks of a perturbation
record.

**Both directions are this crate's alone and the member's record covers
neither**, which is why the second watch is written rather than left to the
pair. The member's claim is about what it does with what it is given: given a
name it binds one, given none it binds none. So a serving load wrongly carrying
the preload value ends with the member binding it, which is the member's record
holding rather than failing, and a diagnostic load wrongly carrying none ends
with the member binding nothing, which is the same record holding again. **A
split claim covers the two crates' behaviours and not the seam between them**,
and the vector is that seam.

A door standing is the member's observable and is deliberately not the watch
here: a test that read it would break on a member-side regression and be masked
by a member-side fix, which is the one-test-for-two-crates shape the split
exists to prevent. The other half is the member's:
`state-preload-door-stands-only-diagnostic`, asserted in `weaver-state-Spec`
section 4, holds that the member binds no name it is not given. Neither record
stands for the other's behaviour, which is why both exist.

```graph
node: admin-preload-name-follows-the-kind
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-preload-name-follows-the-kind
```

**A save point is restored through a descriptor and published at the verb, never
reached by path**, on the operator's rulings of 2026-10-02 on issues #1 and #58,
exactly as section 5 hands the trace sink down and for the same reasons. The member's
state lives in its memory and crosses loads by save points: the member writes each into
its own room, the territory's `state/`, a new file per save point and never one
rewritten, per `weaver-state-PRD` section 4. **At the load, this crate restores**: as
root it opens the chosen save point in the territory's `save-points/` of section 9, the
one `restore` names or the latest published
where none is named, close-on-exec in the opening call, and places it at its fixed
number in the member's process at the spawn alone, the way it places the first door's
end, per `weaver-state-Spec` section 2. No path rides the vector, and the agent, whose
worker never holds the descriptor, never reaches the save point. No save point in the
directory is an absent descriptor and no refusal, the member standing empty and a
rebuild from the trace being the offline builder's, per `weaver-state-Spec` section 3.
**At every load, every unload and every `save-point`, this crate publishes, under the
lock**, on the operator's rulings of 2026-10-06 on #1 (the A3.0 items) (items 1, 3 and 7): as root, at the opening of a load's validate
step under the run lock it took first, after the member has stopped at an unload, and at
once at the `save-point` verb under the invocation lock with the run standing, it copies
each finished save point from the member's room into the territory's `save-points/`
and never moves one, because a move keeps the member's uid and the inventory's entry
judgment would refuse the file as wrongly owned. **Every source is a value the member's
account chose, so it is judged before root reads it**, per section 9: this crate holds
the member's room open as a directory, opens each entry beneath that descriptor without
following links, requires a regular file owned by the member's uid that carries the
save-point format and a finished name, a part name being an unacknowledged save point
that is never published, refuses and leaves in place any entry that fails, naming it,
and copies only from the descriptor it judged, so a link or a planted file in the room
cannot make a root step read a path the member chose. **The room is opened through the
territory's descriptor without following a link and listed through its own**, and each
entry opened without blocking, so a FIFO the member makes under a finished name is
refused as no regular file rather than holding the verb (the custody audit's G1 and
G12). **The scan reads only the stamp line** (Codex on #94, round 30): each entry is
judged by its owner, its size against the save point's bound and its first line, read
to at most 4 KiB, and the whole file is read only at its own copy, where it is opened
and judged again in full and its digest checked against its name, the bytes copied
being the bytes judged in that read. So the scan's reading is bounded by the room's
entry count and not by its aggregate size, a whole image is read only for a file the
cap admits, at most one image stands in this root process's memory, and a file the
member changes after the scan is refused. **One verb publishes at most 32 room files, and copies one
only where its size and 64 MiB more stand free** on the save-points filesystem, this
act's elections (the custody audit's G23); entries go oldest first and the reported one
last, and the cap, the space, a judgment at the copy that fails, a copy that fails and a
standing line whose target differs each stop the verb at the first entry declined rather than skipping it (the #94 survey's
S3), answering a deferral, so every file published is older than every file left for
the next verb and the highest ordinal stays the last taken. A room file that does not
open, stat or read at the scan refuses the verb `BoundaryUnverified` naming it, left out
of the order it would be published later above newer files; one that reads and is no
save point is left in place and named, as it can never publish. **What may be newer
state refuses, never passed over** (the #99 area 1 review): a regular file under a
finished name that another uid owns, the room being the member's own, and a stamp line
naming a save-point format this crate does not read, each refuse the verb
`BoundaryUnverified` naming the file. A finished name is 64 lowercase hex characters and
the suffix, the one form the member writes, so an uppercase name is no finished name
and is ignored, never deferred. **A `save-point` whose
save point does not publish refuses `SavePointNotTaken` naming the publication** (the
#94 survey's S12), as the unload names it, the run still open. **The publication reads
no declaration** (the #94 survey's S8): the member's uid is the account database's by its
derived name and the room is the territory's, so an unload or a `save-point` publishes
whatever the declaration says after the run began, and an agent electing no member has
no room. A leave's own save point so deferred leaves
its unload refusing `SavePointNotTaken` naming the publication, as one that did not
publish does, until later verbs drain the room. **A load whose publication deferred
any entry refuses `BoundaryUnverified`** (Codex on #94, round 30) after publishing what
it published and before it selects, naming the room in `admin.log`: selection would
otherwise restore a save point older than the room's newest, and a later save point
taken from that stale run would carry a higher ordinal than the holdings left in the
room. Each refused load publishes the next oldest entries, so repeated loads drain the
room and the load that finds it drained selects. The copy is written under a
temporary name, root's, grouped to the access group and mode `0640`, the mode set
through the descriptor so the invoking shell's umask narrows nothing, so the operator
and the connector read it and nothing writes it but this crate, its owner and mode
verified through the open file, then renamed to its published name, overwriting
nothing, every step against `save-points/` as opened at section 9's judgment. **The published name is computable from the file's own bytes**:
`<YYYYMMDDTHHMMSSZ>-<digest>.save-point`, the time the stamp line's `taken.wall_ns`
rendered in UTC to the second and the digest the file's, which extends the room's name
rule of `weaver-state-Spec` section 3 to the published form, so a renamed file reads as
not the file its name claims. Only then is the member's copy removed, so a publication
cut short leaves the member's copy standing and is retried at the next verb, and one an
unclean stop left behind is published at the next load. **A line already standing for
the room's copy is judged before the copy goes** (Codex on #94, round 9): the copy is
removed only where the line's target stands and digests to the line; a target gone is
recreated beneath the standing line, the line being the record and no second one
appended; a target of other bytes is left with the copy, then the last sound copy, named
in the log, and stops the verb as a deferral (Codex on #94 at 7a25db2), there being no
skip: the copy may be the newest sound state, so a load refuses until the target is
repaired or cleared. **Several entries published by
one verb are ordered recovered first, then reported, and by the member process's own
count within a kind**, the stamp's `taken.ordinal`, which strictly increases within one
member process, before any ordinal is minted (the operator's ruling of 2026-10-08 on
#99): a reported save point was taken last by construction and is minted last. Neither
the clock nor the covered position orders anything, `taken.wall_ns` naming a file and
never ranking it: the clock can step back between two stranded save points, and the
covered position's run is the run of the last landed event, which changes within one
agent run after a restore once its first distillate lands. **Save points of more than
one member process refuse** `BoundaryUnverified`, naming them, and nothing is published,
as do two of one process carrying one count (a reused process id): the room holds one
member process's files, since a load publishes it whole or refuses before a member
stands, so a second process is a room nothing orders. The operator clears the room, and
until then a load refuses as on any deferral. So the latest the manifest names is the
last taken and never a recovered older file the listing happened to yield later. **A
save point the `restore` verb names that no line names enters with the next, highest
ordinal**, as section 4 says, so every later load continues from it until a newer save
point is published: that is intended, a restore meaning "continue from here" (the
operator's ruling of 2026-10-08 on #99). **Each publication appends one
line to the manifest**, `save-points.manifest` in the territory's `save-points/`, a file
this crate creates root-owned and mode `0644` and opens for reading and appending, never
rewriting a line, a torn tail alone truncated as below: one JSON object per line carrying `ordinal`, a monotonic integer minted under the
lock as one past the highest line standing; `digest`; `name`, the published name;
`stamp`, the run, sequence, turn and schema digest the stamp line carries; `taken`, its
wall clock; `position`, the trace position of the `save_point` event that named it, the
event's own run and sequence as the harness reported them in its `Left` or
`SavePointTaken` answer beside the position the save point covers, the two runs
differing after a restore until a distillate lands, or
absent where the file was recovered from the room after an unclean stop and no answer
carried it; and `arrived`, one of `leave`, `demand`, `recovered`, or `restore` for a
line the `restore` verb appended for a file it judged. **The manifest is this crate's own file and is judged on its descriptor before a byte
is read or written**: opened `O_NOFOLLOW` and close-on-exec, read-only or append-only,
and judged by `fstat` as a regular file, uid 0, gid 0, mode `0644` exactly, link count
one; anything else refuses `BoundaryUnverified` naming what was found, so a manifest the
operator pre-created or replaced refuses by uid, a second link refuses by count, and a
link never opens. It is created only where no entry stands, exclusively as root at mode
`0644`, and the directory is synced so the entry is durable before its first line. The
directory is root's, read by the access group and written by no one else: the operator
removes a manifest only as root, which fails closed per section 9, and cannot replace
or write one, which refuses. **What is written is durable before what depends on it is written**: the
published copy, root's, is synced after its ownership is set and the directory after the
rename, before the manifest line names the entry; the line is synced before the room's
copy goes; a line a failed write left torn is rolled back to the length the file had,
and a torn last line a power loss left is dropped at the read, named, and truncated by
the next append, so the publication it was for is retried through the adoption of its
target rather than blocking every load; and the marker is synced as a temporary, root's
and mode `0644` set through the descriptor whatever the umask, renamed, and its root
synced, so it survives the loss of power it exists to record.
**Every other file this crate touches at a publication goes the same way**: the room is listed through its own
descriptor for names alone and every entry opened and removed through it; the published
copy is created exclusively under a temporary name, its owner and mode verified on the
open file, and renamed with `RENAME_NOREPLACE`, so an entry anyone put under the
published name refuses the rename rather than being replaced or followed; the marker
stands in the root this crate owns. **A save point past one gibibyte
is not read**, a bound both readers enforce on the operator's ruling of 2026-10-08 on #1,
the member never writing one: in the room one refuses the publication
`BoundaryUnverified` naming it until it is cleared, in the directory it refuses the
selection as a fault and a restore naming it refuses, so no file a member or
an operator wrote is read whole into this crate's memory past that bound. **The
manifest is the record of what is loadable, and the latest is read from it**: the
highest ordinal, whose file must stand under its name and digest to the line's digest or
the load refuses, per section 4, and a file no line names is not loadable. The directory is root's, read by the access group,
so a save point on demand is readable by the operator at once; a save point reaches the
member's holdings only at a load, through the descriptor this crate hands it, the live
`restore` ask being retired (the operator's ruling of 2026-10-08 on #99). **The verbs
`save-point`, `restore` and `force-unload` are the operator contract's**, per section 2,
and the publication itself carries nothing across it.

**The one name left stands under the member's territory, and it is derived
rather than told.** Per the operator's ruling of 2026-08-26 the preload name
is the territory with a fixed leaf, so no value the invocation's input
composes reaches it and the worker's identity cannot reach the directory it
stands in at all. The first door has no name to derive, which retires the
shared-derivation clause that stood here and the G5 authority it named: the
harness is handed its end in the enter directive and derives nothing. What
`weaver-admin-harness-contract`'s standing ground relies on is unchanged,
this crate holding no channel to the member on either end it couriers.

**No exchange carries the name**, which is why it rides the vector: the
member holds no channel on which a path could arrive. `weaver-analysis` learns
the preload name from the operator rather than from this crate, having no seam
with it, which is the same route the operator's own tooling learns any path by.

**The name's directory is the member's own territory, which this crate
prepares before the spawn**, per the standing clause above, so the bind races
nothing into being. This crate binds nothing itself, the member binding the
preload name under its own account, which reaches that directory because that
account owns it, as of 2026-09-15. The door's gate is its credential
judgment, per `weaver-analysis-state-contract`, and the directory's wall is
what keeps the worker's identity from ever reaching the name.

**The two costs the shared directory carried are settled by the ruling, named
so a reader does not hunt for them.** The lifetime coupling dies with the
location, the member's territory outliving the worker, so no cleanup of the
worker's takes a name whose peer is the operator's. The squat defect
closes the same way: the name stands where the agent's uid cannot write, so a
name replaced before the operator dials it is unrepresentable rather than
defended. The act that closed it is this ruling's and not the account act the
earlier form of this clause predicted, which is why the name did not wait on
the account and the account, landing 2026-09-15, changed nothing here.

**Nothing is waited on after the start, because there is no name to watch.**
The ruling retires the pathname wait with the pathname: the pair exists
before the member does, so a load cannot race a bind that no longer happens,
and a member that dies before serving is discovered by the enter's own first
traffic on a closed pair, per `weaver-harness-state-contract`'s dead-peer
clause, the leg not standing and never a refused load. **The preload door is
not waited on at all**, its far end being the operator's to dial whenever the
operator dials it.

**The stale preload name is the member's own to clear, and this crate removes nothing.**
The territory is the member's, the member unlinks before binding, which keeps its bind
clean, and this crate holds no reason to touch a directory whose names it no longer
watches, the pathname wait having retired. What made an unconditional removal safe still
holds and now guards the member's own unlink: a second invocation refuses
`InvocationInFlight` at the invocation lock of section 3, a load meeting a live worker
refuses `AgentRunning` at the run lock, which the member itself holds while it lives,
and the member is started only holding the invocation lock and the run lock this load
took, so a name found standing belongs to a run that has ended, the one-member rule of
`weaver-state-PRD` section 4 held by the guard rather than by a check at the name. A
load killed after the member is forked leaves the member holding the run lock, so the
next load refuses `AgentRunning` until an `unload` has ended the member.

**The member retires itself and pid 1 reaps it.** `weaver-state-PRD`
section 4 has the process retiring with each unload while its holdings stand for
the next, and the mechanism is the first door's closure: the worker's channel
closes at unload and the member's serve loop ends with it. A load that fails
before the enter delivers the harness's end leaves that end with this crate,
whose invocation exits, closing it, and a load that fails after the delivery
ends the same way, section 3's rollback ending the worker and its exit
closing the end the worker took: the member reads either closure as the
first door's end and retires, so an abandoned member is a bounded cost
rather than a resident one, on every path alike.

**This crate observes the child and does not reap it, and the distinction is
load-bearing.** The wait above reads the child's exit only to end early, so a
member that dies before binding does not cost the load the full bound. **Reaping
is pid 1's by reparenting**: this crate is one invocation per verb,
per section 2, and exits when the verb answers, so the member outlives it and is
inherited by pid 1. That is the arrangement the residency needs rather than an
accident of it, a member reaped by its starter being a member that dies with the
verb that started it.

**So this crate takes no stop obligation** and section 3's rollback gains none,
on every exit path: what would be reaped retires on its own and is collected
where every orphan is.

**The trace relay holds the trace door, and the agent never does**, on the operator's
ruling of 2026-10-03 on #50. It is a small process the start step launches beside the
worker, under the relay account `weaver-<agent>-relay`, whose one group is the trace
group `weaver-<agent>-trace`, and never under the agent's account or the member's. **The
start step binds its socket and hands it over**: as root, having first removed a
`trace.sock` a previous run's relay left, which a relay cannot unlink from the root's
directory and whose name outlives its listener, and doing so holding the invocation lock
and the run lock it took, as it clears the worker's names, it binds `trace.sock` in the
agent's run directory of section 3, `<coordination-root>/weaver.run/<agent>/`, root's
and apart from the agent's runtime directory, the socket root-owned, mode `0660` and
grouped to the agent's per-agent access group `weaver-<agent>-admin`, which the declared
trace reader must hold, a stream socket (`SOCK_STREAM`) because the door carries one
request in and a continuing byte stream out, framed as `weaver-types-Spec` section 3.1
states, and passes the listening descriptor to the relay at its exec, so the relay never
binds and never needs to write the directory. The relay also inherits the run lock's
description, per section 3, and marks it close-on-exec as its first act and never closes
it, as the worker and the member do. **The door stands only for a file sink.** A pipe's
reader is the operator's, and a second reader would steal its bytes, and a socket sink
cannot be opened for reading at all, so where the declaration's sink is a pipe or a
socket the start step starts no relay, binds no `trace.sock`, and the door stays closed.
For a file sink it passes the relay a read-only descriptor of the same open file section
5 opened for the worker, reopened through `/proc/self/fd/N` of the write descriptor and
never by the declaration's path, and confirmed by comparing the two descriptors' device
and inode before it is passed, so **the relay serves the loaded run's own file by
descriptor and records its identity**, and an operator's edit to the declaration's sink
while the run stands changes nothing the relay reads (the item carried on #50,
issuecomment-5969507004). **It admits exactly one reader**, the `trace-reader` of the
agent's `roles.toml`, judged by the kernel's peer credential before a byte of the
request is read, and refuses and logs every other caller that reaches it. **Only a
member of the socket's group reaches it**: a process outside `weaver-<agent>-admin`, the
agent's own among them, is refused by the socket's mode at `connect` and never reaches
the peer check, so the relay logs refusals of group members that are not the reader, and
the kernel's refusal of everyone else leaves no record of this crate's. **The newest
connection from that reader replaces the old**, the relay holding one follower at a time
inside its one process, so the replaced follower's connection is closed, the reason
logged since no stream line carries one, and never more than one follower stands (the
item carried on #50, issuecomment-5969438713). The stream is `weaver-types-Spec` section
3.1's: a `TraceHeader` line, then the file's own lines from the verified position
exactly as written, then following with a heartbeat while idle. **The relay parses no
event**: it hashes one record's bytes to verify a position, a bounded number of blocks
per pass of its loop, so a record of any length costs it one block of memory and never
holds the loop away from the lifetime pipe or the follower, and copies bytes, whole
lines where it can, so no line of its own lands inside a record, and a truncation met
inside a line longer than one chunk closes the connection without its `truncated` line
for the same reason. Otherwise the `truncated` line, once queued, is written whole
before the connection closes, the follower kept while the reader takes it within the
write wait, so a full send buffer never cuts the stream's last line. **It copies at most
one chunk per pass of its loop**, so however large the backlog and however slow the
reader, the loop returns to the lifetime pipe, the door and the reader's hangup every
pass, and the relay never outlives its worker behind a reader. **A request is read as it
arrives, inside the same loop**, so a reader withholding its newline holds neither the
lifetime pipe nor the standing follower for the request's five seconds. A heartbeat
follows five seconds idle, and a queued write the reader has not taken whole within five
seconds of its queuing drops the reader, logged. Its operations, connects, replacements,
refusals and disconnects, go to the operations log through a descriptor the start step
passes, never a line per streamed record. **Its descriptors stand at fixed numbers**:
the listener at 3, the read-only sink at 4, the operations log at 5, the lifetime pipe's
read end at 6 and the run lock's description at 9, and its vector is the declared
reader's uid, the agent's name and the boundary file's digest, the last two for its log
lines.

**The trace stream across runs is stated, because a reader depends on it.** **A run's
sink is not a new file**: the declaration names one sink per agent, which section 5
opens for appending at every load, so every run of the agent appends to the same file
and no per-run naming is introduced. **A run's records open with its `load` event**, the
first line the harness authors for a serving run, so a reader finds each run's start in
the file it is already reading. **A stream resumes across runs within that file**: the
relay runs only while the agent runs, so between runs nothing streams and the door is
closed, and a reader that held a position at the last run's end resumes at the next load
from the same offset and prior digest, the file having only grown. Where the operator
replaces or truncates the file between runs, the relay's header names a different file
or the position fails to verify, and the reader starts again from offset zero.

**The relay's lifetime is bound to the worker's by a pipe, and the failure mode is
stated.** The start step makes one pipe before it forks either process, and closes its
own copies of both ends once both children hold theirs. The worker inherits the write
end across its exec at descriptor 8 and marks it close-on-exec as its first act, before
it starts any thread or forks anything, per `weaver-harness-Spec` section 2.2, so no
organ ever holds it and no window exists in which one could. The relay holds the read
end. The worker never writes it, so the relay's read blocks for the run's life and
returns end-of-file only when the last holder of the write end is gone, which is the
worker's death, clean or not, the kernel closing the descriptor whatever killed the
process. The relay exits on that end-of-file, closing its follower's connection and
logging why, since no stream line carries a reason. **The one remaining failure mode
runs the other way**: if the relay dies first, the worker serves on, the trace keeps
landing in the sink, and the door is closed until the next load, which the reader sees
as a refused connection, and which nothing records, a dead relay writing no line. **The
relay's tests watch the pipe's end ending the relay**, behind a slow reader too. The
start step closing its own copy once the worker holds the write end, which an organ's
copy would mask since an organ dies with the worker by its death signal, is held at
review here, the load path needing a provisioned box, and watched by the deploy act's
end-to-end run, where the relay must exit with the unload. Neither process waits on the
other, and the start step's detach leaves both reparented to pid 1.

**Worker death is observed, not reported.** The channel's closure is the observation,
per `weaver-organ-channel` section 2, and the run lock's release is the fact `show` and
`unload` read, per section 3. Admin repairs nothing on either, per the charter.

## 7. The coordination channel

The channel of charter section 6, dialed fresh per verb.

**The socket type is `SOCK_SEQPACKET`, carrying the election of
`weaver-types-Spec` section 4 rather than re-deciding it.** That Spec named
this document a landing site for the boundary-preserving election, and it
lands here on the connection this crate dials: one write is one message, one
message is one JSON envelope, and no framing enters any contract that draws
the channel. A landing site carries an election and does not declare it, so
the election and its envelope bound are both asserted at that Spec's section
4.3 and neither takes a record here. The socket the harness binds carries the
same type, per `weaver-harness-Spec` section 2.3, because a connect against a
listener of another type fails at the kernel and the two sides elect one thing.

**The boundary the type buys is tested where the connection is made, and that
test is this crate's.** The election is that Spec's record and the conduct at
it is this crate's, the division the receive discipline below already takes:
`weaver-types-Spec` section 5 owes the connecting and binding crates a test
with two halves, and the boundary half is that one envelope written on this
channel is one envelope read, arriving neither split nor merged with its
neighbour. Section 10 names it with the substitution its watch turns on,
`SOCK_STREAM` at the creating call leaving every truncation test passing while
the framing every contract that draws this channel rests on is gone.

```graph
node: admin-one-write-is-one-read
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-one-write-is-one-read

edge: grounds
from: admin-one-write-is-one-read
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The channel is reached in one act, the dial, and the retry bound is the
whole of its subtlety.** The four acts this section carried until 2026-08-05
were admin's bind, listen, accept, and close, and the inversion moved every one
of them to the harness: the socket lives inside the agent's sandbox and the
harness binds it as its first act, per `weaver-admin-harness-contract` section
2. What admin does is connect, per verb, to the per-agent name the operator's
configuration places, with the close-on-exec flag asked for in the socket call
itself and the connection closed when the verb answers.

**The dial retries within a bound because the bind is the worker's first act
and the load's dial may arrive first.** The load's start step starts the worker and then
dials, so the race is real and structural rather than incidental: the elected
bound is one second of attempts at ten millisecond intervals, and a bound
exceeded refuses the load with `NoResidency` rather than waiting without end.
The numbers are this Spec's election and the charter states only that a bound
exists, per its section 4.1 step 6. A retry loop with no ceiling is what this
election exists to refuse, because a worker that never binds would otherwise
hang the operator's terminal rather than answering.

**The connect is nonblocking, and this is a requirement of the bound rather
than a preference.** Measured 2026-08-06: with the listener's backlog full, a
blocking `connect` on an `AF_UNIX` socket was still blocked after three seconds
against a one second ceiling, while the same connect on a nonblocking socket
returned at once with the transient error a retry is for. **A full backlog is
reachable rather than theoretical**, because the harness serves one connection
at a time, so a second verb arriving while one is in flight meets exactly that.
A blocking connect would therefore leave the bound stated here and unheld,
which is the failure the election exists to prevent, reached by a different
road. The flag is cleared once the connection is made, because the enter
directive and the answer it waits for are blocking work and a nonblocking read
would report an empty channel as a fault rather than waiting. Blocking is not
unbounded: every answer read after the connect is read under its exchange's bound of
section 3, the enter's, the leave's, the stop's or the observation's, so clearing the
flag never leaves a verb waiting without end.

```graph
node: admin-dial-retries-within-a-bound
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-dial-retries-within-a-bound

edge: grounds
from: admin-dial-retries-within-a-bound
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The credential check is the harness's and takes no record here.** What
refuses a stranger on this channel is the peer credential read at the
harness's accept, root or refused, per the contract's section 2, and the
record for it is declared by the crate that performs it, per
`weaver-harness-Spec` section 2.3. The four records this section carried for
the earlier design, the bind ordering, the directory's mode, the credential
check, and the listener's closure after one accept, retire with the acts they
described. The closure is not merely relocated: a listener that answers one
verb and closes would leave every later verb with nothing to dial, so the
harness's listener lives as long as the worker and the property that replaced
the closure is the check itself.

**The receive discipline is the shared obligation.** The receive buffer is
sized to the 64 kibibyte envelope bound and a read returning with
`MSG_TRUNC` set is a channel fault and never a message, per the election's
own terms, and the same bound is asserted on this crate's sends. The bound
is the floor's number and the discipline is this crate's conduct at it,
which `weaver-types-Spec` section 5 owes to the pair-creating crates by
name, so the record for it lands here and not there.

```graph
node: admin-truncation-is-a-channel-fault
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-truncation-is-a-channel-fault

edge: grounds
from: admin-truncation-is-a-channel-fault
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The enter directive and its ancillary payload are one message.** The
envelope is rendered to JSON and sent with the sink's descriptor, and the
state channel's end where the member stands, as `SCM_RIGHTS` control data on
the same `sendmsg`, which is what makes each descriptor cross once, in the
enter exchange, with no separate delivery to order against anything, the
receiver telling the two apart by position per the contract's supply order.
The exchange identity is the floor's `ExchangeId { opener: Admin, ordinal }`,
ordinals assigned serially by this crate, per `weaver-organ-channel`
section 1.

**The kind is resolved at the inventory and the payload carries it decided.** The
declaration's `binding_kind` is an option whose absence means serving, per
`weaver-types-Spec` section 2, and the one inventory function of section 4 resolves it,
so the verb and the load cannot resolve differently. The construction follows the
resolved kind: a serving enter takes the declaration's gate instruction into
`EnterBinding`'s serving case, and a diagnostic enter takes its absence. The cross-field
rule lands here because only this crate sees the file whole: a serving declaration
omitting the gate instruction and a diagnostic declaration carrying one are both refused
at inventory as `ConfigInvalid` naming `gate-instruction`, before the start step runs,
which is the taxonomy of `weaver-types-PRD` section 2.1 run over one more field. **The
rule reaches one field and not two.** An act earlier on 2026-08-24 extended it to
`trace-sink`, which the same day's ruling undid by making every binding declare a sink,
so that field is required under either kind and no cross-field question arises for it.
Past the inventory no disagreement exists to carry, the payload's shape holding what was
resolved, per `weaver-types-Spec` section 4.

```graph
node: admin-kind-mismatch-refused-at-inventory
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-kind-mismatch-refused-at-inventory
```

**The permission members are set here and never read from the file.** The
construction writes `refeed_permission` from the resolved kind, granted
under a diagnostic enter and cleared under a serving one, and its sibling
`column_permission` takes the same rule when its act lands. The inventory
refuses a declaration that grants either, `ConfigInvalid` naming the field,
because the member parses with `default` - one type serving the declaration
and the seam, per `weaver-types-Spec` section 2 as corrected 2026-08-31 -
so an operator cannot grant what only the resolved kind derives, and a
written `false` is overwritten indistinguishably from absence, the bound
that parse buys. Per `weaver-spu-PRD` section 13.14. Perturbation: remove
the permission check from the inventory and the granting declaration
loads. Watched under exactly that removal.

```graph
node: admin-granted-permission-refused-at-inventory
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-granted-permission-refused-at-inventory
```

**The run's identity is minted here and the session's is read.** They are two
different kinds of value and the distinction is worth holding, because an
earlier form of this crate derived one from the other and produced a record
whose runs were indistinguishable. The session is the operator's, declared in
the agent's config and carried uninterpreted, per `weaver-types-Spec` section
2 and the ruling in `weaver-admin-PRD` section 10. The run reference is this
crate's, minted at the load as an RFC 3339 timestamp in UTC at millisecond
resolution, the validated agent name, and eight bytes read from the operating
system's randomness rendered as hexadecimal,
`2026-08-14T16:02:11.482Z-alpha-9f3a1c7d4e2b8a01`,
which reads as a date to whoever opens the artifact. **Nothing is
remembered between invocations to produce it**, which is what makes it
answerable by a crate that holds nothing across time.

**The random part carries the guarantee and the other two carry the reading.**
The instant and the name are what make a reference legible, and a builder may
reason about neither when asking whether two references can be equal: the
clock is adjustable and the name is shared by every run of one agent. The
eight bytes are what answer the contract, and they are read from the operating
system at each load rather than derived from anything this crate holds, which
is what keeps the answer available to a crate that holds nothing across time.
The name is the validated one of section 4, so it introduces no second
authority. **A builder who dropped the random part and leaned on the clock's
resolution would be trading a guarantee for a probability**, and the case it
would lose is the one hardest to see: an adjustment moving the clock backwards
across an instant a previous run already took.

**Sorting references gives calendar order and not a monotonic sequence.** The
instant is a wall-clock reading and an adjustment can move it backwards. A
consumer needing strict order uses the sequence, which is gapless and scoped to
the run, per `weaver-harness-trace-contract` section 6.

**The exchange ordinal beside it is a different thing and stays a counter.**
That ordinal is serial within one connection and is the floor's, per
`weaver-organ-channel` section 1, and a per-invocation crate can hold it
because a connection does not outlive the invocation. A reader who sees both
in one envelope is seeing a counter scoped to a connection and a reference
scoped to a session, and conflating them is how the earlier defect was
written.

```graph
node: admin-run-reference-distinguishes
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-run-reference-distinguishes
```

```graph
node: admin-enter-carries-descriptor-in-one-message
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-enter-carries-descriptor-in-one-message
```

**One exchange in flight per worker, and the serialization is now the
harness's.** The channel's layer permits concurrency, per the drawn material,
and the contract forbids a second transition for the same agent. What held
that was admin's map across agents until 2026-08-05, and a per-invocation crate holds
nothing, so the property lands where the standing party is: the harness serves
one connection at a time and answers a directive arriving out of order with a
refusal, per the contract's section 4 and `weaver-harness-Spec` section 2.3.
This crate's obligation reduces to opening one exchange per invocation and
closing the connection when the verb answers, which one verb per process makes
structural rather than disciplined.

## 8. The operations log

**The format is NDJSON, one act per line, and it shares no schema with the trace.** **It
is one per agent, at `admin.log` in the agent's territory**, beside `agent.toml`, the
prompt draft and `save-points/`, on the operator's ruling of 2026-10-03 on #63's sixth
question, carried forward on #50, which moves it from the root's `log-path` of #45, and
on the operator's ruling of 2026-10-07 on #1, which moves it with the declaration from
the operator's home into the territory. The agent's uids never reach it: the agent's own uid
cannot pass the territory, and the member, which passes it by the state group, does not
hold the access group (section 9). The operator and the connector read it through the
access group: it is root's, grouped to the access group, mode `0640`,
set through the open descriptor. This crate appends to it as root, opening
it with `O_NOFOLLOW` so a link planted at the name is refused rather than followed, and
non-blocking and judged a regular file before a line is written, so a FIFO planted at
the name neither holds the verb nor takes a line. `worker.log` is opened and owned the
same way, and the member's `state.log` in its own room is opened without following a
link and judged a regular file, and set to root and the member's group, `0640`, so the
member reads what it wrote. **The worker's own output is not
this log**: the start step points the worker's standard output and error at `worker.log`
beside it, per section 6, because the worker holds that descriptor and an agent holding
a writable handle to the boundary's record could write into it. What this Spec adds is
the rendering: one JSON object per line, the same reading tools the stream's consumers
already hold, and a field set that is this crate's own and deliberately not the event
envelope, because a shared schema is how a second author drifts into the first's record.
The file opens with `O_APPEND` and `O_CLOEXEC` like every descriptor this crate holds.

```graph
node: admin-log-ndjson-own-schema
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-log-ndjson-own-schema
```

**`admin.log` is the boundary's record, and the trace is the constitution's and the
agent's own**, on the same ruling. A verb that changes the agent, a load, an unload or a
stop that closes a turn, goes on the trace with its cause, the uid sudo reports, through
the harness, per section 2. Boundary activity that changes nothing in the agent goes
here: a refused verb, a read-only verb, `show`, a `stop` answered at rest, and the trace
relay's connects, replacements, refusals and disconnects, never a line per streamed
record. Each such line carries the wall time, the command line, the caller's uid, what
was asked, the boundary file's digest in force, and the outcome. **The digest is absent
where no boundary file was read**, a missing or malformed `roles.toml` among them, and
the line says so rather than inventing a value. **A refusal before the agent's root is
admitted has no `admin.log` to reach**: a malformed name or `NoSuchAgent` names no agent
whose territory this crate may write, so that refusal goes to standard
error alone, beside the answer object, and to no log. **The relay's lines carry a schema
of their own**, on #73's second item: the wall time, `actor` naming the relay, the
agent, the event (`connect`, `replaced`, `refused`, `disconnect`, `truncated`, `ended`),
the peer's uid where a peer stood, the boundary digest and a detail, and none of an
invocation's fields, no command line and no sudo cause existing in a process that
outlived its load. **Every line of either writer is one `write` on a descriptor opened
for appending**, so an invocation's line and the relay's never interleave.

**What is logged is the charter's set.** Transitions directed and their outcomes,
refusals issued, rollbacks with what each act undid or could not, and workers started
and stopped, a load's `ready` line naming the agent's SPU key and path per section 9.
Never a fact about what an agent did, per charter section 2: the moment a line describes
conduct rather than supervision it is a second record of the agent, and the review that
finds one has found a defect. The instrument is named in that sentence and is the only
one available, no mechanism being able to tell a line about supervision from a line
about conduct.

**The line between supervision and conduct is a domain line, and that is what
this record grounds in.** What an agent did is a fact about the working the
harness is answerable for, and the trace is where that working is recorded. A
line describing conduct would make this crate a second author of an account it
sees only a part of, which is an organ reasoning about a domain that is not its
own, per apex section 5.5. What stays is supervision, which is this crate's own
domain: the transitions it directed, the refusals it issued, and the workers it
started and ended.

```graph
node: admin-log-never-records-agent-conduct
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-log-never-records-agent-conduct

edge: grounds
from: admin-log-never-records-agent-conduct
to: axiom-harness-integrates-by-the-loop
```

**Retention and rotation are deferred with a named settler.** The charter
declines to fix a format's lifecycle before a rollback has run, and this
Spec follows: the file grows until the operator rotates it by ordinary
means, and a rotation policy is elected when there is a measurement of what
accumulates, which is the charter's own grounds read forward.

## 9. The service's own configuration

**Admin has operator-installed configuration of its own, one root per agent, and this
Spec names it rather than leaving it implied.** Admin is one agent's organ, on the
operator's ruling of 2026-10-01, so an invocation reads the configuration of the agent
it names and nothing shared with any other agent. `WEAVER_ADMIN_CONFIG` names the base,
`/etc/weaver/admin` where it is unset, and the agent's root is `<base>/<agent>/`. Under
sudo the variable never arrives, sudo's environment reset stripping it and the rule
keeping nothing, so a delegated invocation always reads the default base, and a box that
installs elsewhere is one the sudo rule cannot drive until the rule names the binary's
own default. The name is judged before the path is built, per section 4, so `.`, `..`
and any name carrying `/` never reach the base. **The root existing is the admission**:
no root, or a root that is not a directory, answers `NoSuchAgent`, and the look does not
follow a link at the root itself. **The root must be root's and closed to every other
writer**, owned by uid 0 and neither group- nor world-writable, or the invocation
refuses `BoundaryUnverified` before a value is read, since what the root names runs
under the agent's identity and a root another principal could write would hand that
principal the agent. **Every entry of the root is held closed the same way**: a regular
file, never a link, owned by uid 0 and writable by no group or other, or the invocation
refuses `BoundaryUnverified`, and a stray entry, a subdirectory among them, refuses
every verb, `show` included, rather than being reported and passed over (#62), since the
values name programs this invocation runs as root and a directory's own mode stops
neither a writable file inside it nor a link leading out. **Every directory above the
root is held closed too**, as sshd's StrictModes judges a path. The root is resolved
once to its canonical path. Each directory from its parent up to `/` must be owned by
uid 0 and writable by no group or other, unless its sticky bit is set, which keeps
another principal from renaming an entry it does not own. Otherwise the invocation
refuses `BoundaryUnverified`, naming the directory on stderr. A directory another
principal could write would let it rename the judged root away and stand its own in its
place between the judgment and the reads. Every read then goes through that canonical
root, never a pathname re-resolved after the judgment. **The deploy scripts are bound by
the same judgments before they commit**: `create-agent.sh`, `bootstrap-stack.sh` and
`verify-load.sh` apply this rule (`held_closed`), and any judgment admin gains here is
one they must apply before they provision, publish or run a root, so that no script
leaves a root admin then refuses or runs a program admin would not. **The line those
judgments hold is another principal's choice.** The operator runs every script and holds
root, so a value the operator supplies (the environment, the arguments, the build) is
the operator's own choice and not an
escalation. A defect is a root step acting on a value another principal can choose: an
agent's account, its state member's account, any other local user, or a root-owned path
re-pointed through a directory such a principal can write. Every such value is judged
before the step, by this section's rule, by admin's own answer, or by reading it as the
principal that owns it. Every value below is read from the root before any verb, and a
value that fails to read fails the invocation as `ConfigInvalid` with no field.

**The root holds one file per key.** Required: `worker-binary`, `spu-binary`,
`gate-binary`, `coordination-root`, `territory`, `operator` and
`roles.toml`, `run-tool` and `control-tool` retiring with the init system they reached
on 2026-10-03 (#50), the boundary file of section 9 below. Optional: `headroom-bytes`,
`library-path`, per section 6, and `load-bound-seconds`, per section 2, the
`state-store-socket` key having retired with the service engine. **Every path a key names is
absolute**, `worker-binary`, `spu-binary`, `gate-binary`, `coordination-root`,
`territory` and `library-path` alike, and a relative
value fails the read naming the key, so no read resolves against the directory a caller
ran sudo from and two invocations of one root always name the same files. **The whole
agent lives in its territory, and the declaration with it**, on the operator's ruling
of 2026-10-07 on #1, which retires the operator's `~/.weaveragent/<agent>/` of the
2026-10-02 election: `territory` names it, absolute, `/var/lib/weaver-agent/weaver-<agent>/`
as the deploy scripts lay it out, root-owned, and it holds `agent.toml`, which this
crate parses per `weaver-types-Spec` section 2 and the operator edits with `sudoedit`;
the prompt draft `system-prompt.md`, which this crate never reads; `admin.log` and
`worker.log`, per section 8; `save-points/`, holding the published save points and
`save-points.manifest`, which this crate alone writes, per section 6; the trace, per
section 5; and the member's room `state/`, per section 6. One ownership: root reads and
writes root's files in root's directory, so no other principal's choice reaches what a
root step reads. **The access group reads and never writes**: `weaver-<agent>-admin`,
the connector's, groups the files beneath the territory, whose own group is the state
group, and reads the logs and the published save points, and the operator joins it to read without sudo; the state member is never in it; a declaration, a draft or a manifest the group could write
refuses, since the connector must not be able to rewrite the declaration. **Removing
the manifest is root's act**, and doing so makes every published save point
unloadable, a file no line names being not loadable, until a `restore` names one,
which appends the line that makes it loadable again; the recovery is the verb and never
a hand edit of a root-owned file. **Nothing replaces or writes one**: a manifest that
is not root's own, by uid, gid, mode, link count or kind, refuses at the inventory and
at every publication, per section 6, so a planted line never names a loadable file. A
territory holding no `agent.toml` answers `NoSuchAgent`, as a root holding none did. **`operator` names the operator's uid**, on
the operator's ruling of 2026-10-02 on this act's first question: a root-owned key
`create-agent.sh` writes once, the box's own fact, set by root, about whose data defines
the agent, and independent of who invokes a verb. The coordination name changed hands
with the operator socket on 2026-08-05: the operator places it, the harness binds it,
and admin dials it, so one value reaches two crates and the root is where they agree.
These values are not the agent config and no seam carries them, which is why the root
takes no contract of its own. **The file and its values part company at the start step,
and the distinction is worth holding.** This crate is the only one that reads the root.
Three of the values do not stay in it: the coordination socket's name and the two organ
binary paths reach the worker in section 6's argument vector, at the start step's exec
rather than over any seam. What is fixed here is that these values exist, that they are
the operator's to place, and that none of them is discovered at runtime by searching.

**Nothing in one agent's root is read for another.** The box-wide values, the binaries,
the tools, the coordination root and the headroom, are copied into each root by the
deployment scripts from a stack record of their own that this crate never reads, so an
agent's configuration is that agent's even where two roots carry the same values. The
installed program files may stand once on disk and be named by every root, while the
configuration and the processes are each agent's own. **The operations log is one per
agent**, at `admin.log` in the agent's territory, per section 8. Managing
several agents, listing them or holding a map across them, is not this crate's and
belongs to WeaverWeb or a separate application, which drives each agent through its own
admin.

**The organ binaries are in the root and not in the agent's declaration, and the
placement is the ruling rather than a convenience.** Which program runs under an agent's
identity is part of the authority this crate is delegated, so it is placed where the
admission is, in the root the operator holds, and never in a file the declaration's
author edits. The charter's own test settles it from the other side:
`weaver-harness-Spec` section 2 has the organ binaries supplied to the composition root
as a deployment fact, not a discovery, and the declaration is the agent's elections
rather than the deployment's. **`spu-binary` is this agent's SPU**, on the operator's
ruling of 2026-09-28 that `python-spu` is an option and not a takeover: one agent may
serve from the Rust SPU while another serves from `python-spu`, each root naming its
own. **The names the root's binaries carry differ pairwise**: the worker's, the state
member's, the gate's and the SPU's, and a root naming two of them under one file name
fails the invocation as an unreadable configuration fails it, before the start step
runs. They are held distinct rather than keyed around because the stack below is keyed
by file name and `weaver-analysis` carries it into a run's code identity by those names.
No new refusal names any of them, the configuration's failure having no lifecycle case
to be.

**The territory is judged before any value in it is read, on its descriptor**, for every
verb but a `force-unload`, which goes on without it (section 3), on the
operator's ruling of 2026-10-07 on #1, which retires the 2026-10-02 split of the
declaration into the operator's home: the keys that name the programs this crate starts
or hands the worker to start, `worker-binary`, `spu-binary` and `gate-binary`, stay in
the root, root-owned and judged as above, and the declaration stands in the territory,
root's as the root is, so there is no directory another principal could choose and no
file another principal could write among what a root step reads. The value of
`territory` must already be its canonical path, a value reached through a link above
it refusing `BoundaryUnverified`, since the directory is opened by the value and the
logs are made at the canonical path, and the look does not follow a link at the
directory itself. **The territory is held as a descriptor for the verb's life**:
opened once at the judgment with no link followed, judged on that descriptor a
directory owned by uid 0, mode `0710` exactly and grouped to the state group
`weaver-<agent>-state`, judged by name, on the operator's ruling of 2026-10-08 on #1,
which supersedes that day's earlier `0711`: passage by name and no listing for the state
group alone, which the member holds as its primary group and the operator and the
connector join, and nothing for the agent's own uid, which holds neither the state
group nor the access group, so section 4's denial of the sink's directory to the agent's
uid holds of the territory that holds the trace; and carrying no access-control entry
beyond its mode. **The wall is
each file's own mode**: the member's room is the member's `0700`, the trace is
`root:weaver-<agent>-trace 0640`, the logs and `save-points/` are the access group's
and closed to other, and `agent.toml` and `system-prompt.md` are
`root:weaver-<agent>-admin 0640`, read through the access group alone, which the member
does not hold (the custody audit's G11); `save-points/` beneath it is opened
through that descriptor and judged the same way at mode `0750` and the access group;
and every read and every write after goes through those descriptors and never
through the path again: `agent.toml` opened beneath the territory's, and section 6's
published copy made, renamed and synced, the manifest opened and appended, and a file
judged for a load or at a restore, each against `save-points/`. **The declaration's sink
stands in the territory** (Codex on #94, round 10): the trace and the member's room are
derived from the sink's directory, and the territory is the agent whole, so a
declaration whose sink's directory is not the judged territory refuses `ConfigInvalid`
naming `trace-sink`, never standing the trace or the room outside what the territory's
custody, group and archive cover. **The one entry this
crate reads is held closed**, and is read only by the verbs that need it, `validate`,
`load` and `restore` (the #94 survey's S8), never by the verbs that end or read a run, so
a declaration saved invalid while a run stands never strands it: `agent.toml` is opened
with no link followed and without blocking, and judged on its descriptor a regular file, owned by uid 0, grouped to the
access group and mode `0640` exactly, so the access group reads it and never rewrites it
and no other uid reads it; a declaration at any other owner, group or mode refuses
`BoundaryUnverified`, the provisioning being wrong rather than the declaration.
Other entries are not read, `system-prompt.md`, the operator's draft of the prompt that
the seeding step reads and this crate never opens, among
them, save `admin.log` and `worker.log`, the two this crate creates and appends to,
without following a link, per section 8. **Every directory above it is held closed as
the root's ancestors are**, each owned by uid 0 and writable by no group or other
unless its sticky bit is set. Any failure refuses `BoundaryUnverified`, naming the path
on stderr, before a value is read, and a territory that does not exist, is not a
directory or holds no `save-points/` refuses the same way, the operator's provisioning
being incomplete rather than the agent absent. The deploy scripts are bound by this
judgment as by the root's, per the line above. **The operator is one person in the
operator role**, on the operator's ruling of 2026-10-02 on #59, named in three places
that never disagree. On the box it is the uid the root's `operator` key names, the uid
the harness admits the seeding turn from and the one the connector's rule acts for.
Under #50 it is the person the connector acts for when it runs the agent's granted
command lines. In WeaverWeb it is that person's authenticated identity. WeaverWeb
authenticates the person and never chooses the box's operator uid, which is the box's
to name and never taken from a server ("the box decides", toddwbucy/WeaverTools#6).

**Admin reads the declaration as root and the agent never reads it.** What crosses to
the agent's processes is what the parse yields and the enter carries, never a path into
the territory's files and never a handle to them, the same discipline the sink's path
keeps. The ruling's reading of a system prompt is that one: the model receives the text
as its identity prefix at load, and that gives the agent process no read of the file
and no way to change it.

**An optional value is absent only where nothing stands at its path.** Every optional
value of this section, `headroom-bytes`, `library-path` and `load-bound-seconds`, reads
as absent only where the path itself names nothing, which is
asked of the link and never of its target. A dangling link, a directory, bytes that are
not UTF-8, or a read the kernel refuses is the operator's file failing to read, and
fails the invocation before any verb rather than standing a default the operator did not
choose. A required value fails either way, and its message says whether it was absent or
did not read. Every reader of these files outside this crate holds the same line, the
determinism matrix's among them.

**What the SPU binary is, this crate does not judge.** It passes the path the agent's
root names on section 6's vector, and the worker forks it at enter as
it forks any SPU, under the agent's identity, per section 6. That the file is an SPU
honoring `weaver-harness-spu-contract` and `weaver-harness-spu-decode-contract` is the
operator's placement to make true, and an implementation that does not is found at the
SPU's admission, where every SPU's failures are found.

**Which SPU served is recorded in the field that already records the stack, widened to
the binaries this crate hands the worker.** The enter carries the digests of the organ
binaries this crate starts, keyed by the binary's name, per
`weaver-admin-harness-contract` section 3, and the harness copies them into the load
event's `stack`, per `weaver-trace-Spec` section 3. Until this act that meant the worker
and the state member, the two this crate starts itself, and left out the SPU and the
gate, which the worker forks from the paths section 6 hands it. The set now takes those
two as well, digested from the same paths the vector carries, so each agent's record
names the SPU that served it by the file's name and sha256, and two agents served by two
implementations carry two different entries. The map is keyed by name already, so no
type changes, and a record written before this act lacks the two entries, which reads as
those facts being unrecorded. The load's `ready` line in section 8's log names the SPU's
path beside it.

**Two agents on one card is the SPU's question and is already answered.** Admission
judges each assigned device by one inequality, the shard's need plus the headroom
against what the device has free, per `weaver-spu-Spec` section 3, which says the device
is not the SPU's to arbitrate beyond it and that an occupant may be a second agent the
operator wants there. So a second agent's SPU, of either implementation, is admitted
where the card has room, and refused, never evicting, where it has not. Nothing here
changes that, and `python-spu` judges room by the same inequality, per `python-spu-Spec`
section 3.

**Two organ binaries are placed and one is derived, and the difference is the ruling
rather than an inconsistency.** The SPU's and the gate's paths are placed because every
agent needs them, so an installation that lacks either has no agents at all and should
say so at its own configuration rather than at a load. **The state member's binary is
found beside the worker's instead**, on the ruling of 2026-09-04, because the member
stands only where a declaration elects it, and **the classify arm's binary follows that
ruling for the same reason** as of 2026-09-07: admin joins `weaver-spu-classify` to the
directory holding the worker binary the operator placed. The classify arm is not chosen
per agent, its binary being a sibling of the worker's and not of any SPU's.

**That is a derivation and not a search**, which is the property this section
protects. One placed value fixes one directory, and a sibling of a placed
binary is as much the operator's placement as the binary itself. Nothing is
looked for on a path this crate composes from anything but the operator's own
file, and a second copy elsewhere on the box is never reached.

**Where a declared arm finds no binary the load is refused, and not here.**
Admin puts the flag on section 6's vector where the file stands and omits it
where it does not. The harness refuses a declaration electing an arm whose
binary never arrived, `ConfigInvalid` naming the member, which is the organ
judging what it can stand where admin has judged what it holds. **Two
refusals for one fact would be the duplication G5 refuses**, and the one
that survives is the organ's, because only the organ knows whether the
declaration elected the arm.

**The worker's composition root receives what it needs and reads none of this
file.** An earlier wording of this section had that root reading these values
alongside admin, which no longer describes anything: the values reach it as the
argument vector of section 6, admin being the party that already holds both the
validated name and the operator's file. The correction matters beyond
tidiness, because a worker reading this file would take a dependency on a shape
section 11 holds open and would put a second reader on values only one party
places.

**The boundary file names the trace door's one reader, and it must be declared**, on the
operator's rulings of 2026-10-03 on #63 and #50. `roles.toml` stands in the agent's root
under the root's judgments, root-owned, writable by no group or other, and never a link,
and it is **required**: a file missing or malformed refuses by name at `validate` and at
`load`, `ConfigInvalid` naming `roles.toml`, and no reader is ever admitted because a
file was absent. **It is required there and nowhere else**: a damaged boundary file
takes neither `unload`, `stop` nor `show` from a running agent, whose operations-log
lines then carry no digest, per section 8. Its shape is `BoundaryFile` of
`weaver-types-Spec` section 3.1, the one `trace-reader` and nothing else: **the
lifecycle half #63 drafted, groups mapped to verbs, does not return**, who may issue
which verb being the sudo rule's of section 2, which this crate does not read. The
reader is a user, the connector's service user, and a reader that is the agent's account
or its state member's, or that does not hold the agent's access group
`weaver-<agent>-admin`, refuses at the same judgment, so the agent never reaches its own
record through the boundary and the declared reader can always reach the door. **It is
boundary and never constitution.** The constitution, what shapes what happens inside the
agent, is the declaration and the facts the `load` event names, and it is the tuple. The
boundary file says who may read the agent from outside, as its OS identity does, so it
stays out of the declaration's digest, and granting a reader never makes the agent
another agent. It is declared all the same: its digest is written on every line of
`admin.log`, `show` carries it in the `show` rework, and the enter hands it to the
harness for the `load` event's member marked boundary. **It is one file per agent**
because it names that agent's reader, its connector being one agent's own appendage
(toddwbucy/WeaverTools#6), and a shared file would be a roster of every agent's readers,
the fleet view #45 removed from admin.

**The trace relay's account and the agent's access group are provisioned, and the walk
requires them.** `weaver-<agent>-relay`, whose one group is the trace group
`weaver-<agent>-trace`, is the account section 6 starts the relay under, and
`weaver-<agent>-admin` is the group the trace socket is grouped to, which the declared
trace reader must hold. The start step resolves the relay account, its trace group and
the access group wherever a file sink stands a relay, and refuses `BoundaryUnverified`
by name where any is missing, while the reader's judgment of this section runs at
`validate` and `load` for every sink, on the same ground the member's account is
required: a process this crate starts under an account the box does not carry is a start
that fails opaquely, and a reader outside the socket's group is a reader the filesystem
turns away before the relay's credential check runs. The relay's binary,
`weaver-trace-relay`, is this crate's second binary, per section 1, installed beside the
worker binary and found as the state member's is, never a value of its own.

## 10. What is enforced, and by which instrument

Per apex section 11, with the threat walks stated first and each test a walk
made executable.

**The first walk: the agent drives its own lifecycle.** The adversary is an
elected tool running as the agent uid, and the targets are the two ways a
lifecycle verb could be reached. Running the binary is one, and it is refused
because the invocation performs nothing unless it holds root, per section 2.
Dialing the coordination socket is the other, and it is refused at the
harness's accept, whose credential check expects root and holds the record for
it, per `weaver-harness-Spec` section 2.3. The test on this crate's side: the
binary run as a non-root uid refuses before touching any agent, watched to fail
when the root check is removed. The walk lost two mechanisms with the recut,
the operator socket's predicate and the coordination directory's 0700, and it
is stronger for the exchange: both were fences in front of a check that could
not tell the worker from a tool, and what stands now is a check that can.

**The second walk: the agent reaches the sink by path.** The adversary is
the same tool surface, the attack a path traversal to the sink the config
names. The mechanism is the operator's provisioning, verified by the
inventory: the containing directory denies the agent uid the search bit, so
the kernel refuses the lookup before any mode on the file is consulted. The
test: an inventory run against a boundary whose sink directory grants the
agent traversal refuses the load, watched to fail when the check is removed.
The test reaches this one check of section 4's list and none of the others,
which is why that list carries two records rather than one.

**The third walk: admin's own subprocess inherits a descriptor.** The
adversary is whatever the start step execs becoming an unintended holder of
the sink, a connection, or the log. The mechanism is atomic close-on-exec at
every creating call in this crate, no descriptor existing between creation
and flag, and every deliberate gift placed at its child's fixed number from a
copy above every number a child places. **Each child has its own allowlist**,
on #73's first item, the deliberate gifts of section 6 and nothing else:
- **the worker**: its standard streams, input at `/dev/null` and output and
  error at `worker.log`, the run lock's description at 9 and, where a file
  sink stood a relay, the relay's lifetime pipe's write end at 8.
- **the state member**: its standard streams, its own end of the first door
  at 3, the run lock's description at 9 and, under a restore, the save point
  at the number its act elects.
- **the trace relay**: its standard streams at `/dev/null`, the listener at 3,
  the sink's read-only descriptor at 4, the operations log at 5, the lifetime
  pipe's read end at 6 and the run lock's description at 9.

The test spawns each child on its real path, enumerates its descriptors and
requires exactly its allowlist, watched to fail when a gift's placement is
dropped or any single atomic flag is downgraded to a later `fcntl`. The
worker's and the member's run inside a user namespace, as root over the
invoking user's subordinate ids, and so does the relay's.

**The fourth walk: a stranger speaks on the coordination channel.** The
adversary is a process running as the agent's uid, or as any uid on the host
that is not root, dialing the worker's socket. The mechanism is the harness's
credential check at accept, root or refused, and both the mechanism and its test
belong to the crate that performs them, per `weaver-harness-Spec` section 2.3, so
this crate cites the walk and declares no record for it.

**The check separates root from everything else and does not separate this crate
from other root processes, which is stated rather than implied.** `SO_PEERCRED`
yields a uid, so what the harness can know is that its peer holds root, not that
its peer is `weaver-admin`. Any root process on the host may therefore dial an
agent's coordination socket and direct its lifecycle. **That is not an
additional exposure and the reason is worth naming**: a root process already may
`ptrace` the worker, read and write its memory, replace the binary the start
step runs, or kill it, so a channel that admitted root adds nothing to what root
held before it. The boundary this program draws is between the agent and root,
and it is drawn where the agent cannot cross it. A check that separated admin
from other root processes would need a credential the operating system does not
supply at a socket and would defend against a party the trust model already
trusts, per `weaver-admin-PRD` section 2's statement that the program secures
the agent against reaching its own record and secures nothing against the
operator. What this crate owes the walk is the dial itself
being the only route it takes: no second connection is opened, none is kept
across verbs, and the connection closes when the verb answers, so a descriptor
to a running agent's supervisor exists only for the life of one invocation.

**Enforced by the compiler.**

- The floor's three wire enums are exhaustive, so every directive, answer,
  and refusal case reaches this crate's matches loudly. `weaver-types-Spec`
  section 4.2 argues and asserts that property and this crate consumes it.
- Descriptors are owned types end to end, a leak being a move the borrow
  checker sees. That is the ownership half of the custody section 1 opens,
  and the atomic close-on-exec half is section 6's behaviour and the third
  walk's test, so this bullet claims the compiler for ownership only.
- The inventory is one function with two callers, so `validate` and `load`
  cannot drift, the one-code-path rule as a call graph. Where the verb stops
  is not a call-graph property, so section 3 leaves that half to review.

**Enforced by compile-fail tests, because the property is an absence.** The
floor already pins the load-bearing absence this crate depends on,
`PeerIdentity` deriving no `Deserialize`, so a credential cannot be
constructed from bytes a peer sent, per `weaver-types-Spec` section 3. This
crate adds none of its own: it is the path-holding party by charter, so the
no-path pins live on the worker's side of the seams and are declared by the
crates that hold that side, and a pin invented here would pin nothing the
charter claims.

**Enforced by the manifest.** The internal dependency is exactly
`weaver-types` with the `config` feature, read against the graph's one
floor-link under gate H2, and no direct `weaver-traits` line exists, which
is the charter's declared non-link as a checkable absence. No async runtime,
no bus crate, and no logging crate in the resolved tree, by the build-time
`cargo tree` assertion the floor Specs share. Two binaries and no library surface,
read from Cargo's target inventory by `two_binaries_and_no_library_surface` once the
code act renames it. The watch requires the two named binaries and permits integration
test targets alone beside them, as section 1 states. Explicit
and convention-discovered targets pass through the same check.

**Which invariant each claim serves, and why most serve none.** Twelve of the
forty-three carry a `grounds` edge and those twelve carry thirteen edges, one
record grounding in two invariants. **All three records the boundary act of
2026-08-28 added ground in none**, and the paragraph accounts for each rather
than one. Section 6's runtime
directory mode is an election about a boundary this crate provisions, and its access
rule reachability is a consistency rule between two locks - neither is a claim an
invariant reaches. The third, the gid set the denial walk reads, is the same shape: it
corrects the credential set of a walk this crate already performs, and the walk it
corrects grounds in no axiom either. A `grounds` edge to that walk was drafted and
removed - the notation runs from an assertion to an **axiom**, per Document Format
section on edges, and an assertion-to-assertion edge would have been a new relation
smuggled in under an existing name.

**The thirteen are named rather than numbered**, an ordinal in document order being the
thing that goes stale next. Seven run to `axiom-floor-is-vocabulary-behavior-is-socket`,
one to `axiom-contract-is-a-complete-interface`, one to `axiom-organ-and-submodule`, and
four to `axiom-harness-integrates-by-the-loop`. **The test applied is whether the axiom
is the reason the claim exists, or whether the claim is a precondition of the axiom's
own stated reason,** per Document Format section 4. Remove the socket invariant and this
crate has no reason to publish no library and no reason to hold one internal dependency,
no reason for the verbs to sit behind a principal check, no envelope has to arrive whole
or a truncation to count as a fault, and the dial's bound has nothing to bound, so those
six ground in it. **The atomic close-on-exec of section 6 grounds in it by the second
relation rather than the first**, on the argument that clause states. Remove it and the
log is still NDJSON, the FIFO still opens nonblocking, the inventory still repairs
nothing, and the identity is still built from the validated name, so those ground in
nothing. **Thirty-one claims grounding in no invariant is the expected result and not
a gap**, per Document Format section 4: most of what this Spec elects is a rendering, a
mode, an ordering, or a route, and representation is what the invariants are not about.

**`axiom-join-key-travels-with-the-work` takes nothing from this crate,** and
the reason is that invariant's own scope rather than an oversight in this pass.
A lifecycle directive belongs to no turn and carries no turn key, and every
message this crate sends is one, so there is no seam here at which the key
travels. The trace this crate opens a sink for is written by the harness and
admin authors no event in it.

The one edge to the contract invariant is the invocation's answer agreeing with
its exit status, which the recut left standing where the operator surface's two
edges retired with the socket. A contract is a complete interface, so an
interface that could answer one thing and exit another would be two interfaces
disagreeing, and the agreement is what makes the invocation's boundary
readable by a shell and a tool alike. The coordination channel's boundary
election is `weaver-types-Spec`'s record and grounds there. What lands here is
the conduct at it, one write read as one message and a truncation read as a
fault, and both of those ground in the socket invariant instead.

The organ invariant keeps one edge and it is the device check's, the device
having one authority and that authority not being this crate, which is the
domain partition apex section 5.4 draws.

**The four edges to the integration invariant are this crate's charter position stated
from the other side.** Admin authorizes and does not execute, and the reason it does not
is that integrating is the loop's: the stop answer is relayed unchanged because what a
stop found is a fact about a run the harness conducts, the devices a binding assigns are
unchecked because forming a view of them would be this crate reasoning about a domain it
cannot see, the operations log stops at supervision because conduct is recorded in the
trace the harness authors, and the run lock is read as a run's presence and never as
a state because telling a run at rest from a run serving is known to the party that
conducts it and not to the one that started the worker. The ten owings below are the
same rule at the document level.

**One edge moved and one was added beside an existing one.** Both of the organ
invariant's edges were placed in the labelling pass, before the apex held a
fifth invariant, and apex section 5.4 was the nearest section then saying
anything about domains. That section settles what an organ is and apex section
5.5 settles what an organ is answerable for, and a claim about declining another
domain's question turns on the second. So the stop relay carries the new edge
alone, nothing in the classification of organs being a reason to relay an answer
rather than to read it, and the device check carries both, the single authority
it defers to being 5.4's partition and the declining being 5.5's bound.

One call in the pass is worth a reviewer's eye, and the recut vindicated the
reading behind it. The retired directory mode grounded in nothing, because what
the socket invariant turns on is that a peer is known rather than that a
stranger cannot resolve a name, and the inversion made that reading structural:
the socket is now reachable by design and the check is the whole of the
refusal. The descriptor route grounds in nothing on the apex's own terms, since
which party creates a channel and how a descriptor travels belong to the
contract governing that seam rather than to the apex, so the dial and the
ancillary payload elect a route the invariant left open.

**The sharpest decline against the integration invariant is the published
state.** Waiting on a ready aggregate reads at first like this crate deferring
to the harness's domain, and it takes no edge. The aggregate is a value this
crate's contract delivers, keying on it is what a contract's vocabulary is for,
and the record written is admin's own and sits wholly inside admin's domain.
Apex section 5.5 binds what crosses between domains and does not reach what an
organ does inside one, so an ordering held inside a verb grounds in nothing.
The same reading leaves the load's step ordering and the inventory's one
function unedged, each a sequence this crate holds rather than a reconciliation
between two domains. **The residency read is not among them**, being the fourth
of the edges the lead above enumerates and argues, and an earlier form of this
sentence listed it as unedged while its own record carried the edge.

**Where the assertion records sit, and which of these this crate declares.**
The records are at the clauses that argue the claims, across sections 1
through 8, rather than gathered here, per Document Format section 6: this
section sorts by instrument and the arguments are elsewhere, so a block here
would sit apart from the prose that earns it. Forty-three records in all,
twelve tagged for review, twenty-five for perturbation, four for the
manifest, and two for a compile pin, the restore's judgment joining on
2026-09-06 and the member's own account bringing four on 2026-09-15, all four
watched since the spawn's drop moved from review to perturbation on 2026-09-26,
its instrument driving the spawn inside a user namespace, and the start step of
2026-10-03 retiring two review records and one perturbation record with the unit and
bringing three perturbation records of its own, and the service engine's
retirement of 2026-10-02 taking `admin-store-gate-asks-as-the-member` with the
store probe it watched. The residency record
moved from review to
perturbation on 2026-08-06, when the code act gave it a test, and the library
surface from review to the manifest on 2026-09-15, its test having read the
absence the clause said nothing mechanical read. **Three of the
perturbation records arrived without this count moving**, with the boundary
act of 2026-08-28:
`admin-access-rule-reaches-the-socket`,
`admin-runtime-directory-mode-is-stated`, and
`admin-boundary-reads-every-gid-the-worker-holds`. The fourth arrival,
`admin-granted-permission-refused-at-inventory`, is this recount's own act,
counted with the record it adds. Whether the three uncounted arrivals name
their removals was not audited in this recount and is owed beside the two
below. **Two of the twenty-five perturbation records name no removal anywhere
in this document**:
`admin-unload-answers-after-confirmed-stop` and
`admin-kind-mismatch-refused-at-inventory`. A perturbation tag without a
named removal says a test exists and not what breaks it, which is the tag's whole
content, so each is owed the sentence its neighbours carry. Found while checking
whether this file's convention was universal, on the occasion of a record that
follows it. The elections take nodes
because gate H1 would otherwise leave the largest decisions in this Spec
untraceable, and two review tags come from the sorting rather than from an
election: the verb's starting no process and the existence checks no
test reaches are the review halves of splits this section's own bullets take,
and a divided half counts with the bullet it divided out of, per Document
Format section 3.

**Twelve records left with the recut, one of the twelve by moving, and seven
arrived, which took the count from thirty-six to thirty-one.** Measured across
the recut's span by diffing the file's own assertion identifiers rather than by
counting the lists below. **The inversion is why the older reading came out one
short on each side.** `admin-unit-declares-one-open` became
`admin-unit-declares-no-open`, and an earlier form of this paragraph called that
neither a retirement nor an addition. It is both: the identifier changed, so the
graph lost a node and gained one, and a census that counts nodes counts two
movements there. The lists below name eleven departures, ten retired and one
moved, and six arrivals, so each is short by the inversion's one side.

**Thirty-one was this span's endpoint and thirty-two arrived at the next act**,
`admin-run-reference-distinguishes` with the run's identity. Three records took the
count to thirty-four: that one, then `admin-kind-mismatch-refused-at-inventory` on
2026-08-24 and `admin-preload-name-follows-the-kind` on 2026-08-25. Six more reach
forty, the boundary act's three of 2026-08-28 that the tag census names, then
`admin-granted-permission-refused-at-inventory` on 2026-08-31,
`admin-restore-cut-judged-at-the-inventory` on 2026-09-06, and
`admin-missing-home-refuses-and-builds-nothing` on 2026-09-07. Four more reach the
forty-four this section counts above, all on 2026-09-15 with the member's own
account: `admin-member-account-required-at-inventory`,
`admin-member-territory-is-the-members-own`, `admin-store-gate-asks-as-the-member`,
and `admin-member-spawn-drops-to-its-account`, the third of them retiring with the
service engine on 2026-10-02. Each is named so this figure is
checkable against the file the way every other figure here is. An earlier form of
this lead stated no endpoint at all, and stating one is what exposed that the lists did
not reach it. Retired: the operator surface's six, its stream election, its accept-time
refusal, its refusal-by-closure, its serial answering, its bounded request line, and its
bare wire shapes, each dying with the socket rather than relocating. The coordination
channel's bind ordering, its directory's mode, its listener's closure after one accept,
and the one exchange in flight per agent, the last four retiring with the acts and the
map they described. Moved: the credential check, to `weaver-harness-Spec` section 2.3,
where the accept now happens. Added: the root check and the answer-and-status agreement
of section 2, the dial's bound of section 7, the residency read from the init system of
section 3, the state ask that follows a failed dial, of section 6, and the unload's wait
on a confirmed stop, of section 3. **The twelfth departure and the seventh arrival are
the one event the lead above argues**, the unit's declared open inverting to a declared
absence. **A rebuild reads this movement as the recut's delta and not as this Spec's**,
thirteen records having been added since by acts of their own and one retired, so a
census taken against this paragraph alone lands twelve short of section 10's
forty-three.

**A claim this Spec cites and another Spec argues is declared by that Spec,**
not here, because the assertion belongs where its argument and its test live
and a node declared twice is the one-name-two-nodes defect the format forbids
for identifiers. Ten are such owings and carry no record in this document, the
count holding across the recut because one left and one arrived. Two are cited
in this section: the exhaustive wire enums and the missing `Deserialize` on
`PeerIdentity`, both `weaver-types-Spec`'s, the no-path pins on the worker's
side having left this list with the seam clause that cited them. Eight more are
cited where the sections use them. Four are `weaver-types-Spec`'s: the parse's
totality of section 4, the sink's discriminated shape with the socket case's
absent creation flag of section 5, the boundary election of section 7, and the
envelope bound that election carries, which that Spec declares as its own
record beside the election rather than inside it. The denial ordering left the
list with the predicate that applied it. Four are `weaver-harness-Spec`'s: the
OS-surface election of that Spec's section 2.4, cited in section 1, its bind
of the coordination socket and its credential check at accept, both of section
2.3 and cited in sections 6 and 7, and its refusal of a directive arriving out
of order, cited in section 7. The shape behind the list is the crate's own:
admin authorizes and does not execute, so what a run does after the enter
directive is asserted where the run happens.

**Requiring a perturbation-verified test, beyond the walks.**

- The root check: the binary run as a non-root uid refuses before touching any
  agent, confirmed by watching a verb proceed when the check is removed.
- The territory is root's and closed, per section 9 as of 2026-10-08: a directory of
  any mode but `0710`, the earlier `0711` among them, one under any group but the state
  group, one named through a link above it, one carrying an access-control entry, one
  whose `save-points/` carries one, one not root's, one holding
  no `save-points/` or one of any mode but `0750`, an `operator` key absent or naming no
  uid, a declaration that is a link or of any owner, group or mode but root, the access
group and `0640`, and an ancestor
  another principal can write each refuse `BoundaryUnverified` before a value is read,
  confirmed by watching each pass and its declaration parse when its arm of the
  judgment is removed. The enter's `operator` is
  the root's `operator` key and never the caller's uid, confirmed by watching the two
  come out equal when the cause is copied in its place.
- The answer and the exit status agree: a refusal exits non-zero and an answer
  exits zero, confirmed by watching a refusal exit zero when the status is
  taken from the wrong branch.
- The dial's bound: a dial against a name nothing binds refuses within the
  bound rather than waiting, confirmed by watching the invocation hang when the
  ceiling is removed from the retry loop.
- The FIFO refusal: a pipe sink with no reader refuses the load with
  `ENXIO` mapped to its case, confirmed by watching the load hang when the
  nonblocking open is made blocking.
- The rollback's account: a load failed at each step leaves exactly what
  charter section 5 names and the log records what was undone, confirmed by
  watching the account go silent when logging moves off the rollback path.
- Truncation is a fault: an over-bound envelope on the coordination channel
  produces the fault and no directive, confirmed by watching a silently
  shortened answer decode when the `MSG_TRUNC` check is removed.
- The member's own account: an inventory run against a box carrying no
  `weaver-<name>-state` account refuses every election but `none`, confirmed
  by watching the same declaration pass the inventory when the arm is removed,
  a load then standing a member under this crate's identity; and an account
  whose primary group is not the state group, or is gid 0, refuses too.
- The territory is the member's: a prepared territory is `0700` and owned by
  the member's account, and a room widened between loads is closed again,
  confirmed by watching the mode read `0750` when the group-owned preparation
  is restored.
- The member's spawn drops to its account: `stand_state_member` run as root
  inside a user namespace stands a member whose every uid and every gid are its
  account's and whose supplementary set is its own group alone, the access group not
  among it, confirmed by watching the member run as uid 0 when `become_member` is removed
  from the spawn's pre-exec, and carry root's group when `drop_to` is handed it beside
  the member's.
- One write is one read: two envelopes are written back to back on the
  coordination channel and both writes complete before either read, and two
  reads return exactly one envelope each, confirmed by watching the first
  read return both when the socket is created as `SOCK_STREAM`. **Two
  messages are what make the watch reachable.** The truncation bullet above
  cannot see that substitution at all, `MSG_TRUNC` handling being untouched
  by it, and a single small envelope crosses a stream socket whole, so a
  one-message test would pass under the substitution and pin nothing, which
  is the never-failing perturbation apex section 11 counts as worse than no
  test. This is the boundary half of the pair test `weaver-types-Spec`
  section 5 owes the pair-creating crates, argued at section 7, and it
  discharges this crate's side of that owing alone.

**The walk at the start step and the trace door, opened 2026-10-03 on #50.** The
adversaries are a process of the agent's reaching for the run lock or the trace, a
caller reaching for a verb or an argument the sudo rule did not grant, and a relay or
an organ outliving the worker it belongs to. The tests the code act owes are each
perturbation-verified:

- **The worker runs as the agent and holds no group root left it**, watched by a
  stand-in worker recording its credentials after the start step's exec. The
  perturbation skips the supplementary narrowing, and the worker holds root's groups.
- **A concurrent invocation refuses while one is in flight**, watched by an `unload`
  issued while a `load` is still admitting its model: it answers `InvocationInFlight`
  and the load completes. The perturbation drops the invocation lock, and the unload
  ends the healthy start.
- **A load meeting a live worker refuses**, watched by a `load` of an agent whose worker
  answers `Idle`: it answers `AgentRunning` and starts nothing. The perturbation skips
  the run lock's take, and a second worker starts.
- **Each child's ignored signals are reset before its exec**, watched by the unload's
  escalation sending `SIGTERM` to a worker that does not exit after left: the worker
  ends. The perturbation drops the reset, the worker inherits the invocation's ignored
  `SIGTERM`, and only `SIGKILL` ends it.
- **`show` answers a transition in flight**, watched by a `show` during a load still
  admitting its model: it answers `InTransition` at once. The perturbation dials
  instead, and the answer times out.
- **A reserved suffix refuses**, watched by an agent named `x-relay`: refused at the
  name check. The perturbation drops the suffix check, and the agent's accounts collide
  with agent `x`'s.
- **The lock stands where the agent cannot replace it**, watched by the agent's uid
  trying to unlink and recreate `run.lock`: refused by the root-owned run directory.
  The perturbation places it in the runtime directory, and a second worker starts.
- **`unload` answers only once the lock is free**, watched by a worker that answers
  left and does not exit: the escalation signals the lock's holders and the answer
  waits on the release. The perturbation answers on the leave alone, and the worker
  still runs.
- **A wedged observation is bounded**, watched by a stand-in worker that accepts
  `Observe` and never answers: `show` refuses `Unanswered` once the observation's bound
  expires, and a following `unload` takes the invocation lock. The perturbation drops
  the observation's bound, `show` never returns, and its shared hold keeps every
  exclusive verb refusing `InvocationInFlight`.
- **A wedged stop is bounded**, watched by a stand-in worker that accepts stop and
  never answers: the verb refuses `Unanswered` once the stop's bound expires and a
  following `unload` takes the invocation lock. The perturbation drops the stop's
  bound, the verb never returns, and every later verb refuses `InvocationInFlight`.
- **A wedged leave is bounded**, watched by a stand-in worker that accepts leave and
  never answers: the `unload` escalates once the leave's bound expires and answers when
  the lock is free. The perturbation drops the leave's bound, the verb never returns,
  and every later verb refuses `InvocationInFlight`.
- **The escalation ends every holder**, watched by a run left by a killed load whose
  stand-in member
  does not retire on the first door's end: the `unload` signals the worker and the
  member and answers once the lock is free. The perturbation signals the first holder
  the scan finds, and the verb refuses `WorkerWouldNotExit` with the member standing.
- **A reused pid is never signalled**, watched by a holder that exits after the scan
  and a stand-in process given its pid before the signal: the stand-in survives. The
  perturbation drops the descriptor's re-check after `pidfd_open`, and the stand-in is
  killed.
- **The cause is the uid sudo reports**, watched by a `load` under sudo whose `load`
  event names `SUDO_UID`. The perturbation reads the uid from standard input, and a
  caller names any uid it likes.
- **The trace relay admits only its declared reader**, watched by a member of the
  socket's group that is not the reader: refused and logged. The perturbation admits
  any member of the socket's group, and that member receives the record.
- **The agent cannot reach the trace door**, watched by the agent's uid connecting:
  refused at `connect` by the socket's mode. The perturbation binds the socket `0666`,
  and the agent's connection reaches the relay.
- **The newest reader connection replaces the old**, watched by the declared reader
  connecting twice: the first follower ends with a reason. The perturbation keeps both.
- **The relay dies with the worker**, watched by killing the worker with `SIGKILL`: the
  relay exits on the pipe's end-of-file. The perturbation leaves the write end open in
  the start step, and the relay outlives the worker.
- **A verb finishes when its caller disappears**, watched by a `load` whose caller is
  killed after the start step: the agent loads and `admin.log` records the outcome. The
  perturbation leaves `SIGHUP` at its default, and the load dies part way.
- **A second file-sink load binds its trace door**, watched by two loads in sequence
  with an unload between: the second binds `trace.sock`. The perturbation skips the
  stale name's removal, and the second load fails its bind.
- **A caller's hangup ends no child**, watched by a load whose terminal hangs up after
  the relay and the member are forked: both stand. The perturbation drops their
  `setsid`, and the hangup kills them.
- **A load killed at any point leaves its run locked**, watched by a load sent
  `SIGKILL` after the member is forked and before the worker is: the member holds the
  run lock, the next `load` refuses `AgentRunning`, and an `unload` ends the member and
  answers, two members never standing. The perturbation takes the run lock in the
  worker's child instead of before the first fork, and the next load starts a second
  member beside the first.
- **A load never ends an existing run**, watched by a `load` meeting a worker that
  answers `Unloaded` and by one meeting a worker that answers `Observe` only after the
  observation's bound: the first refuses `AgentRunning`, the second `Unanswered`, and
  each worker still runs. The perturbation lets the load end what holds the lock, and
  a live run dies under a load.
- **`unload` ends a run that never entered**, watched by a load killed between the
  start step and the enter: an `unload` finds the worker answering `Unloaded`, goes
  straight to the escalation, and answers once the lock is free, after which a `load`
  starts. The perturbation directs leave at the unentered worker and refuses on its
  answer, and the agent stays stuck until a person intervenes.
- **The relay streams the loaded run's file**, watched by an edit to the declaration's
  sink while the run stands: the stream still reads the worker's file. The perturbation
  reopens the declared path, and the stream reads the new file.

```graph
node: admin-start-step-holds-the-run-lock
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-start-step-holds-the-run-lock

node: admin-trace-relay-admits-one-reader
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-trace-relay-admits-one-reader
```

## 11. Open elections

Each names what settles it, and none is this Spec's to settle alone.

- **How an agent's lifecycle state is observed. Settled 2026-09-04** by the observation
  exchange of `weaver-admin-harness-contract` section 3, per issue #435: the harness
  answers its state from the run with the load's facts beside it, section 3 above says
  how `show` uses it, and `StateNotObservable` left the floor with it. The
  entry stood as: **How an agent's lifecycle state is observed, and what the `State`
  answer carries meanwhile.** Section 3 reports residency in the manager's own three
  values because that is what the init system can answer, and apex section 6's four
  states are the harness's to know. The two halves are one election: the manager's
  `active` covers both `Idle` and `Active` and its `failed` has no `AgentState` case, so
  `lifecycle-answer`'s `State` case has no producer for these verbs until an observation
  reaches the party that holds the run. **Settled by:** the observation exchange on
  `weaver-admin-harness-contract`, `Observe`, which supplies lifecycle state beside the
  enter, leave, and stop that contract already chartered, together with whatever
  `weaver-types` owes its enumeration once that exchange fixes what can be observed. The
  answer arrives with that contract's next opening rather than from a mapping this Spec
  could invent. - **The session-close cue and the enter question.** Charter section 10's
  two cells, settled by the human's ruling and the memory-and-state round respectively,
  carried here only so this list is complete. - **The log's field set and rotation
  policy.** Satellites of section 8, the fields a builder's choice with no cross-crate
  consequence, the rotation elected against a measurement of what accumulates. - **The
  service configuration's shape.** Section 9's file: its format and field list are a
  builder's choice bounded by what that section fixes, and the natural candidate is the
  same dialect the agent config elected, one syntax for everything the operator writes,
  per the common-syntax direction the composability batch recorded on the working list.
  - **`AgentState` and `LoadFacts` field lists.** The floor names the types in
  `lifecycle-answer` and their fields are satellites there, consumed here as drawn.
  `AgentSummary` left with `list` on the operator's ruling of 2026-10-02. -
  **The two values the argument vector does not carry.** Section 6's vector carries the
  socket path, the two placed organ binaries, the SPU's the agent's own, the loop file
  where a declaration names
  one, and the derived classify binary where one stands, and the worker's remaining two
  inputs are named here rather than routed, because routing either now would carry a
  value nothing reads. The assembled prompt's identity is one: the agent's declaration
  holds an identity the SPU makes resident as the session's prefix, the assembled prompt
  holds a second the loop reads, and whether those are one thing is the assembly
  question rather than this act's. The tool schemas are the other, the declaration's
  tool set reaching this crate and going no further while nothing dispatches a tool.
  **Settled by:** the assembly and distillation act for the first, and the tool workflow
  that charters dispatch for the second. Until then the worker defaults both, which is
  why a load stands without either. - **The sandbox's hardening. Settled 2026-10-03**
  with the agent's departure from systemd (#50): hardening is the operator's to wrap
  around a packaged agent, with systemd, a container or anything else, and is not this
  framework's, per `weaver-admin-PRD` section 7. What the start step itself delivers is
  the floor: the agent's own uid and group, no new privileges across the exec, and a
  runtime directory only the agent and the operator reach. The template and
  `unit-properties` key that carried the rest retire. - **Whether a packaging restricts
  the agent's address families to `AF_UNIX`.** Open with a stated cost rather than
  required, per the ruling of 2026-08-05. It
  is **not** a restatement of the rule that no crate here exposes a network surface:
  that rule binds what these crates link, and this would bind what an agent's tools may
  reach, so an agent whose tools fetch anything would break under it. Settled by an
  operator who knows which tools their agents carry.