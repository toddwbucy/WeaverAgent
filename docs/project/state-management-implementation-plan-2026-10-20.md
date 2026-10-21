# State management implementation map

Date: 2026-10-20. Status: implementation tracking, not a Spec amendment.
Updated: 2026-10-21 for the harness/member capture transaction.

The authority is the merged [state management set][set] from PR #182. All code PRs
start as drafts. The Planner controls the reviewed transition out of draft.
The operator merges them.
The store remains in weaver-state. The loop remains in worker/dev_loop and calls
primitives. The SPU owns classification.

## Dependency and merge order

Each row is a reviewable code slice. Dependent branches may be stacked before their
base merges. A PR body must name its prerequisite PRs and the exact owning clauses
it implements. Rows can be split further without changing the Spec or cutover.

| Slice | Prerequisites | Implementation and owning Spec |
| --- | --- | --- |
| F1 | PR #182 | Shared state_flow shapes, codecs and pure validators, types amendment |
| F2 | PR #182 | RAM connection and full catalog, state STATE-CONNECTION/STATE-SCHEMA |
| F3 | F1, F2 | Atomic filing, effects, reads and verified images, state amendment |
| F4 | F1, F3 | Member, paging and capture cleanup, state/contract amendments |
| T1a | PR #182 | Pure trace registry and validated copy, trace amendment |
| T1b | T1a | Typed trace authoring and recorder integration, trace amendment |
| IN1 | PR #182 | Pure borrowed recall query, internal amendment |
| C1a | F1 | SPU transport, admission proof and constrained binder, Spec 02 |
| C1b | C1a | Normative grammar and template proof, Spec 02 O02-1 |
| C1c | C1a, C1b | Model adapter, duty entry and qualification, Spec 02 O02-2 |
| C1t | H1b1 | Harness STE100 socket adapter, Spec 02 CHANNEL and PORT |
| P1a | F1 | Serving declaration, canonical artifacts and verified reads, admin/types amendments |
| P1b | P1a | Staged artifact publication and restore append, admin/persistence amendments |
| P1c | P1a, P1b, F4 | Contained seed driver, admin ADMIN-SEED and TYPES-SEED |
| P1o | P1a, P1b | Offline bind and restore drivers, admin selection/rebind clauses |
| P1d | P1a, P1b, F4, C1 | Serving lifecycle and verified boot, admin/persistence amendments |
| H1a | F1, T1b, F4 | Copy budget, trace proof and sole channel mechanics, harness amendment |
| H1b1 | H1a | Ordered filing ledger, composition, clock and measurements, harness amendment |
| H1p | C1t | Bounded organ, decode and label packet owners, HARNESS-PUMP |
| H1r | H1p | Bounded recording progress and typed operation results, harness/trace amendments |
| H1c | H1r | Run recording, common pump and frozen failure-account support, harness amendment |
| H1i | H1c | Checked input transaction and single-use source handoff, Spec 02 and harness amendment |
| H1s | H1i | Member capture barrier, correlated cleanup and recorded results, foundation/persistence/harness amendments |
| H1b2 | H1b1, H1i, H1s, C1, P1 | Author census, input, lifecycle migration and halt teardown, harness/foundation amendments |
| H2 | H1b2, IN1 | Pair ledger and gate containment, Spec 05 and gate amendment |
| H3a | C1a | Shared SPU rendering and read-only placement counts, SPU amendment |
| H3b | H1b2, H3a | Marks and paired decode protocol, Spec 06 and SPU amendment |
| H4 | H2, H3b | Plan operations and review evidence, Spec 07 and admin amendment |
| H5 | H3b, H4 | Sort, selection and injection ports, Spec 08 |
| Q1 | All above | Serving qualification and coupled cutover, foundation transition |

Spec 09's required evidence producers travel with the primitive that produces each
measure. Evidence analysis remains in WeaverAnalysis. External diagnostic contracts
are amended through the owning repository's review path.

