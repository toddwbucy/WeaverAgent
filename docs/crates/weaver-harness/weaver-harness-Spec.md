# weaver-harness - Spec

**Status:** MERGED. Cut 2026-08-02, fourth of the Spec pass and the first above the
floor. Code is written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-02
**Document ID:** `weaver-harness-Spec`
**Parent:** `weaver-harness-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #72

---

## 0. What this document is

Build instructions for `weaver-harness`: the module layout, the item signatures,
the channel mechanics this crate holds, the descriptor custody as code, and the
elections a builder would otherwise invent. It is derived from
`weaver-harness-PRD` and from the contracts this crate is party to, which that
charter's section 4 counts at eight: `weaver-admin-harness-contract`,
`weaver-harness-gate-contract`, `weaver-harness-spu-contract`,
`weaver-harness-spu-decode-contract`, `weaver-harness-spu-classify-contract`,
`weaver-harness-state-contract`, `weaver-harness-trace-contract`, and
`weaver-harness-diagnostic-contract`, together with `weaver-organ-channel`, the
drawn material the three organ-channel contracts share. **This document derives
from seven of the eight**, the label seam's representation being owed to the act
that builds the classify leg, which is why the count here and the citations
below differ by one.

Level discipline. The charter says what the crate needs and why. This document
says how it is represented, and per gate G2 it elects against grounds the charter
and the contracts state rather than developing grounds of its own. Where this
document and the charter disagree the charter yields nothing.

**This document declares its crate's assertion records, less five claims stated and
declared elsewhere, and no other record,** per Document Format sections 3 and 4 as of
the notation of 2026-08-03, which retired the no-records sentence this paragraph
replaces. The exception is the five replay claims this crate asserts and
`weaver-diagnostic-Spec` both states and declares, at its sections 3.3 and 4 and
sorted at its section 7. This document states none of the five, so it declares neither
their nodes nor their edges, and declaring them here would be the duplicate the format
forbids while dropping them silently would leave part of this crate's assertion set
with nothing recording where it went. **The rule admitting a second `asserts` edge is
owed to `WeaverTools-Document-Format`**, whose section 4 states no cardinality for
that edge, and issue #631 carries the debt. The disposition is argued at that Spec's
section 7 rather than here. The charter stays the source of this crate's node, its
parent edge, its two floor links, its one declared seam, and its artifact edges, and a
Spec that restated any of them would give the mapper two sources for one record, per
that format's section 1. What this document sources is the claims code must conform
to, declared at the clauses that argue them rather than gathered in one place, per
that format's section 6, and `asserts` runs from the crate rather than from this
document, which is why the document needs no node of its own.

**A claim this Spec leans on and another Spec argues carries no record here,** and
there are ten of them. Four are `weaver-types-Spec`'s: the `SOCK_SEQPACKET`
election of its section 4, the 64 kibibyte envelope bound that election carries,
the three exhaustive wire enums of its section 4.2, and loop 0's JSON encoding of
its section 4.3. Two are `weaver-traits-Spec`'s: the tool trait's absent safety
classification of its section 5, leaned on in section 6, and the provider trait's
dyn-compatibility of its section 6, leaned on in section 5. Three are
`weaver-trace-Spec`'s: the identity newtypes whose conversion that Spec's section
1 places at this crate's submit call, the envelope index its section 4 holds for
the assembly read, and the recorder reporting pressure rather than authoring it,
its section 6. One is `weaver-gate-Spec`'s, the parent-death signal's
thread-scoped guarantee of its section 2, leaned on in section 1. A node declared
twice is the one-name-two-nodes defect that format forbids, and a reliance stays
prose because a citation is what it is.

**It is written from the merged corpus alone,** per the ruling of 2026-08-01 that
keeps the old tree's Specs out of the Spec pass. Where a question of fact about
the old tree mattered, the fact is cited as a fact and decides nothing.

**What this Spec can settle is bounded by what is chartered, and the bound is not
a gap.** The token workflow and the tool workflow are unchartered, so the decode
exchanges, the turn ingress traffic, the per-model assembly layer, and tool
dispatch defer here with their settlers named, the same half-chartered discipline
the SPU and gate charters run on. What is fully specifiable today is the
lifecycle interior of loop 0, trace authorship, and the descriptor custody that
protects the record, which is the spine of the deliverable rather than its
leftovers.

## 1. The crate

**Layout.** One module per obligation, re-exported at the root.

    src/lib.rs         re-exports, and nothing else
    src/channel.rs     organ-channel I/O and descriptor custody, section 2
    src/spawn.rs       the organ fork's six calls, section 2.2
    src/lifecycle.rs   the harness type, the run state, the fan-out, section 3
    src/authorship.rs  trace authorship, section 4
    src/assembly.rs    prompt assembly's deterministic floor, section 5
    src/tools.rs       the granted tool result, section 6
    src/engine.rs      loop 1's seat, the extension seam, sections 6 and 6.1
    src/state.rs       the state seam's ask end, section 6
    src/replay.rs      the diagnostic replay loop, section 6.2
    src/failure.rs     the failure vocabulary, section 7
    src/record.rs      the run's record, one shape over two mechanisms, section 9

    src/bin/worker/main.rs    the composition root, framework, section 6
    src/bin/worker/dev_loop/  the loop body, the developer's, section 6

    src/bin/pyworker/main.rs     the Python-iterating worker, framework, section 6
    src/bin/pyworker/py_loop.rs  the connector at the crossing, framework, section 6
    src/bin/pyworker/dev_python/ the loop files, the developer's, section 6

Twelve library files and no placement among them, and two bin targets: the
demonstration worker, arriving with the wiring act, and the Python-iterating
worker, entering the tree 2026-08-19 behind the `pyworker` feature. **This
listing is re-derived from the tree rather than amended**, four modules having
landed without it moving - `spawn.rs`, `record.rs`, `failure.rs` and
`replay.rs` - and `src/tools.rs` having been listed as blocked while the
granted tool result of section 6 fills it.

**The loop files under `dev_python/` would owe conformance headers**, Python being
a supported unit since the operator's ruling of 2026-09-17 and the header rule
suspended until release since 2026-09-27, **and a header here would have no
assertion to cite.** The blade is a compile pin held in
`engine.rs`, the crossing is loop 0's and cited at `lifecycle.rs`, and the
filesystem boundary this section argues is cited from the framework side at
`src/bin/worker/main.rs`, which is where a reader establishes a claim about what
sits outside the developer's directories. A loop file is the builder's own code
arriving by the same path any builder's does, so what it shows is the seat it
composes against rather than a claim it holds. Minting an assertion to close the
reading would convert an honest absence into a documented enforcement. The
condition on which a header lands here is an assertion whose instrument is a
loop file, and until release the headers stand unmaintained and nothing reads
them, the census that read them having retired on 2026-09-27.

**`src/classify.rs` leaves the listing, and the disagreement that leaves behind
is named rather than settled here.** No such file exists and the label seam's
ask end is in the tree all the same: the arm stands on the run in
`lifecycle.rs`, its channel is `channel.rs`'s, and the port is `engine.rs`'s.
Section 6 elects the module by name and the tree elects three others, so one of
the two is behind and this section is not where that is decided. A listing says
what the tree holds and a placement election is section 6's to make, so the
listing names where the ask end is and the election stays at the clause that
argues it, for an act reading that clause against the code.

**The dev boundary is visible in the filesystem, per the architecture seat's
directive of 2026-08-13.** A directory the developer is meant to edit carries
the `dev_` prefix, underscore because Rust's module names take no hyphen, and
no directory without the prefix is theirs to touch. `dev_loop` holds the loop
body, and the bare decode loop lives there as the worked default, occupying
the exact location a replacement goes, so the example and the extension point
are the same place. Everything outside the `dev_` directories is framework,
and a newcomer listing the crate knows where their code goes before reading a
single document. The convention is general: a later builder-editable surface
takes the same prefix at its own directory, named when its act charters it.
**The placement is forced by the blade, not chosen for tidiness.** A bin
target compiles as its own crate linking this library, so the crate-private
constructor stays unreachable from `dev_loop` and the `E0624` pin of section
8 keeps its subject, where a module inside the library would see private
items and the blade would die silently. The boundary also carries the
crossing's whole option space: an alternative realization of `dev_loop`
composes against the same crossing without the framework moving, which is
the general form and the only form this document states.

**The general form has one realization in the tree, `pyworker`, per
issue #134.** Framework identical to the demonstration worker, same
arguments and same crossing, with the loop body marshalled into an embedded
interpreter by `py_loop`: the loop file is read and compiled per crossing,
so the builder iterates at conversation speed and freezes the result into
a Rust `dev_loop` to deploy. The loop files live under `dev_python`, the
developer's directory by the standing prefix. The file the crossing runs is
resolved in a fixed order, per charter section 2's ruling of 2026-08-20 on
issue #243: the `--loop-file` flag first, the declaration's member arriving
on the argument vector per `weaver-admin-Spec` section 6, then the name
`WEAVER_PY_LOOP` carries, the developer's bench override, then the deployed
default. The flag wins because the declaration is the agent's own fact and
the loop is a member of that agent's harness, unique to it, so an
environment leaking across agents cannot silently share what the
declarations separated. The binary and its interpreter dependency enter the manifest
behind the `pyworker` feature, the dependency optional and the bin target
carrying `required-features`, so no default build compiles or links an
interpreter and the deployed worker's build is untouched. The blade holds
by the same placement rule: a bin target links this library as an external
crate, the crate-private constructor stays unreachable, and Python reaches
only what the connector's proxy forwards from the granted seat, the proxy
dying with its crossing. Which binary the start step runs stays a provisioning
fact, and no document elects it.

```graph
node: harness-dev-boundary-in-the-filesystem
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-dev-boundary-in-the-filesystem
```

**Edition and toolchain.** Edition 2024 on the pinned nightly, no nightly
feature used.

**The dependency set is four internal crates and two external ones, and each is
argued.** **It became four when `weaver-diagnostic` was built**, that crate being the
mechanism this crate authors a diagnostic-trace through per the ruling of 2026-08-24,
and a mechanism this crate authors through is linked rather than dialed, which is the
ground `weaver-trace` already stands on. The count here is the manifest's and the
manifest holds four, so the record below reads against what `Cargo.toml` carries and
that file and the corpus's ruling now say the same thing. `weaver-traits` and
`weaver-types` are the floor links the charter declares, the first for the message
model the authoring path licenses, the second for
the wire vocabulary of loop 0 and the identity types the envelope carries.
`weaver-types` is taken **without its `config` feature**: this crate reads no field from
the agent config's file, so it links no parser. The SPU instruction and the gate
instruction are config sections this crate does consume, arriving over the coordination
seam inside the enter directive, already validated, which is the file read staying
admin's, and the readout election rides the first uninterpreted to the admit. The
elections the file carries beyond them, the permission mode and the tool set, are
consumed by no workflow chartered today, and section 9 holds the config read as the tool
workflow's question. **The featureless link rests on a placement owed to
`weaver-types-Spec`.** `SpuInstruction` and `GateInstruction` are config fields and wire
types at once, that Spec defines them in the module its `config` feature gates while
ruling the wire types unconditional, and this crate can construct a directive with the
feature off only if the two types sit on the unconditional side. The owed edit is filed
on the working list rather than made here, a correction to a merged document landing
after this branch rather than beside it. `weaver-trace` is the seam tagged `link`, the
recorder this crate authors through under a serving binding, and `weaver-diagnostic`
is the recorder it authors through under a diagnostic one, per apex section 6 as
ruled 2026-08-24, and it stands in the set on the same ground, the member being
built and the manifest declaring it.
**Its contract landed 2026-08-27 and its edge with it.** `weaver-trace`'s link is
recorded as a seam tagged `link` via `weaver-harness-trace-contract`, and the
diagnostic member's is recorded the same way via
`weaver-harness-diagnostic-contract`, declared in `weaver-harness-PRD` section 4
where the asking side carries it. **The graph and the manifest are both at four**,
having parted on purpose while the member was unbuilt: an edge is declared by a
document and a dependency by a manifest, so the act that wrote the papers moved the
first and the act that wrote the crate moved the second, and the second has landed.
**This section's list reads against the manifest**, which is the instrument that
checks it, and `tests/manifest.rs` asserts the four-crate set and cites this
section for it, so a list at three was the list being behind rather than the tree
being ahead. `serde_json` encodes and decodes the loop 0 envelopes, whose JSON
election is `weaver-types-Spec` section 4.3's. `nix` is the OS surface, elected in
section 2.4 where the grounds and the record live. **The set and the feature are two
records rather than one.** Gate H2 reads the internal edges against
the graph and reads no feature list, and the featureless take is a `Cargo.toml` fact of
its own, so a single record would hand one instrument's read to a claim it does not
cover. **The set grounds in the socket invariant and the feature grounds in nothing.**
Apex section 5.1 is the reason the two organs appear nowhere in this list: a crate this
one asks to do something is reached over a socket, so the internal set is the floor plus
the two recorders this crate authors through, each linked because the call crosses no
process line, which is what apex section 5.1's rule turns on. The harness does ask
each to record, which is why the record is a `seam` tagged `link` rather than a
`floor-link`. Which features a floor link is taken with is a build election that
would read the same under any invariant.

```graph
node: harness-internal-dependency-set
kind: assertion
tag: manifest

edge: asserts
from: weaver-harness
to: harness-internal-dependency-set

edge: grounds
from: harness-internal-dependency-set
to: axiom-floor-is-vocabulary-behavior-is-socket

node: harness-types-without-config
kind: assertion
tag: manifest

edge: asserts
from: weaver-harness
to: harness-types-without-config
```

**No async runtime, no logging, no HTTP.** The old tree's harness carried tokio
with every feature, `async-trait`, `tracing` with two subscriber crates, and an
outbound HTTP client, and none of it crosses, per apex section 7: nothing in
loop 0 or the trace seam awaits anything, the lifecycle interior is serial by
charter, and the executor question belongs to the token workflow with a latency
measurement in hand, per `weaver-traits-Spec` section 6. A logging crate would
be a second account beside the one the program exists to produce. The absence
is checked by the same build-time `cargo tree` assertion the floor Specs share.

```graph
node: harness-no-runtime-no-logging-no-http
kind: assertion
tag: manifest

edge: asserts
from: weaver-harness
to: harness-no-runtime-no-logging-no-http
```

**This crate spawns no thread.** The one auxiliary thread the merged set names
is the stream writer's, and it belongs to `weaver-trace`, per that Spec's
section 5. Everything this crate does in this pass runs on the caller's thread,
and the writer's existence is why the fork sites of section 2 state their
safety bound rather than assuming a single-threaded process. **The organ forks
run on a thread whose lifetime is the worker's, and the sentence is a
constraint rather than a description.** The gate's parent-death backing fires
on the forking thread's termination rather than the process's, per
`weaver-gate-Spec` section 2 as verified by both seats, so a later threading
change that moved the enter fan-out onto a short-lived thread would kill the
gate spuriously while the interior it guards is healthy. Today the posture
above satisfies the constraint by construction, and this sentence is what a
change to that posture must answer. **Both are review's by election and not by
impossibility.** A test enumerating this process's threads reaches the first and
a test comparing the forking thread against the serving one reaches the second,
so what holds them is that this suite buys neither rather than that no instrument
exists. The second is the constraint `weaver-gate-Spec` section 2 filed against
this document on the working list, stated here as a record rather than left as an
implication of the posture above. **The constraint grounds in apex section 5.5 and
the posture does not.** The parent-death backing is a timing guarantee the gate
relies on and cannot hold from inside its own domain, because which thread forks it
is a fact only the harness has, so keeping that guarantee true while the interior
runs is the loop's work by that section's division of answerability. That this crate
spawns no thread of its own is an interior posture no other organ's ordering reads.

```graph
node: harness-spawns-no-thread
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-spawns-no-thread

node: harness-organ-forks-on-worker-lifetime-thread
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-organ-forks-on-worker-lifetime-thread

edge: grounds
from: harness-organ-forks-on-worker-lifetime-thread
to: axiom-harness-integrates-by-the-loop
```

## 2. The channels, and custody as code

This crate is party to six socket seams and creates the pair behind four of
them. The two it does not create are the coordination seam, a named socket this
crate binds rather than a pair, and the state seam, whose pair admin creates at
the member's spawn and whose near end arrives here inside the enter directive,
per `weaver-harness-state-contract`. Party to and creates are different counts
and the charter's section 4 carries both. The mechanics below are one election
carried and the obligations the contracts land here, each with its contract
named.

### 2.1 The socket type, carried

**The pairs this crate creates are `SOCK_SEQPACKET`, carrying the election of
`weaver-types-Spec` section 4 rather than re-deciding it.** That Spec elects the
type for the organ channels and names this document as a landing site: the
boundary property of `weaver-organ-channel` section 2 comes from the socket
type, and of the four pairs this crate creates the residency and gate pairs are
the two that carry the organ envelope. The decode and classify pairs are not
organ channels, per `weaver-spu-PRD` sections 13.2 and 15.2, so what reaches
them is the sentence above rather than that Spec's naming. The election arrives
with its obligation. **The receive buffer is sized to the maximum envelope of
64 kibibytes, and a read that returns with `MSG_TRUNC` set is a channel fault
and never a message.** Verified against a live kernel rather than reasoned:
a short read on `SOCK_SEQPACKET` returns the truncated prefix with the flag set
and the remainder discarded, so an unchecked flag turns a long directive into a
silently shortened one, which is the failure the boundary property was elected
to prevent. The same bound governs this crate's sends: no envelope this crate
writes may exceed it, asserted at the write site, because a bound only the
receiver holds is a bound the sender discovers in production. **The election and
its bound are `weaver-types-Spec`'s records and the truncation test is this
crate's,** that Spec owing the test to the pair-creating crates and section 8
naming it, so what lands here is the obligation rather than the decision. **The
obligation grounds where the election does,** in apex section 5.1. Remove the rule
that a seam crossing a process line is a socket and this channel is not a bounded
datagram at all, so a flagged short read is nothing to rule on.

```graph
node: harness-truncation-is-a-fault
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-truncation-is-a-fault

edge: grounds
from: harness-truncation-is-a-fault
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**One write is one message and one message is one envelope.** The wire carries
`OrganEnvelope` as one JSON document per write, per `weaver-types-Spec` section
4.3, and the socket type is what keeps the framing out of this crate, per
`weaver-organ-channel` section 2. **This is the property the type was elected
to buy, and it takes a test of its own.** It is the boundary half of the pair
test `weaver-types-Spec` section 5 owes the pair-creating crates, whose
truncation half stands above: a substitution of `SOCK_STREAM` at the
`socketpair` call leaves every truncation test in this crate passing,
`MSG_TRUNC` handling being untouched by it, while the framing every contract
that draws these channels rests on is gone. So the consequence of the election
was tested here from the pass and the election's own reason was not, filed as
issue 35 and closed in one act across both pair-creating crates because the
property and its watch are the same on either side. Section 8 names the test
with its watch, and the election stays that Spec's record. **It grounds in the
same invariant as the truncation rule above,** framing being what the socket seam
buys and what every contract drawing these channels reads as given.

```graph
node: harness-one-write-is-one-read
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-one-write-is-one-read

edge: grounds
from: harness-one-write-is-one-read
to: axiom-floor-is-vocabulary-behavior-is-socket
```

### 2.2 Creation, and the atomic flag

**The coordination name's mode is elected in the creating call.** `bind` sets
none, so the socket would land at `0777 & ~umask` and this door's permissions
would be whatever umask the worker inherited - `0777` on one box and `0775` on
another from one build, on 2026-08-28. The bind happens under a umask denying
every bit outside the owner, so the name lands at `0700`.

**`0700` here where the gate's door is `0770`**, per `weaver-gate-Spec` section
3. `accept_root` admits `uid() == 0` and no other, so no group reaches this
door and a mode granting one would describe access it does not offer. The
runtime directory's `0750` narrows who may traverse to the name, and this
narrows who may connect to it. The credential check is the lock that decides and this
is the lock that keeps a stranger from arriving at it, which is the two-locks
reasoning `weaver-harness-PRD` section 5 applies to the trace descriptor.

