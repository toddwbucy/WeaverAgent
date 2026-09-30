# weaver-spu - Spec

**Status:** MERGED. Cut 2026-08-02, seventh of the Spec pass and the last of the set.
Code is written against it under the gates of Working Process section 6.

**Date filed:** 2026-08-02
**Document ID:** `weaver-spu-Spec`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** #731

---

## 0. What this document is

Build instructions for `weaver-spu`: the binary's layout, the two channel ends
it holds, the residency mechanics, the decode submodule's interior, the family
libraries' shape, and the elections a builder would otherwise invent. It is
derived from `weaver-spu-PRD` and from the two contracts this crate is party
to, `weaver-harness-spu-contract` and `weaver-harness-spu-decode-contract`,
together with `weaver-organ-channel`, the drawn material the first of them
draws.

Level discipline. The charter says what the crate needs and why. This document
says how it is represented, and per gate G2 it elects against grounds the
charter and the contracts state rather than developing grounds of its own.
Where this document and the charter disagree the charter yields nothing.

**This document declares its crate's assertion records and no other record,**
per Document Format sections 3 and 4 as of the notation of 2026-08-03, which
retired the no-records sentence this paragraph replaces. The charter stays the
source of this crate's node, its parent edge, its two floor links as this act
corrects them, and its two declared seams, and a Spec that restated any of them
would give the mapper two sources for one record, per that format's section 1.
What this document sources is the claims code must conform to, declared at the
clauses that argue them rather than gathered in one place, per that format's
section 6, and `asserts` runs from the crate rather than from this document,
which is why the document needs no node of its own.

**A claim this Spec cites and another Spec argues carries no record here,** and
there are nine of them, named again in section 10 with the crate that declares
each. Five are `weaver-types-Spec`'s. The floor's exhaustive wire enums are that
Spec's section 4.2. The `SOCK_SEQPACKET` election, the 64 kibibyte envelope
bound that election carries, and the lifecycle channel's JSON encoding are its
section 4.3, the bound being a record that Spec declares beside the election
rather than inside it, which makes the first two a pair of records and not one.
`ModelBinding`'s ordered device set is its section 2. Three are
`weaver-harness-Spec`'s: the descriptor numbering with the order of the two ends
of section 2.2, the OS-surface election of section 2.4, and the fork discipline
that leaves this process two inherited descriptors, of sections 2.2 and 8, which
`weaver-harness-spu-contract` section 1 requires of the harness and this crate
relies on rather than re-tests. The ninth is `weaver-trace-Spec`'s, the
absent-rather-than-zero serialization of its sections 3 and 10, which this
crate's own refusal to produce an empty vector meets from the other side.

**It is written from the merged corpus, with the salvage survey as its quarry
map.** The salvage survey, at
`git show 0bca344:docs/archive/weaver-spu-salvage-survey.md`, records what the quarry
tree holds, what maps across, and the seven places its working code yields to
merged rulings, the seventh filed with the device-assignment ruling. This
document elects against the merged corpus and cites the
survey where a mechanic's provenance matters, per the ruling of 2026-08-01
that keeps the old tree's Specs out of the pass.

**Two corrections to merged documents land in this act, because this Spec
cannot be written truthfully without them,** and both are the token
workflow's own reach arriving late. The charter's section 6 said this crate
does not link `weaver-traits`, and the framing ruling made the family library
render the harness's canonical messages, so it does. The harness Spec's
handoff placed one inherited descriptor at number 3, and the decoder cut gave
this crate a second channel, so the placement needs two numbers and an order.
Each is corrected where it lives, in this act, rather than filed.

## 1. The crate

**One binary, forked and exec'd by the harness during the enter fan-out,** per
apex section 12. Nothing links this crate: it is a process the harness starts,
not a library it calls, which is what the seam being a socket rather than a
link means read as a manifest fact.

```graph
node: spu-one-binary
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-one-binary

edge: grounds
from: spu-one-binary
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**Layout.** The crate is the umbrella's substrate, per charter section 2: the
family libraries and the socket presentation, with each semantic domain in its
own submodule. One submodule exists. The rows below are read from
`crates/weaver-spu` rather than kept beside it: twenty-five modules under `src`,
the seven family modules standing on one line here, with the build script and the
kernels directory. The test targets are section 1.2's and are not repeated here.
**Nothing holds this table to the tree**, and it drifted to two paths no file
answered to and six unlisted modules before the reading of 2026-09-15. The walk
that would catch the next drift belongs beside the fixture surface's own, which
section 1.2 names, and is not bought here, so a reader wanting the rows current
takes the walk rather than trusting them.

    src/lib.rs                  the module declarations, and the lib target the
                                doctest instruments run under
    src/main.rs                 entry, the hygiene sets, the service loop
    src/bin/classify.rs         the classify arm's own binary, section 11
    src/channel.rs              the two channel ends and envelope I/O, section 2
    src/residency.rs            admit and release, and the device judgment that
                                is the third free step, section 3
    src/artifact.rs             resolution and the header read, the first two
                                free steps, and the weights hash, section 3
    src/measurement.rs          measurement production, section 6
    src/readout.rs              the residual tap and its reduction, section 7
    src/sampling.rs             the knobs and their dispositions, section 8
    src/family/mod.rs           the family surface and its registry, section 5
    src/family/<name>.rs        one module per family, seven of them, template
                                and parsing
    src/decoder/mod.rs          the decode submodule, section 4
    src/decoder/session.rs      the resident session and its append path
    src/decoder/backend.rs      the backend seam, GGUF and native as peers
    src/decoder/gguf.rs         the GGUF backend
    src/decoder/gguf_tap.rs     the GGUF tap, the readout's one unsafe surface
    src/decoder/native.rs       the candle-native backend
    src/decoder/native_pair.rs  the native forward sharded across two devices
    src/gpu/mod.rs              the device queries the admission judgment reads,
                                compiled under `cuda` alone, section 3
    build.rs                    the kernel compile, gated on `cuda`
    kernels/transformer.cu      the salvaged kernels, section 10
    kernels/PROVENANCE.md       what crossed and what has not

**`kernels/transformer.cu` would owe a conformance header and the verbatim carry
is what it would owe it under**, CUDA being a supported unit since the operator's
ruling of 2026-09-17 and the header rule suspended until release since
2026-09-27. **No act may write that header while the carry stands**, and the
reason rather than the reading is what this row states: the file crosses verbatim
under section 10's claim, `kernels/PROVENANCE.md` records the hash that makes the
claim checkable, and an edit adding a header falsifies the very assertion the
header would cite. That file carries the rest of it. The header the kernel does
carry is the previous program's and resolves to nothing here, and which assertion
the kernel should cite is a question for this Spec at the act that takes the
kernels off the verbatim carry. The obligation is therefore reported rather than
exempted, an exemption being a hole where a reading is an answer, and until release
nothing reads its standing, the census that read it having retired on 2026-09-27.

**`src/bin/classify.rs` is a second `[[bin]]` target, and `spu-one-binary` above
reads as one.** The manifest declares two, and section 11 states the second in
prose, so this table and the clause that opens this section disagree. **This act
reads the layout and does not re-cut the record**, because which way the clause
settles is not a layout reading: narrowed to the serving binary it leaves the
classify arm as a declared second target, retired it takes with it whatever the
record was buying, which on the prose beside it is that nothing links this crate
rather than that one target exists. The second moves the record set and the
counts section 10 states. The finding is at
`docs/project/audit-2026-09-14-the-corpus-against-v5.md` under the heading on a
manifest tag its own manifest falsifies. **The sharper half is that the
instrument does not exist**: the manifest's own comment says a manifest test
checks for exactly one `[[bin]]` target and `crates/weaver-spu/tests/manifest.rs`
checks nothing of the kind, which is the unenforced-read-as-enforced conversion
apex section 11 warns against. That comment and that file are the crate's to
answer, as is `src/lib.rs`'s header, which states the claim a third time.

**A later operation type is a sibling of `decoder/`, in its own process.**
Per charter section 13.1 the encoder and the other operation types arrive as
their own submodules with their own sockets, and per the operator's ruling of
2026-08-02 each runs in its own process under this domain root. Nothing is
laid in for them: no trait, no variant, no feature, no socket bound early. What
this layout does provide is that adding one is adding a directory beside
`decoder/`, which is the reversibility test passing on this crate's own future.
**The absence is review's, and the ground is stated rather than assumed.** Its
feature half is mechanical already, section 1.1's gates being a manifest fact,
and what remains is an absence with no name to pin: a doctest fixes a named
shape and there is no shape here, the claim being about what was not written for
a submodule that does not exist. This is one of the few places in this Spec
where review is the instrument available rather than the instrument unbought.

```graph
node: spu-nothing-laid-in-for-later-operations
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-nothing-laid-in-for-later-operations
```

**Edition and toolchain.** Edition 2024 on the pinned nightly. Unlike the
floor, this crate may use a nightly feature where the GPU path needs one, and
names any it takes at the site, because a nightly requirement here is a
requirement in one binary rather than everywhere.

### 1.1 The dependency set, and each is argued

**Internal, and one of them is a correction.** `weaver-types` is the charter's
floor link, taken **without its `config` feature**, because this crate reads no
configuration file, per charter section 3, and a parser in a process whose
argument is that it holds little is the weight `weaver-types-Spec` section 1
gates against. **`weaver-traits` is now a second floor link, which corrects
charter section 6 in this act.** That section said this crate draws no trait
and ended by leaving the answer to the decode workflow, and the decode workflow
changed it: the framing ruling of 2026-08-02 has the family library rendering
the harness's canonical messages, and
`weaver-harness-spu-decode-contract` section 7 draws the message model from
`weaver-traits` into this crate accordingly. The link follows the draw, per apex section
5.3, which is what makes the party list checkable against the dependency graph. **This
record carries both floor edges.** That the crate links the floor at all is 5.1's, the
floor being shared vocabulary a socket cannot carry, and that `weaver-traits` in
particular is linked is 5.3's, the draw determining the set. The feature the
`weaver-types` link is taken without grounds in nothing, being a build election that
would read the same under any invariant, per 5.1 as corrected on 2026-08-04.
`provider-trait` is still not implemented here and the charter's reasoning for that
stands untouched: the abstraction lives at the harness's composition root, on the far
side of this seam's transport.

```graph
node: spu-two-floor-links-types-without-config
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-two-floor-links-types-without-config

edge: grounds
from: spu-two-floor-links-types-without-config
to: axiom-floor-is-vocabulary-behavior-is-socket

edge: grounds
from: spu-two-floor-links-types-without-config
to: axiom-contract-is-a-complete-interface
```

**External, the model half.** The two fork pins are the load-bearing dependencies of the
whole crate and the survey names them preconditions. `llama-cpp-2` and `llama-cpp-sys-2`
come from the same git source and revision, never one from the fork and one from the
registry, because two sys crates resolve to two distinct types for one C library and the
flash-attention policy type stops typechecking across them, which the archived tree
recorded against itself. The fork exists for one reason, the ggml scheduler's eval
callback, which is the only route to per-layer activations from a GGUF model without
replacing the engine, and section 7 makes that seam a compile-time pin rather than a
comment. `candle-core`, `candle-nn`, and `candle-transformers` come from their own
pinned fork for `forward_with_intermediates`, the readout's working path, and the qwen2
rotary precision fix: the angle is computed in fp32 and the sine and cosine are cast to
the model dtype afterwards. The pair builder holds that ordering in this crate and is
read directly, and the single card holds it inside the fork's private rotary, which is
watched through the generation: past position 256, the first integer BF16 cannot hold,
the single card agrees with the pair only while both keep the angle. This describes
revision `aee9af9c3e0b23ab9188a492d0b3ebdb5f3a3f4b`, which the pins below record.
`cudarc` is caret-pinned rather than exact, so Cargo unifies this crate's device handles
with candle's inside one minor line, exactness coming from the lock file rather than
from a requirement that becomes unsatisfiable the day candle raises its floor. The
manifest requires `0.19.8` and the lock resolves `0.19.10` as of #684, the first release
whose build script accepts the CUDA 13.4 toolkit.

**The pins, recorded here because no document of this corpus carried them
until 2026-08-06.** `llama-cpp-2` and `llama-cpp-sys-2` both resolve to
`https://github.com/toddwbucy/llama-cpp-rs` at revision
`ecce255bcb14dd6d88f184cc8776c23a85afafeb`, moved 2026-08-17 from `277e4100`
by the split-artifact ruling: the fork gains `load_from_splits`, the
explicit-set door llama.cpp holds for callers whose paths carry no sibling
pattern, which is what a descriptor path is, and the build script's DLL
relink becomes idempotent under concurrent builds. `candle-core`, `candle-nn`,
and `candle-transformers` resolve to `https://github.com/toddwbucy/candle` at
revision `aee9af9c3e0b23ab9188a492d0b3ebdb5f3a3f4b`, moved 2026-09-26 from
`429615148582d62f285879b97cb63dbdd17b47fd` on the operator's ruling of that date.
The new revision is the fork's merge of its pull request #33 and holds the old
one as an ancestor, so `forward_with_intermediates` and the qwen2 fp32 rotary
fix carry over unrewritten. It adds upstream through `aebc405d` of 2026-09-25,
which brings cudarc `0.19.10` and removes `ug`, and the fork's fix for upstream
regression #3972, where `rms_norm` failed on every checkpoint without a norm
bias. **No revision between upstream's of 2026-09-02 and this one is a safe
pin without that fix**, which upstream has not yet merged. **A bf16 reading
does not carry across this move**: greedy tokens agree while logits move by up
to 0.5, reduction order changing under upstream's kernel work, and f32 is
bit-identical, so a bf16 figure compared across the two revisions is re-taken
at this one. **`candle-flash-attn`
is not a fourth name resolving to it,** the manifest declaring it nowhere and
`Cargo.lock` holding it nowhere: it shares the pin as a value rather than as a
resolution, which is what the manifest states and what this clause overstated
until 2026-09-15. The manifest delegates that value to this section by name, so
this clause is the only place it lives. No branch is named for either fork,
because a branch tip moves and a pinned revision does not. Both are full
revisions rather than abbreviations, because an abbreviation is unambiguous only
until the repository
it names grows a collision, and this Spec is the durable record.

**Vendoring to crates.io was tried against `0.11.0` on 2026-09-28 and cannot land at
that release**, which is the record #699's items 639.2 and 639.3 ask for where the move
does not go. Read from the published sources rather than from the fork's account of
them, crates.io `candle-transformers` 0.11.0 carries no `forward_with_intermediates` in
any model, its `qwen2` builds the rotary tables BF16-first, the ordering the fix of this
section retired and its single-card watch fails on,
`the_single_card_keeps_its_rotary_angle_past_position_256` in `tests/native_loaded.rs`,
and crates.io `candle-nn` 0.11.0 lacks the fork's `layer_norm` lookup of a legacy weight
and bias name, the fix for upstream regression #3972. The `cudarc` requirement matches,
`0.19.8` caret on both, so the lock is not what stops it. The fork stays until an
upstream release carries all three, and the pin above is the one to move it from.

**Why the values sit in a Spec at all, which is a question worth answering.** A
revision is representation and belongs here rather than in the charter. What
made recording them urgent is that they lived in exactly two places, the
archived tree's own manifest and a workshop note under no version control, and
the first of those is what phase two's closing checklist deletes. The survey
delegated them, saying both pins are named in the workspace's standing notes,
which was true and is not a home: a note outside every repository is outside
every gate. The G6 sweep of 2026-08-06 found the delegation and this clause
closes it. The forks themselves are external and survive the deletion, so what
was at risk was never the code, only the two numbers that say which commit of
it this program was built against.

**They are verified at first use and not assumed from this page.** The survey's
precondition stands: the llama-cpp fork is confirmed to expose the eval-callback
seam and the candle fork to carry `forward_with_intermediates` at the moment the
crate first builds against them, per section 7's compile-time pin, because a
revision recorded in a document and a revision that still does what the document
claims are two different facts.

```graph
node: spu-fork-pins-resolve-as-pinned
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-fork-pins-resolve-as-pinned

node: spu-cudarc-caret-pinned
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-cudarc-caret-pinned
```

**External, the rest.** `serde_json` for the lifecycle channel's envelopes,
whose JSON election is `weaver-types-Spec` section 4.3's and carries no record
here. `nix` for the OS surface, on the grounds `weaver-harness-Spec` section 2.4
argued and this crate inherits: the calls are `recvmsg`, `fcntl`, the dumpable
`prctl`, and the descriptor adoption of section 2. `safetensors` for the native
backend's weights, `half` and `bytemuck` for host-side tensor math,
`tokenizers` for the artifacts' tokenizers, `blake3` and `walkdir` for the weights
hash of section 3.

**What does not cross, from a working tree that had it.** No HTTP client and no
model downloader: an artifact this crate cannot reach is a refused admit, per
charter section 4.1, and fetching one would make this crate a provisioner of
the operator's own artifact. No logging crate, per the corpus's one-account
rule. No async runtime in this pass, the service being one directive at a time
against one resident session, and the executor question belonging to the
measurement `weaver-traits-Spec` section 6 names. No multi-model catalog and no
apparatus for holding more than one agent's model in one process, both dissolved
by one SPU per agent, per the survey.

```graph
node: spu-no-runtime-no-logging-no-http
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-no-runtime-no-logging-no-http
```

