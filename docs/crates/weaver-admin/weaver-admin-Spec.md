# weaver-admin - Spec

**Status:** MERGED. Cut 2026-08-02, fifth of the Spec pass and the first outside the
agent. Code is written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-02
**Document ID:** `weaver-admin-Spec`
**Parent:** `weaver-admin-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #65

---

## 0. What this document is

Build instructions for `weaver-admin`: the binary's layout, the invocation's
interface, the verb sequencing, the sink openings, the transient unit's
invocation, the coordination channel's dial, and the elections a builder
would otherwise invent. It is derived from `weaver-admin-PRD` and from the three
contracts this crate is party to, `weaver-admin-harness-contract`,
`weaver-admin-operator-contract`, the second now bounding the trace's exit alone,
and `weaver-admin-systemd-contract`, cut this date for the boundary the recut made
load-bearing, together with `weaver-organ-channel`, the drawn material the first
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

**One binary, per the charter's ruled layout.** The crate builds a single
executable, `weaver-admin`, and no library surface is published: nothing links
admin, per the charter's section 7, and a `lib.rs` would be an API for a
consumer the topology forbids. The instrument reads Cargo's target inventory,
which includes both explicit declarations and targets found by convention.
`one_binary_and_no_library_surface` of `crates/weaver-admin/tests/manifest.rs`
requires exactly one binary named `weaver-admin`, permits integration test targets,
and refuses every other target, including a library, build script, example or bench.
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
    src/unit.rs       the transient unit, section 6
    src/channel.rs    the coordination channel's dial, section 7
    src/log.rs        the operations log, section 8

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
section 2 needs. The init system is reached by its command-line interface, per
section 6, so no bus library enters the tree. The operations log of section 8
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

**The verb and its agent arrive as arguments.** One verb per invocation,
`load`, `unload`, `validate`, `stop`, or `show`, with the agent name as the one further
argument, which every verb takes. Arguments rather than a
parsed request line, because the kernel already delivered them as a vector and
re-encoding them into a wire format would be inventing a wire where no seam
crosses. The base the agent's root stands under is the one environment variable read,
per section 9.

**Authorization is the kernel's, and what this crate checks is the name.** The
invocation runs as root or performs nothing, so no predicate, no allow set, and
no deny set exist here: the earlier form's `authorized` call, its group allow
set, and its agent-uid deny set retired with the socket, the operator being the
party the kernel already admitted. What survives is the name check of section 4
and the agent's root of section 9, which are about which agent may be named rather
than about who may name it. The refusal is enacted before any verb touches anything, and the
instrument is a test running the binary as a non-root uid and finding it
refuses, watched to fail when the check is removed.

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

**Residency is read from the init system rather than held, per the recut of
2026-08-05.** A per-invocation crate has nowhere to keep a map across verbs, and
the map is not missed for what it truly knew: whether an agent's unit is
running is a question the init system answers authoritatively, through the same
command-line interface section 6 uses, where a map of admin's own would be a
second account of a fact the process manager already holds.

**What that answer is not is the agent's lifecycle state, and conflating them
would be this crate inventing a fact.** A running unit may be one that has not
yet answered enter, one serving a turn, or one unwinding after leave, and apex
section 6's states distinguish exactly those. The unit's presence is a residency
signal and nothing more. Reading it as loaded-and-idle would also contradict the
charter's own rule that the state publishes only on a ready aggregate, since a
unit is running well before any aggregate returns.

**So `show` answers through the observation exchange, as of 2026-09-04, and residency is
read only where no worker answers.** `show` dials the agent's coordination socket and
opens `Observe`, per `weaver-admin-harness-contract` section 3, and what returns is the
harness's own word: `Unloaded` before any enter or after a leave, `Idle` or `Active`
with the load's facts beside it where a run stands, read from the run and never from the
record. **Those facts are `LoadFacts`, which overlaps the `load` event and is not its
shape**, per `weaver-types-Spec` section 4.2: it carries the session, run and artifact
the event carries in its envelope or not at all, and lacks the event's stack, lineage
and prompt digest, so a consumer that stores both stores two shapes. Where the socket
does not exist or nothing answers the dial, there is no exchange to open, and this crate
reports `Unloaded` from that absence, which is the one place residency is read and it is
read as the absence of a worker and not as a state. A name with no root refuses
`NoSuchAgent` as every verb does, and whether a declaration validates stays `validate`'s
own answer, since no verb chains another. No verb answers for more than the one agent
named, admin being one agent's organ on the operator's ruling of 2026-10-01, and
managing several WeaverWeb's or a separate application's.

**The manager's three values stay the manager's and reach no answer.** `active`,
`failed`, and `inactive` are residency and not lifecycle state, per the paragraph above,
and mapping them onto `AgentState` would be the invention this section has always
refused: `active` covers both `Idle` and `Active`, and `failed` has no case. The harness
holds the run and answers from it, which is why the exchange and not a mapping closes
the election section 11 filed on 2026-08-06. `StateNotObservable` leaves the floor's
vocabulary with this act, the scheduled death `weaver-types-Spec` section 4.2 gave it.

**The record's instrument stays a test.** `show` on an admitted agent whose socket is
absent answers `Unloaded` with no load and constructs no state from the unit, watched to
fail when the verb is made to read the manager's `active` as `Idle`, which is the
invention this clause forbids. Review could confirm the absence of a mapping and could
not confirm that the verb answers the harness's word rather than something plausible.

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

**Two invocations for one agent are ordered by the init system, and the
consequence is stated rather than glossed.** The in-flight flag that held one
transition per agent went with the map, and what remains is that starting a
transient unit whose name already exists fails at the init system, so two
concurrent loads of one agent cannot both start a worker. Two concurrent
unloads reach a worker that answers leave once and refuses the second by the
channel's own ordering, per the contract's section 4. Neither race is prevented
by a lock of this crate's, and the honest statement is that the ordering is
delegated to the two parties that already serialize: the process manager and
the worker.

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
open the sink per section 5, start the unit per section 6, dial the worker's
socket and direct enter per section 7, publish. Seven actions, the charter's
own, the bind-and-listen act the earlier form interleaved here having moved to
the worker with the inversion. **The save-point publication of section 6 opens the
validate step**, before the inventory selects the save point to restore, so a save
point an unclean stop left in the member's room is published and selectable by the
load that follows it; it adds no step to the seven, being the inventory's first read
of what the load restores. Each step's failure returns a typed
`lifecycle-refusal` and enters the rollback below carrying the step's name.

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

**`unload` runs the charter's three steps, and the third waits on the second.**
Direct leave and await the aggregate, stop the unit through section 6's
interface, publishing the member's finished save points per section 6 once the stop
is confirmed and the member has stopped, and answer provisioned-and-unloaded **only once
the stop has been confirmed**; the publication adds no step to the three, being the
second step's tail. A refusal on leave, `ActivityNotAtRest` above all, returns to the
operator unchanged and answers nothing further.

**A stop that is accepted is not a stop that has happened, and this verb waits
for the difference.** `weaver-admin-systemd-contract` section 4 promises that a
stop ask is answered when the unit has stopped rather than when the stop was
accepted, so the ask itself is the confirmation and this Spec elects no timeout
of its own beside it. What the verb owes is not to run ahead of that answer:
**a following state ask that finds the unit `active` refuses with the failure
carried and answers no state**, because an agent reported unloaded while its
worker still runs is the one report this verb must never produce. Where the
unit stands, the run has already left, which the rollback of this section
records as an act it could not undo, per charter section 5.

**The state ask decides and the stop ask's status does not, because that status
cannot say why it failed.** `weaver-admin-systemd-contract` section 3 records
the measurement: the boundary returns the same status for a duplicate unit name
as for a malformed property, and a failed ask reports that an ask failed rather
than which failure it was. So a non-zero stop carries two readings that want
opposite answers. **A unit that is still there** is the case this clause was
written for, and the state ask finds it `active` and refuses. **A unit that is
already gone** is the other, and it is the ordinary end of a clean unload:
the leave the previous step confirmed causes the worker to exit, a transient
unit is collected the moment its main process does, and the stop that follows
then names a unit the manager no longer knows. Refusing there reports a failure
over an agent that unloaded exactly as asked.

**A failed prior unit is its own refusal, and this crate stopped calling it a
bind failure on 2026-08-16.** `BindFailed` answered for two conditions that a
state ask already tells apart. Where the manager reports the unit `active` and
the socket was unreachable, a bind is what failed and the name is right. Where
it reports `failed`, a prior process exited non-zero and the name it leaves
registered refuses every later start under it until the manager is asked to clear
it. That answers `PriorUnitUnreaped`, and the answer claims that and no more:
`failed` does not say whether the worker bound, or how long it served, because a
unit that bound and exited non-zero later reads the same.

**It rests on the one value the boundary states plainly.**
`weaver-admin-systemd-contract` section 3 measures the start ask's status as
unable to say which failure it was, a duplicate name and a malformed property
sharing it, so the refusal is not read from that status. It is read from the
state ask, where `failed` covers one condition and nothing is inferred.

**Found by running the verb rather than by reading it**, as the unload defect
below was: a load whose organ refused a construction parameter left a failed
unit, and every load after it answered `bind_failed` over a socket that was
never the problem, which sent the reading to the wrong place.

**An earlier form of this clause refused on either reading** and its reason
named only the first, which is how it came to over-fire. The defect was found
by running the verb rather than by reading it: every unload of a live agent
answered `bind_failed` and exited non-zero while the device was freed, the
worker gone, and the `unload` event standing in the record. A builder reading
the retired wording would write that again, which is why the ordering is stated
here rather than left to follow from the reason.

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

**Rollback is the reap plus one directive, as data.** What a failed load can
leave is a worker unit, a connected sink, and a device the SPU took, per
charter section 5, and the rollback walks what stands: direct leave where a
run was entered, stop the unit where a unit started, close the sink where one
opened. Each act's failure is logged per section 8, the rollback reports what
it could not undo, and no state is published on any partial outcome, which is
the same rule as the partial load and not a second one.

**The rollback clears a failed unit, on the operator's ruling of 2026-10-01.**
Where the load started a unit, or its start was refused `PriorUnitUnreaped`, the
rollback asks the unit's state after the stop, and on `failed` asks the manager to
clear it, per `weaver-admin-systemd-contract` section 2, logging the clear as one
more act of the account. The refusal is answered as it stood and nothing reloads:
the operator's next load is the retry, and it is not refused unreaped. A unit that
failed while serving is cleared the same way by the first load that meets it,
which answers `PriorUnitUnreaped` and leaves the name free for the one after. Only
`failed` is cleared, the state ask deciding, and a load that started nothing asks
the manager nothing.

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

**The prompt file is read through the parse and by this crate**, as of the operator's
ruling of 2026-10-02. The parse takes the declaration's text and a reader, and the
reader is this crate's: it opens the name the declaration's `identity-file` carries
inside the declaration directory section 9 judged, through the directory as opened at
that judgment and never by a pathname re-resolved after it, judges the file as that
section judges the entries it reads, and answers its bytes. A prompt file that fails
that judgment is the reader's own refusal, which the parse returns unchanged per
`weaver-types-Spec` section 2, and it refuses `BoundaryUnverified`, naming the file on
stderr, as a failed entry of the root does. One that is absent, does not read, or is not UTF-8 refuses
`ConfigInvalid` naming `spu-instruction.decoder.identity-file`, the omission being the
declaration's. **Both digests are this crate's**: sha256 of the declaration's bytes
and of the prompt file's, each as read here, ride the enter as `declaration` and
`identity_file`, per `weaver-admin-harness-contract` section 5, so the record names
what the agent was given without a second reader of either file.

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
election with no member to receive it is malformed, not surplus. `postgres`
requires
`database` and `role` in the declaration and, on the box, the store's socket
present under the path this crate's own configuration names, and the walk asks
the store the two questions the charter's two gates pose: that the member's
account maps to the declared role and that the agent's uid maps to none, each
refused with `BoundaryUnverified` and never `ConfigInvalid`, for the reason the
group case below gives. The grant surface is not judged here, only read, at the
enter and again at the leave, per `weaver-trace-PRD` section 3.1.

**Every election but `none` requires the member's own account too, as of
2026-09-15**, per issue #545. The account is `weaver-<name>-state`, derived
from the same validated name section 1's identity is, and read from the
account database rather than declared, for the reason the agent's home is read
rather than composed: the uid the spawn drops to, the uid that owns the
territory, and the uid the store's first gate is asked about are one kernel
fact, and a declaration naming it would be a second place that fact is stated
and a value the operator could move under a running agent. A box lacking the
account refuses `BoundaryUnverified` naming the account, exactly as a box
lacking the member's binary does and for the same reason, the provisioning
being what is absent. **The refusal stands ahead of the store's questions**,
the account being what the first of them is about.

```graph
node: admin-member-account-required-at-inventory
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-member-account-required-at-inventory
```

**Each gate is asked as the uid it is about, and this is a correction.** Until
2026-09-15 the member's gate was asked from this process, so what the store had
to admit was whichever account admin runs as, which is root, while the agent's
gate was asked by a child under the agent's uid. One question therefore
described the member and named admin, which is the defect issue #545 filed: the
charter derives the object gate's identity from the kernel fact, and a question
asked by a process that never dials the store derives nothing. Both questions
now go the same way, a child re-executing this binary under the named uid and
that identity's whole group set, primary first, and exiting with the answer,
which is the mechanism the agent's gate has carried since 2026-09-04. **The
identity is taken in the child before exec, groups then gids then uids**, as
of 2026-09-24 on issue #675: a probe that left the uid to the spawn's own
setting lost the privilege its group call needed, could not be spawned as any
member, and so every agent electing postgres refused `validate`.
**The second gate is unchanged**, no agent's uid reaching any store being the
property already bought, and it is kept rather than rebuilt.

```graph
node: admin-store-gate-asks-as-the-member
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-store-gate-asks-as-the-member
```

**A restore is judged here too, and it names a save point**, on the operator's ruling of
2026-10-02 on #58, which retires the record restore of 2026-09-04 (issue #432). **It is
judged only where a state member stands**: under the `none` store engine the inventory
selects no save point and resolves no lineage, and a declaration naming `restore` beside
that engine refuses `ConfigInvalid` naming `restore`, per `weaver-types-Spec` section 2.
A declaration electing one names a save point by its bare file name in the declaration
directory of section 9, and the walk reads it under this crate's own custody: the name
is judged as `identity-file`'s is, a save point absent from the directory refuses
`ConfigInvalid` naming `restore`, and one that fails section 9's judgment of the entries
it reads, a link, a wrong owner, or a group- or other-writable file, refuses
`BoundaryUnverified`. A save point the operator edited is an input like a prompt file,
admitted the same way. **The save point's bytes are judged here too**, before any
process exists, on the Planner's ruling of 2026-10-02 on #58's fourth review round: the
save point this load restores, the one `restore` names or the latest published, is read
under this crate's custody, and a torn file, a check over its bytes that fails, or a
stamp that disagrees with the name it was published under refuses `ConfigInvalid` naming
the save point, so a damaged save point never reaches the member and the operator names
another. What the member alone can judge, the save point's schema against the loop's, it
answers on the enter's `restored` ask of `weaver-harness-state-contract` section 2.
**The clean-unload marker is this crate's**: at every load it writes, in the agent's
config root under its own custody, a marker naming the run it mints as open, and a clean
unload marks that run closed; a load that finds a marker naming a run still open
resolves the enter's `reset`, which rides apart from the lineage and whether or not a
save point stands, to that run, `NoCleanUnload`, or `UnitFailed` where the
unit's result corroborates a failure, per `weaver-types-Spec` section 4. An agent's
first load finds no marker and resolves no reset. The lineage that crosses the enter is
resolved here from the save point's stamp: the run, the sequence and the last turn it
covers, and, where the operator's offline builder made it from a record, the record and
the cut it names. The save point is never handed to the agent and never named to the
worker, on the same descriptor discipline as the sink: what the member receives is a
descriptor this crate opened, per section 6. Branching from a record is the builder's,
outside the agent, and this crate never reads a record for a load. **The judgment is
this crate's because the save point is**, per charter section 4.3's custody rule: a look
at a thing admin holds, taken at the load's cheapest moment before any process exists,
and an ask of nobody. **The instrument is perturbation**: a `restore` naming an absent
save point refuses naming `restore`, watched to fail when the presence check is dropped,
and a linked save point refuses `BoundaryUnverified`, watched to fail when the entry
judgment is dropped.

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

**"Through any membership" is the whole of the requirement, and the gid set
the walk reads is where it is met.** The unit carries `Group={identity}` per
section 6, so the worker's runtime egid is the agent's own group whatever
primary group passwd records. A walk reading the passwd gid alone therefore
asks about a credential the worker does not run under, and where the operator
provisioned a shared primary - which is the case section 6's own argument for
naming the group cites - the two disagree: a sink directory at
`root:weaver-<agent>` mode `0710` passes a walk that sees only `users`, and
the running worker traverses it. So the walk reads the group the unit sets,
the passwd primary, and the user's supplementary memberships, and it
**over-approximates on purpose** - the question is what the agent could
reach, so a gid too many refuses a boundary that might have held and a gid
too few admits one that does not.

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
the same validated name is what section 6 interpolates into the unit template,
so a name reaching a path or a unit has one origin and review reads that origin
rather than every use.

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

**`File { path, create }`.** Opened write-only with `O_APPEND`, `O_CLOEXEC`,
and, when the flag is set, `O_CREAT` at mode 0640, owned by root, which is the
custody of charter section 7.
Append-only rides the open file description, verified: a duplicate of the
descriptor carries the flag, so the worker's copy appends wherever it
writes, which is what `weaver-trace-Spec` section 7 relies on from the far
side. The instrument is review, the verification being a fact about the
kernel read once rather than a property this crate's own suite can perturb.

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

## 6. The unit

**The init system is asked over its command-line interface, and the election
is argued.** Starting the worker is one invocation per load of the system's
own run tool, with the unit's properties declared on the invocation: the
agent's `User=`, the fixed template's hardening, and the runtime-directory
declaration the coordination socket is bound inside, per
`weaver-admin-systemd-contract` section 2. Stopping it is one invocation of the
stop verb, clearing a failed one is one invocation of the clear verb, and the same
interface answers the state query of section 3. The
alternative is a bus library, and it loses on the tree, for a handful of
invocations per lifecycle that are neither hot nor latency-bound.

**The runtime directory's mode is declared, because the default is not the
one this boundary wants.** systemd creates a `RuntimeDirectory=` at `0755`
absent an instruction, so every uid on the box may traverse to the agent's
sockets and the gate's own mode is left as the only thing between a stranger
and the front door. The invocation therefore carries
`RuntimeDirectoryMode=0750` **and `Group={identity}` beside `User={identity}`**:
the agent owns the directory, the operator reaches it through membership in the
agent's group, and no one else reaches it.

**The group is named rather than inherited, and the mode rests on that.**
Without it the init system takes whatever primary group the agent user was
provisioned with, so where that is shared - `users`, or `nogroup` - `0750`
grants traversal to every member of it and the sentence above is false. Naming
it also fixes the group the access rule is checked against at section 4's
inventory, so the two locks narrow one set rather than two.

**It carries a provisioning requirement**: an agent user created without a
group of its own makes the invocation fail to determine its credentials, and
the unit does not start.

**Validate catches that, and the reachability check below is where.** A box
carrying the agent user and not its group is refused there by name, the
missing group being a fact about a provisioned agent rather than a fact never
established. **A box carrying neither is not refused**, on the ground that
unresolvable is not unreachable: a declaration may not be refused on a fact
never established, and a bare checkout carries no agent group at all. The two
are different absences and the check answers them differently. Section 4
states the refusal in full and this sentence records only that the
requirement is enforced rather than owed.

The mode figure is stated here rather than inherited for the same reason the
gate states its socket's, per `weaver-gate-Spec` section 3 - a boundary whose
permissions come from an ambient default is a boundary nobody elected.

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
the agent's group, before any unit starts.

**It refuses as `BoundaryUnverified` and not as `ConfigInvalid`.** The declaration is
well formed and the fault is the box's: the operator wrote a uid that ought to reach the
socket and the provisioning has not put it in the agent's group. `ConfigInvalid` names
the declaration's TOML, the format `weaver-types-Spec` section 2 elects, and a deployer
reading it that way deletes the uid - which makes validate pass and breaks the connector
for good, the credential check then denying it at `accept` with nothing saying why. This
is the same fault an unprovisioned home or sink is, and it answers as they do, naming
the group on the way out. 

**A user without its group is named too.** `Group={identity}` is a hard
start-time requirement, so a box that provisioned the agent user and not its
group fails the unit start with an opaque credential error. That state is
checkable and is checked. A box carrying neither has provisioned no agent and
is not refused on a fact about an agent it does not have, which is what lets a
bare checkout and CI run this at all.

**`allowed-gids` is not judged and uid 0 is skipped.** A gid names no
particular peer, and whether one holding it reaches the socket turns on that
peer's own memberships rather than on the gid - a rule admitting an operator's
primary gid works where that operator is a supplementary member of the agent's
group, and an arm refusing every gid but the agent's own refused it. Root is
skipped for a different reason: `CAP_DAC_OVERRIDE` reaches a `0770` socket
whatever group it holds. Both halves rest on the credential check, which is
where they were always decided.

**The template may not take the boundary back.** `systemd-run` honours the
last assignment, so an installed hardening template naming a key this crate
elects would revert the boundary with no diagnostic - and this check's
premise, that the socket's group is `weaver-<agent>` and the worker runs as
the agent, would go with it, so admin would validate a rule against an
identity the worker does not hold. A template naming one has that property
dropped rather than emitted **and the drop announced on stderr**, an override
this crate cannot honour being a configuration it says no to rather than one
it silently wins, and a silent drop being the same failure in the other
direction.

**The set of keys is the closure of what the boundary rests on, not the keys
this crate writes**, and two of its members do their damage without looking
like an override. `SupplementaryGroups=` is **additive**: it grants gids
without displacing anything, so `User=` and `Group=` still read correctly
while the worker holds a gid the denial walk never computed - which reopens
the sink traversal along a second route once the first is closed. An empty
`RuntimeDirectory=` **resets the list rather than setting it**, so systemd
creates no runtime directory and the coordination socket has nowhere to bind,
which is a start failure rather than a boundary one and no less this crate's
to refuse. So the set is `User`, `Group`, `SupplementaryGroups`,
`DynamicUser`, `RuntimeDirectory`, and `RuntimeDirectoryMode`, derived from
what would have to hold rather than enumerated from what is set.

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

**No descriptor is declared on the invocation, and the negative is the point.**
The trace's sink is opened by this crate under root and crosses inside the enter
directive, per section 5 and section 7, so nothing about the record reaches the
unit's properties. An earlier form of this clause named a standard-output
declaration placing the trace's far end, written while that route was under
consideration and left standing after it was declined. It is struck:
`weaver-admin-systemd-contract` section 0 rejects the route because the unit's
standard output is inherited across fork and exec, which would hand every organ
the harness forks a writable handle to the agent's own record. One record path,
and it is the descriptor.

**What this crate relies on from the init system is the contract's and not this
election's.** `weaver-admin-systemd-contract` section 5 states the reliance set,
the identity holding from the first instruction, the sandbox in force before it,
unit-name uniqueness as the concurrency guard, and the cgroup's arrival and
removal with the unit. This Spec elects only how those asks are carried, so a
builder replacing the command line with a bus library would change this election
and breach nothing in that contract.

**The election's ground is no new dependency, stated exactly because a looser
ground was written first.** The clause above said a bus crate brings an async
runtime, and that is not true as written: `zbus` publishes a blocking API and
`dbus-rs` is synchronous over a C library. What holds is narrower and enough.
`zbus` carries async machinery into the resolved tree whatever its surface API,
which this crate's own manifest assertion forbids, and `dbus-rs` trades that for
a C library dependency in a binary that otherwise links none. The command line
costs neither, at a handful of invocations per lifecycle that are neither hot nor
latency-bound. What it costs instead is failure discrimination, named at the
contract's section 3 and not defended here.

**A failed dial is followed by a state ask, so a refusal names the right thing.**
The contract's section 3 records the measurement: a start ask can succeed over a
unit that never runs, so the dial's bound is what proves liveness and the bound
alone would report an absent residency where the truth is a unit that is not
running. Section 7's refusal therefore consults the unit's state before
returning.

**What that ask yields is a state and never a reason, and what the state
separates is narrower than it looks.** `weaver-admin-systemd-contract` section 3
says outcomes carry status and not the failure's cause, so this clause promises no
diagnostic, and why a unit failed is the manager's journal to answer where this
program does not read, per that contract's section 7. Measured 2026-08-05 against
a live manager, the activity value separates three cases and conflates several:
a unit whose process ran and exited non-zero reads `failed`, a running one reads
`active`, and `inactive` covers a unit that stopped cleanly, one that never
existed, and one whose exec never succeeded because its binary was absent. So the
refusal carries the value and claims nothing beyond it. A dial that timed out
over a unit reading `failed` refuses naming that state, and one over a unit
reading `inactive` refuses saying the worker is not running without asserting
which of the three reasons applies, because the boundary cannot tell them apart
and a refusal that guessed would be inventing a fact.

**The instrument is a test whose watch turns on the state it names.** A unit
started against a binary that exits non-zero reaches `failed`, and the test
watches the refusal carry that state rather than the absent residency, watched to
fail when the state ask is removed. The obvious test, a binary that does not
exist, is the one this clause must not use: that case reads `inactive` and would
pass whether or not the state ask ran, which is the never-failing perturbation
apex section 11 counts as worse than no test.

```graph
node: admin-failed-dial-consults-unit-state
kind: assertion
tag: perturbation

