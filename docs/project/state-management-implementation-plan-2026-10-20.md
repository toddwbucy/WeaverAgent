# State management implementation map

Date: 2026-10-20. Status: implementation tracking, not a Spec amendment.

The authority is the merged [state management set][set] from PR #182. All code PRs
start as drafts. Gates and Planner verification precede third-party review.
The operator merges them.
The crate remains weaver-state. The loop remains in worker/dev_loop and calls
primitives. The SPU owns classification.

## Consolidated review stack

The foundation, trace and additive harness roots reached main on 2026-10-21.
The remaining descendants are consolidated in #215 for review against main.
Runtime activation and deployment remain held for the coupled cutover. This table
is the current landing map and takes precedence over the
historical branch descriptions below. Original slice results do not validate a
combined head.

| Root or remaining branch | Contents and next step |
| --- | --- |
| #184 | Shared floor, store/member via #212, admin #193 through #196, and SPU #191 with #197. Merged to main at b18e7d34. |
| #185 | Trace copy and recorder, including #187. Merged to main at ba17720d. |
| #198 | Harness #199 through #210, model repair #214 and subsequent review fixes. Merged to main at 50a00944. |
| #215 | Consolidation root for typed tool causes, external pair execution, paged reads, paired decode measurements and outbound identity sort. The operator authorized main consolidation. The full checks at 2ef9657 are historical. Current continuation, shared-author and held-gate corrections pass 1,167 harness tests and 13 protective mutations. Fresh review remains owed. |
| #216, #217, #219, #220 | Merged down into #215, preserving the original commits and merge ancestry. Their earlier slice evidence does not grade the consolidated root. |
| #218 | Closed as already incorporated. Its exact head 8b387eff entered #219 through merge 32a57b57 and is now an ancestor of #215. No separate content merge is owed. |
| #190 | Independent pure recall query primitive, held until the foundation cutover by the [operator's ruling][recall-hold]. It does not land ahead of that cutover. |
| #189, #192, #213 | Separate Spec proposals. Approval and merge remain owed, so they are not implementation authority. |

The operator merged #220 into #219 at 391c1a0, #219 into #217 at 8dded99,
#217 into #216 at bb63859, and #216 into #215 at fe4d193b. Merge 32a57b57
preserves #217 at bdfa3a76 and #218 at 8b387eff as its two parents. Keep that
ancestry when integrating main into #215. Closed child branches are no longer
landing targets.