**Feature gates are two and each names what it buys.** `cuda` compiles the
kernels of section 10 and the native backend's device path. `gguf` compiles the
llama.cpp backend. A build with neither compiles the channels, the residency
bookkeeping, and the family libraries, which is what keeps the family surface
testable on a machine with no device.

```graph
node: spu-two-feature-gates
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-two-feature-gates
```

**The `gguf` gate is on by default as of 2026-08-23, and `cuda` is not.** The two gates
were written together and only one of them earns its default. The argument the pair
carried, that a build with neither keeps the family surface testable on a machine with
no device, covers `cuda` and does not cover `gguf`: llama.cpp builds and runs on the
host, so the GGUF engine's entry selection, marker promotion, and decode tests need no
card. Gating them bought nothing the argument named and cost twenty-eight tests, which
`cargo test --workspace` could not reach because the gate they sat behind was off.

**What the default costs, stated because it is a real cost.** A workspace build
now compiles llama.cpp, so a machine building this tree whole needs the C++
toolchain that build requires. A machine doing floor work alone is unaffected,
since `cargo build -p weaver-types` reaches no engine. The trade is that
whole-workspace operations gain a toolchain requirement and the workspace
command gains the family surface, and the second is what the gate said it
wanted to preserve.

**`cuda` stays gated and the reason is unchanged.** It needs a device, a build
that compiled candle CPU-only while claiming the device would load every
safetensors artifact into host memory, and the room judgment at admit cannot
see that lie. A machine with no card still builds and still runs everything
that does not need one.

**The engine's own device support rides this crate's gate rather than its
dependency line.** It was unconditional until 2026-08-23, which was harmless
while `gguf` was off by default and became a defect the moment it was on: a
host build would have compiled llama.cpp against CUDA and demanded that
toolchain on a machine with no card, which is the opposite of what the gate is
for. The weak form keeps it conditional on `gguf` rather than pulling the
engine in. **Read by a run rather than by the dependency tree**, which does not
render weak features: under `cuda` the engine reports finding the devices and
assigns layers to one, and under the default it reports no device at all.

```graph
node: spu-gguf-is-the-default-gate
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-gguf-is-the-default-gate
```

### 1.2 The fixture surface, and what a machine can run

**A skip is a pass to the harness.** `tests/loaded.rs` states the rule this section
generalises: an explicit request that cannot be met is a failure rather than a skip,
because a green run on a machine without the fixture reports that the engine was
watched decoding when nothing was decoded. The implicit skip survives only for the
machine that simply does not hold the artifact.

**What was missing is the account.** A green result carried no record of which subset
produced it, and no document enumerated what a machine would need to run everything.
This section is that enumeration, and the manifest test pins it so the table cannot
drift behind the tests.

Two feature gates decide which targets compile at all, per section 1.1, and Working
Process section 6 names the two invocations that reach them.

| target | gate | fixtures it reads |
|---|---|---|
| `entry.rs` | none | none |
| `manifest.rs` | none | none |
| `seam.rs` | none | none |
| `markers.rs` | `gguf` | `WEAVER_VOCAB_<FAMILY>`, one per registry entry |
| `selection.rs` | `gguf` | `WEAVER_ARTIFACT_GEMMA4`, `WEAVER_ARTIFACT_MISTRAL_SMALL`, `WEAVER_ARTIFACT_PHI4`, `WEAVER_ARTIFACT_PHI4_MINI`, `WEAVER_ARTIFACT_SMOLLM2` |
| `native_loaded.rs` | `cuda` | `WEAVER_ARTIFACT_QWEN25_SAFETENSORS`, `WEAVER_ARTIFACT_QWEN25_32B`, `WEAVER_MEASURE_PACE` |
| `loaded.rs` | `cuda` and `gguf` | `WEAVER_TEST_GGUF`, `WEAVER_ARTIFACT_COLUMN_LAYERS`, `WEAVER_ARTIFACT_COLUMN_WIDTH` |
| `two_card.rs` | `cuda` and `gguf` | `WEAVER_ARTIFACT_TWO_CARD`, `WEAVER_ARTIFACT_SPLIT` |
| `readout_neutral.rs` | `cuda` and `gguf` | `WEAVER_ARTIFACT_READOUT`, `WEAVER_ARTIFACT_READOUT_LAYERS` |

**Two targets name a fixture they do not read, and the first form of this
table recorded the mention as a read.** `seam.rs` cites `WEAVER_TEST_GGUF` in a
header discussing the convention that a fixture-less machine verifies nothing,
and `selection.rs` cites it to say in as many words that it is **not** reused.
Both were transcribed from a text search that could not tell a citation from a
read.

**The per-target check caught both on its first run, where a crate-wide union
could not.** That name is read by `loaded.rs`, so the union was
satisfied while the mapping was false in two rows. This is why the assertion is
per target rather than over the crate.

**Three fixtures name a shape rather than an artifact**, added 2026-09-02 with the qwen3
act: `WEAVER_ARTIFACT_READOUT_LAYERS` and the column pair say what the artifact a run
points at owes per forward and per column. They exist because a demonstration is per
architecture, so a run against another architecture must carry that architecture's
counts, and the counts are named rather than derived on the rule those instruments
already followed - a count taken from the thing under test agrees with itself whatever
the tap did. Each defaults to the fixture its target documents, so a run that names no
artifact needs none of them.

**The vocabulary set is per entry rather than fixed.** `markers.rs` asks each registry
entry for its own artifact under `WEAVER_VOCAB_<FAMILY>`, so the set grows with the
registry and a table listing thirteen names would be stale on the next family. The
manifest test therefore checks the shape rather than the roster.

**`WEAVER_DECODE_GGUF` is the decode module's own and is not a test target's.** It
points the engine's unit suite at an artifact whose vocabulary those tests read, which
is why it is separate from `WEAVER_TEST_GGUF`: one variable serving both would answer
a load-path request by failing as an engine error rather than as the mismatch it is.

```graph
node: spu-fixture-surface-pinned
kind: assertion
tag: manifest

edge: asserts
from: weaver-spu
to: spu-fixture-surface-pinned
```

## 2. The two channel ends, and the process facts

**Both ends arrive at descriptors 3 and 4, inherited rather than re-decided.**
`weaver-harness-Spec` section 2.2 elects the numbering with its order and owes
it to this document, and this document takes it: **3 is the lifecycle channel
and 4 is the decode socket**, lifecycle first because it is the channel every
organ has and the one an organ with a single end already places there, so the
gate's placement is unchanged and this crate's first end sits where the gate's
does. The correction that gave that paragraph two numbers and an order rather
than one number landed with the decoder cut of 2026-08-02, so what stands here is
the order read from the receiving side and the
election carries no record in this document.

**Entry adopts both, then performs its two sets, before the first read.** The process
wraps descriptors 3 and 4 as owned handles, sets close-on-exec on both, and clears its
dumpable flag, per charter section 7. Both are sets and never checks, per the rule
`weaver-organ-channel` section 2 states: a step that finds a flag wrong and reports
leaves the descriptor inheritable and the process attachable, which is the condition the
set exists to prevent. The close-on-exec set matters here even though this crate forks
nothing, because `execve` clears the flag and the requirement is stated against the last
exec. **The wrap is not a formality.** Descriptors are owned types end to end in this
crate, so no raw number outlives the thing it names and no close happens twice. **The
close-on-exec set is review's by election and not by impossibility,** an `fcntl` reading
the flag as cheaply as the `prctl` that reads the one set beside it: the second walk of
section 10 buys the dumpable flag and this suite does not buy its neighbour, which is
the same split `weaver-gate-Spec` section 2 takes on the same pair. **Both ground where
their twins ground, in apex section 5.1**, an end that crosses an exec and a process
that can be attached to being two routes to the same descriptor, and the possession case
rests on there being none. The argument is `weaver-harness-Spec` section 2.2's and
`weaver-gate-Spec` section 2's and is pointed at rather than restated. The ownership pin
beside them grounds in nothing, being the representation this crate elected for a
descriptor and not a claim about who can reach one.

```graph
node: spu-descriptors-owned-types
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-descriptors-owned-types

node: spu-dumpable-flag-cleared
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-dumpable-flag-cleared

edge: grounds
from: spu-dumpable-flag-cleared
to: axiom-floor-is-vocabulary-behavior-is-socket

node: spu-channel-ends-close-on-exec
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-channel-ends-close-on-exec

edge: grounds
from: spu-channel-ends-close-on-exec
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**Entry verifies that it holds exactly two descriptors beyond the standard
streams, and refuses to serve if it holds more.** Charter section 7 makes a
build in which this crate holds a trace descriptor broken whether or not it
writes through one, and the harness's fork discipline is what keeps that true,
argued at `weaver-harness-Spec` sections 2.2 and 8 and relied on rather than
re-tested here, but a property this crate depends on and cannot see is a
property worth checking at the one moment it is cheap. The check is a count
rather than an identification, since this crate cannot know what a stray
descriptor refers to, and a count above two means the discipline upstream failed
and this process is not the one to continue past it.

```graph
node: spu-descriptor-count-check
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-descriptor-count-check
```

**The lifecycle channel carries the organ envelope and the decode socket does
not.** Per charter section 13.2 the decode socket is not an organ channel, so
the envelope crosses only on the first end, and the token trio crosses the
second in whatever encoding the measurement of section 12 elects. Both ends
carry the receive obligation the `SOCK_SEQPACKET` election attaches: a buffer
sized to the envelope bound on the lifecycle channel, a read returning with
`MSG_TRUNC` set treated as a channel fault and never a message, and the same
bound asserted on this crate's own writes. Since the segment series of
2026-08-20, the decode socket's writes segment any frame past the envelope
per `weaver-types-Spec` section 4.4, its reads reassemble a series under
the total bound with a short or oversized series a channel fault, and a
send that cannot complete prints its fault and the frame's size before
service ends, per issue #236's two defects closed together. The election
and its bound are
`weaver-types-Spec` section 4.3's, declared there as two records rather than
one, the bound sitting beside the election rather than inside it, and this crate
carries the obligation without redeclaring either. **The two claims stated here
carry two instruments, and the reason they differ is the reason one was unbought.**
The envelope's confinement to the first end becomes watchable the day the decode
socket's encoding is settled, which section 12 holds open, so no capture test has
a settled shape to assert against yet, and it stays review's by non-purchase on
that ground. The truncation fault was reachable all along and three sibling Specs
buy the instrument for it, `weaver-harness-Spec` section 8, `weaver-admin-Spec`
section 10, and `weaver-gate-Spec` section 6 each naming the same watch. Section
10 names it here too, in the act that closed issue 37, which leaves no receiving
crate in the corpus carrying this obligation on prose alone. **The two also ground
in two different invariants, along the same line their instruments took.** The
envelope's confinement is apex section 5.4 read at this crate's two ends: the
lifecycle channel is the two-initiator channel with the harness that makes this crate an
organ, and the decode socket is a second end its own contract governs, so an
envelope crossing it would put the organ channel's carriage on a channel that is
not one and leave the organ line unreadable from a capture. The truncation fault
grounds in apex section 5.1 instead, the obligation existing at all only because a
boundary-preserving socket type was elected under that invariant and a short read is
what defeats it.

```graph
node: spu-envelope-on-lifecycle-only
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-envelope-on-lifecycle-only

edge: grounds
from: spu-envelope-on-lifecycle-only
to: axiom-organ-and-submodule

node: spu-truncation-is-a-fault
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-truncation-is-a-fault

edge: grounds
from: spu-truncation-is-a-fault
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**The service is serial per channel and the two channels are one loop.** One
lifecycle directive at a time, one decode exchange at a time, per both
contracts' ordering, and the decode socket's traffic is what the process
spends its time on. Nothing here is concurrent in this pass, and the shape
that would change it is the executor election deferred in section 1.1. **This
is review's by non-purchase,** two asks pressed onto one channel being as
reachable a test here as the one-answer-per-request test `weaver-admin-Spec`
section 10 buys, and section 10 of this document buying none.

```graph
node: spu-service-serial-one-loop
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-service-serial-one-loop
```

## 3. Residency

The two exchanges of `weaver-harness-spu-contract`, in the charter's order.

**Six of this section's ten records are review's, and the ground is non-purchase rather
than reach.** Section 10's perturbation list was written before the device-assignment
ruling of 2026-08-03 reshaped this section and was revisited in the act that closed
issue 37, which bought what needs no hardware and no seam work: the cheap refusals
below, and, in section 5, the architecture lookup against an artifact header and the
width refusal against a declared set. What is left standing on prose divides in two, and
the division is worth stating because the two halves are unbought for different reasons.
The room condition and the peer reachability condition each read a driver, and they
reach it through a seam a suite could double and this Spec does not introduce, so buying
them is a larger act than the three this one takes. The release's free-before-answer
ordering stood in that group until the load act of 2026-08-07 put the driver itself in
reach, and its record carries the instrument that act bought. The remainder needs no
device at all and is simply unbought: the devices coming from the binding, the
headroom's placement, the hash's failure sentinel, and the once-only admit. Stating that
here once is what keeps six review tags from reading as six findings that no instrument
exists.

**Admit runs the charter's five steps, and the first three are free.** Resolve the
binding to an artifact, read what the artifact declares about itself without loading it,
judge the assigned devices and the readout election, take them in shard order and load
each shard, confirm. The header read is the salvaged mechanic the survey names: parsing
an artifact's header and metadata answers what architecture this is and what its
dimensions are without touching tensor data or the device, which converts the common
shape of a bad binding, an artifact present and wrong, into a refusal costing no device
work. **What section 10 buys is the ordering rather than the parsing,** per the
charter's rule that every step before the fourth is refusable at no cost: a fixture that
would fail the judgment as well as the resolution is what makes the refusal's identity
report which step ran first, and a test that asserted only that the header parses would
pass with the whole ordering inverted.

```graph
node: spu-header-read-touches-no-device
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-header-read-touches-no-device
```

**A refusal on the artifact names the path and the identity it was judged
under, and the account stays on this side of the seam**, per charter section
4.1 step 1 and section 7. What crosses is the floor's typed case,
`ArtifactUnresolvable` where nothing stands at the path as this process and
`ArtifactUnreadable` where something does and will not open, read, or parse,
and the closed set carries no detail field, so the account lands on standard
error as one typed line, the way the engine's account of a failed load
already does: the refusal's name, the path the step failed on, the step, and
the uid and gid this process runs under. The line rides the worker's pipe to
the journal per `weaver-harness-Spec` section 2.2, and is the one place an
operator reads the fault the charter's step names. **Absent and denied are
told apart at the resolve.** The classification reads the error the lookup
returned and confirms no presence: a lookup the kernel refuses says that
something on the path denied this identity, the artifact or a directory above
it, and not that the artifact is there, so it refuses unreadable rather than
unresolvable, the reading the pin already gives an open that fails for a
reason other than absence. The perturbation is dropping the path from the
line, under which the test that reads the line back fails, and the resolve's
own test skips as root rather than passing on nothing, a uid that no
directory denies being unable to see the difference it watches.

```graph
node: spu-artifact-refusal-names-path-and-identity
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-artifact-refusal-names-path-and-identity
```

**The devices are the binding's and this crate selects none.** The assignment
arrives inside the model binding, per `weaver-types-Spec` section 2, ordered,
with the order the shard order. There is no device survey, no ranking, and no
fallback in this crate: the archived tree's `auto_select_gpu` does not cross,
per charter section 3 and the salvage survey's seventh yielding. What this
crate does with the set is judge it, in the order section 3's steps state. The
binding's own shape, the artifact with an ordered device set and an empty set a
parse error, is `weaver-types-Spec` section 2's and carries no record here.

```graph
node: spu-devices-from-the-binding
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-devices-from-the-binding
```

**A set larger than one is judged on three things rather than one.** Each assigned
device must have room for its shard plus the residency's headroom, which is the one
inequality read per device. The devices must be able to reach each other, checked by
asking the driver whether peer access holds between each pair in the set, because a
sharded forward exchanges activations across them and a set without peer access is a set
that cannot serve, discovered at admit rather than at the first turn. And the registry
entry the artifact resolves to must declare that the backend can shard across that many,
per section 5. **Today that number is two where it is greater than one,** because the
salvaged tensor-parallel path is a two-device implementation, `forward_tp2` with an
all-reduce kernel written for a pair, per the survey. A wider set refuses against the
declaration rather than against a hidden limit, so the day an N-way path lands the
declaration changes and nothing else does.

**The room refusal carries the device's capacity beside what was free.** The
driver answers both when asked, and reporting only the free figure discards the
one fact that separates two failures wanting opposite responses. A card with
seven gibibytes free against a thirty-five gibibyte need is a card too small
where its capacity is eight, and a card held by something else where its
capacity is forty-eight. The first is a binding an operator must change. The
second is a machine an operator must clear, and nothing is wrong with the
binding at all.

**Measured rather than supposed.** Under a deliberate occupant on 2026-08-23
the refusal read `NoRoom { ordinal: 0, free: 7362707456, needed: 34911404576 }`
eight times out of eight, identically, and that text is indistinguishable from
a sizing defect in the binding. The failure is correct and the account of it is
not, which is the lie the fail-cheap rule exists to prevent one level up: a
refusal that names the wrong cause spends the operator's attention on the wrong
repair.

**No case is added and none is classified.** The refusal carries the third
figure and says nothing about which reading applies, because a reader with all
three can tell, and a `Contended` case would be a judgment this crate has no
ground to make. The device is not this crate's to arbitrate beyond the one
inequality, per charter section 4.1, and an occupant may be a second agent the
operator wants there.