**Elected rather than set on the path afterwards**, for the reason the gate
states: a mode set after the bind leaves the name live at the inherited mode
in between, races a path an unprivileged process may swap, and on failure
leaves a file behind. The umask is process-global, so the guard serializes on
it.

```graph
node: harness-coordination-door-states-its-mode
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-coordination-door-states-its-mode
```

**Both ends of every created pair carry close-on-exec from the creating act,
by `SOCK_CLOEXEC` in the `socketpair` call rather than by a later `fcntl`.**
This crate creates four pairs across a run, the residency and decode pairs in
one act before the SPU fork, the classify pair before the classify fork, and the
gate pair before the gate fork, and the rule is the same at each. It creates
three where the agent declares no classifier, that submodule being optional by
presence per `weaver-spu-PRD` section 15.3, and the rule does not change with
the count.
`weaver-harness-spu-contract` section 1 requires the harness's own end flagged
from the pair's creation. The atomic form is elected because the alternative
has a window: this process forks a subprocess per tool call, a fork between
creation and a separate `fcntl` would inherit an unflagged end, and an
inherited end of the residency seam hands the tool surface a release directive.
Verified: `socketpair` with `SOCK_CLOEXEC` yields both descriptors flagged with
no interval between them. **The atomicity grounds in apex section 5.1's second
authentication case.** A pair with no name is authenticated by possession of the
descriptor and by nothing else, so an end that crosses an exec is a credential
handed to whatever runs next, and the window a later `fcntl` opens is exactly
where it is handed over.

```graph
node: harness-atomic-cloexec-at-creation
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-atomic-cloexec-at-creation

edge: grounds
from: harness-atomic-cloexec-at-creation
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The child's ends cross the final exec by `dup2` from descriptor 3 upward
and an unconditional clear of the flag on each, because the duplicate law has
a corner.**
Close-on-exec would otherwise close the child's own end at the exec that
starts the organ binary. A duplicate made by `dup2` is born with the flag
clear, but only when the two descriptors differ: `dup2` onto the same number
is a no-op that returns the descriptor with its flag untouched, so a child
whose end already sat at descriptor 3 would keep the flag, lose the end at
`execve`, and start the organ with no channel, silently, on whatever layout
the deployment happens to produce. Both halves are verified rather than
recalled, the clear-on-copy law for differing descriptors and the
equal-descriptor no-op that defeats it, each run by both seats. So the child
duplicates each end it is given to a descriptor from 3 upward, the first
after the standard streams, clears the flag on each by `fcntl` whether or not
a duplication moved anything, and execs, so the organ binary finds its ends at
3 and, where it has a second, at 4 from its first instruction. **The order is
the channels' own:** the lifecycle channel every organ holds takes 3, and an
organ's further channel takes the next number, so the gate's single end sits
where the SPU's first does. Until the decoder cut of 2026-08-02 an organ held
one end and this paragraph named one number, and the SPU's second channel is
what made the order need stating. This realizes apex section 12's topology,
the numbering being this Spec's own election. **The numbering is owed to
`weaver-spu-Spec` and `weaver-gate-Spec`**, each of which inherits it rather
than re-deciding it, the same owing shape `weaver-types-Spec` section 4 used
to reach this document, and `weaver-spu-Spec` section 2 states the order from
the receiving side. **The placement and the corner's repair take separate
records.** The second walk's test of section 8 forks a child and enumerates its
descriptors, confirming the ends each organ is owed and where they sit, which
reaches the number and the order, and the equal-descriptor corner is not what
that test varies. So the unconditional clear is review's by election and not by
impossibility: a double placing an end at descriptor 3 before the handoff
produces the corner deterministically, which is the shape both seats ran to find
it, and this suite does not buy it. One record for the pair would claim the
walk's test for a half it does not reach, which is the overclaim this corpus
refuses in prose and has no reason to admit in a graph.

```graph
node: harness-organ-ends-from-descriptor-three
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-organ-ends-from-descriptor-three

node: harness-child-flag-clear-unconditional
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-child-flag-clear-unconditional
```

**Between fork and exec the child performs six calls, `prctl`, `getppid`, `_exit` where
the parent is gone, `dup2`, `fcntl`, and `execve`, and nothing else.** All six are
async-signal-safe, and the bound
is stated because the worker holds the writer's thread at every fork: a child
of a multithreaded process may safely run only async-signal-safe calls before
its exec, so the enumeration is the safety argument and not a style. An
earlier draft elected two calls and leaned on the duplicate law without its
corner, and the review caught that the tight bound and the silent corner were
one defect, so the middle call is the corner's repair made unconditional. **The
bound is review's by election,** a seccomp filter or a traced exec reaching the
call list between fork and exec, and this suite buying the walks of section 8
instead.

```graph
node: harness-fork-to-exec-bounded-calls
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-fork-to-exec-bounded-calls
```

**The organs die with the worker, so an orphaned SPU never holds the device**, on the
operator's ruling of 2026-10-03 on #50 that the agent leaves systemd, whose cgroup ended
every process of the unit when the unit stopped. The worker owns its organs now: the
child sets `PR_SET_PDEATHSIG` to `SIGKILL` as its first call, so the kernel kills the
SPU, the classify arm or the gate the instant the worker dies, cleanly or not, and then
compares `getppid` with the worker's pid read before the fork, calling `_exit` at once
where they differ, because a worker that died between the fork and the first call would
otherwise leave a child the signal was armed too late to reach. The gate re-arms its own
`SIGTERM` death signal after its exec, per `weaver-gate-Spec`, and the worker's death
still ends it. **The signal follows the thread that forked, not the process**, so the
worker forks its organs from its main thread, which lives for the process, and never
from a thread that may end while the worker serves. **The worker exits after it answers
left**, the run being its only purpose, so the run lock admin's `unload` waits on is
released by its exit together with the member's and the relay's, per `weaver-admin-Spec`
section 3. **Two descriptors cross the start step's exec into the worker and are the
worker's to keep**, at the fixed numbers of `weaver-admin-Spec` sections 3 and 6: the
run lock's open file description at descriptor 9, which it holds for its life and never
passes on, a description lock being released only when the last descriptor of that
description closes, and the write end of the trace relay's lifetime pipe at descriptor
8. **The worker marks both close-on-exec as its first act, before it starts any thread
or forks anything**, so no organ inherits a writable descriptor to root's lock file or a
write end that would keep the relay alive, and no window exists in which one could. The
organs never hold the run lock, being bound to the worker by their death signal instead.
**An empty number is skipped and said**: a worker started by hand, or by a stack from
before the start step, holds nothing at 9 and serves, and it names the absence on its
standard error, as it names a 9 holding something other than a regular file, so a start
step that failed to hand the lock across never serves silently. **The instruments are
perturbation**: an organ stand-in still running after the worker is killed with
`SIGKILL`, watched to fail when the death signal is not set, and the worker binary
started with a stand-in lock's description at 9 and a pipe's write end at 8, its
descriptor table read once it binds, from a user namespace the test owns so the
non-dumpable worker's table needs no root: the lock is held by the worker alone, both
numbers carry the close-on-exec flag, and both are released at its death. It is watched
to fail when the worker closes 9, and the lock frees, and when the first act is dropped,
and the flags are clear. The worker cannot open root's `0600` file anyway, so the watch
is on the descriptor it inherited.

```graph
node: harness-organs-die-with-the-worker
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-organs-die-with-the-worker
```

**The organ's argv carries the construction parameters its composition root is owed, and
its environment is the worker's.** The exec passes the worker's environment as `envp`,
on the operator's ruling of 2026-10-01: an organ starts as the agent is constituted,
never empty. The agent is the service and each organ one of its components, the worker's
environment is the one admin's start step composes for it, never the caller's, since
under sudo a caller's environment is not the agent's, and an organ exec'd with less is a
component the agent was never configured to be. The environment is read and its array
built in the parent before the fork, as the vector below is.

**The environment is the agent's, and argv stays the route for what an organ's
composition root is owed.** A parameter this crate hands an organ is named on the
vector at the call site, where the act that added it is the act that can be read.
A variable reaches an organ because the start step composed it for the agent, so
what the environment carries is the agent's and not a value this crate chose to pass.

**What travels here is a host's fact and not an agent's.** A number two agents
sharing this host cannot sensibly disagree about belongs on this vector, the
room to leave spare on a device being the standing case. A number that is a
property of the agent, its model's context ceiling or what one of its turns may
spend, reaches its organ in the declaration instead, per `weaver-spu-Spec`
section 8, because the declaration is per agent and this vector is per worker.
The distinction is worth stating because both routes end at the same composition
root and a parameter on the wrong one is not visibly wrong from inside it.

**The bound above is untouched by this and the reason is where the work happens.** Every
argument and every pair of the environment is a `CString` built in the parent before the
fork, the way the program name already is, so the vectors the child hands `execve` are
finished before the child exists. The child performs the same six calls. A parameter
that could only be assembled after the fork would not be expressible here, which is a
constraint on what may travel this way rather than a cost.

**Descriptor 2 is the worker's pipe, and the dying organ's last word
reaches the record through it.** The parent opens a pipe before the fork
and the child places the write end at descriptor 2 - one more `dup2`, the
call kind the six-call bound already enumerates, so the bound holds as
stated. The worker's end is a reader thread that tees every line to the
worker's own stderr, so the journal loses nothing it carries today, and
retains the last line that parses as JSON - the SPU already dies printing
a typed reason (`decode_fault_line`, per the fault ruling of 2026-08-26
that made every mid-run fault a death), and before this clause the record
held only that a death was observed while the reason died with the journal.
The observed death's account carries the retained line as `last_word`,
and the member is absent where no line parsed - an organ that never spoke
and one that spoke only garble both leave it absent, because the account
carries typed words and the journal carries everything, so the two facts
stay distinguishable where the tee put them rather than in the member.
The race at the very tail is narrowed and named: the author's read
settles on the reader's end-of-stream, bounded so an arm that never died
cannot hold the death hostage, and a read that hits the bound may miss
the final line, which costs the member and never the death.

**The write end travels clear of descriptor 2, and the parent is where it
is made to.** The placement is a `dup2` onto 2, and a `dup2` whose two
numbers are equal is defined as a no-op returning the number, so a write
end that already sits at 2 is never copied, keeps the close-on-exec flag
the pipe carries, and is closed by the exec: the organ loses the stream
its last word travels on, and the journal loses what it kept before this
clause. The corner is the pipe call's numbering rather than a race,
because the lowest free numbers are what it answers and a parent that has
closed its own stderr leaves 2 free. **The repair is the parent moving the
end and not the child clearing the flag**, for two reasons pointing the
same way: the child's six-call bound is left as it stands, and the same
corner would put the parent's own stderr on this pipe, where the tee
writes its own input back. This is the equal-descriptor corner the
placement above meets at descriptor 3, answered differently because the
two placements differ in who can move an end before the fork.

```graph
node: harness-death-carries-the-last-word
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-death-carries-the-last-word
```

**What may travel it is bounded by who can read a process's arguments.**
`/proc/<pid>/cmdline` carries mode 444 and any local reader sees it unless
`/proc` is mounted with `hidepid`, which is the deployment's option and not this
crate's. **The bound is drawn at the weaker guarantee on purpose**, because a
Spec that assumed the restriction would rest a boundary on a mount it does not
set, and a deployment without that option would then expose what the boundary
promised to hold. So a figure a deployment supplies may travel here and a
credential may not. Where an organ needs something secret the descriptor it
already holds is the route, per section 5.1's possession-as-authentication, and
this vector is not.

```graph
node: harness-organ-argv-carries-construction-parameters
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-organ-argv-carries-construction-parameters
```

### 2.3 The receive site, and adoption

**The trace descriptor enters this crate at exactly one call, and that call
asks for the flag itself.** The trace descriptor crosses once, as ancillary data on
the enter directive's own message, per `weaver-admin-harness-contract` section
3, so the one `recvmsg` site on the coordination channel carries
`MSG_CMSG_CLOEXEC`. Verified both ways: a descriptor received with the flag
arrives close-on-exec, and one received without it arrives clear, which is the
window `weaver-organ-channel` section 2 describes and the reason the obligation
is the receiver's. The site takes no flag argument and returns owned handles
the rest of the crate cannot construct another way, which is the pinned shape
`weaver-admin-harness-contract` section 5 names. This is the claim
`weaver-trace-Spec` section 10 owes this document, its test standing as the
first walk of section 8 and the owing discharged there. **It grounds in apex
section 5.2 rather than in the socket invariant.** The sink is a capability and
not a channel end, so possession authenticates nothing about it. A tool
subprocess holding it writes the record without being the harness, which is
the sole-authorship half of 5.2 defeated by a descriptor, and a second writer
does not merely add events, it adds events belonging to no turn the harness
can attribute them to. The state channel's end crosses the same receive since
the operator's ruling of 2026-08-26 and is the other kind of thing, a
possession-authenticated end whose capture would seat a second peer on the
member's seam, so the flag this walk asks for defends both facts at the one
site, each grounded in its own clause.

```graph
node: harness-trace-fd-cloexec-at-receive
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-trace-fd-cloexec-at-receive

edge: grounds
from: harness-trace-fd-cloexec-at-receive
to: axiom-join-key-travels-with-the-work
```

**The walk reads the kernel's lengths at glibc's width and names no other**, as of
2026-09-07. The control buffer's length on the message header and the length on each
control header are `size_t` on glibc Linux, which is where this crate runs, and
`socklen_t` on musl and most other libcs. The operator ruled on issue #481 that musl
is out of scope, so the site reads both lengths as the `usize` they are and carries
no width cast, and the gate of enforcement device five, which names a same-type cast
on this target, holds the line as written. A port to a libc where the field is
narrower is a change to this clause first, naming the width, and to the line second.

**The coordination socket is this crate's to create, and binding it is the
worker's first act.** Per the inversion ruling of 2026-08-05 and
`weaver-admin-harness-contract` section 2, any socket connecting to the harness
is an internal connection: the composition root creates a `SOCK_SEQPACKET`
socket with close-on-exec in the creating call, binds it to the per-agent name
inside the agent's runtime directory, and listens, before any directive can
arrive. It runs before the serving loop because an admin invocation dials
immediately after starting the worker and a name not yet bound is the race the
ordering exists to prevent, admin's bounded retry covering what remains.
`Harness::adopt` becomes `Harness::listen`, taking the bound listener rather
than a handed end, and the earlier declared-open route retires with the party
that placed it.

**The bind never unlinks an occupied name, and admin's start step is why it does not
have to.** A Unix
socket's pathname outlives the process that bound it, so a bind against a name a dead
worker left would fail. The directory this socket lives in now outlives the worker, the
init system that removed it having left the agent on 2026-10-03 (#50), so admin's start
step clears a name a dead worker left before it starts the next, which is safe because
it does so holding its invocation lock and the run lock it took, so no worker runs and
no other invocation can start one, per `weaver-admin-Spec` section 3. The worker
therefore meets no inherited name and has nothing to clear. **A bind that finds its name
occupied is a fault and never a thing to remove**, because the only ways a name is
occupied are that a live worker holds it, in which case unlinking would strand the
running agent, or that the start step did not clear it, in which case the program's
assumption is wrong and it should say so rather than repair. The instrument is review,
no test in this crate being able to produce a manager that misbehaves. **The seal is
another act and unlinks only the worker's own name** (`weaver-admin-Spec` section 3, the
seal; section 6 item 6): once the leave's outcome is fixed this crate unlinks the name it
bound itself, while the run lock is still held, so no other worker can hold that name
and nothing it unlinks belongs to another run; the rule above, about a name found
occupied before the bind, stands as written. After a seal the next start step finds no
name to clear, which it meets as it meets a clean directory.

```graph
node: harness-bind-never-unlinks
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-bind-never-unlinks

edge: grounds
from: harness-bind-never-unlinks
to: axiom-floor-is-vocabulary-behavior-is-socket
```

```graph
node: harness-binds-coordination-socket-first
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-binds-coordination-socket-first

edge: grounds
from: harness-binds-coordination-socket-first
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**Every accept reads the peer credential and refuses what is not root, before
any byte.** `accept4` carries close-on-exec in the accepting call, `SO_PEERCRED`
yields the dialing peer's uid, and a uid other than root closes the connection
unanswered. This is the check that makes the inversion worth its churn: the
socket is reachable from inside the sandbox by construction, so an elected tool
at the agent uid can dial it, and what refuses that tool is that it is not root.
The earlier design expected the agent's own uid at this check, which every tool
of the agent's satisfies, and leaned on a listener closed after one accept to do
the refusing. **The listener is not closed after one accept**, because admin is
per-invocation and every later verb dials again, so the closure is retired and
the check carries the property alone. One connection is served at a time, which
is what holds the contract's one-exchange-in-flight rule now that no fleet map
does.

```graph
node: harness-coordination-accept-refuses-non-root
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-coordination-accept-refuses-non-root

edge: grounds
from: harness-coordination-accept-refuses-non-root
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The worker's hygiene is performed as sets and not checks, and it survives the
inversion unchanged in substance.** The composition root clears the process's
dumpable flag, the attach defense `weaver-admin-harness-contract` section 2
states, and every descriptor this crate creates or accepts carries close-on-exec
from its creating call. Both are sets because a check that finds the flag wrong
and reports leaves the descriptor inheritable and the process attachable, which
is the set-not-check rule stated at the contract and applied here. What the
inversion removed is the set-again-after-the-last-exec ordering: the listener and
every accepted connection are created after the worker's last exec, so no handed
end exists whose flag an exec could have cleared. **The dumpable flag grounds in
apex section 5.1 and it is the less obvious of the two.** That invariant rests
possession-as-authentication on the claim that no third party can reach a socket
with no address, and `/proc/[pid]/fd` is an address for exactly such a socket, so
clearing the flag is what closes the one route the invariant's own argument
assumes shut, and it reaches the organ pairs whether or not the coordination
channel has a name.

```graph
node: harness-dumpable-flag-cleared
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-dumpable-flag-cleared

edge: grounds
from: harness-dumpable-flag-cleared
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**No path is taken anywhere in this crate.** There is no call that resolves, opens, or
stores a filesystem path to the trace, per `weaver-harness-PRD` section 5, and the organ
binaries of section 3 are the one exception, supplied by the composition root as a
construction parameter the way `weaver-trace-Spec` section 6 takes its queue depth: a
deployment fact the operator places in the agent's own admin configuration, per
`weaver-admin-Spec` section 9, never an election in the declaration and never a
discovery. **The three named shapes are pinned by the compile-fail doctests of section
8, and the general prohibition stays review's,** three doctests reaching the shapes they
name and not the open set of every way a path becomes a call argument. The pinning and
the prohibition are two records for that reason, per section 8, and neither claims the
other's instrument.

```graph
node: harness-no-path-taken
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-no-path-taken
```

### 2.4 The OS surface, elected

**The OS crate is `nix`, and the ground is that descriptor custody is this
crate's central obligation.** The mechanics above are `socketpair`, `recvmsg`
with control messages, `fcntl`, `fork`, `dup2`, `execve`, the dumpable
`prctl`, and `poll` as of the streaming ruling of 2026-08-11, and `nix`
covers that surface over the standard library's owned
descriptor types, so a descriptor's ownership is a compile property and a leak
is a type error rather than an integer left behind. Raw `libc` is the old
tree's answer and puts unsafe integer descriptors at the exact seam whose
custody discipline this crate exists to hold. `rustix` holds the same io-safety
posture and declines to offer process forking as a supported surface, and the
fork is not optional here: the topology of apex section 12 has this crate
forking both organ binaries. One crate covering the whole surface beats two
crates covering it between them, on the floor's own thinness doctrine. **The
election is a manifest property and the record is this crate's,**
`weaver-admin-Spec` section 1, `weaver-gate-Spec` section 1, and
`weaver-spu-Spec` section 1 each inheriting these grounds rather than re-arguing
them: the manifest carries one OS crate and no `libc` or `rustix` line of this
crate's own. The ownership the election buys is the other claim in this
paragraph and it is a type property, so it takes the compiler under section 8's
first sorting rather than the manifest read.