edge: asserts
from: weaver-admin
to: admin-failed-dial-consults-unit-state
```

```graph
node: admin-init-system-over-command-line
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-init-system-over-command-line
```

**The subprocess inherits nothing it was not deliberately given, because every
descriptor this crate holds is close-on-exec atomically at creation,** no descriptor
existing for an instant between its creating call and its flag. The one deliberate gift
is the member's own end of the first door's pair, per the operator's ruling of
2026-08-26: created atomically flagged like everything else and re-armed onto the
member's fixed number in the spawn path itself, so the inheritance is an act at one site
and never a default anywhere. This is the behavioural half of the custody section 1
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

**The unit template is fixed and the name is the one variable.** The template lives in
admin's own service configuration, per section 9, and the only value interpolated is the
validated agent name of section 4, so the delegated authority stays bounded by the
agent's root exactly as charter section 7 requires. **The argument vector the ask carries
takes no value the invocation's own input composes.** Its values are the coordination
socket path of section 7, which this crate already derives from that same validated
name, the two organ binary paths section 9 holds among the operator's installed values,
the SPU's being the agent's own, and, where the declaration carries one, the agent's
loop file as the named flag `--loop-file`, one token on both sides of the vector,
composed here and parsed by the worker under the same spelling, per `weaver-types-Spec`
section 2 and the operator's ruling of 2026-08-20 on issue #243. A worker that holds no
file-read loop refuses the flag at its own argument parse, named rather than ignored, so
a declaration the installation cannot honor fails the load loudly instead of standing as
a fact with no effect. The loop file is the vector's one declaration-sourced value and
it widens nothing: the declaration is the operator's file, validated at section 4's
inventory before any unit is asked, and the worker resolves the path under the agent's
own identity, so the value grants nothing the agent uid did not already have, per the
bare clause below. An absent member puts no flag on the vector, the worker's own default
standing, per `weaver-harness-PRD` section 2. A builder who let any of these values be
composed from the invocation's own input would be widening the delegated authority by
the route the name check closes, so the shape to hold is that the vector reads the
checked name and the agent's root, the installed values and the validated declaration,
and reads nothing else. An earlier form of this clause counted three values
and named the name the one variable, written before any declaration member rode the
vector.

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

**The state member is started here too, and it is not a unit.** The custodian
runs as a direct child of this crate rather than through the init system, per
`weaver-admin-PRD` section 2: the worker is asked of the manager because the
agent's identity and hardening are the unit template's, and the member's
custody is a territory this crate prepares rather than a sandbox the template
declares. Nothing of section 6's manager interface above reaches it, and this
crate holds no channel to it once it runs.

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
supplementary set narrowed to the member's own group, then the gids, then the uids, in
that order, because the two narrowings need the privilege the last one gives away. The
instrument drives `stand_state_member` itself inside a user namespace, where the test
runs as root over the invoking user's subordinate ids: a stand-in member beside the
worker records the identity it runs under into the territory the real path prepared, and
the kernel's status after exec must read every uid the member's, every gid its group's,
and the supplementary set that group alone. **It stood at review until then** on the
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
embedded store, and under the service engine the vector carries the store's socket, the
database, and the role beside it, per `weaver-state-Spec` section 2, the engine itself
first so the member knows which port to stand. The spelling is four flags ahead of the
positionals, `--engine`, `--store-socket`, `--database`, and `--role`, each followed by
its value, the first on every vector and the other three under the service engine alone,
so the territory and the preload path keep their places behind them. **The preload path
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
root it opens the chosen save point in section 9's `declaration-directory`,
`~/.weaveragent/<agent>/` by default, the one `restore` names or the latest published
where none is named, close-on-exec in the opening call, and places it at its fixed
number in the member's process at the spawn alone, the way it places the first door's
end, per `weaver-state-Spec` section 2. No path rides the vector, and the agent, whose
worker never holds the descriptor, never reaches the save point. No save point in the
directory is an absent descriptor and no refusal, the member standing empty and a
rebuild from the trace being the offline builder's, per `weaver-state-Spec` section 3.
**At every load and unload, this crate publishes**: as root, at the opening of a load's
validate step before the inventory selects what to restore, per section 3, and after the
member has stopped at an unload, it copies each finished save point from the member's
room into the declaration directory and never moves one, because a move keeps the
member's uid and the inventory's entry judgment would refuse the file as wrongly owned.
**Every source is a value the member's account chose, so it is judged before root reads
it**, per section 9: this crate holds the member's room open as a directory, opens each
entry beneath that descriptor without following links, requires a regular file owned by
the member's uid that carries the save-point format and a finished name, refuses and
leaves in place any entry that fails, naming it, and copies only from the descriptor it
judged, so a link or a planted file in the room cannot make a root step read a path the
member chose.
The copy is written under a temporary name, owned by the uid the root's `operator` key
names and mode `0600`, its owner and mode verified through the open file, then renamed
to a new name stamped with the run and sequence it covers, overwriting nothing; only
then is the member's copy removed, so a publication cut short leaves the member's copy
standing and is retried at the next verb; one an unclean stop left behind is
published at the next load. That directory stays `0700` to the operator, so a save
point on demand lands with the operator at the next verb, and a live restore of one the
member still holds reads the member's own room with no part of this crate. **This is
interior to this crate**: the verbs, their arguments and their answers are unchanged,
and the operator contract carries nothing new.

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
location, the member's territory outliving the worker unit, so the manager's
cleanup no longer takes a name whose peer is the operator's. The squat defect
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

**The stale preload name is the member's own to clear, and this crate removes
nothing.** The territory is the member's, the member unlinks before binding,
which keeps its bind clean, and this crate holds no reason to touch a
directory whose names it no longer watches, the pathname wait having retired.
What made an unconditional removal safe still holds and now guards the
member's own unlink: a second load of a live agent fails at section 4.1's
step 5, unit-name uniqueness being the concurrency guard
`weaver-admin-systemd-contract` section 5 relies on, and the member is
started only after that ask returns, so a name found standing belongs to a
load that has ended, the one-member rule of `weaver-state-PRD` section 4 held
by the guard rather than by a handshake at the name.

**The member retires itself and the init system reaps it.** `weaver-state-PRD`
section 4 has the process retiring with each unload while its holdings stand for
the next, and the mechanism is the first door's closure: the worker's channel
closes at unload and the member's serve loop ends with it. A load that fails
before the enter delivers the harness's end leaves that end with this crate,
whose invocation exits, closing it, and a load that fails after the delivery
ends the same way, section 5's rollback stopping the worker and the stop
closing the end the worker took: the member reads either closure as the
first door's end and retires, so an abandoned member is a bounded cost
rather than a resident one, on every path alike.

**This crate observes the child and does not reap it, and the distinction is
load-bearing.** The wait above reads the child's exit only to end early, so a
member that dies before binding does not cost the load the full bound. **Reaping
is the init system's by reparenting**: this crate is one invocation per verb,
per section 2, and exits when the verb answers, so the member outlives it and is
inherited by pid 1. That is the arrangement the residency needs rather than an
accident of it, a member reaped by its starter being a member that dies with the
verb that started it.

**So this crate takes no stop obligation** and section 3's rollback gains none,
on every exit path: what would be reaped retires on its own and is collected
where every orphan is.

**The unit declares no descriptor-bearing open, and the absence is the
assertion.** Under the
inversion the worker starts bare and builds its own coordination socket inside
the sandbox, per `weaver-harness-Spec` section 2.3, so nothing is placed into
the unit at start and no descriptor crosses the init system at all. The sink's
descriptor crosses later and elsewhere, inside the enter directive as ancillary
data over the connection admin dialed, per charter section 4.1 step 6. A
builder adding a socket declaration to this invocation would be reviving the
route the inversion retired, so what this clause asserts is that the start
invocation carries no descriptor-bearing property.

**Bare is a statement about descriptors and the argument vector does not
qualify it.** The distinction is worth holding because the two are easy to
read as one absence: a descriptor is a capability the manager would have to
hold and pass, and an argument is a value the worker reads and then resolves
for itself under its own identity. The worker opens what its arguments name, so
a path in the vector grants nothing the agent uid did not already have, which
is why the vector leaves the reliance set of
`weaver-admin-systemd-contract` section 5 untouched while a descriptor route
would not. The assertion below is unchanged by this act and is the one a
reviewer checks: the invocation carries no descriptor-bearing property, whatever
else it carries.

The earlier form of this clause named a listen-fds route, and the measurement
of 2026-08-05 is kept in the charter's section 10 rather than here: the
manager's own descriptor passing does deliver a caller-held end into a unit,
and the design no longer needs it.

```graph
node: admin-unit-declares-no-open
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-unit-declares-no-open
```

**Worker death is observed, not reported.** The channel's closure is the
observation, per `weaver-organ-channel` section 2, and the unit's status is
consulted through the same command-line interface for the report the log
carries. Admin repairs nothing on either, per the charter.

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
and the load's dial may arrive first.** The load starts the unit and then
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
would report an empty channel as a fault rather than waiting.

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
at inventory as `ConfigInvalid` naming `gate-instruction`, before any unit starts, which
is the taxonomy of `weaver-types-PRD` section 2.1 run over one more field. **The rule
reaches one field and not two.** An act earlier on 2026-08-24 extended it to
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

**The format is NDJSON, one act per line, and it shares no schema with the
trace.** The charter fixes the custody, 0640 in a 0750 directory, both owned
by root, one per agent at the path its root names, and never inside an agent home. What this Spec adds is
the rendering: one JSON object per line, the same reading tools the stream's
consumers already hold, and a
field set that is this crate's own and deliberately not the event envelope,
because a shared schema is how a second author drifts into the first's
record. The file opens with `O_APPEND` and `O_CLOEXEC` like every descriptor
this crate holds.

```graph
node: admin-log-ndjson-own-schema
kind: assertion
tag: review