The first two slices are independent and additive. F2 is the complete empty catalog,
not a serving store or valid seed. F3 joins both branches and supplies the complete
state-content/1 behavior. No partial catalog is a deployment option.

F1 does not replace the legacy lifecycle/configuration grammar. P1a adds a separate
serving parser without activating it. Replacement lifecycle changes travel with
P1d/H1b2, and tool containment vocabulary travels with H2. This keeps wire changes
with their complete producers and consumers. Every affected document's transition
clause remains binding.

## Current evidence

F2 is [merged PR #183][catalog], merge commit 7a8651d8. Its full crate
suite, formatting, lint and lock checks passed. Seven deliberate mutations failed
their protective tests. Independent local review found no unresolved catalog issue.

F1 is [draft PR #184][floor], head 4770909f, aligned with main at 7a8651d8. Its
default and config-feature tests, lint, formatting and lock checks passed. Eleven
guard-removal tests failed as intended. Independent local findings are fixed.
T1a is [draft PR #185][tap], head f76260ed, independently based on bacbad39. Its
tests, lint, formatting and lock checks passed. Eleven guard removals failed their
tests. Review found and fixed a context-bearing projection that could omit block zero.

F3 is [PR #186][core], stacked on #184. It supplies atomic statement,
pair, mark and plan filing, bounded structured reads, and verified seed/image ports.
The catalog prerequisite is on main. The shared floor in #184 remains its open PR
prerequisite. The full state crate suite passed, as did lint, formatting and lock
checks. Thirty-seven deliberate guard removals failed their protective tests.
Its current head is 82a4a436 after the review fixes and floor refresh. The member
channel, paging and capture lifecycle are in F4.

T1b is [PR #187][recorder], stacked on #185. Its recorder, typed payloads,
capacity reservations and writer wake path passed the trace suite and lint checks.
Sixteen deliberate mutations failed their protective tests. Independent review found
no unresolved defect in that scope. It has no dependency on the shared floor crate.
Its current head is e2252766.

The serving recall payload needs an exact source/boot representation from its owning
contract before that payload can be implemented. The operator authorized drafting
that amendment for review, now [draft PR #189][recall]. No serving recall constructor
is supplied before approval.

F4 is [PR #188][member], stacked on #186. It supplies the
state-flow/1 member, immutable paging, capture cleanup and seed process ports.
The full state suite, lint, formatting and lock checks pass. Twenty-five deliberate
guard removals failed their protective tests. Independent findings in continuation
identity, page publication, canonical encoding and observation integrity are fixed.
Copied and held byte counters remain explicitly uninstrumented. The optional build
without SQLite still has its inherited legacy binary compile failure. Its current
head is 9e3dab9c. The operator settled snapshot timing, with the narrow measurement
amendment in [draft PR #192][snapshot]. Sampling code follows that review.

IN1 is [draft PR #190][internal-recall], independently based on main at 7a8651d8.
It supplies the pure recall query with an empty-only refusal and an exact borrowed
subject. Its crate suite, lint, formatting and lock checks pass. Seven perturbations
failed their protective tests, including two type pins. H2 owns the caller and read
integration. No serving change or unused harness dependency is added here.

C1a is [PR #191][spu], stacked on #184. It implements the SPU protocol and
binding mechanics. Its remaining production dependencies are recorded in the
[PR description][spu]. It adds no production checker, manifest locator or
serving activation. C1 in later dependency rows means all four C1 slices, including
qualification.
Host and Olympus cuda,gguf suites, lint, formatting and lock checks pass. Fifty
mechanics perturbations and two cancellation-fixture perturbations failed their
protective tests. The GPU pace instrument reports a skip, so no measured-pace claim
follows from that run. Independent review found no unresolved mechanics defect.
The empty-packet correction is published at 19e6afb9. Fresh host and Olympus gates
pass, with three further compiled guard removals. A live empty datagram is framing
failure, while full or write-half closure remains EOF. The correction changes no
dispatcher states or production qualification claim.

C1t is [draft PR #200][ste100-transport], stacked on #199 at 2b721285. It adds the
concrete harness STE100 socket adapter with exact admission echoes, bounded fragments,
strict replies and one outstanding request. Writable and buffered-work accessors let
the common pump progress it without another clock. The full harness suite passes
540 tests with 17 ignored instruments. All 37 new transport and recording tests run.
Twelve compiled protective changes fail their intended assertions, exact bytes are
restored, and independent review verifies that evidence. Lint, formatting, lock and
diff checks pass. [The implementation record][ste100-transport-record] states the
scope and the separate SPU empty-datagram correction now published in C1a. Worker
launch, the common pump, production artifacts and qualification remain open.

H1r is [draft PR #202][recording-support], stacked on #201. It adds bounded
trace wake/drain and preflight terminal APIs, original author custody, held state
terminal observation, interruption-safe release and typed noncapture control and
classifier operation results. H1c adds the phaseful recording and common pump
components. H1i adds input ownership. Lifecycle caller migration remains in H1b2. Its
[implementation record][recording-support-record] carries the precise boundary and
validation status. Both full affected-crate suites pass, totaling 726 tests with
17 inherited ignored instruments. Fifteen compiled protective changes fail their
intended assertions, and exact restored source passes again. Per-crate lint,
formatting, lock and diff checks pass. No active serving path changes in this slice.

H1c is [draft PR #203][run-context], based on #202. Its
[implementation record][run-context-record] states the scope and evidence. It keeps
one recording lifetime through preflight, activation, correct refusal, release
and drain, and supplies the common pump and the complete frozen account producer.
The full affected-crate suites pass 909 tests with 17 inherited ignored instruments
and zero failures. All 26 compiled protective mutations fail their intended checks,
and independent review verifies exact source restoration and the final passing
suites. All-target lint, formatting, lock and diff checks pass. H1i adds input
ownership. H1s adds the member capture owner. Lifecycle callers, terminal report
submission and failed-exit containment remain in H1b2.

H1i is [draft PR #204][input-transaction], based on #203. Its
[implementation record][input-transaction-record] states the scope and evidence.
It owns original frame custody, trace-first parsing, SPU check routing, revocable
single-use tokens, exact source handoff, cancellation and original gate-wait accounting.
The affected-crate suites pass 959 tests with 17 inherited ignored instruments.
Twenty-two behavioral probes fail their intended assertions and one non-Clone type
pin fails compilation. Sources are restored exactly and independent review checks the
evidence. The existing serving path remains active. H1b2 still owes frame/lifecycle
caller migration and Contribution integration before this port serves.

H1s is [draft PR #205][capture-transaction], stacked on #204. Its
[implementation record][capture-transaction-record]
states the member-side boundary. It freezes admission at the recorded turn boundary,
drains actual filing obligations, and keeps Snapshot, Acknowledge, Abort and
ReleaseCapture on the existing state owner. Missing original results trigger correlated
cleanup. Late results record and discard without renewing the cleanup bound. Final
member completion alone never releases the admission hold. The admin lifecycle and
publisher remain separate consumers to connect in P1d/H1b2. The harness suite passes
750 tests with 17 inherited ignored instruments. All 38 new capture tests run, and
20 protective mutations fail their intended assertions. Independent review verifies
the scope, evidence and exact source restoration. The restored capture suite, lint,
formatting and lock gate pass.

P1a adds strict draft/bound declaration parsing, admin-owned canonical TOML,
strict binding/index/guard codecs and descriptor-held artifact verification. It is
stacked on #184, independently of #188. Its read half returns the checked copy and
read-only image descriptor, refuses unindexed or guarded artifacts and retains N6.
It does not append a restore selection, publish a save, spawn a seed member or
activate the serving path. Publication and restore append are P1b. Seed process,
boot and the existing locks with complete protocol producer/consumer integration
remain P1c and P1d. Offline callers are P1o. P1 in dependency rows means all five
slices.
P1a is [PR #193][artifacts], head 7aa326d9. Its 45 new admin tests pass within
the full admin suite, as do the floor's default and config-feature suites. Fourteen
compiled guard-removal probes failed their protective assertions. Independent
review found and closed an inherited ACL-check omission and an overlapping refusal
in a test fixture. None of this evidence certifies the later publication protocol.

P1b is [PR #194][publication], stacked on #193. It supplies the
descriptor-held publisher and indexed restore selection. The publisher copies a seed
before channel close and requires successful member cleanup before publication.
Serving copies require the matching finished answer and durable guard before
Prepared, matching Commit before rename,
and a matching final outcome before guard removal. Rebind preserves the image and
scheme. Publication advances through rename, read-back and strict append with caller
checkpoints around each step. No filesystem result stands for a worker outcome.
The full admin suite passes 429 tests, with 33 inherited ignored instruments and
all 53 new tests running. Thirteen compiled guard removals failed their protective
checks, then restored exact source bytes. The isolated publication module has no
remaining finding from independent local review.
P1c supplies seed process, lock and cleanup callbacks. P1d supplies serving lifecycle
callbacks, including the common windows and peer observations. P1b adds no active
lifecycle grammar.

P1c is [draft PR #195][seed-driver]. Its dependency join combines
#194 and #188, so both must merge before this branch. It supplies the contained
seed driver and its preparation, transport, cleanup and publication sequence.
Admin verifies certificate attribution and exact bytes without starting a checker.
The separate qualification tool and production artifacts remain cutover gates.
This slice does not activate a seed command, change serving load, or complete P1d.
The full admin suite passes 474 tests, including 45 new ordinary tests, with 36
ignored instruments. All-targets lint, formatting and lock checks pass. The separate
credential-drop watch and delegated-cgroup cleanup instrument pass. Fifteen compiled
guard removals fail their protective tests and restore exact source bytes.
Review found and closed premature member exit, a fresh lock-wait budget, and reseeding
beside artifacts whose index was lost. Seed admission and publication both refuse that
last case without recovering or promoting the orphaned files.
The combined real-member seed fixture requires a root-owned delegated host scope. It
is written but unrun because passwordless root is unavailable here. Its attempted
user-namespace substitute correctly refused unmapped host-root ancestors, so that
substitute was removed. Production host admission, the joined root-host fixture,
qualified certificates and serving boot remain owed before cutover.

P1o is [draft PR #196][offline-controls], stacked on #195. It adds
the complete dormant bind-state and restore operations.
Both authenticate before reading fixed inputs, hold the invocation and run locks,
and require the existing cgroup ladder to report unloaded. Preparation judges the
fixed selector and territory draft without seed inputs or a live checker. Bind
preserves the image and scheme while publishing a new bound declaration. Restore
appends a verified indexed selection without rewriting its files. Append uncertainty
and publication evidence remain in the typed failures. Command activation, stdout and
admin.log adaptation remain part of the coupled cutover.
The full admin suite passes 496 tests, with 36 inherited ignored instruments. All
22 new offline tests run. Fifteen compiled guard removals fail their protective
checks, and the restored source passes all 22 again. All-targets lint, formatting,
lock and diff checks pass. Independent source review has no open finding.
The fixtures use actual filesystem publication and locks with synthetic host
inventory and image bytes. They do not certify SQLite, root/NSS provisioning or
a populated delegated cgroup. Existing containment instruments own that last duty.

H3a is [draft PR #197][placement], stacked on #191. It adds
one complete generation renderer shared by the existing append path and immutable
placement measurement on the existing open session owner. The session supplies
checked actual prefix, resident and capacity counts. The open owner adds the resolved
generation limit and single terminator count. Measurement retains no proposal,
permits oversized counts and preserves malformed-render and component-fault outcomes.
The host suite reports 229 passes. The required Olympus cuda,gguf suite reports
295 passes, with the pace instrument explicitly skipped. Both all-target lint gates,
formatting and lock checks pass. The new real-model tests use Qwen2.5-0.5B through
GGUF and native CUDA and check the actual legacy decode socket as well as the
internal operation. Registry fixtures cover every admitted renderer's bytes. These
are not throughput, deployment-capacity or all-model qualification results.
Fifteen compiled perturbations fail their protective assertions. Six cover rendering,
three session bounds, two preparation failures and four the real GGUF measurement
and append path. Exact source bytes are restored and the corresponding clean
baselines pass. Independent source review has no open finding.
This completes the internal SPU primitive, not its replacement socket protocol.
H3b must change the paired floor, harness and diagnostic consumers together. It also
owns marks and the harness's between-turn grant. The existing serving wire and loop
remain active until the coupled cutover.

H3b has a named open-measurement gap. Spec 06 OPEN-FACTS requires an identity-prefix
label at open, while TYPES-DECODE specifies only four counts in Opened and leaves
Generation unchanged. The existing SPU Spec section 6 emits no open measurement.
No implementation may invent the missing carrier. The existing PromptPartition
offset validator is also not wired into production measurement. Its unit tests
cannot establish the section 6 tokenizer-offset requirement. These remain explicit
work for the SPU slice, not evidence supplied by its binding mechanics.

H1a is [draft PR #198][filing], on code/state-harness-filing. Its branch joins
#188 and #187, so both must merge before this slice. It adds the sole nonblocking state
channel, whole-copy retention accounting, a guarded STE100 near side and trace
completion proofs. These are components for the replacement recording context.
They do not install a parallel writer on the legacy serving path. No measured
capacity or wait value is selected by this slice. The component tests use actual
trace admission and capacity pressure. A separate matching-member fixture uses
fd3, fd4 and fd9 to seed, load, file, query and clean up a capture across Unix
sockets. Its synthetic certificate is not production language qualification.
The full harness suite reports 434 passes and 17 ignored tests. The new normal-member
fixture passes separately. All-targets lint, formatting, lock and diff checks pass.
Fourteen compiled guard removals fail their intended protective assertions, and exact
source bytes are restored before the final gates. Independent review checks the
component contracts, test failures and restoration evidence. Planner review remains
owed.

H1b1 is [draft PR #199][filing-coordinator], stacked on #198. It adds atomic
whole-copy installation, source-ordered filing,
recorded classifier reuse and terminal disposal. Its coordinator composes the actual
recording barrier and state channel. A receipt waits for its measurement record,
and source reservations wait for their terminal record. Transition measurements
retain the gauges, timestamp and cumulative counts observed at the transition.
The numeric clock and absolute-bound registry supply the common pump's timing
primitive. They do not replace the existing six source adapters or choose measured
bounds. Serving author preparation supplies exhaustive kind/source attribution.
It does not complete the caller census or pending-author halt account.
The full harness suite passes 503 tests with 17 ignored instruments. The matching
normal-member fixture passes separately. Lint, formatting, lock and diff checks
pass. Twenty-two initial guard removals and two terminal-reply calibrations fail
their intended assertions. A type-pin check rejects a copyable input grant. Exact
source bytes are restored and all affected baselines pass. A complete component-failure
reply remains held for its exact trace record through the peer close required by the
SPU contract. Accepted and rejected language results still refuse peer loss. Independent
review checks this ordering and the calibration evidence. The fixtures prove the filing
contracts with real
trace admission, Unix sockets and scripted classifier/state replies. They do not
qualify production STE100, deployment limits or throughput.

H1p is [draft PR #201][pump-transports], with the bounded packet owners described in
the [transport implementation record][pump-transport-record]. It is stacked on
PR #200. It consumes existing organ, decode and label channels and leaves their
legacy callers active until the coupled cutover. The shared wire types and decode
segment format stay unchanged. Enter's initial ancillary descriptor transfer remains
part of the paired lifecycle migration.
The full harness suite passes 561 tests with 17 inherited ignored instruments. All
20 new transport fixtures and the new raw-readiness clock test run. Eleven compiled
guard removals fail their intended behavioral checks. Exact source restoration and
the restored 20-test transport and 13-test clock suites are independently verified.
All-target lint, formatting, lock and diff checks pass. These establish bounded
transport progress, including real trace pressure, not the future pump scheduler.

H1c supplies the phaseful recording and pump components and the frozen halt account
producer. H1b2 still owes all serving author callers, lifecycle integration, input
and model repair orchestration, bound activation at the owning operations, and
the complete halt containment and terminal publication sequence. C1 supplies the
production STE100 adapter. Scripted adapters prove their contracts only. These
obligations remain part of the complete implementation and cutover gate.

The later rows remain owed.
This is a progress record, not a claim that the full goal or any serving cutover is
complete. Each draft carries its own fresh test and perturbation evidence.

The operator supplied docs/ASD-STE100_ISSUE9.pdf as the normative language source.
The inspected copy is Issue 9, January 2025, with SHA-256:

```text
d1f4ea9e7cd6e46b47aa9057209f99e78c0e9cfc4e27a5b07895b05c1a166431
```

Locating this reference does not qualify the project dictionary, conformance corpus,
span-binding classifier or system prompt. Their evidence remains required by Spec 02
and the foundation cutover. The PDF is not added by this implementation slice.

## Cutover gate

The complete existing serving implementation remains until one code PR retires it
and activates the qualified replacement. Its body must supply the foundation's four
evidence items: tested revisions and tuple, pinned language and reviewed prompt
artifacts, measured classifier/limit selections, and the checked retirement map.
There is no deployment switch for an incomplete replacement.

Measured values remain open until measured on the intended hardware. Local fixture
tests prove mechanics and refusal paths. They do not certify production language
quality, decoder capacity, throughput or absence of paging waits.

[set]: ../crates/state-management/README.md
[catalog]: https://github.com/toddwbucy/WeaverAgent/pull/183
[floor]: https://github.com/toddwbucy/WeaverAgent/pull/184
[tap]: https://github.com/toddwbucy/WeaverAgent/pull/185
[core]: https://github.com/toddwbucy/WeaverAgent/pull/186
[recorder]: https://github.com/toddwbucy/WeaverAgent/pull/187
[member]: https://github.com/toddwbucy/WeaverAgent/pull/188
[recall]: https://github.com/toddwbucy/WeaverAgent/pull/189
[internal-recall]: https://github.com/toddwbucy/WeaverAgent/pull/190
[spu]: https://github.com/toddwbucy/WeaverAgent/pull/191

[snapshot]: https://github.com/toddwbucy/WeaverAgent/pull/192
[ste100-transport]: https://github.com/toddwbucy/WeaverAgent/pull/200
[ste100-transport-record]: state-management-ste100-transport-2026-10-21.md

[artifacts]: https://github.com/toddwbucy/WeaverAgent/pull/193

[publication]: https://github.com/toddwbucy/WeaverAgent/pull/194

[seed-driver]: https://github.com/toddwbucy/WeaverAgent/pull/195

[offline-controls]: https://github.com/toddwbucy/WeaverAgent/pull/196

[placement]: https://github.com/toddwbucy/WeaverAgent/pull/197
[filing]: https://github.com/toddwbucy/WeaverAgent/pull/198
[filing-coordinator]: https://github.com/toddwbucy/WeaverAgent/pull/199
[pump-transport-record]: state-management-pump-transports-2026-10-21.md
[pump-transports]: https://github.com/toddwbucy/WeaverAgent/pull/201

[recording-support-record]: state-management-recording-support-2026-10-21.md
[recording-support]: https://github.com/toddwbucy/WeaverAgent/pull/202
[run-context-record]: state-management-run-context-2026-10-21.md
[run-context]: https://github.com/toddwbucy/WeaverAgent/pull/203
[input-transaction-record]: state-management-input-transaction-2026-10-21.md
[capture-transaction-record]: state-management-capture-transaction-2026-10-21.md
[capture-transaction]: https://github.com/toddwbucy/WeaverAgent/pull/205

[input-transaction]: https://github.com/toddwbucy/WeaverAgent/pull/204