**`poll` joins the surface because the seam streams, and it only waits.** The
serve must hear the operator's stop while a generation streams token by
token, one process against more than one channel, and `poll` is the
primitive that sleeps against a list and wakes on the first readiness. The
reads stay the blocking reads they are, the service stays serial with one
exchange in flight, and the no-runtime election of section 1 stands
untouched: this is the syscall those runtimes wrap, not the runtime that
section refuses. A rotation of timed non-blocking peeks was weighed and
declined, a pause between peeks being latency added to every streamed token
and a walk that lengthens with every organ the body grows.

```graph
node: harness-os-surface-nix
kind: assertion
tag: manifest

edge: asserts
from: weaver-harness
to: harness-os-surface-nix

node: harness-descriptors-owned-types
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-descriptors-owned-types
```

## 3. The lifecycle interior

The harness type, the run state, and the fan-out of loop 0, per
`weaver-admin-harness-contract` section 3.

```rust
pub struct Harness { /* private */ }

pub struct OrganBinaries {
    pub spu: PathBuf,
    pub gate: PathBuf,
}

impl Harness {
    /// The crate's one constructor. Takes the coordination listener this
    /// crate bound and performs the hygiene of section 2.3.
    pub fn listen(coordination: OwnedFd, organs: OrganBinaries)
        -> Result<Self, AdoptionFault>

    /// Accepts connections and serves the directives on each until leave is
    /// answered, or fails on a fault below the exchange layer.
    pub fn serve(self) -> Result<Outcome, ChannelFault>
}
```

**Serving spans connections, and a connection closing is not an ending.** Admin
is per-invocation, so each verb arrives on its own connection and closes it when
the verb answers, and `weaver-admin-harness-contract` section 4 states the
consequence: the ordering is the worker's rather than any connection's, because
connections come and go with verbs and the run does not. So `serve` accepts,
serves the directives that arrive, observes the close, and accepts again, with
the channel state of section 3 held across the whole of it. **In the entered
state that wait widens**, per section 6.2: a standing run waits against the
gate and decode channels beside this listener, and the accept here is one arm
of it. Before enter and after leave the accept-and-serve reading above is the
whole of the wait.

**Leave is the one ordinary ending, and it is the only one that answers.** The
earlier form unwound a standing run when the channel closed, which was right
while the channel lived as long as the worker and is wrong now: a routine close
after every verb would tear down the run the next verb expects to find. **The
loss that reading defended against is gone rather than ignored**, because admin
no longer holds a channel whose closure could report its death. A loaded agent
with no admin attached is the ordinary resting state, waiting for work through
the gate, and an operator who never returns leaves an agent running, which is
what a supervisor that exits after each verb means.

**Service also ends on a fault, and the two endings take different halves of the
result.** A listener that fails, an envelope that does not decode, and a read
truncated against the envelope bound each end service through the error half,
unwinding a standing run on the way out. Leave is what ends it through the
answer half. So the outcome type carries the ordinary endings and there is
exactly one, which is why it has one case: a second would be a case nothing
constructs, and a value nobody can produce is the reserved slot apex section 7
forbids in data as firmly as in an interface.

**Construction fails only when a set fails, and a failed set refuses
construction.** A hygiene call that errors leaves the worker attachable or the
end inheritable, so `listen` returns the fault naming the set rather than
proceeding unset, and a `Harness` in hand means the hygiene held. The fault's
shape is a satellite of section 9. **The one constructor is the compile-fail
doctest of section 8 and the refusal is review's by election,** an absence being
what a runtime test structurally cannot demonstrate, and a test failing a hygiene
call on a closed descriptor reaching the refusal that this suite does not buy.

```graph
node: harness-one-constructor
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-harness
to: harness-one-constructor

node: harness-failed-set-refuses-construction
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-failed-set-refuses-construction
```

**The service is a serial loop and the channel state is a type.** One directive
at a time arrives, is judged against the channel's state, and is answered or
refused, per the ordering rules of `weaver-admin-harness-contract` section 4. A
directive out of order for the state answers `OutOfOrder` and is not queued.
The state's positions are before enter, entered, and left, the last
terminal, and the middle one carries the run and its sub-positions, at rest, turn
in flight and leave pending, per the leave below. **The positions and the refusal
take two records and two instruments,** the split `weaver-gate-Spec` section 6
takes on the same channel state. The positions are a type and take the
compiler, which pins that a directive out of order for the state reaches a
match arm rather than a flag check. What the arm then does is a behaviour and
takes a perturbation, an arm being free to queue or to answer the wrong refusal
while compiling exactly as well, so the pin alone would leave the refusal
claimed and unenforced. The refusal is owed to each organ by
`weaver-types-Spec` section 5, which states it and enforces it nowhere, and
this discharges this crate's side of that owing alone, section 8 naming the
test with its two watches. **Both halves ground in apex section 5.3.** An
ordering guarantee is one of the three things that invariant requires of a
contract, so the coordination channel's order is a stated guarantee rather than
a convention this crate inherited, and a state type holding it is that guarantee
made structural. The pin reaches the completeness half, every out-of-order case
arriving at an arm, and the perturbation reaches what the arm returns, which is
the contract's own named error. Grounding one and not the other would say the
invariant is a reason for the type and not for the behaviour, which is backwards.
**The refusal does not also ground in apex section 5.5, where `weaver-gate-Spec` section
2 and `weaver-spu-Spec` section 9 ground their own out-of-order records, and the
asymmetry is a reading rather than a dropped edge.** That second reason is an organ's: a
directive arrives in an order the organ did not choose, and refusing rather than
queueing returns a cross-domain timing failure to the party that invariant makes
answerable for it. This crate is that party. The same refusal here is one contract's
stated ordering held against the party that breached it, not two domains' orderings
reconciled, and 5.5 says nothing about what happens inside one domain. **The pin is
outside that reading**, its own twin at `weaver-gate-Spec` section 2 carrying the one
edge this crate carries, so there is no asymmetry there to explain.

```graph
node: harness-channel-state-three-positions
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-channel-state-three-positions

edge: grounds
from: harness-channel-state-three-positions
to: axiom-contract-is-a-complete-interface

node: harness-out-of-order-refused
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-out-of-order-refused

edge: grounds
from: harness-out-of-order-refused
to: axiom-contract-is-a-complete-interface
```

**The run state is the fan-out's progress held as data, which is what makes the
unwind total.**

**The `recorder` field below is the serving shape, and what holds either is settled
while the field still waits on the member existing.** `Recorder` is
`weaver-trace`'s concrete type, per `weaver-trace-Spec` section 5, and a diagnostic
run authors through `weaver-diagnostic` instead, per apex section 6 as ruled
2026-08-24. **A shared trait is refused** by `weaver-diagnostic-Spec` section 5,
which mirrors the serving surface and shares no type with it precisely so neither
crate depends on the other, so what holds either field is an enum over the two
concrete recorders. The struct is shown in its current form, which is the form a
serving binding builds, and **the field's type moves in the act that builds the
member**, there being no second type to name until the crate exists.

```rust
struct Run {
    recorder: Recorder,
    spu: Option<SpuChannels>,
    gate: Option<OrganChannel>,
    turn_in_flight: bool,
}

struct SpuChannels {
    lifecycle: OrganChannel,
    decode: DecodeChannel,
}
```

Each `Option` is an arm of the enter fan-out that has or has not stood up, so a
leave arriving after a refused enter unwinds exactly what stands, stopping the
gate where a gate was raised and releasing the SPU where a model was admitted,
and the compiler's match on the options is what makes a forgotten arm
unrepresentable rather than unlikely. **The SPU's arm is a pair of channels
rather than one, and they are one field because they stand up and fall
together.** The decoder cut of 2026-08-02 gave that organ a second socket, and
the two are created in one act and cross one fork, per
`weaver-harness-spu-decode-contract` section 1, so an option over the pair
keeps the arm's all-or-nothing shape where two options would admit a half-stood
arm the unwind would have to reason about. The decode end takes its own type
rather than `OrganChannel`, because `weaver-spu-PRD` section 13.2 rules that
socket not an organ channel and a shared name would carry the envelope's
assumptions onto a seam that does not take them. This is the mechanical form of the rule
that admin's unwind is a reap plus one directive: the directive works because the
harness knows what stands. **The options
ground in apex section 5.5 and the two SPU-shaped records beside them do not.** A
leave has to undo across domains what an enter built across them, and no organ can
see whether the other stood up, so either the loop holds that knowledge or the unwind
is a guess about a domain nobody is looking at. That is the reconciliation 5.5 places
with the loop by construction rather than by convention, and the run state is the
form the knowledge takes. The SPU's arm being one field and the decode end taking its
own type are shapes inside one organ's arm, which that section leaves to the domain.

```graph
node: harness-run-state-options-checked-unwind
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-run-state-options-checked-unwind

edge: grounds
from: harness-run-state-options-checked-unwind
to: axiom-harness-integrates-by-the-loop

node: harness-spu-channels-one-field
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-spu-channels-one-field

node: harness-decode-end-own-type
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-decode-end-own-type
```

**Enter runs four steps in the charter's order, and the answer is the aggregate.**
Receive the sink descriptor from the directive's own message and construct the recorder,
which stands the empty working structure up. The count is one, per the human's ruling of
2026-08-02: the coordination contract's plural was residue of the retired live view,
corrected to the singular in the same act as this sentence. **The receive and the
opening event named here are the serving mechanism's**, `weaver-trace-Spec` section
5's receive taking the one sink descriptor this Spec builds to and the `load` event
opening the run and originating its monotonic clock. **The diagnostic mechanism
presents the same receive and does not reuse that event kind**, per
`weaver-diagnostic-Spec` sections 5 and 3.2 as of 2026-08-27: the surface
mirrors, so the descriptor arrives the same way, and the kind set carries no
`load`, a replay's bracket being its own pair and its clock originating at the
opening event that pair names. So this paragraph's citation of the serving surface
reaches the receive under either kind and the event kind under one. What holds under
either is the count, the descriptor, and that a run opens with an authored event.

**Which recorder it is follows the kind, and nothing else in the walk does.**
Both kinds receive a descriptor and author a record into it, per
`weaver-agent-PRD` section 6 as ruled 2026-08-24, and the kind selects the
mechanism: `weaver-trace`'s recorder under a serving binding and
`weaver-diagnostic`'s under a diagnostic one. This crate is the sole writer
either way and the selection is made once, here, from the payload's binding.
Every later sentence of this Spec that authors an event reads against
whichever mechanism the kind selected, which is stated once rather than
repeated at each site.

**An act earlier on 2026-08-24 had this walk skipping the receive and the
authorship on a diagnostic binding**, on the reading that such a run wrote
nothing at all, and the operator's ruling of the same date replaced that
reading with the composition above. The assertion that carried it,
`harness-diagnostic-authors-nothing`, is retired rather than restated, its
claim having been that no event is authored and the claim now being false.
Retiring it removes a record from the graph, which is named here so a census
comparison against the previous build finds the reason rather than a gap.
Create the residency pair and the decode pair in one act, per
`weaver-harness-spu-decode-contract` section 1, fork the SPU binary carrying
both ends, and open the admit exchange on the lifecycle pair carrying the
SPU instruction uninterpreted. The decode socket is created here rather than at
first use because it crosses the same fork, and a socket the child was not
given at its exec cannot be handed to it afterward. Create the gate pair only after the
SPU's answer has confirmed residency, per `weaver-harness-gate-contract`
section 1, then fork the gate binary and open the raise exchange carrying the
gate instruction uninterpreted **and the socket this crate supplies beside
it**, the gate last so no work arrives before the interior serves, per apex
section 6 as amended 2026-08-24.

**On a diagnostic binding the gate step does not exist.** The match on `EnterBinding` is
where the fan-out learns its extent: the serving case carries the gate instruction and
the walk above is whole, and the diagnostic case ends the fan-out at the SPU's
confirmation, no gate pair created, no gate binary forked, no raise opened. The three
no-clauses name the gate step and nothing wider: the classify arm is interior, elected
by the instruction that carries it rather than by the kind, and stands on a diagnostic
load exactly as it would on a serving one. **The kind excludes Gate alone and selects
the record's mechanism**, per apex section 6 as ruled 2026-08-24, and a reviewer reading
the no-clauses as covering every fork read them wider than they bind. An act earlier
that date had the kind excluding the record too, which the ruling replaced: a diagnostic
binding authors a diagnostic-trace rather than nothing, so there is one exclusion here
and not two. Ready aggregates over the steps the kind declared, per the contract's
section 3. The run state's gate arm stays empty for the whole
residency, which is what keeps the leave and the unwind honest without a second rule:
both lower what stands, reading the same options the checked unwind above already reads,
and an arm that never stood is an arm they never touch.

```graph
node: harness-diagnostic-enter-forks-no-gate
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-diagnostic-enter-forks-no-gate
```

**The gate's socket is a sibling of the coordination socket, and the derivation is the
whole of this crate's part in it.** Per `weaver-gate-PRD` section 2 the pathname is the
program's rather than the operator's, and this crate is the party positioned to know it:
the coordination socket it bound as its first act sits in the agent's runtime directory,
so the gate's name is that directory and a fixed leaf. **What that buys is admin's start
step rather than a rule this crate enforces.** The start step clears a dead worker's
names holding its invocation lock and the run lock it took, before the next worker
starts, per `weaver-admin-Spec` section 6, so the next bind meets no stale name. A
derivation reaching anywhere else would need a cleanup this program has already refused
to write, an unlink racing a live successor.

The listener therefore retains the path it bound, which is the one thing this
crate holds a path for beyond the organ binaries, and the exception is argued
the same way: it is a deployment fact supplied at construction rather than a
name resolved at use. The wait is what the run state's
invariant rests on: `gate` is set only ever after a confirmed `spu`, so the
unwind's reverse order is a property of construction order rather than of
timing. Ready is answered when the last arm confirms. A refusing
arm's reason is wrapped `OrganRefused` and carried into the aggregate
unchanged, per `weaver-admin-harness-contract` section 6, and the scoped
account holds: a refusal before the `load` event leaves the stream clean and
the state at before-enter, and a refusal after it leaves the authored bracket
standing and the run in place for the leave that unwinds it. **Three of this
paragraph's four records are review's by election.** A test reading the
ancillary descriptors on an enter directive for what each is for reaches the
sink's count, per the discrimination the ruling of 2026-08-26 asks below, a test
exec'ing a probe in the SPU's place reaches the decode pair's crossing of the
same fork, and a test holding a double SPU's answer reaches the gate pair's wait
on confirmed residency, and this suite buys none of the three. The scoped
account is the fourth and section 8 names its test. **Two of the four ground in
an invariant and two do not.** The decode pair's creation before the fork is
apex section 5.1's second authentication case read as a construction rule: a
socket with no address cannot be reached later by resolving one, so the only
moment it can reach a child is before that child exists, and the sentence above
about a socket the child was not given is that invariant and not a POSIX
accident. **The gate pair's wait grounds in apex section 5.5.** One organ's
confirmed readiness gates another organ's construction, which is two organs'
orderings reconciled against each other, and neither can perform that reconciliation
because neither can see the other's domain. The reverse unwind order the paragraph
below opens with is the same fact read backwards and is argued here rather than
there. The sink's count and the scoped account are a count and an ordering of this
crate's interior, which no invariant is about. **The sink's count is one under
either kind**, both kinds receiving a sink and authoring into it, and since
the operator's ruling of 2026-08-26 the enter's ancillary payload is not that
count alone: the state channel's end rides beside the sink where the member
stands, so the test that reaches this claim discriminates by what a
descriptor is for rather than counting the payload, one sink always and one
state end where the leg stands, and the node below states the claim its name
always stated, the sink's. An act earlier on 2026-08-24 made the count follow the
kind, on a reading the ruling of the same date replaced.

```graph
node: harness-one-sink-descriptor
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-one-sink-descriptor

node: harness-decode-pair-created-before-the-fork
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-decode-pair-created-before-the-fork

edge: grounds
from: harness-decode-pair-created-before-the-fork
to: axiom-floor-is-vocabulary-behavior-is-socket

node: harness-gate-pair-waits-on-residency
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-gate-pair-waits-on-residency

edge: grounds
from: harness-gate-pair-waits-on-residency
to: axiom-harness-integrates-by-the-loop

node: harness-scoped-refusal-account
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-scoped-refusal-account
```

**Leave runs the reverse order and drains before it answers.** The entered position
has three sub-positions, at rest, turn in flight and leave pending, per the lifecycle
state table approved by the operator on 2026-10-09, recorded on #1, whose text is
`weaver-admin-Spec` section 3 and whose rows section 6 implements and cites. A leave is
heard at rest or mid-turn and is never refused for a turn in flight: it becomes the
pending leave, and from there it quiesces the gate where one stands, the run state's
arm answering whether it does, lets the turn finish or cancels it, runs the wind-down,
lowers the gate, takes the leave's save point, releases the SPU and the member,
authors the `unload` event with the leave directive's cause and the release outcome,
the terminal event, and drains the writer's queue, each step as section 6 states it. Left is answered only after the drain returns, which is what makes
the answer mean what `weaver-admin-harness-contract` section 4 says it means, that
everything admitted reached the stream. **The ordering is review's by election,** a
double sink that drains slowly reaching it, which is the shape the gate's
ready-follows-bind test takes for its own ordering, and this suite not buying one.

```graph
node: harness-left-follows-drain
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-left-follows-drain
```

**Stop answers after the record holds the close.** The stop directive aborts the turn in
flight, the turn's close event is placed with the stop reason and the stop directive's
cause, and only then does the answer carry `TurnAborted`, the announce-after-record
discipline of `weaver-admin-harness-contract` section 3. A stop at rest answers
`AtRest`, a clean close and not a refusal. How the abort lands at the decoder is
deferred with the decode seam, per section 8, and the trace semantics are settled either
way, which is what `basic-inference-loop` section 7 already records.

**Observe answers from any position and authors nothing**, per
`weaver-admin-harness-contract` section 3 as of 2026-09-04. Before an enter the answer
is `Unloaded` with no load. Entered, it is `Idle` where no turn key stands in flight and
`Active` where one does, with `LoadFacts` beside it read from the run: the session and
run, the declaration's digest the enter carried, the artifact, the elections, the store
and whether the member's end arrived, and the composer `serve` was handed, the same
facts the load event was authored from and held on the run for this exchange. **While a
leave is pending the answer is `InTransition`**, whether a turn runs or not, so in S4 to
S6 and S10 `show` answers the same whether the unload's invocation still holds the lock
or has died (`weaver-admin-Spec` section 3, S4 to S7 and S10 x `show`; the operator's
ruling of 2026-10-09). An `Observe` reaches a pending leave only where its holder has died, outside the envelope, and is answered `InTransition`. After a
leave the answer is `Unloaded` with no load, the position being terminal and the
observation not an act, so it is the one directive the left position answers rather than
refuses. No event is authored and no bracket touched: an observation mid-turn reads the
turn key and the held facts and disturbs nothing. **The state the observation answers is
true of the run.** Since 2026-09-05 the turn key stands on the run from the bracket's
open until its close lands, set by the seat where the key is minted and cleared where
the close is authored, on the clean path and on every error path alike, so `Active` is
the run's own fact for exactly the turn's extent and a close that never landed leaves
the key standing for the unwind to name. **And any position is any position, a running
turn included.** The serve loop of section 3 takes this channel's directives between
turns, and the engine's polls take them while a turn runs, between tokens on the
stream and through an invocation per section 6.2, where a stop cancels the
generation or the execution and answers `TurnAborted` after the record and,
since 2026-09-05, an observation answers `Active` with the load's facts lent to the
seat, touching no bracket. The one bound that stands is the token: a directive arriving
inside a single token's decode waits for that token, which is the seam's grain and not a
gap. What issue #441 recorded as a threading fact was two smaller ones, the key never
set and the observation refused out of order between tokens, and both close with this
act.

```graph
node: harness-announce-after-record
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-announce-after-record
```