```graph
node: spu-room-refusal-carries-capacity
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-room-refusal-carries-capacity
```

**The three are judged cheapest first, and the ordering is this document's to elect.**
Charter section 4.1 step 3 enumerates the three conditions and sequences none of them,
so the order is a representation question, and it takes the charter's own
before-the-fourth-step rule one level inward: the width condition is a comparison
between the binding's count and the resolved entry's declaration and reads nothing,
while the room and reach conditions each cost a driver query, so the width is judged
first and a set failing more than one condition is refused on the cheapest. That
ordering is what puts the width refusal inside a test on a machine with no device, which
is why it is elected here rather than left to a builder. **This record stays review's
and the ground is the seam rather than the purchase.** Two of its three conditions read
a driver, a suite could reach them through a double, and this Spec introduces no such
seam, so the judgment taken whole has no instrument here yet. The width condition alone
is watched, at the section 5 clause that declares the set, and dividing it out here
would mint a second record for a property that clause already carries with the same
test.

```graph
node: spu-admission-judges-room-reach-and-width
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-admission-judges-room-reach-and-width
```

**The device judgment reads the driver rather than this crate's own
accounting, and the charter's cell is settled that way.** Charter section 10
holds the question open with the archived tree's answer recorded as a
preference without a reason: that tree held its own allocation ledger as the
authority and marked the driver query as diagnostics only, which prefers the
number that cannot see the thing the check exists for. This crate holds no
fleet ledger to prefer, per charter section 3, and the case it exists to catch
is a device occupied by something this program did not put there, so the
authority is what the device reports free at the moment of admission. **The
cost is named rather than hidden:** one driver query on the admit path, which
happens twice per residency and never inside a turn.

```graph
node: spu-device-authority-is-the-driver
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-device-authority-is-the-driver
```

**The headroom term stays a construction parameter until a measurement
replaces it.** Charter section 9 stages the figure and names the entry
condition, a measurement on a real artifact against a real device, so the
admission inequality takes the headroom from the worker's composition root the
way `weaver-trace-Spec` section 6 takes its queue depth: a deployment fact
rather than an operator election, and a number a builder can supply before the
measurement exists.

```graph
node: spu-headroom-is-a-construction-parameter
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-headroom-is-a-construction-parameter
```

**The weights hash is computed at admit and travels with every measurement.**
The salvaged mechanic is BLAKE3 over a canonical manifest, a single file or a
walked directory, with a sidecar cache and an **empty-string sentinel on every
failure path**, which is the property worth carrying verbatim: a hash that
cannot be computed reports that it could not rather than reporting a wrong
value, and apex section 8 rests replay on the identity being right. **The third
walk's test reaches the hash and not the sentinel,** an alteration between two
admits showing a changed hash and showing nothing about a failure path. The
sentinel is a claim of its own rather than the periphery of that one, and it is
review's by non-purchase, an unreadable artifact being as reachable a fixture as
an altered one. **The manifest is stated so a second party computes the same
value**, as of 2026-09-06 per issue #465, because a catalog that keys on another
digest joins a record to nothing unless it can derive this one from the files it
registers. A reference naming a file hashes that file's bytes, and where the file
is one shard of a split every shard's bytes follow in split order, names excluded,
read through the descriptors the load pinned. A reference naming a directory hashes
every regular file beneath it, walked in sorted name order, each contributing its
path relative to the directory and then its bytes, so a rename is a different
identity and a sidecar the index never names is inside it. The value renders as
sixty-four lower-case hex characters and the sentinel as the empty string. **This
is the record's identity of the artifact and it is at the grain of everything the
binding named**, which is coarser than one file and finer than the weights alone: two
directories holding the same shards beside a different sidecar are two record
identities, and the per-file digest map `weaver-analysis-Spec` section 3 keys a lens
on is determined by this value for a file or a directory and does not determine it.
**A split is the one case where the names are outside the hash**, so two split sets
of identical bytes under different stems are one record identity and two per-file
maps: this value fixes their digests and not their names. Whether the split's
manifest should carry the names, which would change the identity every later admit
of a split records, is the operator's to rule and is named at `weaver-web-Spec` section
10 (`git show 112bc65:docs/crates/weaver-web/weaver-web-Spec.md`, in the web's own
repository since 2026-09-26), where the consequence lands.

```graph
node: spu-weights-hash-at-admit
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-weights-hash-at-admit

node: spu-hash-failure-sentinel
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-hash-failure-sentinel
```

**Release frees the device before it answers, and the ordering is the
contract's.** Stop serving, free the weights and the working allocations and
the cache together because they are one residency, then confirm. A
confirmation is a fact about the device rather than a statement of intent, per
charter section 4.2, and the archived tree's inverse ordering is what its own
record names as producing an overcommit.

```graph
node: spu-release-frees-before-answering
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-release-frees-before-answering
```

**Nothing is idempotent and nothing retries.** This crate begins empty, admits
once, and dies, so a second admit has no prior residency to match, per charter
section 4.1, and the archived tree's idempotence rule loses its premise with
the daemon it was written for.

```graph
node: spu-no-idempotence-no-retry
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-no-idempotence-no-retry
```

## 4. The decode submodule

### 4.1 The backend seam

**One trait, two backends, peers rather than a legacy and a target.** The
archived tree's own ruling of 2026-06-11 made the GGUF and native paths
first-class peers and the survey carries the reasoning forward: GGUF owns
quantized artifacts on consumer devices, and the native path owns what a
tensor-parallel forward and a fine-tunable artifact need, since a GGUF cannot
be fine-tuned and a program that intends training as a continuation cannot let
that path decay. The seam is one trait over open, append-and-generate, cancel,
flush, and close, with the family library above it and the device beneath. The
peer status is review's by non-purchase: one trait with two implementations is a
compile property a bound would pin, and section 10 buys no such pin.

```graph
node: spu-backends-are-peers
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-backends-are-peers
```

**Which backend serves is a property of the artifact, decided at admit.** The
header read of section 3 already answers it, so nothing elects a backend
separately and no configuration field names one. The config's field list is
`weaver-types-Spec` section 2's and what this crate asserts is the derivation,
reachable by admitting two artifacts of different kinds under one configuration
and unbought here.

```graph
node: spu-backend-from-artifact
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-backend-from-artifact
```

**An architecture the native path does not serve refuses as that, before any device is
opened.** That path serves one architecture this stage, the registry's qwen2 entry, and
the artifact it is handed declares its own. **The declared architecture is read before
the shapes are and before the device is**, both being free reads, so an artifact naming
another architecture refuses on the architecture it named whatever the card is doing.
**The refusal says this backend does not serve the architecture and never that the
binary does not carry it**: the registry carries qwen3 and the rest and the GGUF peer
serves them, so a reader told the architecture is unknown goes looking for a registry
row that is present. It names the peer that was asked beside the architecture, and
carries the spelling the file used rather than the fold of it, per section 5's rule that
a miss names the string the header carried. Parsing first answers an architecture
question with whichever field the other architecture happens to spell differently, which
is a true statement about a file and the wrong cause: a stock Qwen3 export declares
`sliding_window` as null where qwen2's struct requires a number, so the reader is sent
to a config line that is correct. **Opening the device first answers it with the card**,
which crosses as a device that could not admit and is the report issue #507 was filed
against. **The floor's word for it is the artifact and not the device**, the refusal
crossing as the unreadable artifact every unresolved selection crosses as, because an
architecture one peer does not serve is not a condition of the card. A config declaring
no architecture, or naming one in anything but a string, is judged by nothing here and
refuses on the field it lacks or the type it got wrong, since an architecture it never
named is not one it got wrong. **A served architecture whose shapes are wrong refuses as
a load and the refusal names where in the file it is**, which is the half of the account
the reader of a correct line was missing, so the shapes are read from the artifact's own
text rather than from a value parsed out of it. **The ordering against the device has no
watch here and the record does not claim one**: reddening it wants a card that is busy
or absent, which no test in this crate can arrange, so what the perturbation buys is the
refusal's identity and its precedence over the shapes.

```graph
node: spu-native-refuses-an-unserved-family-as-a-family
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-native-refuses-an-unserved-family-as-a-family
```

### 4.2 The session

**Append-only, `resident_len`, and nothing ever rewinds.** The session holds the
resident length and no prefix cache, and every generation decodes only the delta at
absolute positions from that length forward. The archived tree's proof is the reason and
the survey records it: a rewind fails silently on architectures that keep recurrent
layers, because the clear returns success while the recurrent state stays, and the
failure surfaces later as a position error far from its cause. **This crate calls no
scoped-clear on a resident range,** which is the discipline stated as an absence, and
the absence is what section 10 pins. **The behaviour and the surface are two records
with two instruments,** the resident length moving only by appends and the two recorded
reducers being section 10's perturbation test and the missing method being its
compile-fail pin, and neither claims the other's half: a pin says a rewind cannot be
written and says nothing about a turn that re-prefills through the append path instead.

```graph
node: spu-session-never-rewinds
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-session-never-rewinds

node: spu-no-scoped-clear-surface
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-spu
to: spu-no-scoped-clear-surface
```

**The prefix is established at open and is permanent.** The open renders the identity
prefix through the renderer the resolved entry names, decodes it once, and records the
resident length it produced. Over a session that keeps serving, no operation reduces the
resident length below that figure - the flush bounds there as arithmetic and the elision
refuses a span that reaches into the prefix - and no operation short of those two
reduces it at all, the close over a backend fault clearing the account with the session,
per charter sections 13.3 and 13.13.

**The session's capacity carries a `Disposition` and is elected like a knob.**
It is frozen with its value compiled in, or operator-tunable and supplied in
`decoder.tunable_values` under the name `context-capacity`. The mechanism is
section 8's and is not restated here beyond naming what it governs.

**An earlier wording of this clause put the capacity on argv as a deployment
fact, and that was wrong.** The ground offered was that what a device holds at a
quantization is a deployment's knowledge rather than a declaration's, and it did
not account for `Disposition::OperatorTunable`, which already names the agent's
configuration as the route for a parameter a builder elects to leave movable.
The capacity is an agent's fact: it is this model's ceiling under this
quantization, two agents on one host differ in it, and a developer changing a
model wants it changed by reloading rather than by rebuilding. **The headroom
term keeps the deployment standing** and section 4.1 is unchanged, room to leave
spare on a device being a property of the host that two agents cannot
disagree about.

**The per-turn generation ceiling carries one too, and gains a clause here for
the first time.** It is the stop condition's backstop for a model that never
emits a stop token, so it is a safety bound rather than a preference, and it is
an agent's fact for the same reason the capacity is: what a turn should be
allowed to spend is a property of the agent that spends it. It is frozen with
its value compiled in, or operator-tunable under the name `max-tokens-per-turn`.
The constant has stood in the composition root since the decode act with no
document naming it, which is the absence this paragraph closes rather than a
decision it reverses.

```graph
node: spu-session-parameters-carry-dispositions
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-session-parameters-carry-dispositions
```

**Overflow refuses and sheds nothing.** A delta that would exceed the
session's capacity is refused with the overflow named and the session's own
account of itself, the harness deciding what a full context means. This crate
evicts and compacts nothing, per the charter, because either would be this
crate deciding which part of the agent's context matters. Review's by
non-purchase, an overflowing delta being a fixture the family libraries can
produce with no device present. **It grounds in apex section 5.5.** What a full
context means is a question about the turn, which is the harness's domain, and
an organ that evicted to make room would be reasoning about a domain that is
not its own, which that invariant names as what a hub carrying traffic alone
invites. The refusal carrying the session's own account is what lets the loop
decide with the fact rather than without it, so the claim is this crate handing
a decision back rather than withholding one.

```graph
node: spu-overflow-refuses-sheds-nothing
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-overflow-refuses-sheds-nothing

edge: grounds
from: spu-overflow-refuses-sheds-nothing
to: axiom-harness-integrates-by-the-loop
```

**The turn terminator is made resident before the answer returns, on every path.** The
archived tree's own hard-won correctness note: a generation that stops at the
end-of-generation marker without decoding it leaves the terminator absent, and the next
turn's framing is then malformed at a boundary nobody looks at. This crate decodes the
terminator after the generation loop, **on the clean path and the cancelled path
alike**, which is what makes the cancel of charter section 13.5 leave a well-framed
session. **The resident count the generation reports includes it, as `weaver-types-Spec`
section 4.4 has defined the member since 2026-08-19, and this is why.** Section 6's
closing count is taken after the terminator lands, so the count as a generation closed
exceeds the count as it opened by the turn's delta, the drawn tokens, and the
terminator, and the terminator is one token by the shape the resolved entry's renderer
declares, a single identifier the request's box names as `terminator`. A reader deriving
the position of a generation's first draw from the closing count therefore subtracts the
drawn tokens and one, and a reader adding the delta to the previous closing count is
exact from the second generation and wrong on the first by the identity prefix, which
the open made resident and no delta carries. Confirmed on 2026-09-05 against the
positions `model.field` reports across three records, nine generations, two precisions,
and two finish kinds, per issue #461, which found the floor's definition unread rather
than absent.

```graph
node: spu-closing-count-includes-the-terminator
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-closing-count-includes-the-terminator
```

```graph
node: spu-terminator-on-every-path
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-terminator-on-every-path
```

### 4.3 The turn, and the cancel

**Append and generate is one call with a token-boundary check.** The delta is rendered
by that renderer, decoded at the resident end, and the generation loop samples until the
stop condition of the renderer the resolved entry names, the harness's cancel, or the
session's capacity. The cancel is checked between sampled tokens, per charter section
13.5, which bounds the stop by one token's decode rather than by a kernel's completion.

```graph
node: spu-cancel-bounded-by-one-token
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-cancel-bounded-by-one-token
```

**The cancel arrives on the same socket and is read without blocking the
loop.** The decode socket is the one carrier, so the loop polls it between
tokens rather than waiting on it, which is why the check is a boundary check
rather than a signal handler: nothing asynchronous is needed to make a
token-boundary stop, and nothing asynchronous is introduced. The bound this
buys is section 10's test and the carrier election beneath it is review's by
non-purchase, the two being separate claims about one loop. **The carrier election
grounds in apex section 5.1,** which is why a signal is not the alternative it looks
like: a cancel is the harness asking this process to stop, so it is a seam, and that
invariant admits no seam across a process line that is not a socket. Remove it and a
signal handler becomes a shape a builder could reach for on latency grounds alone.

```graph
node: spu-cancel-polled-not-signalled
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-cancel-polled-not-signalled

edge: grounds
from: spu-cancel-polled-not-signalled
to: axiom-floor-is-vocabulary-behavior-is-socket
```

**A cancelled generation answers with what it produced, marked stopped, after
the terminator lands.** The partial output, its measurement for the tokens
that were produced, and the stopped marking, in that one answer, so the
harness closes the turn with the reason and the record holds what the model
said before it was stopped.

### 4.4 The flush

**The outcome is the ask's kept length and the mechanism is per resolved entry**,
because two architectures of one lineage can flush differently, as qwen2 and qwen35 do
below. After a flush the session's first kept tokens are resident and everything beyond
them is gone, per charter section 13.9 as amended 2026-08-19: the ask names the cut,
bounded below by the identity prefix's recorded length and above by the resident count,
the bound applied as arithmetic and the confirmation's counts carrying what held. Where
the resolved entry's flush permits truncation to a position the outcome is reached by
truncating to the kept length. Where it cannot roll back, the outcome is reached by
re-establishing the session and decoding the kept slice again, which is expensive and
correct where the cheap path is silently wrong. **The session retains its resident token
sequence** for exactly this reach: the re-establishing mechanism needs the tokens it
re-decodes, the identity prefix alone stopped sufficing when the cut became the ask's,
and the retention is bounded by the capacity the load resolved. **The registry entry the
artifact resolves to declares which it is** and the decode path reads that entry's
declaration rather than inferring it from a version string, so two architectures in one
lineage can flush differently, as qwen2 and qwen35 do. Review's by non-purchase, an
entry declaring each way being two fixtures and one assertion about the resident length
after the flush.

```graph
node: spu-flush-mechanism-from-declaration
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-flush-mechanism-from-declaration
```

### 4.5 The elision

**The outcome is the concatenation and the mechanism is one, narrower than the charter
permits.** After an elision the resident sequence is what preceded the span followed by
what followed it, every surviving position in the order it held, per charter section
13.13: the harness names a half-open span of resident positions, this crate makes it
absent, and the confirmation carries the resident counts either side, taken around the
call. The charter permits a rollback to the span's start where the resolved entry's
flush allows one - `resident[..from]` is a prefix of what stood, so an entry whose flush
truncates could truncate to the span's start and re-decode the tail alone, a primitive
the backend seam already supplies - and this Spec elects the one path anyway: **the
session re-establishes and decodes the kept sequence whole whatever the resolved entry's
flush mechanism declares.** One path, one instrument, and no per-entry divergence to
keep honest, which is the divergence the recurrent-architecture failure the survey
records punishes. The cost is therefore a full re-decode of every survivor, and it is
stated as the ceiling it is: the unelected rollback path would bound the refill by what
follows the span for the entries that can roll back, per section 12, and until that
election the figure a loop's elision policy prices against is a prefill of what remains,
so a loop consolidates its spans rather than eliding per turn. The discipline of section
4.2 holds here unweakened - the interior outcome is reached through the re-establish
path and never through a scoped clear on the backend.