edge: asserts
from: weaver-admin
to: admin-log-ndjson-own-schema
```

**What is logged is the charter's set.** Transitions directed and their outcomes,
refusals issued, rollbacks with what each act undid or could not, and units started and
stopped, a load's `ready` line naming the agent's SPU key and path per section 9. Never
a fact about what an agent did, per charter section 2: the moment a line describes
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
domain: the transitions it directed, the refusals it issued, and the units it
started and stopped.

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
`/etc/weaver/admin` where it is unset, and the agent's root is `<base>/<agent>/`. The
name is judged before the path is built, per section 4, so `.`, `..` and any name
carrying `/` never reach the base. **The root existing is the admission**: no root, or a
root that is not a directory, answers `NoSuchAgent`, and the look does not follow a link
at the root itself. **The root must be root's and closed to every other writer**, owned
by uid 0 and neither group- nor world-writable, or the invocation refuses
`BoundaryUnverified` before a value is read, since what the root names runs under the
agent's identity and a root another principal could write would hand that principal the
agent. **Every entry of the root is held closed the same way**: a regular file, never a
link, owned by uid 0 and writable by no group or other, or the invocation refuses
`BoundaryUnverified`, since the values name programs this invocation runs as root and a
directory's own mode stops neither a writable file inside it nor a link leading out.
**Every directory above the root is held closed too**, as sshd's StrictModes judges a
path. The root is resolved once to its canonical path. Each directory from its parent up
to `/` must be owned by uid 0 and writable by no group or other, unless its sticky bit is
set, which keeps another principal from renaming an entry it does not own. Otherwise the
invocation refuses `BoundaryUnverified`, naming the directory on stderr. A directory
another principal could write would let it rename the judged root away and stand its own
in its place between the judgment and the reads. Every read then goes through that
canonical root, never a pathname re-resolved after the judgment. **The deploy scripts
are bound by the same judgments before they commit**: `create-agent.sh`,
`bootstrap-stack.sh` and `verify-load.sh` apply this rule (`held_closed`), and any
judgment admin gains here is one they must apply before they provision, publish or run
a root, so that no script leaves a root admin then refuses or runs a program admin
would not. **The line those judgments hold is another principal's choice.** The
operator runs every script and holds root, so a value the operator supplies (the
environment, the arguments, the build, a directory under the operator's own home) is
the operator's own choice and not an escalation. A defect is a root step acting on a
value another principal can choose: an agent's account, its state member's account,
any other local user, or a root-owned path re-pointed through a directory such a
principal can write. Every such value is judged before the step, by this section's
rule, by admin's own answer, or by reading it as the principal that owns it. Every
value below is read from the root before any verb, and a value that fails to read fails
the invocation as `ConfigInvalid` with no field.

**The root holds one file per key.** Required: `worker-binary`, `spu-binary`,
`gate-binary`, `run-tool`, `control-tool`, `coordination-root`, `log-path`,
`declaration-directory` and `operator`. Optional: `unit-properties`, `headroom-bytes` and
`state-store-socket`, the last read under a service election alone, the service engine's
conventional directory standing where the file is silent. **The agent's declaration
stands in the operator's directory and not in the root**, on the operator's ruling of
2026-10-02: `declaration-directory` names it, absolute, `~/.weaveragent/<agent>/` by
convention, and it holds `agent.toml`, which this crate parses per `weaver-types-Spec`
section 2, beside the prompt file the declaration's `identity-file` names. A declaration
directory holding no `agent.toml` answers `NoSuchAgent`, as a root holding none did.
**`operator` names the operator's uid**, on the operator's ruling of 2026-10-02 on this
act's first question: a root-owned key `create-agent.sh` writes once, the box's own fact,
set by root, about whose data defines the agent, and independent of who invokes a verb. The
coordination name changed hands with the operator socket on 2026-08-05: the operator
places it, the harness binds it, and admin dials it, so one value reaches two crates and
the root is where they agree. These values are not the agent config and no seam carries
them, which is why the root takes no contract of its own. **The file and its values part
company at the start ask, and the distinction is worth holding.** This crate is the only
one that reads the root. Three of the values do not stay in it: the coordination
socket's name and the two organ binary paths reach the worker in section 6's argument
vector, over the external boundary `weaver-admin-systemd-contract` holds rather than
over any seam. What is fixed here is that these values exist, that they are the
operator's to place, and that none of them is discovered at runtime by searching.

**Nothing in one agent's root is read for another.** The box-wide values, the binaries,
the tools, the coordination root and the headroom, are copied into each root by the
deployment scripts from a stack record of their own that this crate never reads, so an
agent's configuration is that agent's even where two roots carry the same values. The
installed program files may stand once on disk and be named by every root; the
configuration and the processes are each agent's own. **The operations log is one per
agent**, at the path the agent's `log-path` names. Managing several agents, listing
them or holding a map across them, is not this crate's and belongs to WeaverWeb or a
separate application, which drives each agent through its own admin.

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
fails the invocation as an unreadable configuration fails it, before any unit is asked.
They are held distinct rather than keyed around because the stack below is keyed by
file name and `weaver-analysis` carries it into a run's code identity by those names.
No new refusal names any of them, the configuration's failure having no lifecycle case
to be.

**What the operator edits is read from the operator's directory, and what admin execs
stays in the root**, on the operator's ruling of 2026-10-02. The split follows what each
value is to a root process. The keys that name the programs this crate starts or hands
the worker to start, `worker-binary`, `spu-binary`, `gate-binary`, `run-tool` and
`control-tool`, are the agent's authority, so they stay where the admission is, in the
root, root-owned and judged as above. The declaration and the prompt file are data the
operator edits by hand, and a file under root's ownership taxed every edit with a
privileged write. **Their place is the operator's choice and the line above holds it**:
a directory under the operator's own home is a value the operator supplies, and the
defect would be another principal able to choose what admin reads there. So the
declaration directory is judged before any value in it is read, and the judgment is the
root's with the operator in root's place. The value of `declaration-directory` is
resolved once to its canonical path, the look not following a link at the directory
itself, and every read goes through the directory as opened at the judgment. **The
directory is closed to everyone but its owner**: its owner must be exactly the uid the
root's `operator` key names, and a directory any other uid owns, the agent's or the
member's account among them, refuses. **The operator is one person in the operator
role**, on the operator's ruling of 2026-10-02 on #59, named in three places that never
disagree. On the box it is the uid the root's `operator` key names, which owns the
agent's data. Under #50 it is the operator role on the agent's lifecycle socket. In
WeaverWeb it is that person's authenticated identity. WeaverWeb authenticates the person
and never chooses the box's operator uid, which is the box's to name and never taken
from a server ("the box decides", toddwbucy/WeaverTools#6). The directory grants no
permission to group or other and carries no access-control entry beyond its mode, so
neither of the agent's uids, whatever passage the territory's provisioning opened
through the directories above, can list it, enter it or read a file in it. **The entries
this crate reads are held closed**: `agent.toml` and the prompt file are each a regular
file and never a link, owned by that uid or by uid 0, and writable by no group or other.
Their read bits are not judged, the closed directory already denying every other
principal the path, so a file an editor writes under an ordinary umask passes. Other
entries are the operator's and are not read. **Every directory above it is held closed
as the root's ancestors are**, each owned by uid 0 or by the operator and writable by no
group or other unless its sticky bit is set, since a directory another principal could
write would let it rename the judged directory away. Any failure refuses
`BoundaryUnverified`, naming the path on stderr, before a value is read, and a
declaration directory that does not exist or is not a directory refuses the same way,
the operator's provisioning being incomplete rather than the agent absent. The deploy
scripts are bound by this judgment as by the root's, per the line above.

**Admin reads both files as root and the agent never reads either.** What crosses to the
agent's processes is what the parse yields and the enter carries, the prompt's text
seated as the decoder instruction's `identity` and the two digests, never a path into
the operator's directory and never a handle to it, the same discipline the sink's path
keeps. The ruling's reading of a system prompt is that one: the model receives the text
as its identity prefix at load, and that gives the agent process no read of the file
and no way to change it.

**An optional value is absent only where nothing stands at its path.** Every optional
value of this section, `unit-properties`, `headroom-bytes` and `state-store-socket`,
reads as absent only where the path itself names nothing, which is
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
adversary is whatever the run tool execs becoming an unintended holder of
the sink, a connection, or the log. The mechanism is atomic close-on-exec at
every creating call in this crate, no descriptor existing between creation
and flag. The test: spawn the subprocess, enumerate its descriptors, confirm
none of admin's crossed but the one the member's spawn deliberately arms,
the member's own end per section 6's vector clause, watched to fail when any
single atomic flag is downgraded to a later `fcntl`.

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
`ptrace` the worker, read and write its memory, replace the binary the unit
starts, or kill it, so a channel that admitted root adds nothing to what root
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
`cargo tree` assertion the floor Specs share. One binary and no library
surface, read from Cargo's target inventory by
`one_binary_and_no_library_surface`. The watch requires the named binary and
permits integration test targets alone beside it, as section 1 states. Explicit
and convention-discovered targets pass through the same check.

**Which invariant each claim serves, and why most serve none.** Twelve of the
forty-four carry a `grounds` edge and those twelve carry thirteen edges, one
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
nothing. **Thirty-two claims grounding in no invariant is the expected result and not
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
trace the harness authors, and the residency read stays the manager's three values
because telling a run at rest from a run serving is known to the party that conducts it
and not to the one that started the unit. The ten owings below are the same rule at the
document level.

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
would sit apart from the prose that earns it. Forty-four records in all,
fourteen tagged for review, twenty-four for perturbation, four for the
manifest, and two for a compile pin, the restore's judgment joining on
2026-09-06 and the member's own account bringing four on 2026-09-15, all four
watched since the spawn's drop moved from review to perturbation on 2026-09-26,
its instrument driving the spawn inside a user namespace. The residency record
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
below. **Two of the twenty-four perturbation records name no removal anywhere
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
and `admin-member-spawn-drops-to-its-account`. Each is named so this figure is
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
thirteen records having been added since by acts of their own, so a census taken
against this paragraph alone lands thirteen short of section 10's forty-four.

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
- The declaration directory is closed, per section 9 as of 2026-10-02: a directory
  granting group or other any permission, one carrying an access-control entry, one
  owned by any uid but the one the root's `operator` key names (the agent's and the
  member's among them), an `operator` key absent or naming no uid, an entry read that is a link or
  writable by group or other, and an ancestor another principal can write each refuse
  `BoundaryUnverified` before a value is read, confirmed by watching each pass and its
  declaration parse when its arm of the judgment is removed. The prompt file absent
  refuses `ConfigInvalid` naming `spu-instruction.decoder.identity-file`, and the
  enter's `identity_file` is the prompt's digest and not the declaration's, confirmed
  by watching the two digests come out equal when the declaration's bytes are
  digested in the prompt's place.
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
  a load then standing a member under this crate's identity.
- The store's first gate names the member: the walk asks its two questions as
  the member's uid and then as the agent's, confirmed by watching the first
  ask carry the account admin runs as, and by watching an in-process ask that
  names no uid at all, each leaving the pair no longer opening with the
  member.
- The territory is the member's: a prepared territory is `0700` and owned by
  the member's account, and a room widened between loads is closed again,
  confirmed by watching the mode read `0750` when the group-owned preparation
  is restored.
- The member's spawn drops to its account: `stand_state_member` run as root
  inside a user namespace stands a member whose every uid, every gid and whole
  supplementary set are its account's, confirmed by watching the member run as
  uid 0 when `become_member` is removed from the spawn's pre-exec, and carry
  root's group when `drop_to` is handed it beside the member's.
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
  why a load stands without either. - **The unit template's hardening set.** Which
  properties beyond `User=` the fixed template carries is the operator's policy surface,
  named in section 9's configuration and deliberately not enumerated by this Spec,
  because a hardening list frozen in a Spec is a security posture that cannot track its
  host. **The properties the sandbox must deliver are required and their directives are
  not**, per the operator's ruling of 2026-08-05 and `weaver-admin-PRD` section 7: no
  privilege escalation from inside, no reach into another principal's home, and a bound
  on what the agent may consume. The operator owns the posture the way an operator owns
  a firewall configuration. - **Whether the unit restricts the agent's address families
  to `AF_UNIX`.** Open with a stated cost rather than required, per the same ruling. It
  is **not** a restatement of the rule that no crate here exposes a network surface:
  that rule binds what these crates link, and this would bind what an agent's tools may
  reach, so an agent whose tools fetch anything would break under it. Settled by an
  operator who knows which tools their agents carry.