**Pressure is asked for rather than delivered, as of 2026-08-22.** It
arrived as a `Failure` the recorder returned, which this crate could not act
on without also treating a recorded event as a lost one, the variant being
returned after the event had landed. `weaver-trace-Spec` moved the depth to
the recorder's own surface, so **this crate reads it after authoring rather
than receiving it in place of a sequence.** The obligation below is
unchanged: pressure reaches the operator as a `fault` naming
`RecorderCommitPressure`, and it is this crate that authors it.

**The reading is per authored event and the fault is not.** A fault for
every submission above the mark would answer a full queue by filling it,
which is the one direction that cannot help, so the report is authored when
the mark is first crossed and not again until the depth has fallen back
under it. **A pressure report is a report about a condition rather than
about an event**, and a condition that persists is one condition.

**A fault the worker survives is authored, not signalled.** The pressure and
failure reports the recorder surfaces, and the organ deaths observed through
closure after the enter aggregate, reach the operator as the `fault` event on
the stream, per the fault-carrier ruling of 2026-08-01. No run blocks on
anything downstream of the emission. The payload is the floor's
`fault-report`, carried unchanged from the reporting organ and authored
without translation, per `weaver-harness-trace-contract` section 3, and this
crate's own three sources are enumerated at `weaver-harness-PRD` section 5.
The gating an earlier draft of this Spec described lifted when that shape
landed on 2026-08-02. **The two records this paragraph carries are review's by
election,** the never-blocks sentence being section 4's. A test driving a double
organ's fault report to the stream reaches the authoring and the payload alike,
comparing the reported bytes against the authored ones, and this suite buys the
trace side of that pair instead, where `weaver-trace-Spec` section 6 asserts
that the recorder reports pressure and does not author it. **The authoring
grounds in apex section 5.2 and the payload does not.** That invariant's
sole-writer half is this paragraph's rule stated at the apex, a component
reports and the harness authors the event, and a fault an organ signalled for
itself would be a record entry no turn key attributes to anything. What the
event then carries is trace content, and content is where the invariants stop.

```graph
node: harness-faults-authored-as-events
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-faults-authored-as-events

edge: grounds
from: harness-faults-authored-as-events
to: axiom-join-key-travels-with-the-work

node: harness-fault-payload-carried-unchanged
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-fault-payload-carried-unchanged
```

**Loop 0 takes neither a type nor a trait, and the cell closes here.** The question
was held open for this pass, demand-derived rather than reserved. The demand does not
exist: the loop is
the interval between two directives, its state is the `Run` struct above, and
its control flow is the serial service, so an abstraction would have no second
implementor and no caller that varies, which is the reserved slot apex section
9 forbids. The inference loop inside it may yet demand one, and that demand
arrives with the token workflow, which may reopen this with the engine's shape
in hand. **The absence is review's by election,** the named-candidate doctest
the floor Specs buy for absences reaching a trait this Spec could name, and the
claim here being that no demand exists rather than that one named item is
missing, which is the open set a doctest does not close.

```graph
node: harness-loop-zero-takes-no-abstraction
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-loop-zero-takes-no-abstraction
```

## 4. Trace authorship

The authoring half of `weaver-harness-trace-contract`, landed as one module
with one submit path.

**Identity converts at the submit call, and nowhere else.** The envelope
identifies the session, run, and turn as `weaver-trace`'s opaque newtypes, and
this crate holds the floor's `SessionId` and `TurnKey`, so the conversion the
no-dependency rule of `weaver-trace-Spec` section 1 forces is a total function
at the one call site that submits, exactly as that Spec places it, which is why
the record for the conversion sits there and not here: one claim is one node, and
that Spec elects the newtypes and places the conversion in the same clause.

**Both timestamps are stamped at authoring, from the standard library's two
clocks.** Wall-clock milliseconds from the system clock, and the monotonic
reading as nanoseconds elapsed since the run's origin, an instant captured
when the `load` event is authored, per `weaver-harness-trace-contract` section
3. No OS crate is needed for either, and the recorder's clock is never
consulted because the contract denies it the fields. **The stamping site is
review's by election,** a double recorder comparing an event's monotonic reading
against the run's origin reaching it, and this suite buying the trace side's
canonical-form tests instead. **The site grounds in apex section 5.2 and the
choice of clocks does not.** The monotonic reading is nanoseconds since the run's
origin, an origin only the author holds, so a component stamping its own report
would carry a reading placeable in no run, which is the attribution that
invariant's sole-writer half exists to buy. Which two clocks supply the numbers
is representation and would read the same under any invariant.

```graph
node: harness-timestamps-stamped-at-authoring
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-timestamps-stamped-at-authoring

edge: grounds
from: harness-timestamps-stamped-at-authoring
to: axiom-join-key-travels-with-the-work
```

**The licensed combinations are enforced here, before submit.** A message is
judged against the licensing rule of `weaver-traits-Spec` section 3, a `User`
message carrying only `Text`, an `Assistant` message carrying `Text` and
`ToolCall`, a `ToolResult` message carrying only `ToolResult` blocks, and an
unlicensed message is refused by this crate and never submitted. The recorder
cannot hold this rule, per that Spec, so the harness is the party the
perturbation test of `weaver-traits-Spec` section 7 binds, and the test lands
in this document's section 8 set. That Spec names this document as the declaring
side, and the record below discharges the owing.

```graph
node: harness-licensed-combinations-refused-before-submit
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-licensed-combinations-refused-before-submit
```

**A refused submission is handled as the contract orders.** It is not treated
as recorded, not projected, and not retried under a new sequence, per
`weaver-harness-trace-contract` section 3. A refusal on the authoring path is
a defect in the author, and it surfaces as a fault rather than a retry. **The
handling is review's by election,** a double recorder refusing one submission
reaching it and the trace side's gapless-sequence test watching the same
property from the recorder's end, which is the test this program buys.

```graph
node: harness-refused-submission-not-retried
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-refused-submission-not-retried
```

**Pressure becomes an event, authored by this crate.** The harness reads the
recorder's queue depth after authoring and, on the crossing of the high-water
mark, authors the `fault` event naming `FaultCase::RecorderCommitPressure`,
per `weaver-trace-Spec` section 6, carrying the floor's `fault-report` as
section 3 states. **The terminal `unload` is the one exception**: the leave reads the
depth before it authors `unload` and reports any pressure then, and once `unload` is
authored no event follows, the check after it skipped (`weaver-admin-Spec` section 3,
I9; section 6 item 6). **The depth is read rather than returned** as of
2026-08-22: it arrived as a `Failure` the recorder handed back after the
event had already landed, so acting on it meant treating a recorded event as
a lost one.
Nothing on any turn path waits on the sink, per `weaver-harness-PRD` section
5, and the working structure's return is the acknowledgment the interior
proceeds on. The authoring itself is section 3's record and is not restated
here. **The never-waits property is review's by election,** a double sink that
never drains reaching it while a turn proceeds, and this suite buying the
back-pressure measurement section 9 defers instead.

```graph
node: harness-nothing-waits-on-the-sink
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-nothing-waits-on-the-sink
```

## 5. Prompt assembly's deterministic floor

**Assembly reads the message kinds and nothing else, and the discipline is a
kind filter at one site.** The harness assembles a prompt by iterating the
working structure in sequence order, selecting on the lifted `kind` the index
of `weaver-trace-Spec` section 4 holds for exactly this read, taking the three
message kinds, and decoding their payloads. The measurement, lifecycle, and
custody events never enter a prompt because the assembly path cannot see them:
the filter is the kind set at the read site, not a judgment applied after a
full read. This is
the seam `weaver-harness-PRD` section 2 names as what a later recall feature
would breach, held today by one match a reviewer can find and by the fourth
walk's test of section 8.

```graph
node: harness-assembly-kind-filter-at-read-site
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-assembly-kind-filter-at-read-site
```

**The per-model layer is deferred with the token workflow, and the floor
beneath it is fixed.** What phrasing elicits a tool call from a given decoder
is re-erected per model, per the charter, and no model is a dependency of this
crate. What does not vary and is fixed now is the order of parts, the identity
prefix, then the message sequence, then the tool schemas, per apex section 3
step 4, and the property that assembly is deterministic over the working
structure's contents: the same records assemble the same prompt, byte for
byte, which is what makes a replayed run's prompts comparable at all. **The two
claims take two instruments and both are bought.** Determinism is section 8's
perturbation bullet. The order of parts was review's by election, a test
reading the three parts out of an assembled prompt named as what would reach it
and declined, and `tests/authorship.rs` holds that test: it assembles a prompt
from an identity, one message, and one schema, and asserts the three appear in
that order in the rendered string. **So the election is retired rather than
argued down.** `review` says an instrument was not bought and never that none
exists, so a claim whose own named instrument is in the tree takes the
instrument's tag, and this one is section 8's perturbation bullet beside
determinism. The two stay two records because the byte-for-byte comparison
holds whatever order landed and cannot see the order move, which is the reason
the order was given a claim of its own. **Assembly reads the identity prefix and
never its source**: where the prefix comes from is decided at one site, section
6.1's open, and named there.

```graph
node: harness-prompt-part-order
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-prompt-part-order

node: harness-deterministic-assembly
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-deterministic-assembly
```

**The provider is injected, and this crate names no wire format.** Decode
requests leave through `provider-trait`, constructed at the worker composition
root, per `weaver-traits-Spec` section 6, and every deferred decode shape in
this document defers to the same place that trait's signature does, the token
workflow. **The absence of a wire format here is review's by election,** a
manifest read reaching a named HTTP or provider client and section 8's own
manifest bullet buying exactly that, while what this clause claims beyond it is
that no format is named in source either, which the manifest does not see and a
grep at review does.

```graph
node: harness-provider-injected-no-wire-format
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-provider-injected-no-wire-format
```

## 6. The tool system, blocked, and loop 1's seat

**Tool dispatch is chartered as of the tool workflow's opening, 2026-08-17,
and it is the exchange and never the trait.** `tool-trait` opened with
`weaver-traits-PRD` section 3.1's revision and lives on the gate's side of
the seam: this crate resolves nothing against it and holds no tool, opening
the execution exchange of `weaver-harness-gate-contract` section 2 per call
the family parse recovers, which keeps every tool on the far side of the
loop's membrane where the ratified boundary filed it. What the charter fixed
before the workflow still holds: permission modes are consultation policy
and not a boundary, the kernel bounds what a tool reaches through the uid it
runs as, and no safety classification exists here or is coming, per
`weaver-harness-PRD` section 3. The tool subprocess inherits no descriptor this program
holds,
which section 2 delivers by construction rather than by a per-tool argument and
asserts there rather than again here. The absent safety classification is
`weaver-traits-Spec` section 5's record, that trait being where the absence is
pinned, and this clause obeys it rather than restating it.

**The subprocess language above describes harness-side execution, and the
ratified loop boundary of 2026-08-11 retires that as the settled model**, per
issue #115. The kernel-bounds and tool-subprocess clauses stand as mechanics,
the uid and descriptor discipline being true of whatever process shape the
tool workflow elects, and they no longer describe where a tool sits: every
tool is on the far side of the gate by the reasoning-loop criterion, without
exception. The retirement is forced rather than preferential: enforcement by
construction requires a tool
result constructible from exactly one source, and a harness-side execution
path that must stay open cannot be closed by the type system, so the in-loop
construction path and the enforcement are incompatible and the enforcement
was chosen. Whether the gate spawns tool processes or tools are provisioned
as peers is the tool workflow's election and the finding rules on neither.
What is settled is only which side of the loop's membrane the tool sits on.

**The granted tool result closes the loop at the type level, per the audit of
issue #116, run 2026-08-17 with the reversal act landed.** The audit's
findings, recorded where the enforcement lands: the floor's tooling surface
is `ToolCall` and `ToolResultBlock` riding `ContentBlock`, `ToolName`, the
`tool-set` config field, and the trait placement. `ToolName` stays freely
mintable, the family parse legitimately minting one from every emission. The
conversation blocks stay serde records, because the decode seam carries
`Vec<Message>` and the SPU must deserialize a block to render it - the
round-trip hurdle the boundary document registered, resolved by splitting
the record from the grant rather than by sealing the record. The world mints
nothing: the turn ingress parses one `text` member and, from the operator
alone, the seeding line's `role` of section 6.1, and refuses everything else,
verified at the parse.

**The grant is `ToolResult`, a type on this crate's surface with three
negatives, each pinned the way the floor pins `PeerIdentity`:** no
`Deserialize`, no public constructor, no conversion from `ToolResultBlock`.
It is constructed at exactly one site, the completion of the execution
exchange of `weaver-harness-gate-contract` section 2, **whichever of the
exchange's four contents arrived**: the grant carries the tool's result, the
gate's refusal, the machinery's error, or the kill on the caller's clock,
per the taxonomy the tool boundary ruling of 2026-08-18 settled on that
contract, because each is a fact the model must learn and a refusal that
could not author a tool-result turn would leave the conversation with a hole
where the answer goes. One site, four contents, no second door. The author door for a
tool-result message takes the grant and never the block, so a loop 1 that
would fabricate a result has no door: not by construction, not by serde,
not by conversion, which is the enforcement mechanism named and held. The record the
trace and the render consume is authored by
this crate from the grant at that same site. The engine opens the execution
exchange when the family parse recovers a call from the emission, one
exchange per recovered call, **serially and in the order the parse recovered
them**, the next opening after the previous answered, and it authors the
tool-result turns in that same order, so the conversation's ordering is the
emission's and the channel's exchange identity is the only correlation the
wire needs. A turn whose emission carries no call opens none.

```graph
node: harness-tool-result-granted-not-minted
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-tool-result-granted-not-minted
```

**`src/engine.rs` is loop 1's seat, and the seam it composes across is this
crate's public surface.** The loop itself is the builder's, per the charter's
rescope of 2026-08-02: written at the worker composition root, compiled into
the worker binary, and immutable there, which binary the start step runs being a
provisioning fact. What this crate holds is the seat and the granted surface
the loop composes against, which is the whole of sections 2 through 5, the
channels and their custody, the run state, trace authorship, and assembly's
deterministic floor. **The extension seam is crossed at loaded-and-idle
itself,** the name being the charter's per its sections 2 and 6: loop 0
hands a standing interior to whatever loop 1 the binary carries, and takes it
back at the stop and at the leave, the bracket discipline being loop 0's for
every loop alike. A loop that composes what this surface offers costs nothing
anywhere else, and a loop that needs a port this surface does not offer is a
capability change entering through the front door as a charter and contract
edit. The compiled form enforces that blade structurally: there is no call by
which a loop mints a port, because the ports are types these crates own. The
decode surface a real loop needs, the exchanges, sessions, sampling, and the
flush call, arrives with the token workflow per `weaver-spu-PRD` section 8,
the basic loops this program ships land in their binaries by the same path as
any builder's, and the decode surface is chartered at section 6.1, the executor
election closing there in the negative rather than deferring, no runtime
entering and the serial `poll` loop serving the streaming path. **The blade takes
the compiler and the seam's position takes review.** A port is a type this crate
owns with a constructor no consumer can
reach, so a loop composes against the granted surface or does not compile, which
is the same type property section 8's sorting holds for descriptor ownership,
elected here rather than listed there. Where the seam falls
is an election a test could read only by asserting what a loop is handed, which
this suite does not buy. **The blade grounds in two invariants and the seam's
position grounds in neither.** Apex section 5.1 admits no exception for crates
arriving later, and a loop 1 that could mint a port would be a later arrival reaching
another process by a route no contract governs. The blade is that clause held at
the type level against code the builder writes rather than against a crate this
program ships. **It grounds in apex section 5.5 as well, and for the second reason
rather than a restatement of the first.** That section makes the loop the mechanism
that integrates and holds that the mechanism cannot itself be a part, so a builder's
loop minting its own port would be a part doing its own integrating, reaching a
domain by a route the loop never granted and settling a crossing in code no contract
sees. The blade is what leaves loop 1 composing the granted surface, which is where
5.5 puts the crossing. Where in the lifecycle the seam falls is a charter election
that would read the same under any invariant.

```graph
node: harness-loop-mints-no-port
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-loop-mints-no-port

edge: grounds
from: harness-loop-mints-no-port
to: axiom-floor-is-vocabulary-behavior-is-socket

edge: grounds
from: harness-loop-mints-no-port
to: axiom-harness-integrates-by-the-loop

node: harness-extension-seam-at-loaded-and-idle
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-extension-seam-at-loaded-and-idle
```

**The seat carries the context ports, 2026-08-19, second of this date and
the same door again.** The context-management loop holds two more calls,
each the smallest port its need admits. The fullness read answers the last
generation's `resident` and `capacity` as the decode answer carried them,
`None` before any generation, plain counts whose meaning is the loop's:
when a flush is worth its cost is the loop's business, per
`weaver-state-PRD` section 2. The flush drives the decode seam's standing
flush exchange, chartered at `weaver-harness-spu-decode-contract` and
implemented in the engine since the token workflow, valid between turns
and never while a generation streams, which the seat's own granting
already guarantees. Since the cut ruling of 2026-08-19 the flush takes
`keep`, the resident length the session returns to, forwarded to the
directive unjudged: where the cleanup line falls is the loop's election,
the seam bounds it below by the identity prefix and above by the resident
count, and the confirmation's counts answer what held, so the loop reads
its cut's resolution rather than assuming it. On the flush's confirmation
the harness authors the record's `flush` event carrying the resident
counts before and after, per
`weaver-trace-PRD` section 3.1's sixteenth kind, because a context that
shrank without a record would break the accumulation reading the
model.request ruling rests on. After a flush the assembly's working
structure still holds the whole conversation and the decode context does
not, which is exactly the divergence the loop then closes with the recall
ask and its own composed re-entry.

**A lifecycle refusal reaches the record where the run stands.** An enter
refused after its bracket has been authored is clerked into that run, which
**stays entered**: a bracket that stands is a run in place, holding what the
fan-out forked, and what unwinds it is the forced leave admin's rollback directs
within the same load, per `weaver-admin-Spec` section 3, so the next load records the
reset. So the record carries why the enter was refused, and
the close that ends the bracket is the leave's rather than this arm's.

**One site clerks every after-load refusal path**, the aggregate's own arm,
rather than each `after_load` naming its
own: a refusal is one kind of event whatever raised it, and a site per path would be a
chance per path to author differently.

**An enter refused before its bracket stands reaches no record**, the load
event being what opens the run. This crate authors nothing there because
there is nothing to author into, and the hole is named at
`weaver-admin-harness-contract` rather than closed here.

**Every typed refusal this crate receives is authored, 2026-08-22.** Per
the operator's ruling and the decode contract's clause of the same date. A
refusal answering an ask this crate sent becomes a `refusal` event naming
the ask and carrying the seam's own case, and the port then answers its
caller as it did before.

**The ask is named and its values ride only where no other event holds
them**, per `weaver-types-Spec`'s rule for the record: this crate authors a
refused flush's `keep` and a refused elision's span because nothing else
records either, and names the open, the append, and the cancel without
reproducing them because the identity prefix, the turn's delta, and the turn
key each already reach the record under their own kinds. **A classify
refusal carries no ask at all**, its content standing in the
`classify.request` event this crate authored before the exchange, and
reproducing it would be one fact in two places.

**Authoring precedes answering**, on the announce-after-record rule this
section already runs on: a loop told a span was refused before the
record holds the refusal could act on it and leave no trace of why.

**The ports answer `None` for a refusal exactly as they did**, which is
deliberate. A port's answer is what its caller can act on, the record is
what a later reader works backwards from, and widening every port's return
to carry a refusal it cannot repair would put a diagnostic burden on the
party least able to use it. What changes is that the refusal stops
vanishing.

**Where a refusal ends a turn, the close carries `Refused`.** The event and
the close are the division a fault already runs on here.

**The seat carries the elision port, 2026-08-22, through the same door.**
The elision is chartered at `weaver-spu-PRD` section 13.13 and crosses the
decode seam as its own exchange. The port takes `from` and `to`, a
half-open span of resident positions forwarded to the directive unjudged,
and answers the resident counts either side, `None` where the seam refused.