**The bounds refuse rather than clamp, which is the flush's rule reversed
and deliberately.** Four spans describe no removable region, per the decode
contract as amended 2026-08-22: overlapping the identity prefix, running
past the resident count, ending before it starts, and empty. Each answers
the typed refusal and the session stands as the ask found it, where the
flush's over-large `keep` bounds as arithmetic, because a `keep` past the
resident count has a smaller true version and an unremovable span has
none. **The empty span is the case worth asserting hardest**: eliding
nothing succeeds trivially and would confirm a state change that did not
happen. Watched under the guard narrowed to `to < from`, the empty span
then passing it and answering with equal counts, an elision authored over
nothing. A full drop of the bound fails earlier, on the inverted span, and
never reaches this case.

```graph
node: spu-elision-refuses-an-unremovable-span
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-elision-refuses-an-unremovable-span
```

**The removal is exact and resequences nothing.** What remains is the
sequence it was, shorter, membership and order asserted on the tokens
themselves, because the confirmation's counts cannot catch a lost survivor
or a reversed tail - a kept vector built from the tail alone answers the
same arithmetic. Watched under both removals.

```graph
node: spu-elision-removes-its-span-in-order
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-elision-removes-its-span-in-order
```

**A failure partway poisons the session, on the flush's rule and for its
reason.** A re-establish that succeeded before a re-prefill that failed is
an empty backend under a long resident account, the
position-error-far-from-cause failure the append-only discipline exists to
prevent, so the session closes rather than carrying the disagreement. The
confirmation arrives only after the shortened state holds, per the decode
contract's ordering. **The span is state and never the record**: what the
span removes is the resident sequence, and no operation in this program
removes anything from a trace. The record keeps the elided material and
gains the `elision` event, which carries the bounds the loop named beside
the resident counts the seam returned, authored by the harness from its own
ask.

### 4.6 The re-feed drive

**Represented on the paths the generation already walks**, per charter section 13.14 and
the decode contract's sixth exchange, added 2026-08-31. The drive is the append's
machinery with the draw's destination changed and the renderer bypassed: the rendered
contribution tokenizes and appends verbatim, no renderer on the way per the ruling of
2026-08-12, the per-position loop runs the derived seed of section 8.5 and the effective
knobs of section 8 through the sampler exactly as a generation runs them, and where a
generation emits the drawn token and streams its piece, the drive records the draw's
identifier into the measurement's output slots and appends the recorded token instead,
streaming nothing. The field and the column intermediates cross where their elections
stand, on the same sites the generation crosses them. The answer wraps the generation
shape under the floor's own `ReFed` arm, the variant and not the payload being what
keeps a supplied path out of a sampled path's clothes. The permission is judged where
every admitted member is judged, and the two-arm registry is the charter's, cited rather
than restated. **The watches are the building act's**, per the pattern section 7's
unasked vector set: the drive is unbuilt, its instruments cannot exist before it, and
the act that builds it buys them in the same motion or their tags stand as the lie 13.7
refuses. The building act bought them, four watches below, each seen red under the
perturbation it names before its tag landed.

**The session appends the recorded token whatever the draw said**, the
drawn identifiers landing in the answer's output slots and one measurement
taken per recorded position. Perturbation: append the draw instead of the
recorded token and the watch fails, the decode log carrying the draws
where the path belongs.

```graph
node: spu-refeed-appends-the-recorded-path
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-refeed-appends-the-recorded-path
```

**The permission is judged first across the seam**: an instruction without
the admitted permission meets the registry's first arm before the session
is touched. Perturbation: remove the permission check from the dispatch
arm and the drive runs against a serving instruction.

```graph
node: spu-refeed-permission-judged-first
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-refeed-permission-judged-first
```

**An empty path refuses on the registry's second arm**, a replay of
nothing wearing an exchange. Perturbation: remove the empty-path check and
the empty re-feed reaches the session.

```graph
node: spu-refeed-empty-path-refused
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-refeed-empty-path-refused
```

**The null re-feed recomputes the recorded draws exactly**, across two
processes of the real engine against the real weights: the source process
generates and its answer is the record, the replay process re-feeds the
recorded rendered form along the recorded path, and the recomputed
identifiers equal the recorded ones integer for integer, the derived seed
of section 8.5 meeting the same turn key at the same ordinal.
Perturbation: derive the seed at the wrong ordinal in the dispatch arm and
the recomputed path diverges.

```graph
node: spu-refeed-recomputes-the-recorded-draws
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-refeed-recomputes-the-recorded-draws
```

## 5. The family libraries

**One module per family, holding everything that family defines.** Per charter section
14, a family is a template lineage, the models that share one format family whatever
their vendor, held in one module with the orchestration quirks the shared kernels are
driven by, a decode lineage rendering that format through one or two renderers and a
classify lineage rendering nothing and serving its entries from the classify table.
Nothing family-specific lives outside its module, and the kernels beneath are shared,
which is the archived tree's share-kernels-own-orchestration rule promoted to structure.
Review's by non-purchase, the placement of a family's code being as readable to a
module-boundary test as to a reader and neither being bought here. The registry in
`family/mod.rs` is where entries are declared, `qwen2::renderer` serves seven entries
across vendors, SmolLM2's `llama` and NVIDIA's `nemotron_h_moe` among them, and where
one architecture string carries two formats the module holds both entries and the
artifact's own template selects between them, which is the contested-architecture
mechanism below: phi holds two `phi3` entries with disjoint templates and renderers.
**The code keeps the older name for the key:** `Declaration::family`, the header's
`FamilyName` and the `UnknownFamily` refusal name the architecture string, not the
lineage.

**Four things own a lineage's facts, per charter section 14, and this table says which
owns each and where it is read.** A sentence in this document naming a fact's owner
agrees with the table or is a defect.

| Fact | Module, the lineage | Renderer, the format | Entry, the architecture | Artifact | Read at |
|---|---|---|---|---|---|
| placement, one module per lineage | x | | | | `family/<name>.rs`, declared in `family/mod.rs` |
| the shared orchestration quirks | x | | | | the lineage's module |
| the renderers a lineage holds, one or two for a decode lineage and none for a classify lineage | x | | | | `qwen2::renderer`, `phi::tag_renderer` and `phi::sep_renderer`, and none in `modernbert`, whose entries sit in `modernbert::CLASSIFY_FAMILIES` |
| rendering an identity prefix and a turn's delta | | x | | | `Family::render_identity`, `Family::render_delta` |
| the fold where the template names no system turn | | x | | | `Family::fold_for_template` |
| the marker vocabulary | | x | | | the module's markers, `qwen2::RENDERED_MARKERS` |
| the parse of an emission | | x | | | `Family::parse` |
| the stop conditions and the turn terminator | | x | | | `Family::stop_conditions` |
| the architecture string, and the tensor layout it names | | | x | | `Declaration::family` |
| the configuration shape | | | x | | the native path's per-architecture `Config`, `decoder/native.rs` |
| the template identity | | | x | | `Declaration::template` |
| the generation opener | | | x | | `Declaration::generation_opener` |
| which renderer | | | x | | `Declaration::renderer` |
| the markers that select it where contested | | | x | | `Declaration::selecting_markers`, `select` |
| the flush, and rollback derived from it | | | x | | `Declaration::flush`, `Declaration::permits_truncation` |
| the readout tap, and the column | | | x | | `Declaration::taps_readout`, `Declaration::taps_column` |
| the shard widths | | | x | | `Declaration::shard_widths`, `Declaration::shards_across` |
| the operation served, by the table the entry sits in | | | x | | `REGISTRY` for decode, `modernbert::CLASSIFY_FAMILIES` for classify |
| the tokenizer | | | | x | `tokenizer.json` beside the weights, or the GGUF vocabulary |
| the chat template, the selector where contested | | | | x | the header's `tokenizer.chat_template`, read by `select` |
| the weights | | | | x | the container |

**A seated `System` prefix folds where the resolved entry's template names no system
turn, within the render call it arrives in.** The canonical role of an identity prefix
is `System`, per `weaver-types-Spec` section 2, and the parse refuses any other. Two
renderers in the registry, gemma's and mistral's, serve published templates with no
system turn, and both refused the role at `render_identity`, which after that parse rule
left them no usable prefix at all: `user` refused above, `system` refused here. **The
role is the floor's vocabulary and the shape is the family's**, so those families render
the role rather than refusing it, folding **every** `System` message into the user turn
that follows it, within the `render_each` call the message arrives in. A `System`
message with no user turn after it in that call becomes a user turn of its own, a prefix
that rendered as nothing being a prefix the record cannot account for.

**The merge is within one render call and does not span the prefix and the
delta**, which are two: the prefix renders at the open and the delta at the
append. A seated identity therefore folds to one user turn and the first delta
folds to a second immediately after it, adjacent with no assistant between.
That adjacency is not something the fold can close - the prefix is tokenized
and resident before any delta exists, and seating it later would retire the
ruling of 2026-08-20 that the prefix is processed at load as the functionality
test. What the fold removes is the adjacency within each call, which on a
`role: user` declaration was three turns rather than two. The limit is stated
here and asserted by a test rather than left for a reader to discover.

**Every position and not only the first**, which is a decision rather than a
detail. A renderer that renders system content as user content has no position
at which the role stops being renderable, so refusing a mid-prefix `System`
while folding a leading one would be a distinction with nothing behind it. An
earlier form of this clause said "leading" while the code folded all of them,
which is the disagreement that settles here in the code's favour.

**That is the template authority followed and not a shape invented**, which is
the distinction `mistral3` already drew when it declined to mint a system
wrapper. Both lineages' published templates carry system content into the
first user turn, so folding is what those authorities name. A `[SYSTEM]`
block of this program's own devising would be the invention. A `System`
message with no user turn after it becomes a user turn of its own, a prefix
that rendered as nothing being a prefix the record cannot account for, and a
prefix carrying no `System` role renders exactly as before.

```graph
node: spu-system-folds-where-the-template-has-no-system-turn
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-system-folds-where-the-template-has-no-system-turn
```

```graph
node: spu-share-kernels-own-orchestration
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-share-kernels-own-orchestration
```

**The surface a renderer implements is small and named.** Render an identity prefix from
canonical messages, render a turn's delta, fold messages for a template that names no
system turn, parse an emission into canonical content with its markers recognized,
declare the stop conditions, and hand over a declaration. **A renderer's facts are read
from the renderer the resolved entry names, and an entry's from the entry itself, never
from a module by its name.** The entry is the one admission resolved for the artifact,
by `lookup` against the architecture its header declares or, where that architecture is
contested, by `select` against the artifact's own chat template, and that entry is
retained for the session. `lookup` alone refuses a contested architecture, and the
trait's `declaration()` answers for the entry its module is named after, so neither is
where a per-artifact fact is read. The entry's declaration carries the capabilities
admission judges against, per charter section 14, which are the readout tap and **the
device counts the backend can shard a model across**. That last is a set of widths
rather than a maximum, because a backend that serves one device and a pair is not
thereby serving three, and a maximum would imply it does. **The entry's declaration also
names its flush mechanism, per section 4.4, and whether the session's state permits
truncation is derived from that entry rather than declared beside it.** Truncation is
permitted exactly where the flush truncates, so a second declaration of it on the trait
was one fact in two places, and it left the trait on 2026-08-17. The surface's
membership is the `Family` trait's and takes no record. The declaration's shape does,
being this document's own and what section 3 judges against, and section 10 buys the
test: a declared set with a pair in it and a binding naming three devices is arithmetic
that runs with no device present. **The fixture the test uses declares a non-contiguous
set on purpose,** because the set reading and the maximum reading answer alike on every
contiguous declaration, so a fixture declaring one and two would leave the perturbation
that matters unwatchable while the test went on passing. **The declaration's field is a
set type and that half is pinned, per the operator's election of 2026-08-04:** a doctest
reads a declaration carrying a non-contiguous set literal, so a maximum can no longer be
declared, only read wrongly, and the perturbation keeps the judgment honest where the
type cannot reach. The two are two records for that reason, per the division rule of
Document Format section 3.

```graph
node: spu-shard-widths-are-a-set
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-shard-widths-are-a-set

node: spu-widths-set-pinned-by-doctest
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-widths-set-pinned-by-doctest
```

**A registry entry is an architecture and a template together, because the first does
not determine the second.** `general.architecture` describes the tensor layout, how the
weights are shaped and how they shard, and says nothing about how a turn is marked up: a
builder may train any layout against any chat format, and builders do. Measured on this
workshop 2026-08-17, three artifacts declaring `llama` render three different formats,
ChatML for SmolLM2, `[INST]` for Mistral Small, and Llama 3's own headers for a Llama
artifact, so a table keyed on architecture alone hands two of the three a template they
were never trained against.

**The header already carries the answer and this crate now reads it.** The
artifact declares `tokenizer.chat_template` beside its architecture, which is
the format its builder trained it on, so the fact was present and unread rather
than absent. Reading it keeps the key what the artifact's header declares,
which is what the clause below has always said, and widens what is read from
one field to two.

**Where one architecture carries one template the key is unchanged**, which is every
entry this binary carried before this act. Where an architecture carries more than one,
the artifact's own marker set selects among the entries sharing that architecture, and
an artifact matching none of them is refused naming the architecture it declared, per
the rule below rather than as an exception to it.

**The pair of an architecture and a marker set is unique, and the build is what
says so.** Two entries sharing both would make the table's order the selector,
which is a silent substitution wearing a different coat: the same artifact would
resolve differently after a reordering that changed no declaration. The
uniqueness is a compile-time property of the table for the reason the widths set
is, so a duplicate stops the build rather than reaching an admit.

**And an ambiguous match refuses rather than picking.** The compile-time check is
what should make ambiguity unreachable, and the refusal is what happens if it
ever is reached, because a table this crate cannot read unambiguously is one it
must not read by position. The two are belt and braces on purpose: order decides
nothing at either.

**The artifact's marker set is derived by rendering, not by reading the template text.**
A template is a program and a marker it emits need not appear in its source: Phi-4-mini
builds one as `'<|' + message['role'] + '|>'`, so `<|user|>` and `<|system|>` exist only
after the program runs while `<|assistant|>` and `<|end|>` sit there literally. Measured
2026-08-17 over a range request against the first-party artifact. A reading derivation
therefore finds two markers of four and refuses a model that renders exactly what a
carried entry renders, which is a false refusal naming the architecture as though it
were uncarried.

So a canonical conversation is rendered and the entry's markers are matched
against the **output**.

**What renders it is a detector, not an evaluator, and the difference is
load-bearing.** `llama_chat_apply_template` does not evaluate the artifact's
template. It matches the template text against a table of template families it
carries, by substring, first match winning, and then emits its own rendering of
the family it settled on. The upstream source says so in terms: it uses
heuristic checks and it is not a jinja parser. So the derivation is not the
artifact's program run to completion, and a clause claiming that would rest on
a mechanism the dependency does not implement.

**It nonetheless answers the case that motivated it, and that is measured
rather than reasoned.** Phi-4-mini renders `<|system|>`, `<|user|>`, `<|end|>`
and `<|assistant|>` through this path, where its template source carries only
the second pair. The markers arrive because the detector recognised the family
and knows what that family emits, which is a different reason from the one the
finding proposed and the same answer.

**The detector's table is a property of the pinned revision, so the measurements
below name it.** Which families it recognises, and which it settles on first,
change with the dependency rather than with this crate, so a revision bump can
move a marker set this crate never edited. Everything measured here was read
against `llama-cpp-rs` at `277e4100`, the revision this crate pinned when the
readings were taken. The pin moved to `ecce255b` on 2026-08-17 and these
readings were not re-taken there, so they stand as readings of the earlier
revision, and a bump reopens them rather than inheriting them.

**It fails closed on a template it does not recognise, and one lineage this binary
already carries publishes such a template.** Gemma4 returns an error rather than a
rendering, measured 2026-08-17 against `gemma4-31b-it-Q8_0.gguf` as this workshop holds
it. The Phi-4-mini reading above is against `unsloth/Phi-4-mini-instruct-GGUF`, header
read over a range request rather than a held artifact. An unconditional render would
therefore refuse an entry the registry serves today, which is why the render is reached
only where an architecture is contested. An architecture carrying one entry selects on
the architecture and renders nothing, so the artifacts that fail the detector never meet
it.

**The residual risk is mis-detection, and filtering narrows it rather than closing it.**
A first-match heuristic can settle on the wrong family and this crate cannot audit that
judgment. Because the rendering only ever chooses among entries that already agree with
the artifact about its architecture, a wrong detection cannot reach an architecture the
artifact never declared. Within the declared architecture it can still pick the wrong
sibling, and that outcome is a silent substitution: the artifact would be served a
template it was not trained against, with nothing refusing. So this act narrows the
substitution surface from every entry in the table to the entries sharing one
architecture, and it does not eliminate it. An act that closes the remainder has to
compare something the detector does not decide.

**The render needs no weights and no model handle, so the refusal lands before
any weight is read.** The call takes the template text, the canonical messages,
and an output buffer, and nothing else: no model pointer reaches it, which is
what lets this run on the string the header already yielded. The safe wrapper
`LlamaModel::apply_chat_template` requires a `&self` it does not pass down, so
taking that path would order a weight load ahead of a refusal this crate can
reach without one. The election is the lower call, and the reason is admission
order rather than cost.