The consolidated fixes address the multi-call finding in the
[#216 review][pair-execution-review] and the #217 followups in the
[posted review][consolidation-review]. The execution context opens the next effective
pair only after the preceding terminal receipt. Each call has a send receipt window
and a separate terminal receipt window. Successful send retires the former, tool
execution has no harness decision clock, and the matching answer starts the latter.
The next call gets a fresh send window after the preceding terminal commits. Partial
progress never renews a window. Retry lineage can name a filed terminal external pair
still in the active batch when its run and exact request match.

The read adapter checks refusal codes against Read, Next and Release separately.
Next can return result_unavailable or answer_too_large while preserving its acquired
cursor for one release under the original deadline. The external begin_read entry
refuses pending pair, input and author ownership, including gaps between their legs,
and an armed Gate or Cut barrier even without a pending channel operation.
An inward pair will need its own explicit read entry. No inward caller is wired here.

The [consolidation record][consolidation-record] states the exact integration parents,
Spec answers, regression coverage and batch/file dispositions. At 2ef9657, combined
checks passed 1,157 harness tests with 18 ignored instruments, the other crate and
device gates, and 22 protective mutations with exact restoration. Those results and
their source manifest do not validate the current review corrections.

The new round retains accepted read progress, records measurement-author successors
before yielding, and checks the clock before consuming a measurement reservation.
Shared author admission retains a valid original across an initial lifecycle wait
and repeats its checks before submission. The pump no longer reports an already held
gate reply as new progress. Current author validation passes 1,167 harness tests
with 18 ignored instruments and all 13 compiled protective mutations, plus lint,
formatting, lock and diff checks. The current source manifest and evidence are in
/tmp/weaver-pr215-review-round2. Fresh review remains owed. The class sweep confirms
template and outbound continuations already retain their state. The inherited capture
refusal cleanup gap is filed separately as [#223][capture-followup], and gate findings
1 and 2 are tracked in [#222][gate-followup]. The six slice records now separate their
historical inputs from current #215-to-main landing guidance.

The operator authorized consolidating #215 onto main. Runtime activation and
deployment remain held until the coordinated foundation cutover. The paired wire
and typed interrupt must deploy with their complete consumers. The old serving
path remains active until the qualified cutover. Only the operator merges. The
full implementation and epic #146 remain open.

The contained seed fixture that launches the real member under root-owned host
containment remains an open instrument carried by #184. The reviewed scripted seed
fixtures do not replace it. Production language artifacts and measured limits also
remain unqualified. The foundation landing does not remove these cutover obligations.

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
| H1t | H1s | Recorded template transcript and private certificate, Spec 02 and harness amendment |
| H1f | H1t | Exact registered source disposition and committed filing custody, harness amendment |
| H1o | H1f | Complete recorded model-output checks, Spec 02 OUTPUT-UNIT |
| H1q | H1o | Per-generation source receipt leases and original request association, Specs 02 and 05 |
| H2a | H1q | Committed opening and terminal accounting for undispatched requests, Spec 05 |
| H1m | H2a, H1t | Passive model repair, recorded restatement instructions and corrected request custody, Specs 02 and 05 |
| H1d | H1c, F4 | Complete bounded paged read acquisition and structural result checks, HARNESS-READ and STATE-PAGES |
| H1b2 | H1b1, H1i, H1s, H1t, H1f, H1o, H1q, H2a, H1m, H1d, C1, P1 | Author census, input, lifecycle migration and halt teardown, harness/foundation amendments |
| H2g | H1m | Typed gate interrupt, containment and terminal delivery, gate amendment |
| H2e | H1m, H2g | External dispatch, exact gate evidence and terminal commit custody, Spec 05 |
| H2 | H1b2, H2e, IN1 | Inward calls, boot repair, lifecycle composition and returned coverage, Spec 05 |
| H3a | C1a | Shared SPU rendering and read-only placement counts, SPU amendment |
| H3b1 | F1, H3a | Paired decode measurement wire and retiring callers, TYPES-DECODE and SPU amendment |
| H3bm | H1d, H3b1 | Reserved measurement exchange, exact proposal/count custody and existing waits, HARNESS-PLACE |
| H3b | H1b2, H3bm | Marks, journal, recorded placement and engine grants, Spec 06 |
| H4 | H2, H3b | Plan operations and review evidence, Spec 07 and admin amendment |
| H5o | H1m, H1f | SUBJECTS, outbound identity sort and recorded-use adapter, Spec 08 |
| H5 | H3b, H4, H5o | Serve-side selection and injection ports, Spec 08 |
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

## Slice evidence before consolidation

The following counts, grades and branch descriptions record each original slice.
They do not certify the current #215 integration or replace its pending review.

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
head is 9e3dab9c. The narrow snapshot timing amendment remains a proposal in
[draft PR #192][snapshot]. Its ruling and merge are still owed before the matching
sampling change becomes implementation authority.

IN1 is [draft PR #190][internal-recall], independently based on main at 7a8651d8.
It supplies the pure recall query with an empty-only refusal and an exact borrowed
subject. Its crate suite, lint, formatting and lock checks pass. Seven perturbations
failed their protective tests, including two type pins. H2 owns the caller and read
integration. No serving change or unused harness dependency is added here.

C1a is [PR #191][spu], stacked on #184. It implements the SPU protocol and
binding mechanics. The [mechanics record][spu-mechanics] and
[PR description][spu] name its evidence and remaining production dependencies.
It adds no production checker, manifest locator or serving activation. C1 in later
dependency rows means C1a, C1b and C1c, including qualification. C1t is the separately
named harness transport.
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

H1t is [draft PR #206][template-certification], stacked on #205. Its
[implementation record][template-certification-record] covers exact compiled inventory
binding, ordered recorded replies, the private certificate and the original Enter
bound. Correct rejection permits rollback and never grants content. The full harness
suite passes 773 tests with 17 inherited ignored instruments. All 23 added tests run.
Thirteen compiled protective mutations fail their intended assertions, including a
valid capture interleaving attempt. Exact source restoration and the component scope
are independently reviewed. Production grammar, the complete compiled inventory,
runtime expansion and paired boot/forced rollback remain owed in C1b/C1c and H1b2.

H1f is draft [PR #207][filing-receipts], after #206. Its
[implementation record][filing-receipts-record] covers one
registered source transaction through the recorder, classifier, actual member
receipt and terminal recording. It preserves committed sources separately from
correct rejection. It supplies receipt evidence to future pair, mark and selection
owners without granting their operations. The restored harness suite passes 795
tests with zero failures and 17 inherited ignored instruments. All 22 added tests
run. Ten compiled protective mutations fail their intended assertions, and
independent review verifies source restoration. All-target clippy, formatting, lock,
editorial and link checks pass. The Planner grade remains owed.

H1o is draft [PR #208][output-checks], after #207. Its
[implementation record][output-checks-record] covers the
complete assistant message and every original Text index. The check frontier is
independent of ordinary statement filing. Correct rejection is retained for the
repair owner and grants no successful answer. Repair and pair opening still owe
per-request receipt custody without an unrelated sibling filing barrier. The
restored harness suite passes 822 tests with zero failures and 17 inherited ignored
instruments. All 27 added tests run. Twelve compiled protective probes fail their
intended assertions, with exact restoration independently checked. Lint, formatting,
lock and documentation checks pass. The Planner grade remains owed.

H1q is draft [PR #209][request-accounting], after #208. Its
[implementation record][request-accounting-record] closes
per-request source custody without an ordinary sibling filing barrier and derives
original request identities from the complete recorded generation. Pair opening,
repair and terminal accounting remain owed. Malformed recall still needs a Spec
amendment defining its typed refusal. No runtime policy is inferred from that gap.
The restored harness suite passes 871 tests with zero failures and 17 inherited
ignored instruments. All 49 added tests run. Thirteen compiled protective probes
fail their intended assertions, with exact restoration independently checked. Lint,
formatting, lock and documentation checks pass. The Planner grade remains owed.

H2a is draft [PR #210][pair-accounting], after #209. Its
[implementation record][pair-accounting-record] covers
committed openings and not_dispatched terminals for every accepted original request
that the caller bars from execution. Private receipt custody constructs the retained
completion and harness notice. Stop and unload preserve owed filings, and frozen
pair facts enter the halt account. External execution, repaired sends, inward reads,
review authorization, retry lineage, boot repair and placement acknowledgement remain
owed. The restored harness suite passes 902 tests with zero failures and 17 inherited
ignored instruments. All 31 added tests run, and twelve compiled protective probes
fail their intended assertions with exact source restoration. Lint, formatting,
lock, editorial and link checks pass. The Planner grade remains owed.

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
H3b1 changes the paired floor, SPU and retiring harness/diagnostic callers together.
H3b retains marks, measured proposal ownership and the between-turn grant. The
existing serving loop remains active until the coupled cutover. The replacement
decode vocabulary must not deploy separately from its complete consumers.

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
and repair invocation, bound activation at the owning operations, and
the complete halt containment and terminal publication sequence. C1 supplies the
production STE100 adapter. Scripted adapters prove their contracts only. These
obligations remain part of the complete implementation and cutover gate.

H1m is [merged PR #214][repair], originally folded into #198 at a1ce2df5.
It reached main with #198. The following evidence records the original slice.
Its [implementation record][repair-record] states the passive repair owner and
its recorded instruction and pair-accounting ports. The loop still chooses each pass
and calls the decoder. The full harness suite passes 952 tests with 17 inherited
ignored instruments. Ten compiled protective changes fail their intended assertions.
The restored suites, lint, formatting and lock gate pass. Independent review verifies
the repaired custody paths and mutation evidence. Empty corrections and malformed
recall remain explicit no-grant gaps until their owning Spec changes.
The empty-correction proposal is [draft PR #213][empty-correction]. It is not
implementation authority before approval and merge.

H2g was published in [PR #215][gate-interrupt] on code/state-gate-interrupt, originally
based on #198 at a1ce2df5. Its [implementation record][gate-interrupt-record] names the
typed interrupt, containment, late-control and terminal-delivery contracts. Every old
sender supplies Unload. The failed-flow caller and trace-only pair disposition remain in
H1b2/H2. This slice changes the internal wire. Its current consolidation target is
main under the operator's authorization. Deployment still requires the coordinated
foundation cutover and the complete replacement consumers.

H2e was published in [PR #216][pair-execution] on code/state-pair-execution, originally
based on #215 at c9aa7709. Its [implementation record][pair-execution-record] names the
private effective-emission, send-ticket, receipt and rejection contracts. The restored
harness suite passes 1,010 tests with 17 inherited ignored instruments. Twelve compiled
protective mutations fail their named tests, with exact source restoration independently
checked. Lint, formatting, lock, diff and editorial checks pass. The local review is not
a Planner grade. The loop still owes actual controls, lower/reap routing and serving
migration. Inward recall, trusted review, boot repair and placement acknowledgment
remain H2 work. The original branch dependencies are now consolidated in #215. The
current landing map and consolidation record describe the later multi-call and retry
corrections, whose author checks passed at 2ef9657. Activating and deploying the
replacement still waits for the coordinated cutover.

H1d was published in [PR #217][paged-read] on code/state-paged-read, originally based on
#216 at 8f5aa872. Its [implementation record][paged-read-record] covers bounded page
assembly, exact selector/result checks, recorded operation custody and one snapshot
release. The restored harness suite passes 1,055 tests with 18 ignored instruments. The
new real-member fixture was also run explicitly and passed. Thirteen compiled protective
mutations fail their named tests, with exact restoration verified. Lint, formatting,
lock and diff checks pass. This is a local author assessment, not a Planner grade.
The consolidated root adds per-leg refusal checks and external read admission guards.
Their new fixtures and Spec answers are in the consolidation record. The original
slice evidence above does not validate those later changes.

The result grants no recall or placement authority. Draft #189 must settle the recall
carrier before public recall, held-material and boot adapters complete. The later H3b1
and H3bm slices below build on #197's internal SPU measurement primitive. Journal
construction against read/material proofs remains owed. The OPEN-FACTS carrier gap above
remains open. Outbound identity sort and noun extraction can proceed independently of
serve-side selection and plan orchestration. No production language artifact is inferred
from these fixtures.

H3b1 was published as #218 on code/state-decode-measurement, originally based on #184 at
6d5e6b0d. It carries the exact required Opened body, MeasureDelta and DeltaMeasured on
the existing segmented socket. Append takes explicit ordered schema strings through
SPU-owned rendering. Retiring harness callers send an empty schema list, preserving
their model input, and structurally consume the required Opened body. They do not
validate placement budgets or acquire a measured proposal. The implementation and
validation scope are in the [paired decode record][decode-record].

Its original review baseline was #184. The exact producer/consumer change joined the
harness descendant of #217 at 32a57b57 and is now incorporated in #215. The [owned
measurement record][measure-owner-record] tracks that consumer. Cached boot facts,
budgets and recorded placement remain owed. The wire is not independently deployable.
External diagnostic consumers must migrate the same vocabulary before the coupled
cutover.

H3bm is [PR #219][measure-owner] on code/state-measure-owner, joining #217 at
bdfa3a76 with #218 at 8b387eff.
Its reserved decode exchange retains exact proposals and raw replies, checks the
counts and revision, and keeps the enclosing Decision or Cut wait. Cancellation
drains the exchange. Protocol failure latches before its owed trace-only refusal
reports. It returns count evidence, not a PlacementGrant. The engine still owes
idle-state certification for prior nonmeasurement exchanges.

The restored combined harness/types suites pass 1,228 tests, with 18 inherited
ignored instruments. Types with config passes 182 tests. Fifteen compiled protective
mutations fail their named tests. Both changed-crate lint gates, formatting and lock
checks pass. The [owned measurement record][measure-owner-record] holds the scope,
test list and evidence limits. The original union requires both #217 and #218 as
review prerequisites. It is now consolidated in #215, with runtime activation and
deployment still held until the coupled cutover.

H5o was published in [PR #220][outbound-sort] on code/state-outbound-sort, originally
based on #219 at 7da5366c. The [outbound implementation record][outbound-record] names
its pure noun extraction, checked act identities, certified template family and
exclusive recording adapter. It retains the effective emission through trace pressure
and returns no sorted result after cancellation or runtime template rejection. It opens
no pair and elects no workflow. Serve-side selection and its read/place dependencies
remain H5.

The later rows remain owed.
This is a progress record, not a claim that the full goal or any serving cutover is
complete. Each original slice carries its own test and perturbation evidence.

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
[measure-owner]: https://github.com/toddwbucy/WeaverAgent/pull/219
[outbound-sort]: https://github.com/toddwbucy/WeaverAgent/pull/220
[outbound-record]: state-management-outbound-sort-2026-10-21.md
[measure-owner-record]: state-management-measure-owner-2026-10-21.md
[consolidation-record]: state-management-consolidation-2026-10-21.md
[consolidation-review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5607348857
[gate-followup]: https://github.com/toddwbucy/WeaverAgent/issues/222
[capture-followup]: https://github.com/toddwbucy/WeaverAgent/issues/223
[catalog]: https://github.com/toddwbucy/WeaverAgent/pull/183
[floor]: https://github.com/toddwbucy/WeaverAgent/pull/184
[tap]: https://github.com/toddwbucy/WeaverAgent/pull/185
[core]: https://github.com/toddwbucy/WeaverAgent/pull/186
[recorder]: https://github.com/toddwbucy/WeaverAgent/pull/187
[member]: https://github.com/toddwbucy/WeaverAgent/pull/188
[recall]: https://github.com/toddwbucy/WeaverAgent/pull/189
[internal-recall]: https://github.com/toddwbucy/WeaverAgent/pull/190
[recall-hold]: https://github.com/toddwbucy/WeaverAgent/pull/190#issuecomment-6325275500
[spu]: https://github.com/toddwbucy/WeaverAgent/pull/191
[spu-mechanics]: state-management-spu-mechanics-2026-10-20.md

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
[pump-transport-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-pump-transports-2026-10-21.md
[pump-transports]: https://github.com/toddwbucy/WeaverAgent/pull/201

[recording-support-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-recording-support-2026-10-21.md
[recording-support]: https://github.com/toddwbucy/WeaverAgent/pull/202
[run-context-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-run-context-2026-10-21.md
[run-context]: https://github.com/toddwbucy/WeaverAgent/pull/203
[input-transaction-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-input-transaction-2026-10-21.md
[capture-transaction-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-capture-transaction-2026-10-21.md
[capture-transaction]: https://github.com/toddwbucy/WeaverAgent/pull/205

[input-transaction]: https://github.com/toddwbucy/WeaverAgent/pull/204
[template-certification-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-template-certification-2026-10-21.md
[template-certification]: https://github.com/toddwbucy/WeaverAgent/pull/206
[filing-receipts-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-filing-receipts-2026-10-21.md
[filing-receipts]: https://github.com/toddwbucy/WeaverAgent/pull/207
[output-checks-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-output-checks-2026-10-21.md
[output-checks]: https://github.com/toddwbucy/WeaverAgent/pull/208

[request-accounting-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-request-accounting-2026-10-21.md
[request-accounting]: https://github.com/toddwbucy/WeaverAgent/pull/209

[pair-accounting-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-pair-accounting-2026-10-21.md
[pair-accounting]: https://github.com/toddwbucy/WeaverAgent/pull/210

[empty-correction]: https://github.com/toddwbucy/WeaverAgent/pull/213

[repair-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-repair-2026-10-21.md

[repair]: https://github.com/toddwbucy/WeaverAgent/pull/214

[gate-interrupt-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-gate-interrupt-2026-10-21.md

[gate-interrupt]: https://github.com/toddwbucy/WeaverAgent/pull/215

[pair-execution]: https://github.com/toddwbucy/WeaverAgent/pull/216
[pair-execution-review]: https://github.com/toddwbucy/WeaverAgent/pull/216#pullrequestreview-5604724227
[pair-execution-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-pair-execution-2026-10-21.md

[paged-read]: https://github.com/toddwbucy/WeaverAgent/pull/217
[paged-read-record]: https://github.com/toddwbucy/WeaverAgent/blob/bdfa3a7685aee0b3172e1b2b5e22ff016d4967c5/docs/project/state-management-paged-read-2026-10-21.md

[decode-record]: state-management-decode-measurement-2026-10-21.md