**Which span to elide is the loop's election and this crate holds no
policy about it.** The mechanic is here and what to keep is the loop's, with the
operator's settings where the loop exposes one: a port that judged a span would be
this crate deciding what a context is worth, which is the possession
`weaver-spu-PRD` section 13.9 places with the loop and 13.13 keeps there.
The seat forwards, the seam refuses a span describing no removable region,
and the loop reads the refusal as it reads the flush's resolution.

**On the confirmation the harness authors the record's `elision` event**,
carrying the span the loop named beside the resident counts the seam
returned, per `weaver-trace-PRD` section 3.1's twentieth kind. **The span
comes from the ask and not from the answer**, the answer echoing no span
because each party writes what it is the authority on, per the decode
contract. A context that shrank without a record would break the same
accumulation the flush's event exists to protect, and an interior removal
breaks it harder: a flush leaves a suffix a reader could infer from the
counts, and an elision leaves a sequence no count describes. **A refused authoring still
answers the counts**, the removal having happened, and since 2026-09-26 the only refusal
reachable there is the stream's own standing failure, under which nothing more can be
recorded and the record's ending is the evidence. Perturbation-verified: the scripted
seam's answer carries counts deliberately unroundable and no span at all, the
confirmation variant having none to carry, so a site reading the span from the answer
has nothing to read and one defaulting it fails on the span the loop named.

```graph
node: harness-elision-authors-from-the-ask
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-elision-authors-from-the-ask
```

**The seat carries the state port, 2026-08-19, through the front door this section
names.** The context-injection loop needed a port the granted surface did not offer, and
the capability change entered as a charter and contract edit rather than as an import,
this clause and `weaver-harness-state-contract`'s serve sections landing in one act.
`Ports` grows one call, the shape ask of that contract's section 2, returning the
session's shape as the member answered it, or nothing where the leg is down, the answer
malformed, or the bound expired, each of which is the contract's dead peer converted at
the seat into the same absence a missing leg serves. The port rides the same blade as
every other: a type this crate owns, granted with the seat and mintable nowhere, so the
loop that holds a seat holds the ask and no narrower grant exists for the boundary to
name. The contract's fourth ask, `grants` of 2026-09-04, is the seat's own and reaches
no `Ports` call: `src/state.rs` asks it once the seam stands under a serving record and
again at the leave, and the `unload` event carries the comparison, per
`weaver-trace-Spec` section 3, a loop having no business with the
boundary its state sits behind. The fifth, `identity` of the same date, is the seat's
own for the opposite reason: it is asked once at the enter, before the decode open, and
its answer is the open's material rather than a loop's, so no loop composes a prefix and
no `Ports` call exists to hand one out. The seam's ask end lives in `src/state.rs`, a
clone of the standing state channel held on the run beside the tee the enter attaches,
that channel arriving in the enter directive as the harness half of the pair admin
created, per the operator's ruling of 2026-08-26, dialed nowhere and derived from
nothing, the ask written and the answer awaited on the serving thread inside a bound
this crate elects, generous against a member whose answer is one pass over its own
holdings. Serialization on the shared channel is the serving thread itself: the tee's
feed and the seat's ask both run on it, so no ask can interleave a distillate's octets,
and the member's answers are the only traffic that ever flows toward this crate, so
whatever the wait reads is the answer or is malformed, with no third case to
disambiguate.

**The seat carries the recall port, 2026-08-19, its clause landed by the
audit of 2026-08-26.** The port was built with the context-management
loop's act and the shipped loop has called it since, and this enumeration
never carried its sentence - the audit's finding, chartered here with the
date the capability entered rather than the date the sentence did. `Ports`
grows one call, the recall ask of `weaver-harness-state-contract`
section 2: the conversation as custody holds it, in landing order, bounded
to the most recent turns where the caller passes a bound, or nothing where
the leg is down, the answer malformed, or the bound expired, the same
dead-peer conversion the shape port performs. Every judgment about what to keep,
summarize, or drop in a rebuilt context is the calling loop's, per the three-way
division of `weaver-state-PRD` section 2. **An answered recall reaches the record as a
`recall` event before the loop holds it**, as of 2026-09-26 per `weaver-trace-Spec`
section 3, naming the ask and every returned event by identity, and a recall the
recorder will not take answers nothing, the flush's rule.

**The seat carries the replay port, 2026-08-24, through the same front
door.** The diagnostic replay loop needed the session's elected events
whole, and the capability enters as this clause beside
`weaver-harness-state-contract`'s replay ask, landing in one act. `Ports`
grows one call, the replay ask of that contract's section 2, returning the
session's elected events in landing order as the member answered them, or
nothing where the leg is down, the answer malformed, or the bound expired,
the same dead-peer conversion the shape port performs. The bound is the
loop's to pass, because the replay ask is the one whose answer lawfully
waits, parked at an open preload until the seal, and only the asking loop knows how long
a preload is worth waiting on. **An answered replay reaches the record as a `recall`
before the loop walks it**, as of 2026-09-26, named by the answer's first and last
events and its count rather than by every identity, per the same clause.

**The seat carries the score port, 2026-09-26, on issue #523.** `Ports` grows one call:
the predicate a task answered on this run, whether it held, and, where the task supplies
a denominator, the count the run measured against it, answering whether the record holds
the verdict. **The task is the loop's and the verdict is the loop's**, the harness
holding the mechanic and no motive, so the port records what it is handed and judges
nothing. It authors the record's `score` before it answers, per `weaver-trace-Spec`
section 3's score clause, and refuses without authoring where the call is out of place
or malformed: inside a standing turn, a second verdict for the run, an empty predicate,
one term of the ratio without the other, or a zero denominator. Those are the loop's own
malformed calls rather than a seam's refusal, so no `refusal` event carries them. Under
a diagnostic binding the port answers false, the replay's record carrying no score.

**The seat carries the classify port, 2026-08-19, through the same front
door, with the classifier code act's opening.** `Ports` grows one call:
content in, the artifact's scored labels back, or nothing where the leg is
down, was never declared, refused typed, or answered malformed, each
converted at the seat into the same absence a missing leg serves. The
seam's ask end lives in `src/classify.rs`, a clone of `src/state.rs` at
the label seam's size, holding the channel end the enter fan-out stood up:
where the declaration's `spu-instruction` carries a `classify` subsection
the fan-out grows an arm per `weaver-spu-PRD` section 15.3, the harness
creating the pair before the fork like the other two SPU seams and holding
its own end on the run, the arm standing and falling all-or-nothing under
the unwind like every arm before it, and where the declaration carries no
subsection no process is forked and the port answers as the missing leg.
On the exchange's outcome the harness authors the record's
`classify.request` and then either `classify.output` or `refusal`, per
`weaver-trace-PRD` section 3.1, the turn carried where one stands open and
absent where the loop asks between turns. **A typed refusal lands as a
`refusal` and never as a fabricated answer**, which is unchanged in what it
guarantees and changed in which kind carries it: it was the output's own
refused case until 2026-08-22, when the refusal class took every seam's
typed refusal into one kind and `classify.output` became the scored labels
alone.
**The ask waits thirty seconds for its answer, and an arm that misses the bound once is
retired.** The harness holds a classify answer to 30,000 milliseconds, generous against
a forward of tens of milliseconds and the turn thread's protection. A bound that
expires, or a channel that faults on the send or the receive, retires the arm
one-strike, the state seam's economics on the label seam: the loss is authored as a
`fault`, a death and not a refusal per `weaver-harness-spu-classify-contract` section 5,
and every later ask of the run answers as the missing leg with no request authored. The
bound is one deadline per ask, taken before the first receive, and a late readiness
frame is skipped against it rather than taken for the exchange's answer, so a skipped
frame does not renew the bound and a stream of them cannot postpone the retirement. A
loop asking about many positions in a turn waits up to the bound on each, since the asks
are serial and each one blocks.

**The loop these ports serve is state management**, per the operator's rulings of
2026-10-02 on issue #1 and `weaver-agent-PRD` section 5.5. **One loop is deployed, the
Rust loop compiled into the worker under `dev_loop`**, which is the freeze section 1
already names, and no declaration elects another: the Python-iterating worker stays as
section 1 states it, untouched, the bench and experiments lane a loop is iterated on.
The ports above are the loop's whole reach into the store and the classifier, so state
management composes against the same granted surface as every loop and mints nothing.
**Its opinions are settings in the declaration's `[state-management]` table**, each with
a compiled default, wherever an operator could reasonably want a different answer, and
the `load` event this crate authors records every value in effect, per
`weaver-trace-PRD` section 3.1. **This Spec gives the table its meaning and
`weaver-types-Spec` gives it its grammar**, with the declaration's other keys, on the
operator's ruling of 2026-10-02 on #58's sixth question. The table's members are a later
act's, and this clause states only the rule: custody, the rebuild from the trace, and
every state write passing through the tee are never settings. **Every judgment the loop
makes reaches the record before the loop acts on it**, the classify pair and the score
above being the precedents: a ranking the loop makes over held facts is authored as its
own kind by the act that makes it, per `weaver-trace-Spec` section 3, so the state the
store rebuilds holds the judgment as the record holds it and never asks a model again.
**The save point is the loop's to time**: the store initiates nothing, so this crate
sends the `snapshot` ask of `weaver-harness-state-contract` section 2 once at a serving
leave, so that every clean serving unload takes one, and on the operator's demand under a
serving binding. **A diagnostic binding takes no save point**, at the leave or on
demand: its holdings are a preloaded replay the diagnostic loop manages no state over,
and a save point of them would be published and could be selected by a later serving
load as production state. The offline builder's save point is the builder's write, per
`weaver-state-PRD` section 3, and never a diagnostic load's. The asks go under the
dead-peer conversion every state ask takes, per the operator's rulings of 2026-10-02 on
#58. **The leave's save point is taken at rest, before the `unload` event**, on the
operator's rulings of 2026-10-06 on #1 (A3.0 items 4 and 6). **The leave itself is the
lifecycle state table's**, approved by the operator on 2026-10-09 and recorded on #1,
whose text is `weaver-admin-Spec` section 3: its states S0 to S11, its transitions and its
invariants I1 to I9 are the authority, and each rule below cites the row or invariant it
implements rather than restating it.

**The graceful leave drains, in the table's order** (`weaver-admin-Spec` section 3, the
graceful unload, steps 2 to 7). A `Leave` with `forced` false is heard at rest or
mid-turn, never refused for a turn in flight, and becomes the pending leave (S2 x unload,
S3 x unload); heard mid-turn, its dialer's connection leaves the turn's verb slot so the
listener is heard again, and it runs at the turn's close before any held request is
served.

1. **Quiesce** (step 2; S4). This crate sends `Quiesce` the moment the leave is heard,
   after the interrupt of item 2 where a tool call is out at the gate, since a gate
   executing a tool reads only that execution's continuation until it ends. It reads the
   gate channel until `GateQuiesced`, which the gate sends only after every frame it
   admitted, per `weaver-harness-gate-contract` section 2. Every frame met there, and
   every frame this crate held unserved behind the turn, a seeding line among them, is
   refused on its own exchange with the `refused` close naming the unload, `the agent is
   unloading`, while its connection still stands, and is recorded as a refusal of the
   leave carrying `Unloading`; none opens a turn (S4 x dialer request, S5 x dialer
   request, I5).