The probe conversation is a frozen input of this crate, pinned where the marker
sets are pinned rather than built where it is called, because a derivation whose
input varies by call site compares two artifacts against two different
questions.

**It also closes the error the other way.** A marker present in an artifact's
token table but never emitted by its template would match a vocabulary scan and
should not, and a rendered probe cannot be fooled by it, because what it reads
is what the artifact emits.

**Marker sets are compared and templates are not.** A chat template is a program, and
two that render the same turns can differ in whitespace, in comments, and in branches no
turn of this program reaches, so comparing them whole would refuse artifacts that agree
about everything this crate uses. What this crate renders is a renderer's control
markers, so what it compares is those, and the comparison is the same property
`spu-marker-promotion` already tests from the other side.

**Two artifacts differing only in whitespace are the standing case rather than
the hypothetical one.** Phi-3.5-mini and Phi-4-mini render the same markers and
differ in trailing newlines, so one entry serving both is what this clause buys,
and a later act proposing to compare more than markers is proposing to refuse
them.

**The several-templates branch is reachable on the first artifacts anyone
tries.** This workshop holds SmolLM2 rendering ChatML and Mistral Small
rendering `[INST]`, both declaring `llama`, beside the Llama 3 artifacts the
entry was written for. It is not a contingency waiting on an unusual model,
which is why the code act follows this one closely rather than leaving the
branch unexercised.

**The registry is compile-time and admission consults it.** A table of the architecture
entries this binary carries, keyed by what the artifact's header declares, with no
default and no fallback: an artifact whose architecture this binary does not carry is a
refused admit naming that architecture, which is the archived tree's own
no-silent-substitution ruling carried forward from its encoder registry. Section 10 buys
the test, an artifact header naming an architecture the binary does not carry being a
fixture and the refusal arriving before any device call, and what the test reads is the
architecture the refusal names rather than the load's outcome, so a refusal arriving on
some other ground does not satisfy it.

```graph
node: spu-family-is-architecture-and-template
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-family-is-architecture-and-template

node: spu-architecture-and-markers-are-unique
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-architecture-and-markers-are-unique
```

```graph
node: spu-registry-no-silent-substitution
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-registry-no-silent-substitution
```

**The key is compared folded, so the two containers' spellings of one architecture
select one entry.** GGUF writes `general.architecture` and a safetensors export writes
`config.json`'s `model_type`, and the two do not spell several architectures alike:
`qwen35moe` against `qwen3_5_moe`, `qwen3moe` against `qwen3_moe`, `gpt-oss` against
`gpt_oss`, per #212's finding of 2026-08-21. A table keyed on the verbatim string
carries an architecture for one container and refuses it at admission for the other
while reading as carried for both, which is a declared capability nothing implements. So
the comparison, and only the comparison, folds both spellings to one key: ASCII letters
fold to lower case, the separators `_`, `-`, and `.` are dropped, and every other byte
stands as written, the key the entry declares and the key the artifact carries both
passing through the same fold before they meet. The registry's spelling is unchanged,
each entry keeping the spelling it was declared with, and the refusal still names the
spelling the artifact's header declared. Two entries spelled differently may not fold to
one key, and the build says so beside the marker-set uniqueness above, so the
contested-architecture reading is what it was: entries sharing a key compete on their
markers. **A spelling difference that is a different architecture is not the fold's to
bridge.** Safetensors `mistral` and the registry's `mistral3` are two names for two
things and stay two keys, and an architecture the registry spells otherwise than a
container does is a registry question, answered by a row and never by widening the fold.

**Both directions of the template requirement bind here, per the charter, and both are
bought.** Inbound, the reference test shape is the archived tree's marker promotion:
every control marker a renderer emits tokenizes to exactly one token under the
artifact's tokenizer, because a marker that degrades to subword text is structure the
model reads as prose. Outbound, the parsers are the recorded bridge from the verbatim
emission to the canonical form, and a parse that recognizes no call where the emission
attempted one is its own reported fact rather than a clean turn, which is the archived
tree's distinction between a call that could not be rendered and a call whose name could
not be recovered. **The two directions are two records and each carries its own bullet
in section 10,** the two being separate behaviours of one module rather than the halves
of one claim: a marker that tokenizes cleanly says nothing about what the parser does
with an emission it cannot recover. The outbound bullet lands in the act that closed
issue 37, an emission attempting a call the parser cannot recover being a fixture rather
than a device. An earlier wording of this clause said both directions are tested here
while the suite bought only one, and the assertion pass demoted the outbound half to
review rather than naming a test, which is the position this act closes from the other
end.

```graph
node: spu-marker-promotion
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-marker-promotion

node: spu-parse-reports-unrecovered-call
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-parse-reports-unrecovered-call
```

**Modules become member crates when a second operation type consumes them,
and not before,** per charter section 14. A directory move is what that
promotion costs, which is the reversibility test paying out.

## 6. Measurement production

**The signals are computed where the distribution is, before the sampler.**
Entropy over the logits at each step and the surprisal of the token that was
drawn, both in bits, computed inside the generation loop because the
distribution exists there and nowhere else. **The math is log-sum-exp stable
and the reason is arithmetic rather than taste:** a naive exponential overflows
`f32` above a logit around eighty-eight, which the archived tree records as
happening routinely on production vocabularies, so the stable form is the only
correct one at the scale this runs at. **The placement and the arithmetic are
two claims and two records,** neither the periphery of the other, and section 10
buys the first alone. The second is review's by non-purchase rather than by
reach: a logit vector carrying a value above that threshold is a literal, and
what it should produce is a finite number.

```graph
node: spu-signals-pre-sampler
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-signals-pre-sampler

node: spu-log-sum-exp-stable
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-log-sum-exp-stable
```

**The surprisal vector renders where its election stands and the
perplexity carries no election**, per charter section 13.12. Unelected and
with a distribution read, the measurement carries `perplexity` and no
`surprisals` member, and the absence is the same absence every unproduced
reading takes: no member rather than an empty vector, on the rule stated
just below. Elected, both
render, and a consumer that finds them disagreeing has found a defect
rather than two readings.

**`perplexity` carries no election and is absent on the one ground the
entropies are absent on**, which is that nothing was measured. A mean needs
terms, and a backend serving no distribution supplies none, so a generation
that measured nothing renders no perplexity and no entropies together. An
earlier form of this clause called it a plain number that always renders,
which was wrong about the same case the entropies already document and
would have obliged this crate to render a mean of nothing.

**The accumulator behind it holds no vector.** The mean is carried forward
per position and the terms are discarded as they are used, which is what
makes the unelected posture a smaller computation rather than a withheld
reading, per the charter's own line on where the election is honest.

**The reduction renders its own shape.** The residual reductions cross as one
number per layer per forward, in order, and the layer count and the forward
count render beside them. The layer count is what the tap folded per forward
and the forward count is how many times it did, so their product is the
array's length and a reader checks rather than assumes.

**The figures crossed alone until 2026-08-24, and the shape was arithmetic.**
A reader recovered the layer count by dividing the array's length by the token
count plus one, which no document stated and which held only while every
forward tapped every layer.

**That is why the counts are recorded rather than derivable.** Layer election
is named as an economy the readout may take, and under the old form the day it
landed the division would have returned a wrong layer count rather than an
error, while every reading taken before that day became retrospectively
ambiguous with nothing in any record marking which side of the change it fell
on. A record whose decomposition depends on a fact outside it is a record that
cannot be read alone, which is the property the trace exists to have. A future
election that taps a subset now says so in the record it produces rather than
in a document a reader must already have found.

```graph
node: spu-reduction-renders-its-shape
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-reduction-renders-its-shape
```

**Which readings turn on an election and which do not, and why absence is
still one rule.** The surprisals and the residual reductions are produced
where their elections stand. The entropies and the perplexity carry no
election and are produced wherever the distribution is there to read.
**That is not the same as always.** A backend serving no distribution
measures nothing and the generation still answers, per the signals' own
rule that an absent observability is a reported absence rather than a
fault, so an unelected reading is absent exactly when nothing was measured
and an elected one is absent when either its election did not stand or
nothing was measured. The absence rule below covers every reading on the
same terms, and what an election adds is a second reason a member may be
missing rather than a second kind of absence.

**A consumer cannot tell those two reasons apart from the measurement, and
does not have to.** The `load` event's elections say which readings were
asked for, per `weaver-trace-Spec` section 3, so an absent surprisal beside
a standing election is a reading that was not measured, and the same
absence beside a declined election is a reading that was not asked for.
The record carries the pair rather than the measurement carrying a reason.

**The vectors are positionally paired with the token identifiers and absent
when not produced,** per charter section 13.6 and `weaver-trace-PRD` section
3.2. This crate produces no empty vector to mean nothing was measured: it
renders no member for an unproduced reading, so the measurement blob it splices
carries the absence to the record directly. As of the model-events splice act
of 2026-08-11 the measurement is this crate's rendering the harness carries
opaque, so the omission is this crate's to produce rather than the trace's to
serialize: the earlier reading assigned it to `weaver-trace-Spec`'s
`skip_serializing_if`, and that election retired with the typed measurement
struct, this crate now owning both the production and the omission as one
obligation. **The
absence takes a type instead of a fixture, per the operator's election of
2026-08-04:** the vectors travel as an option of a non-empty vector, head and
tail, so an empty vector meaning nothing was measured is unrepresentable rather
than unproduced, and the earlier non-purchase ruling priced a fixture the shape
now retires. The pairing itself stays review's, positional correspondence being
a semantics no field type carries. The two are two records for that reason, per
the division rule of Document Format section 3, and the pin's doctest reads the
field's type at section 10's bullet.

```graph
node: spu-absent-not-empty-vector
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-absent-not-empty-vector

node: spu-absent-shape-pinned-by-doctest
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-absent-shape-pinned-by-doctest
```

**The token identifiers in and out are the turn's, and neither is the session's.** The
input identifiers are the turn's delta as the resolved entry's renderer rendered it, the
span the partition labels `turn-delta`, and the identity prefix the open made resident
per section 4 is outside them on every turn, the first included, so their count is not
the resident length at the generation's first draw and a reader that takes it for one is
wrong by the prefix. The output identifiers are the tokens the generation drew, paired
one for one with the two signal vectors, and the turn terminator the generation makes
resident after them per section 4 is outside them, so their count is not the
generation's contribution to the resident count either. The session's own figure is the
closing count below, and the two members are defined here because they travelled without
a definition from the seam act to 2026-09-05 and a contract read the input count as the
resident length, per issue #461.

**What travels with a generation.** The token identifiers in and out, the two signal
vectors, the timings the charter's row names, the model identity and its weights hash
from section 3, the prompt-block partition, the residual reductions when elected, the
session's resident token count and its capacity as the generation closed - the
generation's own typed members beside the two spliced boxes, never measurement members,
per issue #221's arc and `weaver-types-Spec` section 4.4 - and beside the measurement
the model.request content whole, per the custody act of 2026-08-11: the turn's delta as
the resolved entry's renderer rendered it, per the ruling of 2026-08-12, the template
identity, and the turn's effective sampling values, rendered by this crate because the
template and the knobs are this crate's, and spliced into the record's request box as
the measurement splices into its own. **The template identity is the request's member
and not the measurement's**, per the ruling of 2026-08-17 on #129: it is an input-side
fact, what was asked rather than what was read off the answer, which is the sampling
values' own ground for sitting where they sit, and the earlier text listing it in both
boxes was one fact in two places with no authority named, the defect G5 files. The code
and `weaver-trace-PRD` section 3 already carried it this way, and this clause catches up
to them. The harness carries both opaque, neither assembled from members it would parse
the splice to read. **The generation's bound renders in the request's box beside the
sampling values**, per charter section 13.6's clause of this date. It is a `stop` object
carrying `max_tokens`, the resolved per-turn ceiling, `stop_tokens`, the identifier set
in force, and `terminator`, the turn ender of the renderer the resolved entry names. The
box is the request's rather than the measurement's on the rule the template identity
settles just above: a bound is an input-side fact, what the generation was told, and the
`finish` it explains is what was read off the answer.

**The values are the resolved ones and never the declared ones.** The
ceiling arrives frozen from the composition root or tunable from the
operator's file, so this crate renders what `effective.session` holds at the
generation rather than what any instruction carried, which is the same rule
the sampling block already follows. **The stop object is absent from no
generation**, because unlike an unproduced reading there is no state in
which a generation ran under no bound, so a missing `stop` is a defect
rather than an absence with a meaning.

The partition comes from the tokenizer's offsets, which is the salvaged
`tokenize_with_offsets` mechanic with its invariant intact: the last offset
equals the rendered text's length, so a partition covers the prompt exactly
and a consumer joining text to tokens has no gap to guess at. The row's
membership is the charter's and takes no record. The offsets invariant does,
and it is review's by non-purchase, an assertion over one rendered prompt
needing no device.

**The partition's spans are labelled, and this crate declares the labels, per
the operator's ruling of 2026-08-10.** The organ originates the service, so the
organ declares the vocabulary, and the trace's block takes what the organ
declares with the harness converting nothing and inventing nothing. The names
are functionally descriptive, saying what a span does in the render, and the
declared set is what the render produces today and nothing more:
`turn-delta`, one span per appended turn in append order. The identity
prefix's own span, `identity-prefix`, arrives with the act that renders the
prefix into a measurement, named here in that act, the open exchange
producing no measurement today for a span to sit in. **The
binding property is consistency rather than the set:** the same span kind
carries the same label in every measurement this seam ever emits, and a new
span kind arrives with the act that renders it, named in this list in the same
act, so a label appearing in a measurement and absent here is a defect in one
of them. The declaration rides the partition's existing record: what the
labels are is stated here, and that the partition they label covers exactly is
the assertion above.

```graph
node: spu-prompt-partition-covers-exactly
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-prompt-partition-covers-exactly
```

**Nothing is retained after the answer.** Produced, reported, gone, per
charter section 3's no-state rule, which is what keeps a measurement from
becoming a second account this crate holds. This is the crate's face of the
program's statelessness and it is review's by non-purchase, which is the least
comfortable of this Spec's review tags: two identical generations answering
identically, with nothing of the first reachable from the second, is a
behaviour a suite can watch, and section 10 buys no watch for it.

```graph
node: spu-nothing-retained-after-the-answer
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-nothing-retained-after-the-answer
```

## 7. Residual readout

**The election arrives at admit and no tap runs without it.** The SPU
instruction carries the readout election beside the binding, per charter
section 13.7 and `weaver-types-Spec` section 2, and an admit whose election
the backend cannot honor refuses at admit rather than failing at the first
turn, which is the charter's fail-cheap-or-lie-expensive rule.

```graph
node: spu-readout-refused-at-admit
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-readout-refused-at-admit
```

**Two backends, two taps, one shape returned.** The native path uses the
candle fork's `forward_with_intermediates`, which returns the per-layer
tensors and is the readout's working ancestor per the survey. The GGUF path
uses the ggml scheduler's eval callback the llama-cpp fork exposes, pinned by
the archived tree and never driven, and standing it up is code this program
writes rather than salvage it inherits.

**The GGUF tap stands as of 2026-08-22, and these are its mechanics.** The
callback is installed on the context at creation. **The member it sets is
upstream and the setter that reaches it is not**: `llama_context_params`
carries `cb_eval` and `cb_eval_user_data` in unmodified upstream llama.cpp,
and what the fork adds is the Rust setter `with_eval_callback` that writes
them. Nothing upstream of the fork exposes that setter, which is the whole
of what section 10's pin holds open. Every decoder-only body in the fork
ends its layer with the tensor named `l_out-<il>`, formatted by the
context's own graph callback and shaped `[n_embd, n_tokens]` at every layer
but the last, and that is the post-residual-add value the readout reduces.

**Three facts a reader would otherwise learn by breaking something.** The
callback fires per **ubatch** rather than per decode call, so a prefill
crosses it many times where a single-token decode crosses it once. The final
layer gathers its rows to the output positions before the residual add, so
what the callback sees there is `[n_embd, n_outputs]`: **the first dimension
is the others' and the second is not**, which is the reverse of what a
reader told only that the shape differs would look for. The gather keeps the
batch order of the positions it retains, so the final column carries the
last position in both shapes and one rule reads both. And the copy is that
column alone, `n_embd` values contiguous at `f32` - the figure the artifact's
own width sets, kibibytes a layer against the gigabytes per prefill that
copying whole tensors at a full batch would move.

**The refusal narrows to the flag.** `readout::judge` refused an elected GGUF load on
the container, because the tap did not exist. With the tap standing the container is no
longer a ground and the resolved entry's declaration is: an entry that does not declare
its native tap still refuses, at admit, per charter section 13.7's rule that a load
granting an election it cannot honor fails at its cheapest moment.

**The deployed artifact serves the election as of 2026-08-23, and the run that bought it
is named.** The registry's `qwen35moe` declared `taps_readout: false` while the claim
was unmeasured, because flipping that flag is a claim about the architecture rather than
a line in a table: that this tap reads that architecture's residual correctly and
changes no token doing it. Both halves were measured against
`Qwen-AgentWorld-35B-A3B-Q8_0` on one card, forty layers and two hundred fifty-six
experts of which eight fire per token. Sixty-four tokens drawn under one declaration are
identical with the election on and off, at each of two seeds, and the elected run folded
one figure per layer per forward. **Two seeds rather than one, because the comparison
has to be known to discriminate**: an equality that held for every input would pass
whatever the tap did, and the two sequences differing is what says it would not. The
flag stands on that measurement.

**The qwen3 entry declares both taps as of 2026-09-02, and the run that bought them is
named.** The registry's `qwen3` declared `taps_readout: false` while the claim was
unmeasured, which is the same posture `qwen35moe` held before its own act. Both halves
were measured against `Qwen3-8B-BF16` on one A6000: sixty-four tokens drawn under one
declaration are identical with the election on and off at each of two seeds, the elected
run folding 2,340 figures over sixty-five forwards at thirty-six layers, and the column
stream crossing one message per sampled position at thirty-six layers by four thousand
and ninety-six. **The instruments moved to take it**: the neutrality demonstration and
the column watch each named their artifact's layer count and width rather than deriving
them, on the rule the demonstration already carried, so a run against another
architecture buys that architecture and not this one. **The sibling keys are
untouched**: `qwen3moe` and `qwen35` declare their own architectures and each owes its
own measurement, which is what per-entry means.

**The bar is a run and the run is kept.** Charter section 13.7 has an
elected readout change no token, shown per tap rather than once for the
election, and section 10's non-purchase rule would leave this to a reading
if the reading could reach it. It cannot: the hazard is that installing the
callback turns one compute into a walk of windows and unapplies a fusion
candidate straddling a boundary, and no host run touches a fused kernel.
The measurement is therefore an instrument in the tree rather than a note
in this document, and it is watched failing under a one-seed perturbation,
which is what tags it: Document Format section 5 fixes an assertion's
instrument to five names and apex section 11's third device makes a
behavioral test perturbation-verified by obligation, whatever the section
that argues it calls the run.

```graph
node: spu-elected-readout-changes-no-token
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-elected-readout-changes-no-token
```

**The bar this tap must clear is why the mechanics above matter.** Installing
an eval callback changes how the scheduler computes the graph, not only what
is read from it: one compute over a split becomes a walk of windows with a
synchronize after each, and a fusion candidate straddling a window boundary
is not applied. A fused kernel and its unfused equivalent are not guaranteed
bit-identical in floating point, and `l_out-<il>` sits next to a normalise
that is a fusion candidate. **So no reading of the source settles whether
this tap is observational**, which is why charter section 13.7's
demonstration is a measurement against the real artifact on the real device
pair rather than an argument, and why the run recorded above is the thing
that answers it. The one-shape claim is review's by
non-purchase, the pin section 10 buys on this seam being the fork's callback
and not the shape either tap returns.

```graph
node: spu-two-taps-one-shape
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-two-taps-one-shape
```

**The reduction happens in place, at the tap, and under the serving binding
nothing else leaves the device.** Apex section 3 step 6 has the activations
reduced in place and the reduction returning by the same path as the
generation, so no per-layer tensor crosses the seam and the volume the seam
carries is the reduction's. **The clause is the serving binding's**, per the
operator's ruling of 2026-08-30 and charter section 13.7, and the two
paragraphs below carry what the diagnostic binding may ask instead. Review's
by non-purchase, a capture of the decode seam under an elected readout being
the watch and section 12's open encoding being what it waits on.

```graph
node: spu-reduction-in-place-at-the-tap
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-reduction-in-place-at-the-tap
```

**The diagnostic binding asks past the fold, and the column is already in hand.**
Charter section 13.7 as amended holds the three elections and this section represents
them. Representation follows the two fold paths this section already distinguishes. On
the GGUF path the fold's input is a host-side copy of one layer's final column, `n_embd`
values contiguous at `f32` - the figure the artifact's own width sets - taken per layer
per forward and dropped once its norm is folded: the diagnostic answer is that column
continuing instead of dropping, and no second capture exists. On the native path
`fold_norm` is the stronger in-place reading, one scalar crossing and no column held, so
whether an entry's tap holds a column to answer is a member of its declaration per
charter section 14, spelled `taps_column` beside `taps_readout` and held to the same
bar: an entry declares it only where the answer has been shown on the engine that would
serve it, so qwen2 declares it on the GGUF answer shown first and every other entry
declares `false` until its own act. **The answer covers every sampled position**: each
decode forward, and each append's prefill final position, a turn holding one append per
generation so a tool round adds its own. Nothing else of the prompt crosses, and the
bound's ground is the tap's one-column copy per 13.7, not the engine's gather, which at
every layer but the last sees every prompt position.

**The permission is admin's, the ask is the harness's, and this crate judges the pair.**
`column_permission` arrives at admit inside the instruction, set from the binding's kind
by admin per 13.7, so this crate still never learns the kind itself. `column_ask`
crosses once, at session open, per the same clause's cadence election. An open carrying
the ask refuses typed at the open on the three arms of 13.7's registry - no permission,
no readout election, no column in the resolved entry's declaration - cited whole rather
than restated in part, that clause holding the registry so no document decides a share.
**The no-column arm fires at the open rather than at admit on purpose**: admission
cannot know an ask will come, the ask being the open's member, and a diagnostic replay
that elects the readout for norms alone against an entry whose tap holds no column is a
working configuration - the native tap's own shape - that an admit-time refusal would
refuse. The open is the cheapest moment that knows the ask, which is 13.7's rule pointed
at the right fact. An asked column that does not arrive at a sampled position is charter
13.10's fault, owned there since this act: the draw site faults the session loudly where
the armed take answers nothing - the native tap's own shape meeting an ask its resolved
entry's declaration let past - because the silent absence is what the fault rule
forbids. Perturbation: restore the silent skip at the draw site and an armed generation
completes with no column and no fault. Watched under exactly that restoration.

```graph
node: spu-asked-column-arrives-or-faults
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-asked-column-arrives-or-faults
```
 The efficient framing of the column message joins section
12's open token-seam encoding election, the provisional form being
`weaver-types-Spec` section 4.4's bare JSON like every crossing.

```graph
node: spu-no-vector-unasked
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-no-vector-unasked
```

Bought by the building act of 2026-09-01, as this clause required: an open carrying no
ask under a permission-granting instruction generates against the real engine and no
`Column` message crosses the whole stream, watched red under the arming made
unconditional. Beside it the act buys the registry's watch - three arms in the clause's
own order, the third judged at the unit because no small artifact here carries an entry
with a tap and no column - and the crossing's own: where the ask stands, one column
message per sampled position, the layer count and the width the model's own, watched red
under the take removed.

```graph
node: spu-column-registry-three-arms
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-column-registry-three-arms
```

```graph
node: spu-column-crosses-where-asked
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-column-crosses-where-asked
```

**One forward per prompt when readout is elected, never a batch.** Batching
prompts changes what the attention sees, so residuals taken from a batched
forward are not the residuals of the prompt in isolation, which the archived
tree's probe records as the reason it clears the cache between prompts. The
same fact means the readout's cost is not amortizable, and that cost is the
operator's to elect per load. Review's by non-purchase, and this one wants a
device: the property is a count of forwards under a real backend.

```graph
node: spu-one-forward-per-prompt
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-one-forward-per-prompt
```

**The tap's failure while elected is a fault, not an absence.** Per charter
section 13.10, because an elected observability that silently stopped
observing is a record that reads as a run without readout rather than a run
whose readout broke. Review's by non-purchase, a tap made to fail under an
election being reachable wherever the tap itself is.

```graph
node: spu-tap-failure-is-a-fault
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-tap-failure-is-a-fault
```

## 7.5 The probability field

**It is produced at the signals' own site and from the same read**, per
charter section 13.11. Section 6 already takes the distribution before the
sampler and already holds the logits as a host-side vector to compute the
entropy and the surprisal, so the field costs no additional read and no
additional copy: it is a ranking of a vector this crate is already
holding. Producing it anywhere else would either read a state the draw has
moved past or pay for the vector twice.

**The ranking is a partial selection and not a sort**, the depth being
small against a vocabulary in the hundreds of thousands. What is reported
is the depth's worth of candidates in descending probability with the
realized rank beside them, probabilities rather than logits because the
consumer's axis is probability and a consumer re-normalising per position
would be doing the same arithmetic this crate already did.

**The depth refuses below the sampling cutoff, at admit.** Charter
section 13.11 rules the floor and the mechanism is this section's: the
effective top-k of section 8 is known at admit, the declared depth is
known at admit, and a depth below it refuses there naming both numbers.
Refusing at the first turn instead would spend the admit's device work to
learn what a comparison of two integers answers, which is the
fail-cheap-or-lie-expensive rule again.

```graph
node: spu-field-depth-refused-below-the-cutoff
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-field-depth-refused-below-the-cutoff
```

**The field is observational and that is a test rather than a claim.**
Charter section 13.11 obliges an elected field to change no token, and
section 10 buys it: one declaration, one seed, the same turn run with the
election standing and without it, the token sequences compared whole. The
watch is what makes the split-run technique legitimate - two agents
carrying two elections and their records joined at token position - which
is otherwise a joint over sequences nothing proved identical.

```graph
node: spu-field-changes-no-token
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-field-changes-no-token
```

**Absent where unelected, and the absence has no shape.** No empty vector
crosses per position and no member sits null on the intermediate: the
message is not sent. A consumer counts what was produced rather than
filtering what was not, per the decode contract's section 2, and why the
election did not stand is the load event's to say rather than this seam's.

## 8. Sampling, and the dispositions

**Every knob carries a `Disposition`, elected at the worker's composition
root.** Per the composability ruling of 2026-08-02 and charter section 13.8:

```rust
pub enum Disposition<T> {
    Frozen(T),
    OperatorTunable,
}
```

A knob is `Frozen` with its value compiled into the binary, or
`OperatorTunable` and routed from the agent's configuration at load. The
machinery beneath takes a plain value and never learns which side supplied it,
which is what makes the two dispositions cost the same and a change between
them one line and a recompile. A knob left without an election does not
compile, the type carrying no third case and no default, which is the claim
section 10 sorts under the compiler.

```graph
node: spu-disposition-compels-election
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-disposition-compels-election
```

**The knob set is temperature, top-k, top-p, the repetition penalty and its
window, and the seed.** The seed is a knob for the first time in this code's
lineage: the archived tree carried it as a hardcoded default and a determinism
test and never made it configurable, so the disposition mechanism is its first
real home.

**The seed is operator-tunable as of 2026-08-20 and the rest of the surface
stays frozen**, per the operator's ruling and apex section 8's own terms. That
section makes stochastic re-entry from a declared starting field available
"when the binary declares one" and says what freezing buys: a frozen seed
"narrows variance and buys audit rather than determinism". Freezing it here
bought neither, because one binary then draws one trajectory per task however
many times it is run, and a distribution over a task needs draws rather than
repetitions of one. Re-entry survives the move and improves: the effective
seed rides every `model.request`'s sampling block already, so a recorded run
names the draw it was and is re-entered by declaring that seed again, which is
the same property held per run instead of per binary. What must not move
beside it is the rest of the surface - temperature, the truncating filters,
and the penalty pair stay `Frozen` - because a distribution gathered while
those varied would report the sampler rather than the task. The no-defaulting
rule reaches the new election in full: a declaration that supplies no `seed`
refuses the load naming it, per this section's own rule below, so a run can
never draw under a seed nobody chose. Compile-pin
on the operator's election of 2026-08-04, superseding this clause's earlier
non-purchase ruling: the membership is a struct literal a doctest reads, every
member riding its `Disposition`, so a knob added or dropped stops the build,
and the bullet section 10 was missing is the bullet that now reads it. The
ruling's premise was the missing bullet, so the ruling falls with it.

```graph
node: spu-knob-set-includes-the-seed
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-spu
to: spu-knob-set-includes-the-seed
```

**Frozen values never cross the wire and that is checkable.** Only the
operator-tunable remainder crosses at all, and section 10 makes it a test
rather than a promise.

**The remainder crosses in the declaration at admit, not on the token seam,
and this act moves it.** The earlier route had a value sourced from the agent's
configuration at load, transported per turn on the token seam, and consumed at
`open_session`, and those three do not meet: the engine builds its sampler once
at open, so a value arriving with a token directive has no engine to reach.
That is why every knob is `Frozen` today, which reads as an election and is a
forced hand.

`decoder.tunable_values` carries the remainder instead, arriving once with the
instruction the admit already brings, at the moment the session is about to
open. Resolution stays where it was, `Knobs::resolve` against the supplied map,
and neither the dispositions nor the resolve change shape.

**What this is watched by reads one direction.** The capture of section 10
fails when a sampling value reaches this crate on the token seam and ignores
what leaves on it, because `Generation` carries the request this crate rendered
whole with its effective sampling among it, per charter section 13.8's rule that
the record holds what a turn ran with whichever side set it. A capture reading
both directions would fail on the record and call it a leak.

**The token seam's tunable map goes rather than staying unread.** With the
values arriving at admit, a map on the token directive would be a payload field
whose only reader is unbuilt, which apex section 9 refuses in the same words it
refuses an empty joint. A per-turn tunable remains possible and is a different
feature, changing a value mid-session, and it wants the engine to rebuild its
sampler between turns. When that is built it brings its own carrier back rather
than finding one waiting.

**What this buys is the iteration loop the disposition mechanism was for.** A parameter
left tunable is set in the agent's declaration and reaches the engine on the next load,
so a developer moves it by reloading the agent. A parameter frozen for a production
binary cannot be moved by any declaration and never travels. Which is which stays one
line and a recompile at the composition root.

```graph
node: spu-tunables-arrive-in-the-declaration
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-tunables-arrive-in-the-declaration
```

```graph
node: spu-frozen-values-never-cross
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-frozen-values-never-cross
```

**The effective values are what the record holds, whichever side set them.**
This crate reports the values it sampled with into `model.request`'s payload,
per `weaver-trace-PRD` section 3.2, so a frozen knob is as visible in the
record as a tunable one and a replay reads one list rather than joining two.
Review's by non-purchase, and it is the near twin of the test above read the
other way: that one asserts what the seam does not carry and this one asserts
what the record does.

```graph
node: spu-effective-values-recorded
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-effective-values-recorded
```

## 8.5 The seed's derivation

**One function, stated here so two readers cannot differ.** The derived
seed of a generation is `splitmix64` applied to the declared seed mixed
with the turn and with which generation of that turn this is:

```rust
pub fn derived_seed(declared: u64, turn: &TurnKey, generation: u64) -> u64
```

**It is stated to the bit, because two implementations that differ by one
constant produce runs that cannot be compared and nothing says so.** All
arithmetic is on `u64` and wraps. The turn enters as a hash rather than as
itself, the reference being a string admin minted and this crate needing a
number from it without caring what it spells.

    fnv1a(bytes):                       the reference's UTF-8 bytes
      h = 0xcbf29ce484222325
      for each byte b:  h = (h XOR b) * 0x100000001b3
      answer h

    finalize(x):                        splitmix64's finalizer
      z = x + 0x9e3779b97f4a7c15
      z = (z XOR (z >> 30)) * 0xbf58476d1ce4e5b9
      z = (z XOR (z >> 27)) * 0x94d049bb133111eb
      answer z XOR (z >> 31)

    derived_seed(declared, turn, generation):
      state = finalize(declared XOR fnv1a(turn))
      answer  finalize(state XOR generation)

**The generation ordinal is zero-based**, so the first generation of a
turn is zero and the base case below pins it. `splitmix64`'s finalizer is
chosen for being short enough to state in full, standard enough to be
recognised, and well distributed on sequential inputs, the last mattering
because the ordinal is sequential and adjacent turns must not draw
adjacent streams. Measured on the vectors below, adjacent ordinals differ
in 33 of 64 bits and adjacent turns in 31.

**Test vectors, which an implementation checks itself against.** They are
the specification's, computed independently of any implementation, and a
build that disagrees with one has a defect rather than a variation.

    declared              turn   generation   effective
    0                     t-1    0            0x458763dd2277adcc
    11                    t-1    0            0x6025a13ad2f4a430
    11                    t-1    1            0x5144c61d6c67c975
    11                    t-2    0            0x25f1b675aac3c47d
    2814393375            t-2    3            0xcf4ad3d2e2e14630
    18446744073709551615  t-1    0            0x007be97cffceca04

    fnv1a("t-1") = 0x5627091943b2a601
    fnv1a("t-2") = 0x5627061943b2a0e8

**The generation ordinal counts within its turn and resets with it.** A
turn runs as many generations as its tool rounds, up to the harness's
bound, and two of them sharing a seed would draw one stream twice: the
model would answer its own tool result with the same run of luck that
produced the call. Counting within the turn rather than across the
residency is what keeps the derivation free of history, since replaying a
turn issues the same generations in the same order.

**Nothing about the count is carried between residencies**, so a second
run of the same turn under the same declared seed derives the same value.
That is the property the whole ruling exists for, and what it takes to use
it is worth stating exactly, since a reader who supplies the wrong member
gets an answer rather than a refusal.

**Re-entering a recorded generation supplies the whole sampling block and
checks one member of it.** The declared seed, read from `seed`. The rest
of that block's values beside it, temperature and the truncating filters
and the penalty pair, because the stream those three inputs fix is read
through them and a different filter answers differently. The turn, as its
reference. The generation's index within that turn, zero-based. And the
same resident context and weights standing behind all of it, since the
derivation governs the stream and never the distribution it is read
against.

The block is the record's own, one place holding every value the sampler
ran with, so a re-entry reads it rather than assembling the surface from
a declaration that may have moved since.