2. **The turn in flight finishes** what it can without further input, its answer going
   to its dialer through the still-standing connection (step 3; S4 x turn closes). A tool
   call out at the gate when the leave is heard has its return blocked: this crate sends
   `ToolInterrupt` on the open execution, which the gate answers `Killed { by: unload }`;
   a call the model asks after the leave was heard is never sent and is recorded killed
   by the unload in this crate's own word. Either way the call reads as never finished
   and re-runnable at the reload, and the turn closes there without it (S4 x tool return).
   **The first outcome wins**: where the gate's result was already sent when the
   interrupt reached it, the result stands, is recorded completed and delivered to the
   turn as in S3, and is never re-runnable; only a call the interrupt actually ended is
   `Killed { by: unload }` (`weaver-harness-gate-contract` section 2).
   The turn's close is `Clean` where the turn finished, and `Stopped { reason: unload,
   cause }`, the leave's cause, only where the unload ended it (S4 x turn closes).
3. **The wind-down turn** (step 4; S5). This crate tells the model the agent is
   unloading and asks it to summarize the work and where it stands for resuming after the
   reload: the request a `user` message it authors, its text the constant
   `WIND_DOWN_REQUEST`, "The agent is unloading now. Summarize the work so far and where
   it stands, so that it can be resumed after the agent is reloaded. Do not call any
   tool; a tool call will not run.", one generation, the request and the summary on the
   record and teed into state as elected, no dialer to answer. A tool call the model makes
   there is never sent and is recorded interrupted, the turn closing there (S5 x tool
   return). This crate runs it outside the loop, as it runs the seeding turn
   (`Ports::seed`); a storeless agent runs it, the record alone holding it; an unseeded
   one runs it with no identity prefix; a diagnostic binding runs none, having no gate and
   no work to resume.
4. **Lower** (step 5; S6). `GateStopped` comes only once every owed response is written,
   the relay dropped then, per `weaver-harness-gate-contract` section 2; the drain left
   nothing unanswered for the lower to meet.
5. **The leave's save point** (step 6): the four legs below, the `save_point` event
   authored on the finished answer.
6. **The release, then `unload`** (step 7; S7): the SPU and the member are released
   first: the SPU by its `Release`, answered `Released` (`Confirmed`) or not
   (`Unconfirmed`), an SPU that dies in it authored as a `fault` per
   `weaver-harness-spu-contract` section 5; the member by closing the state seam, it
   having no release exchange and retiring on its own (`Closed`, or `Unconfirmed` where
   the seam was already dead or the close failed). The leave then reads the recorder's
   queue depth and authors any pressure `fault` the crossing calls for, as the pressure
   rule of section 4 states; then `unload` is authored with `forced`, the leave's cause
   and the release outcome, the run's terminal event, nothing authored after it (I9); the
   writer is drained, and `Left { save_point, forced, no_state }` answered, `no_state` true
   where the run has no state to keep (every diagnostic binding, and a serving run with no
   member seam), `forced` on both being
   the leave's state at its end, never the directive it began as: false where it stayed
   graceful, true where a join or a declared bound turned it forced (I2). The member,
   released before `unload`, needs nothing after it: its save point is taken and its
   grant surface read before the release, and no save point ever carries `unload`. A run with no `unload` event never answered `Left`: the worker's death in S7 is S8
   as far as admin can tell, the marker left `Open` (S7 x the worker dies).

**A missed leg stops a leave that stayed graceful in S9** (S6 x member misses a save-point leg):
the run stays entered at rest with the gate lowered, as the leg rule below states. The
retry goes straight to the save point, the lower being done (S9 x unload); a
`save-point` at rest is taken as from S2 (S9 x save-point); a force acts as the sole
forced leave from there (S9 x force-unload); and a dialer meets no listener (S9 x dialer
request).

**The forced leave stops work and keeps state** (`weaver-admin-Spec` section 3, the
forced unload (sole); S2, S3 and S9 x force-unload). A `Leave` with `forced` true and `rollback` false, set by
admin's `force-unload`, has this crate interrupt a tool call out as item 2 does, then
lower the gate at once from its raised position, with no quiesce and no drain, reading
the channel to `GateStopped` and recording every frame met there refused, carrying
`Unloading`, its connection
closed by the gate unanswered (I5); cancel the turn in flight as a stop does, recorded as
a stop naming the unload, state holding what landed up to the cancel; run no wind-down;
take the leave's save point and report it as any leave does; release the SPU and the
member as item 6 does; and author `unload` last with `forced` true and the release
outcome, answering `Left { save_point, forced: true, no_state }`. Where the save point cannot
be taken, the member dead or a leg unanswered, the forced leave comes down without one,
`Left` naming none and the miss recorded as a lifecycle refusal of the leave naming the
leg, so the next load carries the reset admin resolves from its marker (I4).

**The rollback's leave is its own row** (`weaver-admin-Spec` section 3, S1 x Leave; K1).
A load's rollback directs a `Leave` with `rollback` true (and `forced` true), which
this crate tells apart by the directive and never by its own position, a `Ready` that
lands just after admin's enter deadline leaving it `Entered` while admin rolls back
(Codex on #109, round 28): in any position it takes no save point and runs no wind-down, releases what it admitted, and
where the bracket stands it authors `unload` last with `forced` true and the release
outcome, the `unload` carrying the load's cause. It is exempt from I2 by name, admin
writing the marker by its marker-write rule (K1).

**A force that joins a pending leave turns it forced from where it stands**
(`weaver-admin-Spec` section 3, the forced unload (joining); S4, S5 and S6 x
force-unload). `JoinLeave { cause }`, the force admin sends without the invocation lock
beside the graceful unload that holds it, joins it: the one overlap the operating
envelope admits (`weaver-agent-PRD` section 6.1). Its caller, answered `Left`,
concludes as every answered caller does (`weaver-admin-Spec` section 3, the
conclusion). What remains of the quiesce's wait, the drain and the wind-down is
skipped: a tool call out is interrupted, the turn or the wind-down's generation is
cancelled as a stop, and a gate already quiescing or draining is brought down with
`Lower`, as the sole force's is: the gate sends every frame it admitted and had not yet
sent, this crate records each refused with `Unloading` and its connection closes (I5),
inside the lower bound. Only where the gate does not answer `Lower` inside that bound
does this crate close its end of the gate channel and reap the gate, closure being death
to the gate per `weaver-harness-gate-contract` section 1. The save point is still taken, and a leg it misses comes down as the forced
leave's does rather than stopping in S9; a force heard while the legs run does not
abandon them. The pending leave keeps its own cause, the graceful caller's, and records
the joining cause as `forced_by` on `unload` where that join is the first force to turn
the still-graceful leave forced, and every dialer is answered with the same `Left {
forced: true }` (I2); a later join, or a join into a leave a declared bound already
turned forced, is answered with that `Left` and named nowhere. `JoinLeave` with no leave pending is `OutOfOrder`, which
tells admin the lock's holder is no unload. **A `Leave` meeting a pending leave is
`OutOfOrder`, forced or not**, as is a `SavePoint`: while the holder lives admin's lock
refuses them before they dial (I1), and where it has died, outside the envelope, the
refusal is admin's cue for its conservative answer (`weaver-admin-Spec` section 3,
outside the envelope), this crate taking over no leave.

**From S7 on, a join gets the `Left` as it stands** (`weaver-admin-Spec` section 3, the
forced unload (joining), and the ordering of #94's round 21; I2). From S7 on the
leave's outcome is fixed, its save point taken or missed, so a join held at the seal adds nothing to the record,
records no `forced_by`, and is answered with the same `Left` the leave's own dialer
gets, its `forced` as the leave came down; its caller concludes all the same
(`weaver-admin-Spec` section 3, the conclusion).

**Every wait of the leave hears the coordination listener** (Codex on #94, round 17; I1,
I3), through one helper: the wait for `GateQuiesced`, the drain, the wind-down, the
lower, the save point's four legs and the grants read-back poll the listener beside what
they wait on, and the dials that land while this crate waits on what cannot be polled
beside it, the SPU's release, the trace's drain and the reaps, are swept from the
listener's backlog before the leave's dialers are answered, by the from-S7 rule. **S7
runs the seal's steps** (`weaver-admin-Spec` section 3, the seal: S7's own steps, the
authority, not restated here): the pathname unlinked, the backlog drained into held
connections, the release, `unload`, the writer's drain, and only then the `Left` to the
leave's dialer and every held `JoinLeave`, before the listening descriptor closes and the
worker exits.

**The drain and the wind-down are unbounded by default, and every other leg is bounded**
(I3; the operator's ruling of 2026-10-09 on #1; #107, area 1, R2). The turn in flight
finishing and the wind-down's one generation have no bound unless the declaration sets
`[lifecycle] drain-bound` and `wind-down-bound`, in seconds, per `weaver-types-Spec`;
without them `force-unload` is the recourse, admin waiting for `Left` with no deadline.
This crate reads the two from the enter, `EnterPayload.drain_bound` and
`wind_down_bound`, its only source, since it holds no declaration; each absent means that
phase is unbounded, and admin's own wait comes from the same parse it put on the enter,
so the two agree by construction.
Where the declaration sets one, a leg past its bound turns the leave forced from where it
stands, as a join does, with no `forced_by` and the cause staying the graceful caller's,
`unload` and `Left` then carrying `forced` true and a missed leg coming down as a forced
leave's does; with both set, admin's bound is their sum and 150 seconds, per `weaver-admin-Spec`
section 3. Every other leg is bounded in this crate: the save point's ask and `finished`
legs and the grants read-back under `ANSWER_BOUND_MS`, its answer leg under
`SNAPSHOT_ANSWER_BOUND_MS`, and `GateStopped` from a draining gate under the lower bound,
past which an unread delivery is lost per `weaver-gate-PRD` section 13.4; these, the
wait for `GateQuiesced`, the trace's drain, the SPU's `Release` and the reaps sum inside
admin's 150 seconds. A force, sole or joining, skips the drain and the wind-down, so its
legs sit inside those 150 seconds, and a harness that does not answer within them is
escalated by admin (I3).

This crate sends the leave's `snapshot` ask after the last turn, the wind-down's
included, and before it authors `unload`, so the save point
holds every elected event of the run but the `unload` event's own distillate, which is
lifecycle provenance the record keeps and no holding needs, and the next load replays no
tail; the holdings restored at a load therefore never carry the prior run's `unload`
event, and a rebuild through the preload door to the stamp stops before it too, so the
member's restore-equals-rebuild instrument, `a_reloaded_store_equals_a_full_replay`,
stays equal by construction. **A save point has four legs**, per `weaver-harness-state-contract` section 2:
the ask, carrying its ordinal, counted per residency from 1 (Codex on #94, round 10),
the answer echoing it and naming the finished name and the digest, this crate's
`acknowledge` of that ordinal and that digest, and the member's `finished` answer
echoing the ordinal, on which this crate authors the
`save_point` event, per `weaver-trace-Spec` section 3, the save point by digest and the
position it covers, an event the tee never sends to state under any election, so the
save point and a rebuild to its position hold the same; a save point is never
overwritten, so each answer names a new one. **The answer leg has its own bound**, on
the operator's ruling of 2026-10-08: 120 seconds (`SNAPSHOT_ANSWER_BOUND_MS`), enough for
the member to write a 1 GiB image before it answers, while the ask, the `finished` leg
and the small asks keep the two seconds of `ANSWER_BOUND_MS`, `restored` having its own
120 seconds (section 6.1) and a parked ask (`replay`, or `identity` under a diagnostic
binding) the parked bound of 600 seconds. **Where any leg misses, a leave that stayed
graceful does not complete** (S6 x member misses a save-point leg): this crate authors no
`unload`, answers admin `SavePointNotTaken` naming which leg, the answer (a member's
failed write among its causes), the acknowledgement's answer, or the member being dead,
and stays entered at rest with the gate lowered and the run open (S9), so the operator
retries with `save-point` and `unload` or, where the member is dead, forces the unload;
nothing is silent and the dead-peer conversion does not apply to this ask at the leave.
A leave forced at the miss, directed so, joined or past a declared bound, comes down
without the save point, as the forced leave
above states. **The seam
stays alive across the miss**: a missed answer or finished leg retires nothing, and before
its next ask the seam drains, without blocking, whatever lines or part of one the member
sent late, discarding them and saying so in one diagnostic line with the count, the
resync for every ask; **and a late answer is dropped by its number**: the retry's ask
carries the next ordinal, and an answer of either kind, the `snapshot` answer or the
`finished` answer, that arrives past the drain carrying the ordinal of an ask before is
dropped and said while the wait goes on inside the bound, so a
member whose every snapshot outruns the bound never leaves the retry one answer behind,
which a longer bound would not close, the race being one of order and not of time.
**A snapshot-protocol frame is never another ask's answer**: one that lands past the
drain while any other ask is awaited, `recall`, `grants`, `shape`, `identity`, `replay`
or `restored`, is dropped and said and the wait goes on inside that ask's own
bound, so a late save-point frame never misses the next ask and never retires the seam;
only a line that is neither the awaited answer nor such a frame misses it. A
late finished answer's file stands in the room unrecorded and is published as recovered
at the next publication, the leave's at the latest. A missed write, the ask itself unsent, is the dead peer as every send failure is.
**A save point on demand
is the `SavePoint` directive's**, admin's `save-point` verb over the coordination
channel: at rest, the same four legs, the same event, and the answer `SavePointTaken`
naming the digest, the finished name, the position covered and the trace position of
the event, its own run and sequence, since after a restore the covered position is the
prior run's until a distillate lands and the event is this run's, which admin's
manifest records; while a turn runs it refuses
`ActivityNotAtRest`, as `Stop` would not, because a save point of a turn in flight would
hold half of it. **No save point is taken after the tee was lost** (the operator's
ruling of 2026-10-08 on #99): the tee detaches silently on a stalled or broken seam, and
the recorder marks it lost; the seam, a clone of the same socket, may still answer, but
the holdings stopped growing at the detach, so the save point refuses
`SavePointNotTaken` naming the member dead, the clean unload with it, and a force ends
the run with the reset recorded. **A run that takes no save point refuses the demand `OutOfOrder`**: a
diagnostic binding, or a serving run with no member seam, has no member to ask, and
`SavePointNotTaken` naming the member dead would report a healthy member dead. The `Left` answer names the leave's save point the same way, so admin
publishes it at once with its trace position. **The live
`restore` ask is retired** (the operator's ruling of 2026-10-08 on #99): a restore is a
reload of state, made only at a load through the save point admin hands the member at
spawn, and this crate sends no `restore` ask. The live restore belongs to A5, with the
decode seam's `Reopen`: the decode seam has no close or reopen today, its directives
being `Open`, `AppendAndGenerate`, `ReFeed`, `Cancel`, `Flush` and `Elide`, and the flush, the
reopen on a differing prefix and the rebuild of the prefix alone (the operator's
clarification of 2026-10-06, #1) are that act's to charter.

### 6.1 The decode surface, chartered

**This subsection closes what section 6 deferred and section 9's first election
named**, the exchanges, sessions, sampling, and the flush call, per the operator's
direction of 2026-08-11 that the harness half of the turn is drafted before it is
built. What it charters is the granted surface and the seam's mechanics, never the
loop that composes them, which stays the builder's.

**The session opens at the enter fan-out and not at the first turn**, per the operator's
ruling of 2026-08-11 answering the handoff's first question. Loop 0 opens the decode
session as the last step of standing the interior up, after the SPU's admit confirms
residency, so the surface loop 0 grants at loaded-and-idle is already a session at rest.
The identity material the open carries has one source under both bindings, the state
member, on the operator's ruling of 2026-10-06 (#1) that the system prompt is state,
which supersedes the ruling of 2026-10-02 on #58's fifth question that the prompt file
was authoritative. **Under a serving binding the enter asks the member for `identity`**,
the ask of `weaver-harness-state-contract` section 2, after `restored` and before the
open, and the open seats the answer: the turnless `message.system` events of the newest
run the restored holdings carry, in order and with their boundaries, byte for byte,
which is the prompt the seeding turn entered or a later seeding replaced, carried by
the save point the load restored. An empty answer is an agent not yet seeded, which
opens with no prefix and is seeded by its first turn, per the seeding clause below. **A
serving load with no member elected asks nothing and seats nothing**: a store of
`none` is the operator's election that nothing holds state across loads, the prompt
included, so such an agent is seeded in every residency, as its first turn, and the
seeding turn is admitted there as anywhere, since it is the one way the prompt enters. The
declaration carries no identity and the enter no prompt, per `weaver-types-Spec`
sections 2 and 4, so this crate holds no file, no path and no handle into the
territory's declaration or draft. **The load writes none of what it seats to the record**, on
the operator's ruling of 2026-10-06 (later, #1) that a session's trace and the state it loaded from are not tied together once the agent is unloaded: the `load` event's lineage names the save point the session started from, the
`recall` of the identity ask names the events the member handed back for seating, and
the prompt's text is on the trace where it was entered, at the seeding turn, and
nowhere else. This retires the rule of 2026-09-04 that each run's record carries its own
prefix text; a reader of one run reaches the prefix through the save point its lineage
names, which is where the operator says it lives. **The rebuild at a load is the prefix
alone, the member's identity answer; nothing else is replayed**, on the operator's
clarification of 2026-10-06 (#1): state loads from the save point, the decode session
opens on the seated prefix with its cache clear, and nothing further from the holdings
or the record reaches the model unasked, the rest being the operator's turn by turn,
with recall on demand. **Under a diagnostic binding the identity is the preloaded
store's**, on the operator's ruling of 2026-10-02 on #57's sixth question: the same
ask, once the seam stands and before the open, parked on the driver's seal, and the
record's own `message.system` events seat. **A missed ask under either binding refuses
the enter**, the dead-peer conversion of every other ask notwithstanding: a run whose
bounding cannot be read is not a run with no bounding, so the enter fails closed
through the same after-load failure a refused open uses, per ruling 2 of issue #422.
**This is the one site where the identity's source is read**, and the prefix the open
seats comes from nowhere else. State supplying the identity was the operator's
long-range direction, and this ruling is its first step; the semantic-graph store that
sleep-cycle consolidation keeps current is the memory leg, out of scope per
`weaver-agent-PRD` section 9 and not built here. The run's session identity is the
open's `session` in every case.

**The seeding turn enters the prompt**, on the same ruling. A request line of
`weaver-gate-world-contract` section 2 may carry `role` with the one value `"system"`
beside its `text`, and such a line is admitted only where the frame's `dialer`, the
peer uid the gate read at accept and carries inward per
`weaver-harness-gate-contract` section 2, is the operator's uid the enter carries,
`operator` of `weaver-types-Spec` section 4; a line whose dialer is absent or is any
other uid, the connector's service user among them, or whose `role` is any other
value, is a refused turn, the response frame's `refused` kind naming the role, and the
channel stands. An admitted seeding line is one turn in the residency that seeds: this
crate authors the prompt, one `system` message of the line's text verbatim, turnless
through the identity door so the record and the tee carry it, then appends it as the
session's context, `AppendAndGenerate` with that one message and no user message, and
the model's answer closes the turn, so the seeding exercises the whole loop (gate,
harness, model, trace, tee, state) with the one prompt that matters, which is the
operator's last test before the agent goes into production. **The prefix the next load
seats is the member's `identity` answer**, every turnless `message.system` of the
newest run that holds any, in order, and the load writes none: a seeding in a later
residency therefore replaces the prefix, the newest run's being served, and two seedings
in one residency are served both, in order. **The residency's seedings are bounded by
the state seam's answer bound, spent here at the sender**: this crate keeps the
rendered size of the residency's seedings so far, as the `identity` answer of
`weaver-harness-state-contract` section 2 would carry them, and a seeding whose addition
would carry that answer past the one-mebibyte frame bound of that contract's section 3
refuses as a turn, the response naming the bound and the size, nothing authored and
nothing appended, the channel standing, because a prefix the member cannot answer is a
prefix the next load cannot seat.
Within the seeding residency the prompt is appended context and not the session's
prefix, so a flush there would drop it from the context, which is why the runbook ends
that residency at the save point: load, seed, save point, unload, and the production
session is the next load. The first save point taken after the seeding is the agent's
starting state, per `weaver-state-PRD` section 4, taken on the operator's demand with
admin's `save-point` verb, or by the first leave's snapshot. **Because the seeding turn generates, the agent's starting state
holds the model's first answer beside the prompt**: that answer is on the record as
every `message.assistant` is, the tee lands it, and a save point taken after the
seeding carries both, by design and not by leak. The line's bound is the gate's, 32 kibibytes of octets before the
delimiter per `weaver-gate-Spec` section 4, which a prompt written as prose sits well
inside. A refused open is a refused enter, returned through the
fan-out's after-load failure so the authored bracket stands for the forced leave of admin's rollback to unwind, the
aggregate naming the decode seam as where the fan-out stopped. **Under a restoring load
the store answers from the save point**, per the operator's rulings of 2026-10-02 on
#58: admin hands the member the save point at spawn, per `weaver-admin-Spec`, the member
holds what it held, and the open carries the identity the member answers and nothing
more, the loop composing each turn's context from the restored state as it composes it
from any held state. The record restore of issue #432, which seated a session's
conversation as prefix and wrote it as `message.restored`, retires with those rulings: a
session branches from a record offline, by a rebuild to a cut that writes a save point,
per `weaver-state-PRD` section 4. **Every serving enter where the member stands asks
`restored` before it authors `load`**, per `weaver-harness-state-contract` section 2 on
the ruling of 2026-10-02 on #58's fourth review round: the answered stamp, the save
point's digest, run, sequence and last turn, must equal the same four members of the
enter's `Lineage`, or be absent where the enter names none, the other members,
`named_at_restore` and `built_from`, being admin's resolution and never the member's to
answer, and a refusal, a miss, or a disagreement refuses the enter before it authors
`load`, through the fan-out's before-load refusal with the stream still clean, so no
`load` event names state the member did not restore. **An enter naming a lineage with
no member's end refuses the same way** (#99): no member stands to have restored it and
none can be asked, so the enter refuses before `load` rather than skip the comparison.
**The `restored` answer is waited
on for 120 seconds** (`RESTORED_ANSWER_BOUND_MS`, the operator's ruling of 2026-10-08 on
#99): the member judges and seats the save point only after the opener reaches it, up to
the save point's 1 GiB bound, and answers after that, so the two seconds of the small
asks would refuse every load of a large save point. Admin's load bound
(`load-bound-seconds`, 900 by default) spends this wait with the rest of the enter. **The reset rides the `load`
event**: where the enter carries `reset`, beside its lineage and whether or not a save
point stands, resolved by admin from its clean-unload marker per `weaver-admin-Spec`
section 4, this crate copies it onto the `load` event beside the lineage, per
`weaver-trace-Spec` section 3, the prior run and the reason, the save point reset to
being the lineage's or none where the store stands empty, so a rebuild from the record
honours it, per `weaver-trace-PRD`.
**Each enter ask that returns events reaches the record as a `recall` before the open is
built from it**, as of 2026-09-26 per `weaver-trace-Spec` section 3, a recall the
recorder will not take being named in an `identity_prefix_unrecorded` fault rather than
refusing the enter. **The `restored` ask is not one**: it returns a stamp or a refusal
and no event, and it is answered before `load`, so it is recorded by the `load` event's
lineage it confirms and never as a `recall`. **Under a diagnostic binding the enter
records nothing**, its asks being setup the diagnostic record does not carry as it
carries no load and no prefix: that record opens with `replay.opened`, and the ask that
feeds the model is the replay port's. **A restoring run numbers its turns from one**, on
the operator's ruling of 2026-10-02 on #59: a restore from a save point is a new branch
and so a new run, and a turn key counts within its run and restarts with the next, per
`weaver-gate-world-contract` section 2. The save point's last turn stays on the record
as a fact of the lineage and numbers nothing. The derived seeds of `weaver-spu-Spec`
section 8.5, which fix a generation's stream from the declared seed, the turn's
reference, and the generation ordinal, therefore draw the new run's own streams, its run
reference distinguishing them from those of the run the save point was taken from.
**Where the door stands the enter's identity ask waits on the driver's seal**, under the
parked ask's bound rather than the two seconds a member answering from holdings at rest
takes, per `weaver-harness-state-contract` section 2 and `weaver-state-Spec` section 4,
and one typed line on standard error names what is waited on and for how long before the
ask is written, so an operator who has not started the driver reads why the load stands
still. The door stands only under a diagnostic binding, the preload being that binding's
alone, and a short bound would refuse the enter unless the driver had sealed inside it,
the identity ask's miss being one of the two the dead-peer clause does not convert. **A
diagnostic enter therefore waits on the driver as its replay does and refuses without
one after the bound**, and the answer it opens on is the record's own. **The instrument
is perturbation, on the ordinal**: the first turn of a restoring run is numbered one,
watched to fail when the ordinal is taken from the lineage's last turn.

```graph
node: harness-restoring-run-numbers-turns-from-one
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-restoring-run-numbers-turns-from-one
```

**The column ask is written where and only where the binding is diagnostic
and the readout is elected**, per `weaver-spu-PRD` section 13.7's cadence
election and the decode contract's supplies clause, and the discipline that
a serving harness never writes it is this crate's own, watched here as that
clause named owed. The derivation reads the two facts the enter already
holds - the binding's kind and the instruction's readout election - and a
diagnostic load without the election asks nothing, the ask having no tap
to continue and the registry's second arm waiting for it. Perturbation:
derive the ask from the election alone and the serving combination writes
it. Watched under exactly that change.

```graph
node: harness-serving-writes-no-column-ask
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-serving-writes-no-column-ask
```

**The turn is loop 0's machinery, granted, and loop 1 supplies the delta and
receives the outcome.** The granted surface offers a turn call: loop 1 hands the
turn's delta as canonical messages and receives back what the generation produced,
the emission and its finish, and loop 1 authors nothing. Loop 0 drives the
append-and-generate exchange, consumes the token stream, reads the close, and
authors the record, which is the sole-writer property held where the turn runs
rather than only where the load event is written. This is the custody model of
`weaver-harness-PRD` section 5: loop 1 is the reasoning that supplies intent, loop 0 is
the clerk that authors what happened, and the blade of section 6 is why the split
falls here, a loop 1 that authored its own events reaching the trace by a route no
seat granted. **The election flagged for the operator: the turn call authors
internally rather than handing loop 1 a lower surface it authors over.** The ground
is the blade and the sole-writer rule pulling the same direction, and the cost is
that a loop wanting an unauthored generation has none, which no chartered loop wants
and the tool workflow may reopen.

**The three model events are authored across the turn, at the submit call where
conformance is checked.** `model.request` carries the turn's rendered contribution
the answer's close splices, the delta per the ruling of 2026-08-12, the full
effective context being the accumulation the record determines. `model.output`
carries the emission verbatim, its finish, and the generation's two session
counts, the resident and the capacity this crate already reads at the same
close to answer the seat's fullness port, so the record and the loop take
one reading rather than two, and
`model.measurement` carries the measurement the close splices, each event the
harness authoring what the SPU rendered, the organ owning the content and the trace
owning the boxes per the custody model. **A fourth model event authors per
decode position where the field election stands**, as of 2026-08-21:
each `Field` intermediate of the decode seam becomes one `model.field`
event, authored as it arrives rather than accumulated, because
accumulating a generation's worth would hold megabytes to write them at
a close that has other work. It is shaped rather than spliced, per
`weaver-trace-Spec` section 3, its members being plain values this crate
holds typed on the way through.

**The load event carries the elections the enter declared**, each named individually,
per `weaver-trace-PRD` section 3.1. The harness is the party that knows them, and they
arrive by two routes rather than one: the readout, the field, and the surprisal ride the
SPU instruction, and **the tee's election arrives beside it on the enter payload**, a
sibling of the instruction rather than a member of it, per `weaver-types-Spec` section
4. The harness authors the bracket from both, so the record's declaration of its own
posture still costs one read of what it already holds. **The tee's election joined that
declaration 2026-08-25**, per `weaver-trace-Spec` section 3, and this clause named three
elections and one route until then. The rendered contribution and the measurement splice
into their own boxes, one member per box per the streaming ruling, and the harness
checks each spliced member's conformance to its event's accepting shape at the author
call, the price the splice's opacity is paid at. **Two members of that declaration are
not elections and arrive by a third route**, as of 2026-09-03: the loop that composes
the run's prompts is handed to `serve` by the binary that runs it, since only the binary
knows whether its loop is compiled in or a file it resolved and read, and whether the
state member stands is the presence of its end on the enter, which this crate receives
and no one else does. Neither is read from the deployment, so the record cannot disagree
with what ran. A third member that is not an election joins on 2026-09-04: the
declaration's digest, which admin computed at the inventory and the enter carries, so
the record names what the run was built from without this crate reading a file, per
issue #435. The boundary file's digest and the load's cause join as of 2026-10-03
(#50) by the same route, each copied from the enter and the boundary's marked boundary
and never constitution, per `weaver-trace-Spec` section 3, which this crate copies and
does not compute. **No prompt digest joins them**, on the operator's ruling of
2026-10-06 (#1): the prompt is state, and the record of it is the turnless
`message.system` event the seeding turn authors, the `recall` of the identity ask at
each load, and the lineage the `load` event names.

**The seeding turn's prefix is authored through the identity door, one turnless
`message.system` of the line's text**, per section 6.1 and `weaver-trace-PRD` section
5, before the turn that appends it, so the record and the tee carry the prompt where it
entered. **A load authors no prefix**, on the operator's ruling of 2026-10-06 (later, #1) that a session's trace and the state it loaded from are not tied together once the agent is unloaded: what it seats it was
handed by the member, the `recall` names that answer by its events' identities, and
the prompt's text stands on the trace at its seeding turn and in the save point the
load's lineage names. The event carries no turn, a prefix preceding every turn of the
residency it seeds.

**The door is `author_identity` and not `author_message`.** The message door takes a
turn key and applies the licensing rule to it, so admitting a turnless message there
would put a path with no turn inside the door every turn message uses. The second door
authors the same kind with the turn member absent, applies the same licensing rule to
the message itself, and refuses every role but system, because the only thing it exists
to write is a prefix. **A role reaching it that is not system is a defect in its
caller rather than a message to record**, and it refuses on the same reasoning the
tool-result door refuses in the message door: a door that writes what it was not built
for launders a bad line into a record that looks well formed. **It judges the four rules
of `weaver-types-Spec` section 5 itself**, the role, each block's licence, a message
carrying nothing and a text block carrying no text, since 2026-10-06 the seeding line's
door as much as any: an empty prefix is the empty turn by another route. **The system-only
rule has no exception**: the record restore of issue #432, which wrote a restored
conversation through this door as `message.restored`, retires with the operator's
rulings of 2026-10-02 on #58, and a restoring load seats the identity alone, per section
6.1, where the `load` event's lineage records where its state came from.

```graph
node: harness-identity-door-writes-system-only
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-identity-door-writes-system-only
```

**The door's refusal at the seeding is the turn's refusal**, the response frame's
`refused` kind naming the rule, per section 6.1, and nothing is authored: the line never
reached the decode session, so there is no seated prefix the record cannot show. The
`identity_prefix_unrecorded` fault of `weaver-harness-PRD` section 5 keeps its one case,
the enter's `recall` the recorder would not take, per `weaver-trace-Spec` section 3; the
load authors no prefix since the operator's ruling of 2026-10-06 (later, #1) that a session's trace and the state it loaded from are not tied together once the agent is unloaded, so the hole that case once named at a
load, a prefix seated into the decode context with no event to show it, no longer exists
there.

**The stop is heard mid-stream by `poll`, which is why it joined the surface.**
While a generation streams token by token, loop 0 waits against two descriptors at
once, the decode channel for the next token and the coordination listener for an
operator who dials to stop. `poll` sleeps against both and wakes on the first
ready, the token read staying the blocking read it is and the service staying serial
with one turn in flight, per section 2.4's election. A coordination dial arriving
mid-stream is accepted and credential-checked at the accept as every dial is, and
its directive is read without wedging the turn: the accepted connection is itself
polled before its read, so a dialer that connects and then delays or sends nothing
cannot hold the streaming turn hostage, and a hangup or an error on it closes the
connection and returns the loop to the stream rather than blocking on a peer that
left. The stop it carries cancels the turn at the decode seam, the outstanding
generation answering with its partial output marked stopped and the tokens already
streamed standing in the close, and the stop then
answered on the coordination seam with the turn's fate, the announce-after-record
discipline placing the turn-close event before the answer. **The election flagged
for the operator: the stop multiplexes by polling the decode channel against the
coordination listener, rather than by a second thread or a runtime.** The ground is
section 2.4's `poll` election and the streaming ruling's rejection of the timed
rotation, and the no-runtime rule of section 1 stands, which closes section 9's
executor election in the negative rather than leaving it open: the streaming path
is served by `poll` and a serial loop, and no runtime enters.

```graph
node: harness-session-opens-at-enter
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-session-opens-at-enter