**`generation_seed` is not among what is supplied.** It is the recorded value a
correct re-entry reproduces, which makes it the check rather than an
input, and nothing in this crate accepts a seed by that route: the sampler
is built from `derived_seed` of the three above and from nothing else.
Supplying it in place of them is not a shortcut but a different operation
this program does not offer.

```graph
node: spu-seed-derives-per-generation
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-seed-derives-per-generation
```

**The sampler is built per generation and holds nothing between them.**
Both engines construct their sampler from the derived seed at the start
of each generation, so no stream survives a generation boundary and no
generation's draws depend on another's. The penalty window is restored
from the resident tail rather than accumulated across generations, the
window's length being a knob and the tail being where the tokens are: the
native path already read the tail directly and the GGUF chain accumulated
its own copy, so reading one source makes the two agree by construction
rather than by two implementations staying in step.

**This retires the flush's coupling to sampling rather than repairing
it.** The GGUF chain's reset cleared the penalty window and reseeded the
draw together, because llama.cpp's chain reset reaches every sampler in
it. With the window read from the resident tail there is nothing to clear:
after a truncation the tail is the truncated tail, and after a
re-establishment it is what was re-established. The reset is not called
and the accident it carried cannot recur.

**The watch is a flush standing between two generations**, which is the
case the retired coupling lived in: two generations of one turn with a
flush between them draw the streams their ordinals name, and the second's
stream is what it would have been had no flush happened. Watched under the
reset's restoration, which reseeds the second generation to the declared
value and fails the comparison.

```graph
node: spu-sampler-holds-nothing-between-generations
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-sampler-holds-nothing-between-generations
```

**The request records both seeds, under these two names.** The rendered
sampling block of the `model.request` carries them as `u64`:

    seed              the declared run seed, resolved from the disposition
    generation_seed   the derived value this generation drew from

**`seed` keeps the meaning it already had**, which is the reason for the
naming rather than a preference between words. Every record written before
this act carries `seed` as the value the sampler was built from, and that
value was the declared one because the two were the same number. Reusing
the name for the derived value would silently reinterpret every earlier
record, and the 2026-08-20 corpus is the one this program's first findings
rest on. The new member takes the new name.

**Neither is called effective**, and this section's prose does not use the
word for them either: this crate already spends it on the knobs a
disposition resolved, and `seed` is one of those. A member named for the
derivation says what it is without borrowing a word that already means
something one layer up.

Both are recorded because either alone is a half-answer. The declared seed
does not say what a generation drew from, and the derived seed does not
say what to declare in order to re-enter it. Section 10's capture of the
sampling block reads both members and their values, the same instrument
that already watches which knobs cross.

This crate renders the block, so the pair reaches the record without the
trace crate learning either name.

## 9. The failure vocabulary

```rust
pub enum ChannelFault {
    Truncated { bound: usize },
    Undecodable,
    Closed,
}
```

**Refusals are the floor's and faults are below the exchange layer,** the same
split `weaver-harness-Spec` section 7 and `weaver-gate-Spec` section 5 make.
Every refusal this crate issues is a `lifecycle-refusal` on the residency seam
or a `token-refusal` on the decode seam, drawn from the floor and never
twinned. A fault the worker survives is a `fault-report`, also the floor's,
carried on whichever seam the exchange belongs to, per charter section 13.10.
**The split itself is `weaver-harness-Spec` section 7's and the record here is
this crate's side of it.** That Spec argues the principle and `weaver-gate-Spec`
section 5 adopts it with a record of its own, so one node with three edges would
say the wrong thing: what each crate asserts is which floor type its own seams
carry, and this crate is the only one of the three holding two seams and two
refusal types. The principle is cited and the mapping is recorded. Review's by
non-purchase, a refusal raised on each seam being reachable without a device. **It
grounds in apex section 5.3.** A contract names the errors each party can return, so
a crate that twinned a local error type beside the floor's would answer with a case
its own contract does not describe, and the completeness that invariant claims would
hold for the document and not for the code.

```graph
node: spu-fault-below-the-exchange-layer
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-fault-below-the-exchange-layer

edge: grounds
from: spu-fault-below-the-exchange-layer
to: axiom-contract-is-a-complete-interface
```

**A directive out of order for its seam's state answers `OutOfOrder` and is not
queued, on both seams.** `weaver-types-Spec` section 5 owes the refusal to each
organ and enforces it nowhere, and section 3 of `weaver-harness-spu-contract` and
section 3 of `weaver-harness-spu-decode-contract` each bind it here against a
state of their own, so this document states it twice rather than once in the
abstract. **The order is judged against the channel's recorded position before
the directive reaches residency or the session,** which is what makes not-queued
mean anything at all, a refusal that had already run the work being a refusal
about nothing, and it is what puts every position inside a test with no artifact
resolved and no device taken.

**The residency seam has three positions and the last is terminal,** per that
contract's section 3: before-admit, admitted, released. Admit is first and
happens exactly once, so a second admit answers `OutOfOrder` whatever the first
answered, this crate admitting once and dying per section 3 rather than matching
a prior residency against a later request. A release with no completed admit
before it is refused and not queued, there being no residency for it to end, and
a directive of any kind arriving after a release answers the same.

**The decode seam holds the richer state and the decode contract rules all of
it.** Open is first, happens once, and is valid only after the residency it
serves is confirmed, so an open before residency answers `OutOfOrder`. One
generation is in flight at a time, so a second append-and-generate while one is
outstanding answers the same, one turn behind one intent. Flush is valid only
between turns, so a flush arriving mid-generation answers the same, the cancel of
section 4.3 being what that case has instead. The elision is valid on the flush's
ground and for its reason, per the decode contract's ordering: a span named
against a sequence still growing names positions that will have moved by the
time it lands. **Cancel is the one directive whose window is the session
rather than the generation,** and a cancel at rest answers at rest rather
than refusing, which is the contract's own reading and not a fourth position
this document adds.

**Each seam carries its own refusal type and neither carries a twin of the
other's,** per the split above: the residency seam answers a `lifecycle-refusal`
and the decode seam a `token-refusal`, both drawn from the floor with the
`OutOfOrder` case the floor already holds. **The two are two records,** because
the two seams hold two different ordered states watched by two different
fixtures, and one record would report a single instrument for behaviours that
fail independently. Section 10 buys both, which discharges this crate's side of
the owing on both seams, the way `weaver-gate-Spec` section 6 discharged the
gate's on its one. **Both ground in apex section 5.3,** whose completeness reaches
the ordering guarantees a party relies on and provides: an interface that names its
vocabulary and its errors and leaves what happens outside its ordered state to the
implementation has a hole in it, and a queue is what fills that hole silently.
**Both also ground in apex section 5.5, and that is a second reason rather than the
first one restated.** A directive out of order arrived in an order this crate did
not choose, the harness driving both seams and no organ ordering its own traffic, so
the mis-ordering is a timing failure between domains and that invariant makes the
loop answerable for it. Queueing would be this crate reconciling that failure inside
its own domain, which is the reconciliation 5.5 assigns to the loop by construction,
and the refusal is what returns the failure to the party answerable for it. The
decode seam carries the sharper case, an open being valid only after the residency
it serves is confirmed, which is one contract's ordering read against the other's.

```graph
node: spu-out-of-order-refused-on-residency
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-out-of-order-refused-on-residency

edge: grounds
from: spu-out-of-order-refused-on-residency
to: axiom-contract-is-a-complete-interface

edge: grounds
from: spu-out-of-order-refused-on-residency
to: axiom-harness-integrates-by-the-loop

node: spu-out-of-order-refused-on-decode
kind: assertion
tag: perturbation

edge: asserts
from: weaver-spu
to: spu-out-of-order-refused-on-decode

edge: grounds
from: spu-out-of-order-refused-on-decode
to: axiom-contract-is-a-complete-interface

edge: grounds
from: spu-out-of-order-refused-on-decode
to: axiom-harness-integrates-by-the-loop
```

**The admit refusal cases are the charter's enumeration and this Spec adds
none.** Charter section 10 holds the set open with a candidate list and names
the decode workflow's measurements as what closes it, and the cases the
merged floor already carries, the artifact unresolvable or unreadable, the
device unable to admit, are what this crate maps its failures onto.

## 10. What is enforced, and by which instrument

Per apex section 11, the threat walks first.

**The first walk: this process holds a handle it should not.** The adversary
is a fork discipline that failed upstream, leaving this process holding the
trace descriptor or the coordination channel to admin, which would put a
writable handle to the agent's own record inside the agent's model server. The
mechanism is the harness's close-on-exec discipline at the fork, per
`weaver-harness-spu-contract` section 1, and this crate's own count check at
entry, per section 2. The test starts the binary with a deliberately leaked
extra descriptor and confirms it refuses to serve, watched to fail when the
count check is removed.

**The second walk: a same-uid process attaches.** The adversary is an elected
tool running as the agent uid, the attack a `ptrace` or `/proc/[pid]/fd` reach
into the process holding the weights and both channels. The mechanism is the
dumpable flag cleared at entry, a set and not a check. The test confirms the
flag is clear after entry, watched to fail when the set is removed.

**The third walk: the agent alters the artifact it runs on.** The adversary is
the agent's tool surface writing to the model artifact, which would change what
the record's weights hash describes. The mechanism is the operator's
provisioning, the artifact owned by a principal that is not the agent and
readable into it, per charter section 7, and this crate's contribution is the
hash computed at admit from the bytes it loaded rather than from a
manifest handed to it. The test alters an artifact between two admits and
confirms the recorded hash differs, watched to fail when the hash is cached
across the change.

**Enforced by the compiler.**

- The fork seam is a compile-fail pin, per the archived tree's own device and
  this Spec's inheritance of it: a doctest calling the eval-callback setter
  fails to compile against the upstream crate and compiles against the fork, so
  reverting the pin breaks the build rather than silently removing the readout
  capability. This is the pin the survey names as the one thing standing
  between the fork and a quiet loss. **Its record is tagged for the mechanism
  this bullet names rather than for the heading it sits under,** which is a
  doctest that must fail to compile, and it sits at the end of this section
  because the argument for it is nowhere else.
- The floor's wire enums are exhaustive, so every case added later reaches this
  crate's matches loudly. `weaver-types-Spec` section 4.2 argues that property
  and declares its record, and this crate consumes it.
- Descriptors are owned types end to end.
- `Disposition<T>` makes a knob without an election a compile error, so a
  builder cannot leave one unstated.
- The knob struct is a literal a doctest reads, every member named and the seed
  among them, so section 8's set is membership the build checks rather than a
  list a builder consults.
- The signal vectors' field is an option of a non-empty vector, its doctest
  reading the field's type, so the empty vector section 6 forbids is
  unrepresentable and the pairing is what remains for review there.
- The width declaration's field is a set type, its doctest reading a
  non-contiguous set literal, so a maximum cannot be declared, and the admission
  judgment against it is what remains for the perturbation below to watch.

**Enforced by compile-fail tests, because the property is an absence.**

- No path-taking model loader beyond the binding's own resolution: doctests
  constructing a loader from a bare `&str` and a `PathBuf` outside the
  admission path fail to compile. Two named shapes rather than a claim about
  all possible shapes, **with the general prohibition staying review's,** per
  the split the floor Specs make. **The split is two assertions rather than
  one,** the pinned shapes and the prohibition itself, both sitting at the end
  of this section because this bullet is the only clause that argues either: a
  single record tagged for the mechanical half would claim the doctests for the
  whole, which is the overclaim this corpus refuses in prose and has no reason
  to admit in a graph.
- No scoped-clear over a resident range: the session type exposes no method
  that reduces `resident_len` except the flush and the elision, and the elision
  reaches its interior outcome by re-establishing and re-decoding the survivors
  rather than by the range-clear the pin forbids, per section 4.5. Both
  reducers are recorded, so a rewind stays unrepresentable rather than
  forbidden, which is the append-only discipline made structural. Its record
  sits at the section 4.2 clause that argues the absence, beside the
  behavioural record the perturbation bullet below carries, the two being two
  claims about one property rather than one claim twice.

**Enforced by the manifest.** The internal dependencies are exactly
`weaver-types` without its `config` feature and `weaver-traits`, read against
the graph's two floor links under gate H2, the second landing with the charter
correction of section 1.1. No async runtime, no logging crate, and no HTTP
client in the resolved tree, by the build-time `cargo tree` assertion the floor
Specs share. Both fork pins resolve to their pinned revisions and
`llama-cpp-sys-2` resolves to the same source as `llama-cpp-2`, checked in the
same assertion, because the two-sys-crates failure is a resolution fact rather
than a code fact.

**Requiring a perturbation-verified test.**

- The native path judges the architecture before the shapes: a config declaring an
  architecture it does not serve refuses naming that architecture, and one
  declaring none reaches the shapes as before. Watched by judging after the parse
  instead, where a stock Qwen3 export refuses on a null `sliding_window` and names
  a config line that is correct, which is the state issue #507 met.

- A `System` message renders on every renderer that serves a conversation, and
  folds into the user turn that follows it where the template names no system
  turn. **These are two claims with two watches, and an earlier form of this
  entry named one outcome for both.**

  The rendering is confirmed by watching gemma or mistral answer
  `MalformedForFamily` when the role is removed from a delta path, which is
  the state that broke every turn of a dev-loop run against those families
  while the declaration's prefix rendered perfectly.

  The folding is confirmed differently, because once the role renders, an
  unfolded message no longer refuses - it renders as its own turn. So the
  watch is the merged text: replace `render_each`'s `fold_for_template` call
  with the messages unchanged and the two texts arrive in separate turn
  wrappers rather than one.

  **Both watches cover the delta path and not only the prefix**, the control
  loop's own voice travelling as a delta, and `weaver-harness` taking no
  dependency on this crate, so no workspace build compiles the loop's roles
  against the families that must render them.
- Truncation is a fault: an envelope over the 64 kibibyte bound on the lifecycle
  channel produces `Truncated` and no directive, confirmed by watching a silently
  shortened directive decode when the `MSG_TRUNC` check is removed. This is the
  watch `weaver-harness-Spec` section 8, `weaver-admin-Spec` section 10, and
  `weaver-gate-Spec` section 6 each buy, and until issue 37 closed this crate was
  the only one of the four receiving crates carrying the obligation on prose
  alone.
- A lifecycle directive out of order is refused and not queued: a release before
  any admit answers `OutOfOrder`, a second admit answers the same whatever the
  first answered, and any directive after a release answers the same, the
  released position being terminal. Confirmed by watching the second admit reach
  the resolution step when the admitted position is allowed to accept one, and by
  watching a held release run at the next directive when the refusal is replaced
  by a queue. The positions are driven against the channel's recorded state, per
  section 9, so the fixture resolves no artifact and takes no device. **The
  second-admit arm watches the refusal and not the idempotence claim beside it,**
  section 3's no-matching rule being about what a completed residency would be
  compared against, which this fixture never produces.
- A decode directive out of order is refused and not queued: an open before the
  residency it serves is confirmed answers `OutOfOrder`, a second
  append-and-generate while one is in flight answers the same, and a flush
  arriving mid-generation answers the same. Confirmed by watching the second
  generation reach the session when the in-flight position is dropped, and by
  watching the flush reach the session's truncation path when the between-turns
  condition is removed. What each arm reads is whether the directive was
  dispatched or refused, which is what not-queued means and what the recorded
  state puts within reach of a fixture holding no residency. **The flush arm is
  the one worth having,** its perturbation corrupting a session rather than
  answering wrong, which the next turn's framing reports and the flush's own
  answer does not.
- The elision's two watches, both at section 4.5's records: an unremovable
  span refuses and the session stands as the ask found it, watched under the
  guard narrowed to `to < from`, where eliding nothing answers with equal
  counts, an elision authored over nothing. The second: the removal is exact and
  ordered, asserted on the tokens themselves and watched under a kept vector
  built from the tail alone, because the confirmation's counts cannot catch a
  lost survivor or a reversed tail.
- The cheap refusals precede the device judgment: against a fixture entry declaring the
  widths one and two, with a binding naming three devices, a binding whose artifact
  resolves to nothing answers the resolution refusal rather than the width refusal, and
  an artifact whose header cannot be read answers on the header rather than on the
  devices. Confirmed by watching both fixtures answer on the width when the resolve and
  header steps move below the judgment. **The three-device set is what makes the watch a
  watch,** since it fixes what the reordered path would answer, so the two arms differ
  by the ordering alone and neither reaches a device under either arrangement.
- The session never rewinds: a multi-turn session's resident length moves only
  by appends and by the two recorded reducers, the flush and the elision, and
  each turn's prompt tokens are the delta's alone, confirmed by watching the
  count return to the full history when the append path is replaced by a
  re-prefill. This is the archived tree's own session test carried over, and
  it is the one that proved the recurrent-architecture failure. The elision's own
  re-decode runs through the re-establish path the flush already takes and is
  not the rewind this bullet forbids, being recorded and being a return to no
  state that ever stood.
- The terminator lands on both paths: a cancelled turn leaves a session that
  accepts the next delta cleanly, confirmed by watching the next turn's framing
  break when the terminator step is skipped on the cancelled path.
- The stop is bounded: a cancel lands within one token's decode, confirmed by
  watching the bound fail when the check moves outside the loop.
- Marker promotion, per renderer: every control marker tokenizes to exactly one
  token, confirmed by watching a marker degrade to subword text when the
  special-token path is bypassed. **This is the inbound direction of section 5
  and only that,** the outbound direction taking the bullet below, which is why
  that clause reads as two records.