node: harness-turn-authors-the-model-events
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-turn-authors-the-model-events

node: harness-stop-polled-during-the-stream
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-stop-polled-during-the-stream
```

### 6.2 The invocation, chartered

**This subsection charters how loop 1 comes to run, per the operator's ruling
of 2026-08-12 and the charter's criterion: the seat is granted on work that
arrives owed an answer, and on nothing else.** The dataflow ground is the
decode contract's section 2 rule, nothing originating in the SPU arrives owed
work, and the composition ground is the charter's section 2, restating the
reasoning-loop boundary: the loop calls through the gate rather than
sequencing it, so the agent's outer wait is loop 0's clerking and never the
loop's reasoning.

**A second criterion stands beside it since 2026-08-31, and it is the
Gateless binding's**: under a diagnostic binding the seat is granted once,
at the run's opening, on the run itself as the work - the operator's sealed
preload is what arrived owed an answer, the certification is the answer it
is owed, and no frame ever grants this seat because no Gate exists to carry
one. The two criteria partition by binding and neither widens the other: a
serving seat still waits on the frame and nothing else, and a diagnostic
seat runs from the opening and takes no frame ever. The loop entry's
concrete signature is the frameless form of the serving entry, listed with
section 9's satellites, and the entered-state wait under this binding spans
the coordination listener and the decode channel alone, the gate channel
having no end to poll.

**The entered-state wait spans three descriptors, by `poll`, serial as ever.**
Between enter and leave, loop 0 waits against the coordination listener, the
gate channel, and the decode channel at once, and wakes on the first ready.
The coordination listener's accept and credential check are section 2.3's
unchanged. The decode channel is heard at rest for one thing, the SPU's
emitted report, there being no exchange the SPU may open at all, the report
riding as the seam's one emission owed nothing back, per the decode
contract's second ruling of 2026-08-12. A report never arrives mid-stream,
the contract's ordering emitting it only while no exchange is outstanding,
so the stream's poll of 6.1 hears tokens and the close and nothing else. The
gate channel carries the raise's answer at enter, the frames between, and
the quiesce and the lower at leave. Section 2.4's `poll` election covers this wait as it
covers the stream's, one mechanism in two states, and the no-runtime rule
of section 1 stands.

**Dispatch is by payload kind, and only the frame grants the seat.** A
directive is the lifecycle interior's, served as section 3 serves it, a leave
at rest proceeding at once and a stop at rest answering nothing in flight. A report
is clerked: the harness authors the `fault` event from what it was handed,
per the fault-carrier ruling, and answers nothing, the receipt having
retired with the second ruling of 2026-08-12, and no turn opens and no seat
is granted. A frame is the one arrival owed an answer, so it and it alone
grants the seat: loop 0 enters the loop the binary carries, handing the
granted surface of section 6 and the turn's content, and the entry's return
is the response the frame's exchange is answered with, the turn opened and
closed inside by the surface's own turn call per 6.1.

**A response that answers a turn names it and names the run it belongs to**,
per `weaver-gate-world-contract` section 3 and the client-facing ruling of
`weaver-gate-PRD` section 1. This crate composes the response, so the members
are rendered here. The turn key is the one this crate minted for that turn and
the run reference the one it was entered with, both carried rather than
rebuilt, because a second construction is a second chance to disagree with the
record.

**The closes that open no turn carry neither, and the boundary is where the
turn is minted.** A frame that does not parse is refused before the seat is
granted, and a prompt with a hole closes before it, so in both the turn key
does not exist to be rendered. **A turn that opened carries its key whatever
became of it**, refused by the seam or stopped by the operator alike, those
being the closes a client is most likely to need to name.

**The seat is granted per frame and returns by the entry's return.** At rest
no seat is outstanding, which is what leaves the leave free to proceed, and a
stop mid-turn forces what the return otherwise yields: the cancel of 6.1
closes the generation or the cancel below closes the invocation, the turn
closes stopped, and the entry returns the seat with the stopped outcome. A
leave mid-turn is held pending, per section 6 (`weaver-admin-Spec` section 3,
S3 x unload), and the seat returns by the turn's own close, the blocked tool
return of section 6 included; only a forced leave cancels the turn, as a stop
does (S3 x force-unload), so the seat is never reclaimed by force but through
the stop's own path. **The election flagged for the
operator: the seat is granted per frame
with the wait staying loop 0's, rather than lent for the run with a receive
port granted to loop 1.** The ground is the boundary rule above and the
blade of section 6, a run-long seat wanting a port to wait on, which would
hand the loop the agent's outer coordination and the scheduler's position
with it. The cost is that a loop with work of its own between turns has no
seat while idle, which no chartered loop wants and a later workflow may
reopen through the front door.

**The invocation wait spans the same descriptors as the stream's, and a stop
heard there cancels the execution, on the operator's ruling of 2026-09-22.**
Before that ruling the wait on an open execution exchange read the gate
channel alone, so a stop dialed while a tool ran was deaf until the gate
answered, which #646's characterization measured at the whole of a two-second
gate delay against under half a millisecond mid-stream, with the tool's
result fed back to the model before the stop was heard. The wait now polls
the gate channel, the coordination listener, and an accepted coordination
connection at once, by section 2.4's `poll` and the serial loop of 6.1, one
mechanism in its third state. A dial is accepted and credential-checked as
every dial is, and a stop read there sends the contract's cancel on the open
execution exchange, at the continue position, and then keeps waiting for
that exchange's close, which the gate guarantees, the cancel having brought
the clock forward and added none. The completion is authored from whichever
of the four contents arrives, a kill naming the cancel in the common case
and the tool's own words where it finished as the cancel crossed, so the
record holds the invocation's end whatever it was. The turn then closes
stopped with no generation after it: the granted result is constructed at
the exchange's completion as section 6 requires and is fed to nothing, the
calls the emission still held are not opened, and the stop is answered on
the coordination seam after the close event is placed, per the
announce-after-record discipline. An observation read at the same wait
answers `Active` as it does between tokens. **The election flagged for the
operator: the stop cancels the execution through the gate rather than
closing the turn around it.** Closing around it leaves the tool running past
a turn that no longer exists, which the characterization saw as a live
process for the length of the stop, and narrowing the promise instead would
have amended `weaver-harness-PRD` section 2 and apex section 6 to carry an
exception neither states. #642's detached descendant is the gate's group
kill's to contain and is untouched by this clause.

```graph
node: harness-stop-polled-during-the-invocation
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-stop-polled-during-the-invocation

node: harness-stopped-invocation-feeds-no-generation
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-stopped-invocation-feeds-no-generation
```

The first claim's watch is #646's measurement inverted, and it is built so
that the route the ruling declined cannot pass it: the fixture gate holds
its answer for a delay it gives up only on a cancel correlated to the open
exchange, answering a kill by cancel then and its result at the delay's end
otherwise, so a harness that abandoned the exchange would close early with
the fixture still waiting and the completion never recorded. The watch
asserts one cancel and no second, the completion event placed before the
turn's close and the close before the stop's answer, the close well inside
the delay, and, in a second arm where the fixture answers a result as the
cancel crosses, that result recorded and the turn still closed stopped. It
fails when the coordination descriptors leave the wait, the close then
following the delay. The second claim's watch drives a two-call emission,
stops during the first, and asserts no append-and-generate after the stop
and no second execution opened, failing when the stopped check after the
completion is removed and the result feeds back. Both are authored ahead of
their code and stand uncited until the act that buys them cites them, per
Working Process section 6, and that act retires the characterization test
of issue #646, which asserts the deaf interval and must fail once the
interval is closed.

**The frame's parse is loop 0's, at the seat's threshold, and it refuses
rather than faults.** A frame's member decodes per the election of
`weaver-types-Spec` section 4.1, and the octets parse as one JSON value
against the request shape `weaver-gate-Spec` section 4 fixes: one object,
one `text` member and, from the operator alone, the seeding line's `role` member
of section 6.1, whose one value is `system`. What parses becomes the turn's content,
the text as the user message, or as the new prefix where the line is the operator's
seeding line, roles otherwise never the client's to name. What does not parse, the
transport decode included, answers as a refused turn on the frame's
exchange, the response frame's `refused` kind with its reason, and the
channel stands: the failure is the content's and never the channel's, which
is the layer split the frame election bought and this clause spends. **The
election flagged for the operator: whether a refused line takes a
bracket.** Two readings stand. Unrecorded, the refusal is answered and no
turn opens, which keeps the trace's kind set closed and leaves the record
blind to an authenticated client probing the mouth. Bracketed, the refusal
opens and closes a turn with its refusal marked, which accounts every
answered line and costs a front-door edit to the close's reason set. The
wiring act takes the unrecorded reading unless the operator rules otherwise
at this act's review, that being the reading no merged document contradicts,
and the ruling is owed before the epic's wiring act closes.

```graph
node: harness-parse-refuses-not-faults
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-parse-refuses-not-faults
```

**What this subsection does not elect, named so the deferral reads as the
corpus's.** The frame's interior closed on 2026-08-12, `weaver-types-Spec`
section 4.1 electing base64-carried octets by argument, and the parse
closed the same date with the turn-half act, the clause above spending what
the election bought. What stays open is the entry's concrete signature, a
satellite of section 9 the wiring act shapes against the chartered parse,
and the refused line's bracket, flagged above for the operator.

**The crossing is single and named, per the architecture seat's directive of
2026-08-13.** Loop 0 calls one named function in `dev_loop`, hands the
granted seat and the parsed request, and receives the response content, and
that call is the whole of the traffic across the dev boundary: no harness
reach into the loop's internals, no loop reach back into harness state
except through what the seat grants, the blade making any other reach a
compile error rather than a review comment. A trait would buy nothing the
one function does not, the loop being compiled in with no dynamic selection
chartered, so the crossing is a function and its signature is the section 9
satellite the wiring act shapes.

```graph
node: harness-frame-grants-the-seat
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-frame-grants-the-seat

node: harness-idle-report-authors-without-a-turn
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-idle-report-authors-without-a-turn
```

## 7. The failure vocabulary

```rust
pub enum ChannelFault {
    Truncated { bound: usize },
    Undecodable,
    Closed,
    SocketPathUnusable { errno: i32 },
    WrongPeer { uid: u32 },
}

pub enum Outcome {
    Left,
}
```

**Five faults, and the split from refusals is principled.** A refusal is a
typed answer on an exchange, and every refusal this crate issues or carries is
a `lifecycle-refusal`, the floor's type, drawn and never twinned. A channel
fault is a failure below the exchange layer: a truncated read, octets that do
not decode to an envelope and so cannot be attributed to any exchange for a
refusal to answer, closure, which `weaver-organ-channel` section 2 rules is
observed as death and never synthesized into an answer, a coordination name
that could not be bound, and a dialing peer refused at the accept before any
byte. **The last two arrive with the socket inversion of 2026-08-05 and this
block stood at three until it was read against the enum**, the bind and the
credential check being the clauses of section 2.3 that inversion landed. Neither
can be answered on
an exchange, there being no exchange yet in either case, which is the same
ground the first three stand on. `Outcome` names the one way service ends
ordinarily, the leave answered, so the composition root branches on a value
rather than a guess. **The split is review's by election and the outcome enum
is a type property.** A test can watch
a refusal answer where a fault should have ended the service, which is the reach
this suite declines while `weaver-gate-Spec` section 5 and `weaver-spu-Spec`
section 9 adopt the same split from their own sides, and the enum's one case is
exhaustive so a second reaches every caller loudly. **The split grounds in
apex section 5.3 and the outcome enum does not.** A contract naming the errors
it can return is what makes its refusal set closed, so a failure below the
exchange layer sits outside that set by construction rather than by taste, and
dressing one as a refusal would put a case on the wire the vocabulary clause
never drew. `Outcome` crosses no seam and is drawn by no contract, and its
exhaustiveness serves this crate's own caller.

**`Undecodable` stays one case, and the operator channel names what
refused.** The decoder's own error carries the specifics - an instruction
from a newer floor refuses on `unknown field: column-permission`, naming
the exact member the two builds disagree about - and the map to the closed
case discarded it, so a mixed-version deployment died saying nothing an
operator could act on and the diagnosis cost an hour it should have cost a
second, per issue #404. The decode that maps a failure to `Undecodable`
first prints a typed line on the operator channel naming the channel and
the decoder's detail. The enum does not widen: the wire carries the case,
per the paragraph above and `weaver-types-Spec` section 4.2's arithmetic,
and the specifics belong to the journal the operator reads - which the
clause of section 2.2 now also carries into the record where the death is
an organ's.

```graph
node: harness-undecodable-names-its-detail
kind: assertion
tag: perturbation

edge: asserts
from: weaver-harness
to: harness-undecodable-names-its-detail
```

**The outcome record is `harness-outcome-one-case` and was
`harness-outcome-two-cases`.** The slug said two over an enum of one variant
while section 3 argued the one case correctly on its own page, so the identifier
moved, with its two source headers and the doc comment that names it, rather
than the enum moving to meet the slug. The old spelling stands in the audit of
2026-09-13, which is a dated record of a reading and not a citation.

```graph
node: harness-fault-below-the-exchange-layer
kind: assertion
tag: review

edge: asserts
from: weaver-harness
to: harness-fault-below-the-exchange-layer

edge: grounds
from: harness-fault-below-the-exchange-layer
to: axiom-contract-is-a-complete-interface