- The parse reports an unrecovered call: an emission opening a renderer's call marker
  and naming nothing the parser can recover answers with that fact rather than with a
  clean turn's content, confirmed by watching the fragment arrive as ordinary assistant
  text when the unrecovered case collapses into the text path. The fixture is a rendered
  string against a family module, so the watch reads the parse's own answer and reaches
  neither a model nor a device.
- The registry substitutes nothing: an artifact whose header names an architecture this
  binary does not carry refuses at admit naming that architecture, confirmed by watching
  the admit proceed on a carried architecture's module when the table gains a
  nearest-match fallback. The watch reads the architecture the refusal names rather than
  the fact of a refusal, so a refusal arriving on some other ground does not pass it.
- The width condition refuses against the declared set: a fixture entry declaring the
  widths one, two, and four refuses a binding naming three devices, and the refusal
  names the width. Confirmed twice, by watching the binding pass the condition when the
  width test is dropped from the judgment, and by watching it pass when the declaration
  is read as a maximum. **The non-contiguous fixture is what makes the second watch able
  to fail,** a declaration of one and two answering alike under both readings, so a
  contiguous fixture would leave that perturbation unwatchable while the test still
  passed. The width is judged before either driver query, per section 3, which is what
  keeps the whole test on a machine with no device.
- Signals are pre-sampler: the entropy of a step matches the distribution
  before sampling, confirmed by watching it collapse when the computation moves
  after the sampler.
- Frozen knobs never cross: a capture of the token seam contains no frozen
  value, confirmed by watching one appear when a knob's disposition is changed
  to tunable without changing the binary's declaration.
- Readout elected but untappable refuses at admit, confirmed by watching the
  load succeed and the first turn fail when the check moves off the admit path.

**The kernels cross verbatim and their tests cross with them.** The CUDA
kernel set and its build script are carried unchanged, per the operator's
ruling of 2026-08-02 and the carry rule's first door, and the golden fixtures
that compare each kernel against a candle reference come with them, because a
salvaged kernel with its comparison left behind is a kernel this program has
not checked. Their provenance is the survey. **This one is review's by reach
and not by election,** which is the rare case: what the fixtures assert about a
kernel fires when the kernel drifts, and no suite can watch a comparison that
was left behind, a test being unable to detect its own absence. Whether the
comparisons crossed is therefore a fact about the carry that a reader
establishes and a runner cannot.

**Which invariant each claim serves, and why most serve none.** Fourteen `grounds`
edges run from eleven of the eighty-nine: four to
`axiom-contract-is-a-complete-interface`, six to
`axiom-floor-is-vocabulary-behavior-is-socket`, three to
`axiom-harness-integrates-by-the-loop`, and one to `axiom-organ-and-submodule`.
**Three records carry two edges each**, which is the whole of the difference between
the fourteen and the eleven: the two out-of-order refusals, because the contract
states the ordering and the loop is answerable for that ordering holding, and the
floor-link record of section 1.1, which says at its own clause that it carries both
edges. **The test applied is whether the axiom is the reason the claim exists, or
the claim a precondition of the axiom's own stated reason.** Remove the socket
invariant and this crate has no reason to be a process the harness starts rather
than a library it calls, no reason to carry a truncation obligation attached to a
socket type nobody would have elected, and no reason to refuse a signal-borne
cancel, so those three ground in it. **The two hygiene claims
of section 2 ground in it by the second relation rather than the first**, that invariant
resting possession-as-authentication on no third party reaching a socket with no
address, and a descriptor that crosses an exec or a process that admits an attach each
being such a reach. **The floor-link record is the sixth**, and its edge there is the
plainest of them: the crate links the floor at all because a socket cannot carry
shared vocabulary, which section 1.1 argues where the record sits.
Remove the invariant and the session is still append-only, the registry still
substitutes nothing, and every sampling knob still carries a disposition, so those
ground in nothing.
`axiom-join-key-travels-with-the-work` takes nothing from this crate, and the absence is
that invariant's own scope rather than a gap: every directive on the residency seam
belongs to no turn and apex section 5.2 exempts it by name, and on the decode seam,
where the work does belong to a turn, this document asserts nothing about attribution
because that seam's encoding is section 12's open election. The nine claims section 0
names are another Spec's to ground, including the socket election and the envelope bound
the truncation obligation rests on.

**The loop invariant reaches three claims and stops at this crate's interior.** It
binds what crosses between domains and says nothing about what happens inside one,
so the decode submodule's relationship with its own organ, the one service loop over
two channels, and the placement of each family's code are outside it whatever they
resemble one level up. The three claims are on the other side of that line, each
being this crate declining what the harness orchestrates rather than settling it
here, and each argued at its own clause: the two out-of-order refusals of section 9,
where the mis-ordering is a timing failure the loop is answerable for, and the
overflow refusal of section 4.2, where what a full context means is the harness's to
decide. **Two candidates were weighed and refused.** The envelope's confinement to
the lifecycle channel grounds in the organ axiom, which is the reason it exists, and
the loop invariant adds no second reason to it. The entry check on the descriptor
count exists for the first walk above, a writable handle to the agent's own record
sitting inside the agent's model server, and the discipline it checks is the
harness's at the fork rather than the loop's at a seam.

**Seventy-eight claims grounding in no invariant is the expected result and not a gap**,
per Document Format section 4, and the ratio is low here for a structural reason
rather than an unfinished one. The bulk of this document is decode mechanics,
sampling, family libraries, kernels, device judgment, residency bookkeeping, and
session monotonicity, and none of that is a question the five invariants ask. Much
of what makes this crate correct is not an invariant question. The one claim a
reader will look for and not find is `spu-nothing-retained-after-the-answer`, this
crate's face of the program's statelessness: the apex states that at section 2, as
what proto-stateful means, and its section 5 declares five invariants that do not
include it. An edge there would point at the nearest heading rather than at a
reason.

**Where the assertion records sit, and which of these bullets another crate
declares.** The records are at the clauses that argue the claims, across
sections 1 through 9 and the classify arm's section 11, rather than gathered
here, per Document Format section 6:
this section sorts by instrument and the arguments are elsewhere, so a block
here would sit apart from the prose that earns it. Four are the exception and
sit at the end of this section, being the claims argued only here: the fork
seam's doctest, the path-taking loader's two pinned shapes, that same claim's
general prohibition, and the kernels' comparisons. Eighty-nine records in
all, the custody act of 2026-09-05 adding the artifact refusal's account and
the definitions act of the same date the closing count's review record,
the elections taking nodes because gate H1 would otherwise leave the largest
decisions in this Spec untraceable. Four of these arrived with the
re-feed act of 2026-08-31 without this count moving, that act's own
oversight, and the column act of 2026-09-01 that recounts here adds three -
the registry, the crossing, and the arrives-or-faults rule the review seat
held it to - and moves one from review to perturbation, the unasked-vector
watch its ground obliged it to buy. **The split this paragraph used to owe, this
section's sorting against the elections outside it, is retired for a count that
needs no reading.** It read thirty-one and twenty-nine against a total of sixty,
was never brought forward, and by 2026-09-15 disagreed with the total three
paragraphs above it: that sixty and the nineteen records it put since sum to
seventy-nine where the crate carries eighty-nine. It was never recoverable
mechanically either, since which bullet here covers which record is a judgment and
an election is a reading rather than a tag. Where each record is declared is a
fact, and this is the count, each subsection counting to its section: nine in
section 1, seven in section 2, eleven in section 3, eighteen in section 4, nine in
section 5, seven in section 6, ten in section 7, two in section 7.5, five in
section 8, two in section 8.5, three in section 9, four in this section, and two
in section 11. **Sections 0 and 12 declare none and the zero is stated rather
than left out**, section 0 saying what this document is and section 12 holding
the open elections, so those thirteen are every section that declares a record
and the sum is complete rather than right by coincidence.

A divided claim's two halves both count with the sorting, per
Document Format section 3, which is where the absence and width divisions of
the operator's 2026-08-04 election count. Three of that section's are tagged
for review and every other one of them carries a mechanical instrument: the
loader's general prohibition, which is the review half of a split, the pairing
half of section 6's absence claim, divided likewise, and the kernels'
comparisons, which the paragraph above argues. **One bullet
above is a claim another crate argues,** and carries no record here: the floor's
exhaustive wire enums, which `weaver-types-Spec` section 4.2 declares. Eight
more claims this Spec cites and another Spec argues are named where the sections
use them and listed in section 0, and the shape behind all nine is that this
crate holds neither the floor's definitions nor the fork that creates its
channels.

**Thirty-one records carry `review` as of the definitions act of 2026-09-05,
the closing count's record joining the thirty of the column act's recount, and what
has moved off the tag is what needed neither hardware nor seam work, plus
the one watch whose seam arrived.** The five that moved are
the cheap refusals of section 3, the registry's refusal and the width refusal
of section 5, the truncation fault of section 2, and the outbound parse of
section 5, all of them reachable with a fixture and none of them touching a
device. What
remains on prose in the residency half is what reads a driver: the room and reach
conditions of the admission judgment and the release's free-before-answer
ordering. Those reach the driver through a seam a suite could double and this
Spec does not introduce, so they are unbought for the seam rather than for the
watch, and introducing that seam is a larger act than this one. Every `review`
tag in this document states its ground at its own clause, and outside two
cases the ground is that no instrument was bought rather than that none exists.
The two exceptions say so where they sit: the absence of anything laid in for
an operation type that does not exist, and the kernels' comparisons above.
Section 7's unasked-vector watch stood third among them until the column act
built its seam and bought it, as its ground obliged. What tag it carries since
is recorded where that act's changes are, in the paragraph on where the records
sit, and is not restated here.

**The out-of-order refusal is stated and bought on both seams, per section 9,
and this crate's side of the owing is discharged.** `weaver-types-Spec` section
5 owes it to each organ and enforces it nowhere, the gate discharged its one seam
at `weaver-gate-Spec` section 6, and the two bullets above are the residency and
decode seams answering the same demand against two different ordered states. What
made this crate's half the largest of the three is that the gate and the harness
each state the behaviour and lack only a test, while this document stated it
nowhere, so the act had to write the claim before it could buy one.

```graph
node: spu-eval-callback-pinned-by-doctest
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-spu
to: spu-eval-callback-pinned-by-doctest

node: spu-loader-shapes-pinned-by-doctest
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-spu
to: spu-loader-shapes-pinned-by-doctest

node: spu-no-path-taking-loader
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-no-path-taking-loader

node: spu-kernels-cross-with-their-fixtures
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-kernels-cross-with-their-fixtures
```

## 11. The classify submodule

How charter section 15 is represented: the process, its binary, its channel,
its family, and the exchange's execution. Written against
`weaver-harness-spu-classify-contract` and `weaver-types-Spec` sections 2 and
4.5, and everything here restates a charter clause or elects a representation
against one.

**The binary is a second bin target of this crate, `weaver-spu-classify`,
settling charter section 10's cell.** The alternative was one binary under a
role argument, and the named binary wins on the provisioning ground the
worker-binary precedent set: which binary a unit starts is a provisioning
fact stated plainly, and a role flag on the decode binary would let one
mistyped argument stand a decoder where a classifier was meant, refusing
nothing at the point the mistake was made. The bin shares this crate's
library, its family registry, and its admission judgment, which is what
keeps the one-authority claim a fact about the crate rather than about a
process.

**The channel takes the `SOCK_SEQPACKET` election of section 2 unchanged**,
one write one message from the socket type, with the same receive
obligation stated for this seam explicitly: the receive buffer is sized to
the election's 64 kibibyte bound, a read returning with `MSG_TRUNC` set is
a channel fault and never a message, and each side asserts the bound on
its own writes. A frame that would exceed the bound therefore fails at the
send, which on the harness's side the seat converts into the same absence
every failed ask serves, so the wire bound needs no second refusal name:
the artifact's token bound is judged after tokenization on the process's
side, per the trio's `Oversized`, and the datagram bound before any send
on the asker's. The answer is a label vector whose size is the head's,
nowhere near the bound. The encoding is the
label trio's settled JSON, per `weaver-types-Spec` section 4.5, and unlike
the token seam's it defers nothing: there is no hot path to measure.

**The classify process receives exactly one descriptor, its label seam
end, and section 2's count check holds here at one.** The check stays a
count rather than an identification, per section 2's own reasoning, and a
count above one means the discipline upstream failed and this process is
not the one to continue past it. The decode-carrying process's own count
stays two and this act moves neither.

**The binding rides `spu-instruction` as a `classify` subsection beside `decoder`**, per
`weaver-types-Spec` section 2: the artifact and its device assignment, the same shape at
the smaller size, handed to the classify process at its exec per charter section 15.3.
Admission judges the binding against the resolved entry's declared capabilities and the
device's room exactly as the decoder's admission judges its own, the model first per the
charter's ordering, and the readiness emission is the label seam's first message.

**The first classify family is `modernbert`, and it enters a table of its own beside
section 5's registry, `modernbert::CLASSIFY_FAMILIES`, keyed by architecture and
selected by `select_classify`.** Charter section 14 has the operation an entry's models
serve be the table the entry sits in: a decode admission selects among the registry's
entries, by architecture and the marker set as before, and a classify admission selects
among the classify table's, so neither operation's candidates shadow the other's. The
template joins the key only where an entry renders one, per the template-joins-the-key
ruling, and a classify entry renders none, so its key is the architecture and the
operation alone. The decoder's selection behavior does not move. `modernbert` serves
classify and nothing else, renders no template, and reads its head from the artifact's
own declaration, the label names being the artifact's `id2label` in index order, which
is the head defining its labels the charter requires. The scores are the head's softmax,
finite by construction over finite logits, one per label. Where the logits arrive
non-finite the exchange faults rather than answers, per the trio's finiteness rule. The
forward is the fork's `ModernBertForSequenceClassification`, and the tokenizer walks the
same artifact-sidecar path the native decode path walks.

**Statelessness is structural.** The process holds the admitted artifact
and no session type: each exchange tokenizes, forwards, and answers from
its own stack, nothing written that outlives the answer, which is what the
contract's identical-asks conformance watches from outside.

```graph
node: spu-classify-answer-is-the-head
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-classify-answer-is-the-head

node: spu-classify-stateless
kind: assertion
tag: review

edge: asserts
from: weaver-spu
to: spu-classify-stateless
```

## 12. Open elections

Each names what settles it, and none is this Spec's to settle alone.

- **The token seam's encoding.** The hot-path measurement the charter and both
  floor documents defer to, per `weaver-types-Spec` section 4's boundary rule.
  Taken against real decode traffic, which this crate's own first
  demonstration produces.
- **The diagnostic answer's efficient framing.** Section 7 puts the column
  message on the wire in `weaver-types-Spec` section 4.4's provisional bare
  JSON, and what stays open is the framing worth shipping at the column's
  volume. It is settled by the same act that settles the token seam's
  encoding above, measured against the consumer draining it rather than
  elected blind - the reader being downstream of the sink rather than a party
  to any delivery contract, per `weaver-analysis-PRD` section 3.
- **The headroom figure.** Charter section 9's staged item, a construction
  parameter until a measurement on a real artifact against a real device
  replaces it.
- **The session capacity figure, and the per-turn generation ceiling.** Both
  carry a `Disposition` per section 4.2, so what is open is the compiled value
  a deployment freezes rather than the route. What settles the capacity is a
  measurement of what a device holds at a quantization, which is the cache cost
  per resident token against the room left once the weights are resident, and
  the artifact's declared window bounds the answer without being it. The
  ceiling settles against what a turn is allowed to spend before the backstop
  fires, which is a judgement rather than a measurement.
- **Which parameters this binary leaves tunable.** The election is the
  builder's per charter section 13.8 and it is a deployment's question rather
  than this Spec's, the way the family set is. What this Spec fixes is that
  every parameter carries an election and none defaults, not which way any
  deployment casts it.
- **Whether the artifact's declared shape is verified against its tensor
  data.** Charter section 9's second staged item, priced by what the check
  costs on the admit path.
- **The family set this binary carries.** Which families ship is a
  deployment's question rather than this Spec's, and the registry of section 5
  is what makes the answer a list rather than a rewrite.
- **Shard widths beyond two.** The salvaged path is a two-device
  implementation and the capability declaration is what a wider one would
  change, so an N-way forward and its all-reduce are work this program does
  rather than salvage it inherits, entered when a deployment needs a model
  wider than a pair.
- **The executor.** Deferred with `weaver-traits-Spec` section 6's
  measurement, and this crate is where the latency it would buy or cost is
  measurable.
- **The elision's rollback-assisted path.** Section 4.5 elects the re-establish-whole
  mechanism for entries of either flush and prices the elision at a prefill of the
  survivors. For an entry whose flush truncates the primitive for a cheaper path exists,
  truncate to the span's start and re-decode the tail alone. What settles it is a
  deployment whose elision spans sit late in long sessions, where the tail refill is the
  difference that matters, and the election is priced against the second instrument the
  per-entry divergence would then need.
- **The satellite types.** `Disposition<T>`'s companion accessor shape,
  `ChannelFault`'s spelling against the two identical enums in the harness and
  gate Specs, the family surface's trait name, and the backend seam's. Choices
  with no cross-crate consequence, listed so what this Spec leaves to a builder
  is complete rather than implied.