node: harness-outcome-one-case
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-harness
to: harness-outcome-one-case
```

## 8. What is enforced, and by which instrument

Per apex section 11. The threat walks are stated with their mechanisms, per
the threat-anchoring rule of the external review of 2026-08-01, and each test
below is a walk made executable.

**The reference walk: a rogue elected tool reaches for the trace.** The
adversary is the agent's own tool surface, a subprocess this crate forks per
call, running as the agent uid with `bash` as the reference case. The attack
is inheritance: without close-on-exec every tool subprocess holds a writable
handle to the trace and edits its own account through a descriptor requiring
no path and passing no check, or an end of the member's channel and seats
itself as a second peer on that seam, the second capability the receive has
carried since the ruling of 2026-08-26. The mechanism is the one receive site
of section 2.3 asking for `MSG_CMSG_CLOEXEC` in the receive itself. The test
forks a child and confirms it holds no trace handle and no state end, watched
to fail when the flag is removed from the receive, which is the test
`weaver-trace-Spec` section 10 owed this document, landed and widened.

**The second walk: the same tool inherits a channel end.** The attack is an
inherited end handing the tool surface a capability that varies by which end
it got: a residency or gate end carries a lifecycle directive, a release
above all, and a decode end carries a resident session's traffic, which is a
different capability rather than a smaller one. The mechanism is the atomic
`SOCK_CLOEXEC` of section 2.2, no window between creation and flag. The test
forks a child and enumerates its descriptors, confirming that each organ fork
carries exactly the ends that organ is owed, two at the SPU's and one at the
gate's, and that nothing crosses a tool fork, watched to fail when the atomic
flag is dropped to a later `fcntl`.

**The third walk: a same-uid process attaches.** The attack reads or drives
the worker's descriptors through `ptrace` or `/proc/[pid]/fd`, no tool fork
required. The mechanism is the dumpable flag cleared at adoption, per
`weaver-admin-harness-contract` section 2, which reparents the proc entries
to root and refuses the attach. The test confirms the flag is clear after
`adopt` and clear again in a forked child's parent, watched to fail when the
clearing is removed.

**The fourth walk: the model reaches its own record through the prompt.** The
adversary is the model eliciting its measurement and custody events into
context, the recall-shaped breach `weaver-harness-PRD` section 2 names. The
mechanism is section 5's kind filter at the read site. The test assembles a
prompt from a structure holding measurement and lifecycle events and confirms
none of their content appears, watched to fail when the filter widens.

**Enforced by the compiler.**

- The run state's options make the partial fan-out representable and the
  unwind a checked match, so a forgotten arm is a compile error in the leave
  path rather than a leaked residency.
- The floor's three wire enums are exhaustive, so every directive, answer, and
  refusal case added later reaches this crate's matches loudly.
- Descriptors are owned types end to end, so a handle that escapes its owner
  is a move the borrow checker sees, not an integer copied silently.

**The in-run event table is enforced by a decision function and its table**, on the
operator's agreement of 2026-10-09 recorded on #1, owed by the lifecycle act's code.
`weaver-admin-Spec` section 3's transitions for events inside a run (state x dialer
request, tool return, turn close, a missed save point leg, a death) are this crate's
decision: one pure function from the run's position and the event to what this crate
does and records, an exhaustive `match` with no wildcard arm, so a new position or
event does not compile until it has a row: a compile-time pin. It is paired with a
table-driven test transcribed row for row from that table, which fails when any row's
expected act or record changes. **The I/O paths take the decision's result and do not
decide**: the gate's refusals, the record's authoring and the save point's legs are
given what the function returned, and are tested by integration cases over those
paths, not one per row.

**Enforced by compile-fail tests, because the property is an absence.**

- One constructor: code constructing a `Harness` other than through `adopt`
  fails to compile, the fields being private and no second path existing.
- No path-taking surface: doctests handing `channel.rs` a `&str`, a `String`,
  and a `PathBuf` where it takes owned descriptors each fail to compile,
  three named shapes with the general prohibition staying review's, per the
  split the floor Specs make. **The split is two assertions rather than one,**
  the pinned shapes here and the prohibition itself at the section 2.3 clause
  that argues it: a single record tagged for the mechanical half would claim
  the doctests for the whole, which is the overclaim this corpus refuses in
  prose and has no reason to admit in a graph.

**Enforced by the manifest.** The internal dependency set is exactly the two
floor links, the trace seam, and the diagnostic mechanism section 1 counts as the
fourth, read against the graph under gate H2. No async runtime, no logging crate,
and no HTTP client in the resolved external tree, by the build-time `cargo tree`
assertion the floor Specs share.

**Which invariant each claim serves, and why most serve none.** Nineteen of the
sixty-eight carry a `grounds` edge and one of the nineteen carries two, so the edges
number twenty: ten to `axiom-floor-is-vocabulary-behavior-is-socket`, four to
`axiom-harness-integrates-by-the-loop`, three to
`axiom-join-key-travels-with-the-work`, and three to
`axiom-contract-is-a-complete-interface`. **`axiom-organ-and-submodule` takes
nothing from this crate,** which reads oddly for the hub that invariant names and
is right: the hub property is topology, which crate holds a two-initiator channel
with which, and this document sources no topological record. The charter sources this
crate's node, its parent edge, its floor links, and its one declared seam, per
section 0, and everything below that line is this crate's interior or its
channels' mechanics. A crate is an organ or is not by a test read against the
apex, and no claim here is that reading. **That 5.4 still takes nothing while 5.5
takes four is the two sections' own division working rather than an inconsistency.**
5.4 fixes the topology and the charter is where the topology is stated, so a Spec has
nothing to add to it. 5.5 is the work the topology exists for, and a Spec saying how
the loop stands the organs up and takes them down is arguing exactly that work.

**The test applied is whether the axiom is the reason the claim exists, or whether
the claim is a precondition of the axiom's own stated reason,** the second relation
per Document Format section 4. Remove the socket invariant and this crate has
no reason to bound a receive, to demand that one write arrive as one read, to flag a
descriptor before an exec, to create a pair before a fork, or to keep a loop from
minting a port, so those ground in it. Remove it and `nix` is still the OS crate,
descriptors are still owned types, the child's ends still land at 3 and 4, and the
fork still runs a bounded call list, so those ground in nothing. **Forty-nine claims
grounding in no invariant is the expected result and not a gap**, per Document
Format section 4: most of what a Spec elects is a format, a name shape, a count, or
an ordering of its own interior, and representation is what the invariants are not
about. **Both figures in this paragraph read against the sixty-eight**, the
disclosure below covering the narrative below it and not this one, which is the
reading two findings of the audit of 2026-09-13 reached opposite answers on. The
numerator has not moved since it was written and the denominator had, so the
paragraph read as a live count and was one, wrong by eighteen in both terms.

**Four calls are worth stating rather than left to be read.** The descriptor
numbering is ungrounded on the apex's own words, 5.1 leaving how a far end
travels to the contract governing that seam, so the number and the order are
contract material while the flag on the end is the invariant's. The dumpable
flag is grounded where a process-hardening set might have been passed over,
because 5.1 rests possession-as-authentication on no third party reaching an
unaddressed socket and `/proc/[pid]/fd` is that address. The authorship claims
of sections 2.3, 3, and 4 ground in 5.2 while their neighbours do not, who
stamps and who authors being the sole-writer half and what an event carries
being content. And conformance to one contract clause draws no edge, or every
citation in this document would draw one: what 5.3 grounds is completeness, so
the channel state's totality carries an edge while left-follows-drain and
announce-after-record do not.

**The fifth invariant reaches four claims here, and the scope limit is what keeps
it to four.** 5.5 binds what crosses between domains and leaves what happens inside
one to the organ, so a claim reaches it only where the reason cannot be stated
without naming a second organ's ordering. The gate pair's wait on confirmed
residency is the plainest, one organ's readiness gating another's construction. The
run state's options are the same fact held as data, the loop knowing across domains
what it must later undo across them. The thread the organ forks run on is a timing
property the gate relies on and cannot hold from where it sits. The port blade is
the fourth and takes a second edge rather than a moved one. **Three near misses are
worth naming, because the crate this invariant names is where a wide reading would
do the most damage.** Left-follows-drain reconciles the drain against admin's
answer and the drain is this crate's own domain, so one organ is named and not two,
and the reverse unwind order its paragraph opens with is argued at the gate pair's
wait instead. Announce-after-record is that same shape at the stop. The descriptor
numbering stays declined for the reason the labelling pass gave, 5.1 delegating how
a far end travels to the contract governing the seam, and 5.5 does not take it back:
where an organ finds its end is contract material the organ presents rather than an
ordering the loop reconciles.

**Two records landed 2026-09-02 and are counted by this sentence rather
than folded silently into the totals below**, which predate them: the last
word of section 2.2 and the named detail of section 7, both perturbation.

**Where the assertion records sit, and which of these bullets another crate
declares.** The records are at the clauses that argue the claims, across
sections 1 through 7, rather than gathered here, per Document Format section 6:
this section sorts by instrument and the arguments are elsewhere, so a block
here would sit apart from the prose that earns it. One record is the exception
and sits at the end of this section, the doctest pinning of the three
path-taking shapes, whose argument is nowhere else and whose general
prohibition is section 2.3's. Sixty-eight records in all, measured from the records
and re-measured 2026-09-15 at the same figure, where the narrative below
reads forty-eight as of the inversion of 2026-08-05 and did not move with the acts
since, which is named here rather than restated. **The disclosure reaches the
narrative below this sentence and never the paragraphs above it**, stated because
it was read both ways: the grounded and ungrounded figures above are live counts
and are re-derived from the records. Forty-eight as of the inversion of
2026-08-05, which retired the adopted end's close-on-exec, that end no longer
existing, and added the coordination bind, the accept's credential check, and
the bind's refusal to unlink, all three of section 2.3. Twenty come from this
section's sorting with the four walks counted in and twenty-eight from the
elections outside it, the elections taking nodes because gate H1 would
otherwise leave the largest decisions in this Spec untraceable. Two of the
twenty carry a review tag rather than a mechanical one, the path-taking
prohibition and the child handoff's unconditional flag clear, each being the
half a split divided out of a bullet this section already carried, and a
divided half counts with the bullet it came from, per Document Format section
3. **One bullet above is a claim another crate argues,** and carries no record
here: the floor's three exhaustive wire enums, which are `weaver-types-Spec`
section 4.2's. **Three of the claims sorted here discharge owings that Specs
other than `weaver-types-Spec` filed against this document,** the three that
Spec files being the paragraph below's, and each record sits at the clause
that argues it rather than at the sorting: the first walk's close-on-exec test
that `weaver-trace-Spec` section 10 owes, at section 2.3, the licensed
combinations that `weaver-traits-Spec` section 7 owes, at section 4, and the
second walk's descriptor placement with the fork discipline that
`weaver-gate-Spec` section 0 cites as this document's, at section 2.2.

**Five claims this crate asserts are declared by another document, and
`src/replay.rs` is where they are held.** `weaver-diagnostic-Spec` sections 3.3
and 4 argue that the record identifies itself at the open, that an absent
identity is not invented, that an outcome is not manufactured, that a divergence
position is the resident length at the draw, and that an unaccounted input is
refused at identity. This crate is the author and that one the mechanism, per
`weaver-diagnostic-PRD` section 1, and the instrument for each stands in this
crate's suite, so each of the five carries an `asserts` edge from here beside the
one from the crate whose record it describes. The instruments are the in-file
suite of `src/replay.rs`, eight tests carrying nine citations across the five, and
the file's own header cites all five for the module. **The sixty-eight above does
not move**, no record landing here, and `weaver-diagnostic-Spec` section 7 is the
authority on the five claims' wording, on their sorting by instrument, and on why
the edge crosses at all.

**`src/replay.rs` carries fourteen citation lines and every one of them names a slug
this crate does not prefix**, which is why the file is named here rather than
left to section 1's layout. They cross because the mechanism and its author are
two crates by the charter's own division rather than because a header reached for
the wrong slug, and the second edge is what makes that checkable from the graph
rather than by noticing a prefix. **No instrument compares a citation's slug
against the crate of the file carrying it**, so a later crossing is found by a
reader and not by a gate. The reading that would have closed that was
`act-20-census-readings` of issue #569, which closed under the ruling of
2026-09-27.

**`weaver-types-Spec` section 5 files three owings against this document, and
all three now carry a record.** The truncation half of the pair test has been
this crate's since the Spec was cut, argued and recorded at section 2.1. The
boundary half of that same test, one envelope write arriving as one envelope
read, was unnamed here until this act, filed as issue 35 and landed at the
section 2.1 clause that argues it with its perturbation named below. The
out-of-order refusal is the third: it was stated at section 3 and sorted by no
instrument, filed as issue 32, and it lands at that clause on the shape
`weaver-gate-Spec` section 6 settled for the gate, the compile pin holding that
the refusal reaches a match arm and the perturbation holding what the arm then
does. Each record is this crate's side of an owing and no other organ's, so
issue 35 closes on the pair-creating crates together while issue 32 stays open
for the organs whose side is unwritten.

**Requiring a perturbation-verified test, beyond the four walks.**

- Truncation is a fault: an envelope over the 64 kibibyte bound produces
  `Truncated` and no directive, confirmed by watching a silently shortened
  directive decode when the `MSG_TRUNC` check is removed.
- One write is one read: two envelopes are written back to back on a created
  pair and both writes complete before either read, and two reads return
  exactly one envelope each, confirmed by watching the first read return both
  when `SOCK_SEQPACKET` is changed to `SOCK_STREAM` at the `socketpair` call.
  **Two messages are what make the watch reachable.** A single small envelope
  crosses a stream socket whole, so a one-message test would pass under the
  substitution and pin nothing, which is the never-failing perturbation apex
  section 11 counts as worse than no test. The truncation bullet above cannot
  see the substitution at all, which is why the boundary the type was elected
  to buy needs a watch of its own, per section 2.1.
- A directive out of order is refused and not queued: a leave arriving before
  any enter answers `OutOfOrder` and reaches no unwind, and a directive
  arriving after a leave answers the same, the left position being terminal,
  but for `Observe`, answered `InTransition` while a leave is pending and
  `Unloaded` once it is left, and `JoinLeave` while a leave is pending, the one
  join the envelope admits. The compile pin of section 3 holds that the refusal reaches a match
  arm rather than a flag check, and this holds what the arm then does, an arm
  being free to queue or to answer the wrong refusal while compiling exactly as
  well. Confirmed twice, by watching the early leave reach the unwind path when
  the before-enter arm stops refusing it, and by watching a left channel accept
  an enter when the terminal arm is collapsed into the entered one. The refusal
  is owed to each organ by `weaver-types-Spec` section 5, which enforces it
  nowhere, and this discharges this crate's side of that owing alone.
- The unload's tool interrupt and the first outcome (`weaver-admin-Spec` section 3,
  the graceful unload step 2 and S4 x tool return; section 6 item 2), owed by the
  lifecycle act's code: with a tool call out when a leave is heard, this crate sends
  `ToolInterrupt` before `Quiesce`; a result that crossed the interrupt is recorded
  completed with its result, delivered to the turn and not re-runnable, and only a
  call the interrupt ended is recorded `Killed { by: unload }`. Confirmed by watching
  the test fail when the crossed result is recorded interrupted, and when `Quiesce` is
  sent ahead of the interrupt.
- The declared bounds turn the leave forced (`weaver-admin-Spec` section 3, the bounds;
  section 6), owed by the lifecycle act's code: under an enter carrying `drain_bound`
  and `wind_down_bound`, a drain or a wind-down past its bound turns the leave forced
  from where it stands, `Left.forced` true, the `unload` event naming no `forced_by` and
  keeping the graceful caller's `cause`; under an enter carrying neither, the same
  stand-in turn is waited for; under an enter carrying `drain_bound` alone, the drain
  past it turns the leave forced while a wind-down past any time is waited for, and
  under `wind_down_bound` alone the reverse, each phase held to its own bound or to
  none. The bounds are read from the enter alone. Confirmed by
  watching the test fail when the harness ignores the enter's bound, when it writes
  a `forced_by`, and when a lone bound is applied to the other phase too.
- `Observe` answers `InTransition` while a leave is pending (`weaver-admin-Spec`
  section 3, `show` in S4 to S7 and S10; section 4), whether a turn runs or not, owed by
  the lifecycle act's code: an `Observe` heard in the drain with a turn running, and
  one heard in the wind-down's save point legs with none, each answers `InTransition`
  and authors nothing. Confirmed by watching the test fail when the drain's answer is
  `Active`.
- `unload` is the run's terminal event (`weaver-admin-Spec` section 3, I9; section 6
  item 6), owed by the lifecycle act's code: with a stand-in SPU that dies in its
  release, the record carries the `fault` and then `unload`, whose `release.spu` reads
  `Unconfirmed`, and no line follows `unload`; with both organs releasing cleanly,
  `release.spu` reads `Confirmed` and `release.member` reads `Closed`; and with a
  recorder whose queue crosses its high-water mark at the `unload` submit, no pressure
  `fault` follows `unload`. Confirmed by watching the test fail when the release is
  moved after `unload`, the fault then landing past the terminal event, and when the
  depth check runs after `unload`, a pressure report landing past it.
- Announce-after-record: a stop's answer follows the close event's placement,
  confirmed by watching the answer precede the record when the two are
  reordered.
- The scoped refusal account: a refusal before the `load` event leaves the
  stream empty, and one after it leaves a bracket with no `unload`, each
  confirmed by watching the account degrade when the authoring point moves.
- The licensed combinations: an `Assistant` message carrying a `ToolResult`
  block is refused before submit, confirmed by watching it reach the recorder
  when the check is removed, per `weaver-traits-Spec` section 7.
- Deterministic assembly: one working structure assembles one prompt,
  byte-identical across runs, confirmed by watching the comparison fail when
  iteration order stops being sequence order.
- The order of parts: a prompt assembled from an identity, one message, and one
  schema renders the three in that order, confirmed by watching the assertion
  fail when the render puts the schemas before the messages, and again when it
  puts the identity last. **The determinism bullet above sees neither move**,
  both reorderings being deterministic, which is why the order carries a watch
  of its own rather than riding the comparison. Retagged from `review` here,
  the election having named this test as the one not bought while the suite
  held it.

```graph
node: harness-path-shapes-pinned-by-doctest
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-harness
to: harness-path-shapes-pinned-by-doctest
```

## 9. Open elections

Each names what settles it, and none is this Spec's to settle alone.

- **The executor, closed 2026-08-11 in the negative.** Loop 1's seat and the
  decode surface it composes against are chartered at section 6.1, and the
  runtime question they carried is answered by the streaming ruling of that
  date: the streaming path is served by `poll` waiting against the decode
  channel and the coordination listener, with the service serial and one turn
  in flight, so no runtime enters and section 1's no-runtime rule stands.
  Recorded as closed rather than deleted, this list naming what settled each
  entry, and the latency measurement of `weaver-traits-Spec` section 6 remains
  owed as a number rather than as a question that could reopen the rule.
- **Stop mechanics at the decoder is closed.** The stop lands at the token
  boundary, ratified at the token workflow's charter act of 2026-08-02 and
  carried at `weaver-spu-PRD` section 13.5, with the family's turn terminator
  made resident before the answer returns. Recorded as closed rather than
  deleted, this list naming what settled each entry.
- **The seat's criterion under a Gateless binding, closed 2026-08-31.**
  Section 6.2 carries the second criterion: the seat granted once at the
  run's opening, on the run itself as the work, the sealed preload having
  arrived owed its certification. The two criteria partition by binding and
  neither widens the other. `diagnostic-replay-loop` section 2 was the
  creditor and its entry clears with this act.
- **The tee back-pressure election.** Blocking, shedding marked, or detaching
  marked, per `weaver-admin-operator-contract` section 3. A measurement
  against a real consumer at a real rate, taken with the queue's high-water
  mark of `weaver-trace-Spec` section 12, the two settling together.
- **The fault payload's shape is closed.** The case set closed across all
  three organs on 2026-08-02 and the shape landed at `weaver-trace-PRD`
  section 3.2 as the floor's `fault-report`, so section 3's fault path is
  shaped and ungated.
- **The diagnostic recorder's surface, settled 2026-08-27 and recorded here as
  closed.** A diagnostic run authors a diagnostic-trace through
  `weaver-diagnostic`, this crate being the sole writer under either kind and the
  kind selecting the mechanism, per apex section 6 as ruled 2026-08-24. The shape
  this item held open is elected in `weaver-diagnostic-Spec` section 5: the member
  mirrors `weaver-trace`'s receive and submit surface and shares no type with it, so
  one call site here serves both mechanisms and the binding's kind selects the arm.
  What this Spec owes against that election is the arm itself, which the code act
  carries, this section holding the election rather than the implementation. An
  earlier form of this item had the measurement relayed outward to an assembler
  outside the agent, which the ruling replaced with the member composed inside.
- **The config read.** No chartered workflow reads the declaration from this crate, and
  the crate links no parser: admin reads the file at its inventory and what this crate
  receives is the enter, the sink's descriptor, and the loop's path on the worker's
  vector, per `weaver-admin-Spec` section 4. The read this charter once declared returns
  with the tool workflow, which consumes the tool set and the permission mode, and the
  charter's section 5 names that re-entry in place of the edge it retired on 2026-09-04.
  **The sink half of this item closed the same date, per issue #311**: the config's
  `trace-sink` field names the sink and the charter has the agent never told the name,
  and the two never disagreed, because the charter's own descriptor paragraph scopes
  them. The harness never resolves a trace path, the descriptor is opened by an admin
  principal and crosses the coordination socket with `SCM_RIGHTS`, and
  `weaver-admin-Spec` section 5 asserts the one open site the path dies at, so the field
  is admin's to read and `weaver-types-Spec` section 2 now names it so. The kernel's
  search-bit lock stands between knowledge and reach either way, and nothing here
  retains the field because nothing here ever receives it.
- **Tool dispatch, the execution context, and the permission-mode
  enforcement point.** Blocked with `tool-trait`, per `weaver-traits-PRD`
  section 3.1.
- **The satellite types.** `AdoptionFault`'s case set, `OrganChannel`'s
  exchange-surface spelling, `SpuChannels`' name, the licensing error's shape,
  `DecodeChannel`'s token send and receive surface, now chartered in section
  6.1, the loop entry's concrete signature, deferred with the client line's
  parse per section 6.2, and the
  channel-state enum's name. Identifier and shape choices with no
  cross-crate consequence, listed so what this Spec leaves to a builder is
  complete rather than implied.
