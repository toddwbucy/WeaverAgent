# State management foundation - Spec

**Status:** MERGED.

**Landing PR:** #244

**Version:** 0.19 **Date filed:** 2026-10-20 **Document ID:**
`state-management-foundation-Spec` **Parent:** state-management-primitives-PRD

## 0. What this document is

This document combines the flow, store/read and seed/boot specifications. It serves
CAP-1 through CAP-5, FLOW-1 through FLOW-8, TAP-1 through TAP-5, SEAM-1 through SEAM-3,
STORE-1 through STORE-8, PERSIST-1 through PERSIST-7, READ-1, READ-4, READ-5 and the
boot duties of PROMPT-1 through PROMPT-3. The remaining Specs supply classification,
pairs, general marks, plans, placement, strategies and measurements. The governing
source is the [PRD](state-management-primitives-prd-v0_6.md),
with the operator's subsequent rulings stated below. A review record is evidence for
drafting, not a dependency of an implementable primitive.

The baseline is main at 2eb5c5a2e47f89309631327ff121393d393be85c, read from local main
on 2026-10-20. Current code is inventory evidence, not a claim that this replacement is
implemented. All names remain `weaver-state`.

The owning amendments named in section 6 are normative parts of this foundation.
Sections 2 through 4 below define one cross-crate walk. Section 5 owns the persistence
representation and actor tables. Section 6 maps each owning document to its replacement.
They are revised together, never independent implementations. No source draft or test
double grants production qualification.

### Rulings and the primitive boundary

**FOUNDATION-SCOPE.** The deliverable is the state-management primitives, each with a
typed port, its invariant and its enforcement. The loop remains compiled in
crates/weaver-harness/src/bin/worker/dev_loop/ until the pre-release reorganization. It
owns orchestration. This set supplies no required pass order, strategy schedule, review
script or context-selection workflow. A component's protocol ordering remains binding,
including commit before receipt, checked provenance before placement and filing before a
destructive cut. These protect the component regardless of its caller. Reason: the
primitive performs a mechanic, and its consumer holds the purpose. [CAP-1, CAP-5, SEAM-1
through SEAM-3, operator 2026-10-20 rulings 12 and 13]

The following are the operator's rulings of 2026-10-20. They supersede earlier wording
where it conflicts. The requirement IDs remain stable. An ID that describes
orchestration identifies a consumer obligation, not a workflow that this primitives
deliverable implements.

R20-8 corrects document-status vocabulary and adds no primitive requirement, so it has
no row in the tables below.

| Ruling | Binding disposition |
| --- | --- |
| R20-1 | Rust loop text enters through a certified template and its own trace event. The compiled system prompt and recap builders retire. Boot reads the prompt from state. |
| R20-2 | Measured backlog limits cause intake to wait and mark lag. Only expiry of the existing model-blocking bound turns that wait into queue_limit failure. Copies are never truncated. |
| R20-3 | Semantic models for state management run as SPU processes from weaver-spu, each behind its typed Unix socket. state-ste100/1 is the classifier port. The loop chooses when to call it. |
| R20-4 | The TASKSTATS observer belongs to WeaverAnalysis. Its issue 14 owns the root collector amendment and the retained no-socket-crate pin. Kernel CPU/disk sampling is snapshot-only. Cheap operation timestamps and byte counters remain under R20-19. |
| R20-5 | This repository's analysis-state contract is authoritative. WeaverAnalysis mirrors it until the suite repository takes ownership. |
| R20-6 | Missing snapshot or finished results remain timeout/retry outcomes. The initial 120-second allowance is a declared, loaded setting, measured and adjustable. It is not a snapshot-missing halt. |
| R20-7 | Worker failure after valid Commit breaks the entire save point. Publish nothing, keep committed artifacts where they are and report them for manual repair. |
| R20-9 | Flush and Elide both remain available primitives. Neither is elected over the other. Qualify both when wiring a loop. |
| R20-10 | A repeated step instruction at a step prefix is a typed reference returned by a query about the previous step, not a whole prior-work dump. |
| R20-11 | The root-only plan-update verb is approved. |
| R20-12 | Orchestration is removed from these Specs. Local loop scratch work is outside the deliverable. The primitive and its invariant remain. |
| R20-13 | The set supplies the complete primitives needed to manage state, beginning with the store. |
| R20-14 | Only final PRD, Specs, amendments and the STE100 contract enter the document PR. Review records, indexes, handoffs, scratch work and superseded drafts stay local. |
| R20-15 | Cut-and-reload is a fixed composite primitive in the harness engine. Spec 06 retains its protocol and classifier dependency. R20-16 supplies staging and the next-append confirmation. |

The same evening's dispositions add the following bindings. [operator 2026-10-20]

| Ruling | Binding disposition |
| --- | --- |
| R20-16 | The composite ends Staged. Its tail is the mandatory head of the next AppendAndGenerate, confirmed from Generated.resident. No prefill-only serving exchange is added. |
| R20-17 | Each operator Category declares whether closure requires a review pair. Typed ClosureEvidence enforces that declaration. |
| R20-18 | Wind-down pass count and summary policy belong to the loop. The declared bound, no restart and existing containment remain. |
| R20-19 | Store observations are retained as operation metadata, never classified content. The harness carries them to the trace. Only an explicit typed query returns them as query data. Kernel sampling remains snapshot-only. |
| R20-20 | PERSIST-PUBLICATION-GUARD's pending file and load refusal are approved for this representation, subject to a later amendment if needed. |

The October 18 rulings still bind where not superseded: admin owns hashing and
publication and loads the bound declaration (R1 and R2), broken flow takes no save point
(R3), pair fates are unclassified and loaded open pairs close through the normal feed
(R4), plan updates are root-only (R5), plan categories are the operator's (the surviving
invariant of R6), cuts occur between external turns (R7), preload is diagnostic-only
(R8), analysis is external (R9), the label scorer remains alongside STE100 (R10), and a
cut preserves everything before N (R11). The October 19 rename deferral keeps
weaver-state throughout.

READ-2's automatic retrieval timing, PLAN-4's progression and STRAT-1/STRAT-2's strategy
scheduling belong to the eventual loop under R20-12. Their query, condition-selection,
placement and cut primitives remain specified here and in Specs 06 through 08. The PRD
remains unchanged in this pass. This allocation neither claims the deferred wiring
exists nor weakens a primitive's invariant.

## 1. Applicability and the serving cutover

**FOUNDATION-TRANSITION.** The previous serving contracts and the complete existing
serving implementation remain applicable until the replacement passes the cutover review
below. Merging this document does not remove the implementation or activate any
replacement path. No declaration switch selects a partly implemented flow. Diagnostic
mode remains a separate binding under R8, not a fallback for serving. Reason: FLOW-7
requires a dependable feed before its consumers depend on it, and PERSIST-5 requires a
usable seed before the live seeding path retires. [FLOW-7, PERSIST-5, RELEASE-1]

Every amended section in a crate Spec, charter or contract carries this sentence, with a
working link to this section:

> This replacement applies at the state-management foundation cutover. Until then,
> this section's previous serving contract remains applicable under
> FOUNDATION-TRANSITION.

The code cutover is one reviewed code pull request for the coupled flow, store/read and
seed/boot replacement. It may clear review only when its body names all four evidence
groups below. This is a review instrument, not a declaration value or a claim that a
documentation test can qualify a running model. [FLOW-7, PERSIST-5, CLASS-7, PROMPT-2,
RELEASE-1]

1. Qualification results tied to the tested revision and deployment tuple, exercising
   seed, boot, input conformance, model repair, filing, reads, saves and terminal
   failures through the replacement contracts.
2. The pinned STE100 edition, verb dictionary and conformance corpus, plus the
   separately reviewed system prompt and the exact template certificates.
3. Measured classifier selection and all measured limits required by the path, including
   gate, cut, load and snapshot qualification. A scripted double proves protocol
   behavior only.
4. The retirement and reader map below checked against every serving consumer, including
   the compiled Rust loop and deploy tooling.

**FOUNDATION-RETIRE.** Retire each old serving path in the same cutover. Do not keep the
#76 seeding turn alone after removing its writer, schema or identity reader. Reason: a
serving reader cannot depend on tables that no serving writer maintains. The
no-empty-store boot and one-feed requirements apply together. [FLOW-1, FLOW-7, STORE-8,
PERSIST-5, PERSIST-6]

| Old surface | Cutover disposition |
| --- | --- |
| trace tee distill/ingest frame and owned socket | TRACE-COPY yields AdmittedCopy to the one harness owner |
| event/field serving writer and Store::land | STATE-FILE writes the fixed serving schema with a receipt after commit |
| identity_of and identity ask | Read the seeded system_prompt span through its end mark |
| recall, Recalled.pairs and implicit boot-tail recall | Typed ReadEntry, bounded conversation query for explicit consumers, no boot-tail replay |
| shape | Aggregate content_block by source run and kind |
| grants | Retain the member-room surface read at enter and leave |
| save-point position | Read the latest committed event SourcePosition from the serving schema |
| schema, index_election, retire_and_index | Fixed serving catalog, legacy methods remain diagnostic only |
| #76 seeding turn and its acknowledgement | Root-only pre-agent seed, followed by the ordinary verified load |
| empty first load and state-store.engine none | Refuse serving load, every serving agent has a seeded store |
| dev_loop SYSTEM_PROMPT and copied prompt in re-entry | Remove, only the seeded boot read supplies the argument head |
| old one-file save-point serving adoption and room recovery | Three-file verified artifacts, no recovery publication after a broken flow |
| deploy --system and load-before-seed instructions | Seed before the first load, with the same fixed admin verbs and custody |

The deploy files reached are `turn.py`, `create-agent.sh`, `verify-lifecycle.sh`,
`test_plans.py`, `update-stack.sh`, `HowToDeployANewAgent.md` and `REDEPLOY.md`, under
`deploy/` where they exist. The owning-document register supplies exact current paths
before landing. This documentation pass changes no deploy implementation.

## 2. Duties, interfaces and refusal boundaries

**FOUNDATION-OWNERS.** The trace records one completed event and leaves it whole. The
tee takes its copy and applies only the validated envelope declaration. The classifier
labels a selected statement or reports its correct rejection. The harness carries a
valid statement or complete pair item on the store's one serving channel. The member
commits its content and index atomically, then returns its source-matched receipt. Reads
use that same owner and return held content without filing it again. Reason: the fixed
verbs and single-feed rule need one accountable owner at each handoff. [FLOW-1, FLOW-2,
TAP-1, TAP-4, SEAM-1, SEAM-2, STORE-8]

The seeded system prompt is the sole pre-agent insertion. At load it is read into the
head once, before the user channel opens. The trace head records the deployment and
start identity and never passes the tap. Boot recall records the seated prompt for
replay and is refused by partition validation. Every later content insertion comes from
a selected trace copy, including pair repair and operator plan updates. The store
separately records its own operation metadata under STATE-OBSERVE and R20-19. That
bookkeeping is never classified content. [PERSIST-5, PERSIST-6, PROMPT-1, FLOW-1,
PLAN-6]

| Boundary and owning clause | Sender may assume after success | Receiver must refuse or report |
| --- | --- | --- |
| Admin to member/harness, ADMIN-LOAD and TYPES-ENTER | Root-selected image and declaration identify the same indexed artifact | Bad custody, A/B mismatch, unsupported schema or scheme, inconsistent start position |
| Harness to trace, TRACE-SUBMIT and HARNESS-AWAIT-AUTHOR | Recorded has one sequence and a consumed tap outcome | Bad envelope or sink failure, distinguish CapacityPending from admission |
| Trace to harness, TRACE-COPY and HARNESS-RECORD | Immutable StatementCopy or PairCopy with original source identity | Projection failure distinct from Unselected, never silently skip a selected item |
| Harness to STE100, HARNESS-CLASSIFY and Spec 02 PORT | Matching accepted, correct defect or explicit component result | Wrong request/target/scheme, malformed result or lost component |
| Harness to member, TYPES-WIRE and STATE-FILE/PAIR-FILE | filed names a committed source, not merely carried bytes | Invalid binding/reference, duplicate source or pair transition, wrong run or scheme |
| Member to harness reader, TYPES-READ and STATE-PAGES | Complete bounded tagged result from one immutable snapshot | Invalid selector, wrong cursor, over-limit result or unavailable read, never empty-success substitution |
| Harness/admin/member persistence, section 5 | Published identity only after the defined confirmation and index act | Wrong phase/capture/hash, unconfirmed room image or changed loaded declaration |

**FOUNDATION-FAILURE.** Failed copying, failed carry, refused filing, malformed
component results and component loss revoke decisions before reporting. Pending
classification is not a conformance failure. Correct non-STE100 model output enters
repair, and a repair cap closes that turn without halting the agent. Gate and cut waits,
and the result/receipt barrier elected in Spec 02, have finite declared bounds. The pump
observes every armed deadline while authoring or I/O is pending. The gate alone chooses
a tool execution timeout. Reason: every model-blocking wait needs an end, while a
correct defect must remain a repairable outcome. [FLOW-4, FLOW-5, CLASS-8, CLASS-9,
GATE-1, PAIR-2]

A halt stops the tap and all decisions, records the owed sources and uncertain commits
where the trace works, contains tools and performs bounded teardown. It sends no new
filing and takes no dump. It cannot return successful Left. Admin's existing Open-marker
recovery remains visible. R20-6 preserves snapshot timeout/retry. R20-7 makes an
incomplete save broken after post-Commit worker failure. Neither permits a halt dump.
[FLOW-4, FLOW-5, October 18 R3]

## 3. One serving state table

**FOUNDATION-WALK.** This table joins load, file, read and save. It is the shared walk,
while the named owner tables define each subprotocol's events and wire representations.
Channel state and immutable read-cursor state are separate. A held cursor does not lock
out filing, because its complete result is already frozen. A capture does lock out all
ordinary flow until it is released. Reason: flow, store and boot cannot each assume a
different moment of readiness or acceptance. [FLOW-7, SEAM-2, SEAM-3, PERSIST-4,
PERSIST-6]

| Standing | Trigger | Admin | Harness and trace | State member and next standing |
| --- | --- | --- | --- | --- |
| No serving run | Root seed on a new chain | Validate fixed inputs, stand seed member | No harness or tee exists | Create schema and prompt span, return image, publish under PERSIST-SEED |
| Unloaded | Load indexed start | Verify custody, A/B, scheme bytes and loaded copy | No user decisions | Scratch-adopt image, AwaitOpen |
| AwaitOpen | Matching serving open | Await enter within existing load budget | Receive opened and verify exact start/scheme/read limits | Ready, no content mutation |
| Preflight | State open accepted | Await enter | Record untapped load, validate partition, activate one tee | Ready |
| Certifying | Partition accepted | Await enter | Start STE100, check every template and record each reply | Ready, no prompt write |
| Seating | Templates accepted | Await enter | Read prompt once, record boot recall, cache and open decoder with it first | Return the seeded span, no filing |
| RepairingPairs | Decoder head seated | Await enter | Run BOOT-REPAIR through ordinary pair records and receipts, then empty recheck | Close only selected current-run repair events, Ready |
| ReadingPlan | Pair repair receipts settled | Await enter | Read plan_current and cache committed PlanSummary | No filing, Ready |
| Serving | All enter duties complete, including empty-pair recheck and committed plan summary | Report loaded | Open user channel, accept the next frame | Ready |
| Serving | New completed event | No action | Record once, consume selected copy or unselected outcome | No write until a request arrives |
| Serving | Selected statement | No action | Classify/reuse exact result, record disposition, queue accepted filing in source order | Ready |
| Serving | Selected pair item | No action | Validate typed metadata, queue file_pair in the same source order | Ready, no classifier request |
| Ready channel | Earliest ready filing | No action | Carry once, retain bytes/identity until full request and matching receipt | Validate and commit atomically, Committing |
| Committing | Successful commit | No action | Only matching filed establishes acceptance | Queue receipt, Ready after full answer |
| Committing | Refusal or lost answer | Observe failed run when invoked | Halt, keep refused or uncertain source account, never resend | Roll back on refusal, otherwise uncertain commit, close |
| Ready channel | Read or small control | No action | Send one typed control with the next request ID | Return complete result or explicit refusal, Ready |
| Ready with cursor | next/release or filing | No action | Preserve query identity and full-result deadline | Serve immutable page/release, or commit new filing without changing held pages |
| Serving | Save or leave directive | Pin observed loaded facts | Queue save to boundary, preserve existing leave drain/wind-down rules | Finish ordinary accepted writes |
| Barrier | Between-turn capture begins | Await image | Stop next turn, settle every owed source through frozen frontier | No source beyond the frontier may enter capture |
| Capturing through AwaitCommit | PERSIST-SAVE continuations | Copy/hash and prepare bound declaration | Relay exact matching legs, record confirmation before Commit | Hold immutable image, no ordinary writes or reads |
| After Commit | Publication succeeds | Publish/index, send Published | Record published, release capture, answer original directive | Release image, Ready or graceful close |
| Before Commit | Explicit capture refusal | Preserve previous selection | Record refusal, clean up only when the held exchange permits | Holdings unchanged, release part/cursor as required |
| Any serving standing | Component failure | Preserve failure evidence, no invented Left | Latch, stop decisions/tap, bounded no-save teardown | No recovery publication from a room file |
| Pre-serving | Any enter failure | Existing rollback of this process set | Refuse enter, never open user channel | Discard RAM, no new save |

A pending author ticket has no source sequence. Completion slots preserve concurrent
results until that event can be recorded. They may not author recursively or release an
unrecorded result into a decision. A queue full of ordinary events leaves the terminal
trace reservations available, but does not guarantee sink survival. HARNESS-AWAIT-AUTHOR
and TRACE-CAPACITY specify those distinct claims. [FLOW-2, FLOW-3, FLOW-4, FLOW-6]

## 4. Store, read and acceptance commitments

**FOUNDATION-CATALOG.** STATE-SCHEMA is the sole owner of the initial state-content/1
DDL. Source counters keep their unsigned representation, and the store's increasing
content ID orders inherited history and this session's filings. Each statement holds its
complete Text block, bindings and pointer spans. Each pair holds its complete
start/terminal block and the classified request reference. The noun and verb indexes
support the opposite read/write entry points. Exact structured matches use BINARY bytes,
not normalization or semantic nearness. Reason: retrievable pairs are an index into
retained content, not a second text store with independent ordering. [STORE-2 through
STORE-5, READ-1, READ-4]

The initial catalog now includes Spec 06's mark, prefix and finalized-member tables.
Licensed mark and plan statements carry copied context and atomic effects. The plan
catalog is joined under STATE-PLAN-SCHEMA, STATE-PLAN-FILE, STATE-PLAN-READ and
STATE-PLAN-FLAGS. STATE-OBSERVATION-SCHEMA adds separate operation metadata and
STATE-OBSERVATION-READ exposes it only by typed query under R20-19. STATE-ADOPT accepts
exactly the finished catalog, never whichever subset of these drafts an implementer
happened to build. [MARK-1, PLAN-1, STORE-3]

**FOUNDATION-RECEIPT.** One request is unacknowledged at a time across file, file_pair
and controls. This is a first-release throughput assumption measured by Spec 09. Socket
write completion is not acceptance. Commit followed by lost receipt is uncertain and
terminal to this run. An accepted source is never resent. The source-order ledger
distinguishes filed, correctly_rejected and still owed, so neither a cut nor a save can
use a maximum sequence as proof of completeness. [SEAM-2, MARK-4, CLASS-6, FALS-5]

**FOUNDATION-OBSERVATION.** Each correlatable serving answer carries Spec 09's
StateOperationObservation. TYPES-OBSERVATION, STATE-OBSERVE and HARNESS-OBSERVE give its
matching type, producer and consumer. A snapshot continuation is a new observed
operation within its existing exchange. The harness records the validated observation
before granting completion, with the single held open result recorded after load and
partition activation. A malformed answer halts even when its filed payload claims
commit. Unknown optional OS counters do not halt. Reason: the operator needs store costs
and uncertainty without adding a content feed or weakening the receipt. [COST-2, COST-5,
SEAM-2, FLOW-4]

**FOUNDATION-READ.** Every caller consumes the tagged QueryResult. Ordinary queries
return entries with whole paired outcomes, while open_pairs returns its explicit pair
list. Read pagination frames one complete bounded result and exposes no partial content.
A valid read refusal is an operation outcome, not fabricated empty history. The system
prompt read is mandatory once for boot and permitted later as an explicit recall,
without a second insertion. subjects returns whole matched blocks with all their
bindings and pairs. statement resolves one original StatementRef. ReadBinding preserves
that original ordinal in every filtered result. The harness owns selection and
placement, not the store's read mechanic. [SEAM-3, STORE-8, PERSIST-6, READ-1, READ-5]

marks returns the allocation/open-path view, and span_preview returns an explicitly
tagged view of work through a committed frontier. Neither writes or closes a mark.
Closed span membership and its pair frontier are fixed at closure, so a later pair
cannot change the addressed content. The read adapter preserves these distinctions
before issuing held-content or cut grants. [MARK-1, MARK-3, MARK-5, MARK-6]

plan_current and plan return Spec 07's typed plan snapshots with
PLAN-SUMMARY-EVIDENCE's operator document digest and exact last-transition
SourcePosition. The member validates their stored provenance. The harness checks
strict shape and already-held trusted expectations, then projects its summary at
HARNESS-RECALL's Recording/Recorded transition. A failed mandatory boot read cannot
establish enter success. A failed required refresh after a committed effect latches
component failure and cannot expose a successful stale Observe/show summary.
plan_flags returns its attributed count snapshot for an exact PlanRef and
last_source.block_id as the last-effect SourceBlockId. These are metadata reads and
cannot become held-content or cut grants. Their result tags, count sum, stale-source
refusal and measurement consumer are defined by PLAN-FLAG-READ and the owning
amendments. They use the same channel and bounded read lifetime, with no additional
writer. [PLAN-1, PLAN-3, SEAM-3, COST-4]

observations returns the bounded, run-keyed metadata range defined by
TYPES-OBSERVATION-READ and STATE-OBSERVATION-READ. Its read does not include its own
later observation, alter content order or grant automatic presentation. The outer
response metadata remains a separate harness-bound account. [SEAM-3, COST-2, R20-19]

## 5. Persistence representation and actor protocols

This section elects the representation for the persistence obligations in [persistence
obligations][persistence-contract]. Its named clauses are implementation-level Spec
text. Each destination in section 6 inherits the cutover condition. Pending decisions in
section 7 are review stops.

### 5.1 Published artifact and selection

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-FORMAT.** A published artifact is a directory with three files: state.sqlite,
agent.toml, and binding.json. A is SHA-256 of state.sqlite. The operational declaration
in agent.toml contains A at state-management.start.dump-sha256. Admin's root keys,
including restore-artifact and binary paths, remain outside that copy. B is SHA-256 of
those finished configuration bytes. binding.json contains version 1, schema_id, seed_id,
dump_sha256 A, config_sha256 B, parent_artifact_id or null, position, and confirmation.
Neither file carries its own hash. Reason: the checked pair identifies both starting
inputs without a self-reference. [PERSIST-1 through PERSIST-3]

Admin alone hashes, writes the configuration, and publishes beneath the judged
territory's save-points directory. Directory name is the next 20-digit publication
ordinal, a hyphen, and B. The directory is root:admin-access-group 0750, files 0640. The
existing save-points.manifest remains the root-owned JSON-lines loadable index, with
ordinal, artifact_id, A, B, seed_id, parent_artifact_id, cause, position, confirmation
and arrival. Arrival is published or selected, as ADMIN-SELECTION defines. Confirmation
and cause are the exact TYPES-PERSIST enums. artifact_id is the directory basename,
consisting of the 20-digit decimal ordinal, a hyphen and lowercase 64-hex B.
binding.json verifies one artifact, the outer index selects artifacts. The highest
index ordinal is latest. An invalid latest entry refuses load and never falls back to
an earlier entry. Each appended entry has the next ordinal. A selected entry
points to an existing artifact without renaming its directory or changing its original
confirmation. A directory with no index line is never loadable or automatically
recovered. ADMIN-SELECTION defines the external root key and the argument-free restore
verb. Neither accepts a caller path. [PERSIST-2, PERSIST-3, R1, R2, R3]

Admin writes in an unlisted part directory, fsyncs files and directory, renames with
RENAME_NOREPLACE, appends and fsyncs the index, and fsyncs its directory. Existing
judged-descriptor path rules remain. Duplicate artifact names refuse. A failed index
append cannot be reported as successful publication. The recovery invocation may
validate an already indexed entry, but never promote a bare member-room image or an
unlisted directory. This retires automatic recovered publication from room files.
[PERSIST-3, FLOW-5, R3]

**PERSIST-ARTIFACT-CODEC.** admin's save_points.rs owns StateBinding and
StateIndexEntry. The former is the binding.json object, and the latter is one
save-points.manifest JSON line. They deny missing, unknown and duplicate fields at every
depth. version is the JSON integer 1. schema_id is state-content/1. seed_id is 32
lowercase hex characters. Digest fields use lowercase 64-hex strings. position is the
shared SourcePosition or explicit null, and parent_artifact_id is an artifact ID or
explicit null. confirmation and cause use TYPES-PERSIST's exact tagged/enumerated
shapes. [PERSIST-2, PERSIST-3]

StateBinding has exactly the fields listed above. StateIndexEntry has exactly ordinal,
artifact_id, dump_sha256, config_sha256, seed_id, parent_artifact_id, cause, position,
confirmation and arrival. The A/B names in this section mean dump_sha256/config_sha256
in both objects. ordinal is a positive DecimalU64, starting at "1" and incremented with
overflow refusal. Its 20-digit zero-padded rendering is the directory's creation
ordinal. A selection entry retains that directory name while using a new index ordinal.
Neither JSON reader accepts the old stamp or arrived fields as aliases. [PERSIST-2,
PERSIST-3]

Validate the selected entry against its directory name and StateBinding, then perform
the three hash checks and declaration parse. A seed confirmation has null parent and
position. A rebind confirmation names the same nonnull parent as the binding. Serving
confirmation retains the captured position, including null when no operating source has
been filed. Never skip an invalid newest entry. Reason: one exact stored representation
prevents a publisher and loader from interpreting the same index differently.
[PERSIST-2, PERSIST-3, PERSIST-5]

### 5.2 Save protocol and its confirmation boundary

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-SAVE.** An operator save request may queue during S3 and waits for the next
turn boundary. It cannot force a mid-round cut. This replaces ActivityNotAtRest for
save-point only. The harness starts no new turn after that boundary until save finishes.
The capture uses the loaded configuration copy, never a newly edited territory draft. A
successful rebind is a separate administrative act. [PERSIST-1, PERSIST-4, R2, R7]

| State | Admin event or duty | Harness event or duty | Member event or duty |
| --- | --- | --- | --- |
| Requested | Send save window, or leave under its existing lifecycle timing | Queue to boundary, or drain/wind down/lower for leave | No action |
| Beginning | Receive SavePointBeginning and validate its windows | Freeze input, send the exact capture and tail windows | No action |
| Barrier | Await outcome within applicable windows | Drain authoring and filing through frontier using cut_wait | Finish accepted writes |
| Capturing | Await named image | Send snapshot with new capture_id | Serialize and write part, answer name/size/position |
| Hashing | Open judged room part, copy into private publication directory, compute A | Remain paused and keep state slot | Hold capture, accept no file/read |
| Finishing | Send A through harness | Send acknowledge with same exchange ID | Finish named image, return finished |
| Preparing | Write configuration with A, compute B, write binding, send SavePointPrepared | Check same healthy frontier and no halt | Hold stable image |
| Confirming | Await SavePointCommit | Author save_point confirmation with A/B, then send Commit if latch still clear | No mutation |
| Publishing | Publish and index after valid Commit | Remain paused until outcome | No mutation |
| Published | Return SavePointTaken or successful leave outcome | Record outcome, resume serving or finish clean unload | Release capture |
| Explicit refusal | Return SavePointNotTaken, leave may follow old explicit-refusal policy | Record refusal and release barrier | Discard part, holdings unchanged |
| Broken seam before Commit | Do not publish | Latch failure, no Commit, halt | Discard unfinished part on exit |
| Missing snapshot/finish result at its bound | No publication, await cleanup outcome before permitting retry | Keep barrier, send correlated abort before another ask, discard late result, await aborted | Delete held part or finished file, answer aborted, Ready |
| Cleanup fails or aborted misses its bound | Failed run, no retryable save outcome | Latch component failure, no further state ask | No successful cleanup inferred |
| Failure after Commit | Mark broken, stop publication, retain artifacts and report their locations | Halt, no further capture | No new save |

The ladder above summarizes duties. The actor tables below determine transitions. The
member's complete state table is STATE-PERSIST-STATES. The harness owns the one
outstanding state exchange. On image/finished timeout its only permitted continuation
is PERSIST-CLEANUP's abort with the original request and capture IDs. No fresh state ask
passes it before matching aborted. Admin never treats a room file as a protocol answer.
[SEAM-2, PERSIST-4]

| Admin state | Event | Action and next state |
| --- | --- | --- |
| Idle | Save or leave accepted with matching observed load facts | Pin loaded copy and run, send directive, AwaitBeginning |
| AwaitBeginning | Matching SavePointBeginning and valid windows | Hold capture ID and unchanged deadlines, AwaitImage |
| AwaitBeginning | CaptureNotReady for the queued on-demand save | Return explicit refusal, Done, no publication |
| AwaitBeginning | No result at the on-demand save window or existing outer leave deadline | Existing unanswered-command outcome, infer no worker phase or success |
| AwaitImage | Matching SavePointImage | Open judged room part, Hashing |
| Hashing | Complete bounded copy and A computed, deadline valid | Send ImageChecked, AwaitFinished |
| AwaitFinished | Matching ImageFinished | Build configuration and binding, Preparing |
| Preparing | Complete A/B and binding, deadline valid | Send Prepared, AwaitCommit |
| AwaitCommit | Matching Commit | Captured frontier authorized, Publishing |
| Publishing | Rename, index append and sync complete | Send Published, AwaitOutcome |
| AwaitOutcome, on demand | Matching SavePointTaken | Validate report against indexed artifact, answer caller, Done |
| AwaitOutcome, leave | Matching SavePointCaptureComplete | Disarm capture window, retain shared/outer leave windows, AwaitLeft |
| AwaitLeft | Matching Left | Validate indexed report and normal leave duties, answer caller, Done |
| AwaitImage or AwaitFinished | SavePointNotTaken after worker cleanup | Cancel unconfirmed publication, refuse caller, Done, merged retry remains available |
| Any before Commit | Explicit worker refusal or local preparation error | Send Aborted where the exchange permits, remove unlisted root part, refuse caller, Done |
| Any before Commit | Worker EOF, invalid message or deadline expiry | Do not publish, preserve Open marker, failed-run recovery, Done |
| After Commit | Worker fails or no final outcome | Mark the pending capture broken, publish nothing further, retain artifacts, never invent Left |
| Any | Wrong run, capture ID, direction, phase or hash | Protocol failure, cancel unconfirmed publication, preserve evidence |

| Harness state | Event | Action and next state |
| --- | --- | --- |
| Running | Save accepted | Allocate capture ID, author requested save_point with its original window before any barrier, QueueBoundary |
| QueueBoundary | Current turn finishes, window remains, no halt | Freeze next turn, queue SavePointBeginning, Beginning |
| QueueBoundary | On-demand save deadline before boundary | Author refused, answer CaptureNotReady, Running |
| LeaveLowered | Flow healthy, no rollback, tail window remains | Allocate capture, start save window, author requested, queue Beginning |
| Beginning | Whole SavePointBeginning carried | Start filing barrier within the unchanged save and leave windows, Barrier |
| Beginning | Partial carry | Retain bytes, service original deadlines and lifecycle |
| Barrier | Every owed source filed or correctly rejected | Send snapshot, AwaitImage |
| AwaitImage | Matching snapshot result | Relay Image, AwaitChecked |
| AwaitChecked | Matching ImageChecked | Send acknowledge on held state exchange, AwaitFinished |
| AwaitFinished | Matching finished result | Relay ImageFinished, AwaitPrepared |
| AwaitPrepared | Matching Prepared, same frontier, latch clear | Author confirmed, AwaitRecorded |
| AwaitRecorded | confirmed returns Recorded and latch remains clear | Send Commit, AwaitPublished |
| AwaitPublished | Matching Published | Author published, release_capture, Completing |
| Completing, on demand | Capture released and trace records complete | Answer SavePointTaken within capture window, resume serving |
| Completing, leave | Capture released and trace records complete | Carry SavePointCaptureComplete within capture window, then release/unload/Left under shared tail |
| Any before Commit | Explicit operation refusal | Author refused, abort or release only when its state slot permits, answer SavePointNotTaken |
| Any | Flow component fails or an undisputed blocking bound expires | Latch halt, no new capture, preserve phase and uncertain outcome |
| AwaitImage or AwaitFinished | Member result still pending at its bound | Keep barrier, enqueue correlated abort, Aborting |
| Aborting | Matching late result for the timed-out leg | Validate identity and observation, record observation, discard result, remain Aborting |
| Aborting | Matching aborted | Record refusal, release barrier, answer SavePointNotTaken, lifecycle owns retry |
| Aborting | Partial carry or WouldBlock | Retain bytes and cleanup deadline, remain Aborting |
| Aborting | Missing, refused, malformed or wrongly correlated cleanup, failed carry or dead peer | Component failure, halt without successful leave |
| After Commit | Aborted or other backward continuation | Protocol failure, do not retract a sent Commit |

Phase-specific rows take precedence over Any rows. A queued on-demand expiry is
CaptureNotReady. A missing image/finished result is R20-6's timeout, with retry after
PERSIST-CLEANUP, not an immediate halt. A component failure, including failed cleanup,
is still a halt. After Commit, R20-7 forbids finishing an
incomplete publication after worker failure. [FLOW-4, FLOW-5, PERSIST-4, R20-6, R20-7]

The cross-cutting rows below apply in every listed phase, not only at the barrier. They
replace the corresponding serving cells of admin Spec 3.0 and harness Spec 6. [FLOW-4,
FLOW-5, PERSIST-4, R3]

| Event | Required disposition |
| --- | --- |
| Member death before serving capture completes | Harness latches failure, admin receives no successful Left. No new capture. Before Commit cancel publication, after Commit retain a broken save point under R20-7 |
| Admin disconnect before Commit | Harness cancels capture when its state slot is available and reports refusal, a missing component continuation follows its bound |
| Gate death during paused save | Apply admin Spec 3.0's organ-death cells, including containment and forced leave where applicable, preserve healthy capture, halt if containment or the flow fails |
| Second save, seed, bind or restore invocation | Existing invocation-lock refusal, never a second capture |
| Force-unload joins an unload | Existing join changes leave mode, keeps the same capture and deadlines, no second publisher |
| Force-unload meets an on-demand save holder | InvocationInFlight under the existing verb table |
| Explicit image/finish refusal in graceful S6 | SavePointNotTaken and S9 remain available for a healthy state channel |
| Missing image/finish result at its bound | PERSIST-CLEANUP then merged retry under R20-6, keep its missing-result attribution distinct from explicit refusal |
| Publication error before index append | No public success, latest unchanged, discard unlisted root part |
| Index appended but final worker outcome missing | Mark the save point broken and ineligible, keep artifact identity and Open marker as evidence |

Commit authorizes one healthy captured frontier. It proves neither publication nor
completion of the save operation. A worker failure before final matching SavePointTaken
or Left breaks that save point, even after a valid Commit. Admin stops publication,
keeps committed artifacts in place and reports the capture, A/B when known, phase and
every extant location. It never promotes the leftovers in recovery. A save completed
before a later unrelated worker failure remains a good save point. [PERSIST-2,
PERSIST-4, FLOW-5, R20-7]

**PERSIST-PUBLICATION-GUARD.** Before sending Prepared, admin creates and fsyncs a
root-only sibling file save-points/.<artifact_id>.pending, using no-replace creation.
Its strict canonical JSON is {format:"state-publication/1",artifact_id:String,
capture_id:DecimalU64,state:PublicationState,phase:PersistencePhase,
locations:Vec<String>}. PublicationState is pending or broken. locations are
admin-derived artifact/part names relative to the judged save-point root, never caller
paths. The file survives rename/index append. After the matching final SavePointTaken or
successful Left, admin removes it and fsyncs the parent before reporting success. It is
protocol custody metadata, not a fourth file in a completed save-point directory.
[PERSIST-2, PERSIST-3, PERSIST-4, R20-7, R20-20]

A failed or uncertain operation keeps the guard. On observed failure admin atomically
replaces it with state broken and the last confirmed phase/locations, and syncs that
replacement. If that update fails, the pending guard still prevents load. Load, restore
and bind-state refuse any selected artifact with either guard state as
StatePersistenceInvalid/load/broken_save_point, or stage bind for bind-state. They never
silently skip the newest entry. The operator may inspect the retained artifacts and
explicitly restore a previously completed save point. Repair of the broken capture is
manual. The loader requires no absent worker to resolve this guard. [PERSIST-3,
PERSIST-7, R20-7]

Snapshot image waits use the declared persistence.snapshot_wait. The initial explicit
setting is 120 seconds. The finished leg keeps its merged 2-second allowance. Both
include enqueue/carry and are capped by the current outer save and leave windows.
Missing image/finished answers keep the exchange unsettled until matching aborted.
PERSIST-CLEANUP discards late results, then allows SavePointNotTaken and retry. Its
existing two-second small-control bound starts at abort enqueue and includes carry,
without resetting save or leave windows. Failed cleanup, dead peers and malformed
answers remain component failures. [PERSIST-4, CLASS-8, R20-6]

### 5.3 Load, seed, rebind, and unclean end

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-LOAD.** Admin selects an indexed artifact, checks dump=A, config=B, and the A
inside config equals binding A. It parses and runs that configuration copy. It checks
that its custody paths and agent identity agree with the judged territory. It never
substitutes the mutable draft after checking B. It hands raw dump fd4 to the member,
plus verified StartIdentity through the existing enter fields. The member never receives
configuration or a caller path. [PERSIST-2, PERSIST-3, R2]

| Load state | Admin | Member | Harness |
| --- | --- | --- | --- |
| Selected | Verify three checks and declaration, or refuse before spawn | Absent | Absent |
| Spawned | Create channels, fd3/fd4/fd9 under existing custody. The read-only fd4 now carries the raw dump image | Verify/adopt scratch image | Stand required ports under remaining load budget |
| Opening | Await enter | Answer opened only after valid image | Open state channel, receive StartIdentity with its position |
| Head recorded | Await enter | No content writes | Author untapped load with A/B, declaration and template inventory identity |
| Partition validated | Await enter | No content writes | Validate semantic partition and activate recorder, or record refusal and roll back |
| Templates checked | Await enter | No content writes | Start STE100 duty after load, record turnless ste100.result replies, certify inventory or refuse enter |
| Boot read | Await enter | Serve system_prompt selector through end mark | Read once, author recall with full seated prompt, cache head |
| Placed | Await enter | No prompt filing | Open decoder with cached prompt first |
| Pair repair | Await enter | Accept normal-flow repair only | Run Spec 05 BOOT-REPAIR and require its empty recheck before serving |
| Plan summary | Await enter | Serve plan_current | Cache held PlanSummary under HARNESS-PLAN, no filing |
| Serving | Report loaded | Normal flow | Open user channel only now |
| Any pre-serving failure | Existing failed-enter rollback | Close and discard RAM | Refuse enter, no user input |

The boot read is the first content read after open and structural load checks. The load
metadata and template checks are not content reads. No message.system is recorded or
tapped. recall is forbidden by the partition and carries the exact boot Message so
replay sees it. This explicitly replaces the merged rule that a load records none of
what it seats. Subsequent argument records remain FLOW-3's duty. The mandatory boot read
occurs once per load, while later explicit span retrieval is legal and never refiles the
prompt. [PERSIST-6, PROMPT-1, FLOW-3, STORE-8]

**PERSIST-SEED.** The root-shell-only admin verb `seed` takes the agent name already
used by admin and no prompt/path argument. It reads the fixed root-owned prompt draft
and certificate from the judged territory, stands a seed-mode member briefly, and
requests STATE-SEED. It refuses a loaded agent or an existing agent chain. Its image
uses the same publication format and index. A changed prompt requires a newly created
agent chain, not rebind or live mutation. The seed's A/B account for its sole pre-agent
write. No agent, decoder turn, or live tee exists during seed. [PERSIST-5, PERSIST-7,
PROMPT-1]

`bind-state` is also root-shell-only and argument-free apart from agent name. It selects
the currently requested indexed dump, copies it unchanged, and combines it with the
judged territory's new configuration draft. Admin computes the same A and a new B,
publishes a new indexed artifact with the prior artifact as parent, and runs that copy
on the next load. It refuses changing seed_id or prompt. It never modifies a loaded
agent. Neither new command is added to connector sudo rules. [PERSIST-2, PERSIST-3,
TAP-2, R2]

**PERSIST-RECOVERY.** An unclean end without Commit retains the last indexed save point
as the next start. Under existing S0d/S8b recovery, admin supplies Reset {prior_run,
reason} and the verified StartIdentity on enter. The harness records load.reset beside
load.state_start, so the same load line holds the unclean run and selected A/B. No
standalone reset event or new Reset member is added. Unconfirmed pre-Commit parts may be
discarded under the existing cleanup. A pending or broken publication guard keeps its
artifacts for manual repair. Only a fully completed save with no guard is selectable,
and no incomplete publication is recovered. Refuse a malformed latest index entry
instead of skipping it. Old one-file save points are unsupported_layout, and old agents
are recreated. [PERSIST-3, PERSIST-7, FLOW-5, R3]

### 5.4 Passes, refusals and timing

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Admin's verify pass certifies the three hash equalities, custody, and parseability of
the loaded copy. It does not certify prompt meaning or SQLite schema. Member adoption
certifies schema, source structure, and complete seeded span, not semantic truth. Seed
completion certifies insertion of the exact reviewed/certified bytes and end mark, not
the quality of that review. These separate outcomes are all required before loaded. A
model/classifier double proves only protocol handling. [PERSIST-3, PERSIST-5, PERSIST-6,
PROMPT-2]

Public refusal names added to the lifecycle vocabulary are unsupported_layout,
seed_exists, seed_requires_unloaded, seed_certificate_invalid, seed_failed,
binding_mismatch, configuration_mismatch, publication_failed, and boot_failed. Each
names its stage and artifact where available. Any missing component result follows the
bounded failure path rather than one of these conformance/operation refusals. [FLOW-4,
PERSIST-3]

The outer save index, binding and admin journal must agree on A/B and Confirmation.
Where the source trace records are available, they must name those same values. Their
absence at the sink is a trace delivery question, not a new re-read reconciliation duty
or proof that the store is invalid. A serving confirmation names run and capture_id.
Seed and rebind have no serving capture_id and are accounted for by their tagged
identities. Their agreement proves identity and confirmation, not that a trace sink
survived. Sink durability is still the trace charter's obligation.

An explicit refusal before finished uses abort on the held capture exchange. After a
timely finished result, it uses a fresh release_capture exchange and removes the
unlisted root part. An image/finished timeout instead uses abort on the original
exchange, even when the member has reached FinishedHeld. A missing cleanup result
follows component failure.
Before publication a size/refusal result remains a completed operation with unchanged
latest. [PERSIST-4, FLOW-5]

**PERSIST-TIME.** SaveWindow is {began_ns:DecimalU64,deadline_ns:DecimalU64}, using Spec
09's raw CLOCK_MONOTONIC readings, never a serialized Instant or a restarted remaining
duration. Both sides retain the same values. An on-demand SavePoint directive carries
window. Admin samples began_ns just before carrying its first byte and computes
deadline_ns from the loaded persistence.save_wait. The prior Observe exchange spends its
own existing bound. The window includes carry, any queued turn, barrier, image, hashing,
confirmation, publication and the result. The harness requires the declared duration and
began_ns no later than its receive reading. A well-formed window already expired in
transit follows queued expiry, not a renewed timer. Reason: one absolute window exposes
transport delay without electing another clock or resetting a bound at a later phase.
[PERSIST-1, PERSIST-4, CLASS-8, COST-4]

**PERSIST-LEAVE-TIME.** Leave and JoinLeave keep their existing drain, wind-down, force
and escalation rules in admin Spec 3.0. No save window runs during S4 or S5. If either
declared lifecycle bound is absent, the graceful phase remains unbounded. After the
lower, at entry to the save stage, the harness samples began_ns and computes the capture
deadline from save_wait, capped by its existing shared leave deadline. That shared
window still starts at the recorded interrupt/lower anchor, under harness Spec 6, and
uses LEAVE_BOUND_MS. A rollback or flow halt starts no capture. Reason: a save's budget
must not replace the operator's graceful-drain election. [PERSIST-4, FLOW-5, admin Spec
3.0, harness Spec 6]

**PERSIST-BEGIN.** Before the filing barrier, the harness carries SavePointBeginning
{capture_id,window,tail_window} on the original lifecycle exchange. window is the exact
on-demand window, or the newly started leave capture window above. tail_window is null
on demand and the actual shared leave SaveWindow on unload. It is not a new command or a
grant to publish. The harness completes that frame before beginning the barrier and
retains the same bytes across partial sends. Later messages may not replace either
window. [PERSIST-4, SEAM-2, COST-4]

Admin validates the run/exchange and capture ID. On demand it requires equality with the
sent window and null tail_window. On leave it requires began_ns <= now, the declared
capture duration capped by tail_window, and a leave-window duration equal to checked
LEAVE_BOUND_MS * 1_000_000 nanoseconds. The leave anchor cannot follow the capture
anchor. Unknown fields, overflow or incompatible phase/shape are protocol failure. All
finite windows require began_ns < deadline_ns. If a window has already expired, no
snapshot or next publication stage begins. [FLOW-5, PERSIST-4]

Admin retains its existing outer leave window from leave_window() and, once Beginning
arrives, also observes the announced capture and shared tail deadlines. The earliest
applicable deadline wins. This makes S6's already bounded tail visible even when the
preceding graceful drain was unbounded. A join may shorten a wait under the existing
force rule and never restarts a capture. The capture window ends when the image is
released and the capture's trace outcomes are recorded. On demand that includes the
command reply. On leave, the harness then carries SavePointCaptureComplete {capture_id}
before disarming that window. Admin consumes that exact completion before disarming its
capture window. It is permitted only on a leave exchange after Published and is not
Left. Remaining release/unload/Left work uses the unchanged shared tail and admin's
outer leave window. [PERSIST-4, FLOW-5, admin Spec 3.0, harness Spec 6]

**PERSIST-EXPIRY.** If the on-demand window expires while still queued, the harness
withdraws that save and returns CaptureNotReady after recording its refusal. It
continues the current turn. Admin reports that case only when it receives the typed
answer. Silence at its own deadline is Unanswered, never an invented CaptureNotReady or
successful save. No new turn starts past a begun save barrier until that operation
completes or is refused. Missing component progress during the barrier uses its
model-blocking bound. Missing image/finished results use R20-6 and PERSIST-CLEANUP. No
new Commit is sent after expiry. Post-Commit failure or expiry breaks the save under
R20-7, with no Left inferred from an artifact. [PERSIST-1, PERSIST-4, CLASS-8, FLOW-5]

The deadlines bound waiting and permission to start a later protocol stage. They do not
claim to interrupt a kernel filesystem call at its exact deadline. Admin checks the
clock before and after each bounded copy/hash/publication step. A late return cannot
authorize an unconfirmed publication or become a timely success. An index append made
before the operation failed remains evidence under its pending/broken publication guard.
It is neither silently retracted nor loadable. [PERSIST-4, FLOW-5, COST-4]

The member snapshot leg's declared snapshot_wait deadline starts when the harness
enqueues its snapshot request on the sole state owner, including carry time. The
finished leg's 2-second deadline starts when acknowledge is enqueued. This moves the old
post-send anchor to include queued/partial carry. Both are capped by the outer remaining
save and leave budgets. Public capture_not_ready is an explicit SavePointNotTaken
result, not a component-failure code. [CLASS-8, FLOW-4, PERSIST-4]

Measure save_wait over declared tuple, store size, queued-turn work, concurrent filing,
capture-copy cost, room storage and admin hashing. Test on-demand and leave anchors
separately. Freeze the value before held-out qualification and report every phase and
blocked-request wait. Measure maximum supported store capture and finish cost under
memory pressure against the initial snapshot allowance and retained tail windows. The
loaded snapshot allowance must exceed the maximum measured capture duration at the
maximum supported store size under the same qualified tuple and memory pressure. Its
earlier outer save/leave bound must still permit that capture and the retained
finish/publication windows. A tuple that cannot satisfy both is not qualified. Publish
the measured setting and load identity under R20-6. The initial 120 seconds is
inherited, not claimed as qualification. A long unbounded graceful drain is not part of
this save-window measurement. [COST-2, COST-4, PERSIST-4]

## 6. Owning amendments and enforcement

**FOUNDATION-AMENDMENTS.** The following replacements form one document act.
Representation belongs in the crate Specs and this domain Spec. Contracts retain
meaning, duties and failure outcomes. The annexes carry exact destination sections and
replacement clauses, not instructions for a coder to write a contract later. Reason:
every party must read the same seam before implementation starts. [SEAM-1, SEAM-2,
SEAM-3, FLOW-7, October 18 A1]

| Owning document and destination | Normative replacement |
| --- | --- |
| weaver-types Spec 2/4/5 | [Declaration, shared types, read/file codecs and lifecycle vocabulary](weaver-types-Spec-amendment.md) |
| weaver-trace Spec 3/5/6/9/10/11 | [Envelope, registry, tee, writer capacity and load/result evidence](weaver-trace-Spec-amendment.md) |
| weaver-state Spec 2/3/4/5/6 | [Schema, atomic writes, read service, seeded adoption and member capture](weaver-state-Spec-amendment.md) |
| weaver-spu Spec 1/4.2/4.3/5/6/9/10/new 12 | [Placement/rendering near side and separate STE100 duty](weaver-spu-Spec-amendment.md) |
| weaver-internal Spec 1/2/5 and harness Spec 1/6 | [Pure recall constructor and the exact dependency/query consumer](weaver-internal-Spec-amendment.md) |
| weaver-harness Spec 2/3/4/6/7/8 | [Authoring, loops, classifier port, single owner, waits and halt](weaver-harness-Spec-amendment.md) |
| weaver-admin Spec 3.0/4/6/9/10 | [Custody, fixed verbs, load/save/selection and unclean recovery](weaver-admin-Spec-amendment.md) |
| Gate and diagnostic Specs | [Gate interruption and refused input](weaver-gate-Spec-amendment.md), [diagnostic conversion and isolation](weaver-diagnostic-Spec-amendment.md) |
| Charter clauses named by the register | [Exact charter amendments and retirement register](charter-amendments.md) |
| Harness-trace/state/gate/SPU-decode, gate-world and admin-harness contracts | [Flow and boundary contract text](contract-amendments.md) |
| New harness-SPU STE100 contract | [Admission, checking, result, failure and custody agreement](weaver-harness-spu-ste100-contract.md) |
| Admin-operator, admin-harness and harness-state persistence clauses | [Persistence obligations](persistence-contract-amendments.md), represented by section 5 here |

R20-5 elects this repository's analysis-state contract as authoritative. WeaverAnalysis
mirrors it and owns its reader conformance. The same single document PR carries the
required owning amendments and domain documents. [SCOPE-1, October 18 A1, October 19
document-PR direction, R20-5]

| Claim | Instrument | Protective change that must fail or review evidence required |
| --- | --- | --- |
| FOUNDATION-TRANSITION | Review of cutover body and every amended section | Missing any of the four evidence groups or a transition reference blocks code cutover |
| FOUNDATION-RETIRE | Caller census and owning-crate integration tests | Restore an old-table reader, duplicate prompt, empty first load or none engine |
| FOUNDATION-OWNERS | Shared codec corpus and real recorder/member circuit | Put a socket in the tee, bypass the copy, or refile a boot read |
| FOUNDATION-FAILURE | Lifecycle/fault matrix with a held trace sink | Continue a decision after latch, halt on correct rejection, or begin a capture after a broken feed |
| FOUNDATION-WALK | All table rows, each actor killed at each await state | Admit a turn before pair repair, reuse a source after CapacityPending, or overlap a capture with ordinary writes |
| FOUNDATION-CATALOG | Literal DDL, exact-catalog adoption and query-plan fixtures | Accept a partial catalog, missing noun index or orphan pair source |
| FOUNDATION-RECEIPT | Real commit with held/lost answer and a second queued source | Accept write completion, resend uncertain commit or let later filing overtake |
| Foundation circuit | STATE-CIRCUIT-INSTRUMENT's spawned member and real recorder, with in-crate transaction hooks under STATE-FILING-INSTRUMENTS | Skip the real member, add a test-only serving query or treat a scripted transaction as SQLite commit evidence |
| FOUNDATION-READ | Cross-run history, open-pair, cursor and result-limit fixtures | Swap tagged results, expose partial content or return empty on refusal |
| PERSIST-ARTIFACT-CODEC | Shared publication/load/restore corpus, including invalid newest with valid older artifact, plus types' state-start-v1 fixtures | Accept an old alias, inconsistent binding/index, wrong ordinal/name or A/B, or fall back past malformed newest |
| PERSIST-TIME, PERSIST-LEAVE-TIME | Admin/harness fake-clock actor walk, existing leave_window baseline | Arm save_wait during an unbounded S4/S5, move an anchor on partial carry, or ignore an earlier tail |
| PERSIST-BEGIN | Shared frame corpus and fragmented lifecycle exchange | Accept Image before Beginning, change a window, or accept CaptureComplete as Left |
| PERSIST-PUBLICATION-GUARD | Crash before/after rename, index append and final outcome, plus failed guard update | Publish after worker failure, load a guarded artifact, skip newest silently or remove guard before final success |
| PERSIST-EXPIRY | Queued and active save cases, delayed local I/O completion | Infer CaptureNotReady from silence, issue a late Commit, or claim kernel I/O was preempted |
| PERSIST-CLEANUP projections | Harness/member actor walk through late snapshot, finished and refusal, successful abort and cleanup failure | Send filing before aborted, retain a held image after cleanup, publish a late result or halt on the original timeout alone |
| FOUNDATION-AMENDMENTS | Cross-document review against baseline main | Leave a retired merged clause effective at the same cutover |

The flow pass certifies one accounted disposition per selected source and committed
acceptance before dependent actions, under the tested tuple. The load pass certifies
verified starting bytes, required duties and a seated prompt before input. The save pass
certifies the healthy captured frontier and the publication outcome that the chosen
protocol proves. None certifies model truth, sink durability or untested future tuples.
Each behavioral test above must fail when its protective check is removed. A written
acceptance test or a test double is not its execution result. [FLOW-4, FLOW-7,
PERSIST-3, PERSIST-6, CLASS-7]

The baseline inventory coordinates below define each affected mechanic and its
disposition here. They are specified replacements, never claims that current code has
changed. No separate inventory document is needed to interpret an INV reference.

| Item | Baseline mechanic | Replacement and check |
| --- | --- | --- |
| INV-1 | distill copies message.restored whole without consulting the election | TRACE-VALIDATION refuses a partition selecting restored content. Boot-copy refusal fixtures keep restoration from filing a second copy |
| INV-2 | Recorder::submit detaches a failed tee and returns success. take_save_point checks tee_lost only later | TRACE-STOP and HARNESS-HALT stop decisions at failed copy/carry. Failed-flow perturbations prove the halt does not wait for a save |
| INV-3 | distill copies turnless message.system whole before consulting the election | Remove the carve-out. ADMIN-SEED and HARNESS-BOOT read the held prompt through its end mark before user input, and boot creates no filing |
| INV-4 | SavePoint serializes a stamp, check line and SQLite image in one file | Section 5 replaces that representation with raw dump, bound configuration and manifest. Adoption fixtures check A, B and the binding |
| INV-5 | Ports::classify is an optional label scorer that can retire its channel and return None | Spec 02 and HARNESS-CLASSIFY add mandatory STE100 result/failure shapes beside the retained scorer. Missing-result and correct-rejection fixtures distinguish the outcomes |
| INV-6 | Message contains role and Vec<ContentBlock>. ToolCall has name and arguments, without a call ID | TRACE-PROJECT selects whole Text blocks by source index. PairCopy and Spec 05 carry complete pair payloads and their association. Mixed-block fixtures reject unsafe projection |
| INV-7 | distill maps malformed canonical input to None and skips missing paths | TRACE-COPY distinguishes Unselected from failed projection. Malformed-copy fixtures must halt rather than silently omit selected content |
| INV-8 | Lifecycle clones the inherited state stream between Tee and StateSeam | HARNESS-OWNERS gives one channel owner all files and controls. Lost-receipt and overlapping-control fixtures detect a second reader or writer |
| INV-9 | Writer::enqueue waits on a full queue and blocks its submitting harness | TRACE-CAPACITY and HARNESS-AWAIT-AUTHOR reserve without blocking, retain the pending Event and service deadlines. Pressure fixtures prove ownership and progress |
| INV-10 | Sqlite::land uses one transaction but ingest sends no success receipt | STATE-FILE/PAIR-FILE return source-matched acceptance only after commit. Real-member fixtures hold or lose the answer without granting acceptance early |
| INV-11 | AgentConfig parses state-election while the apex names state-management | TYPES-CONFIG implements the chartered table and retires the old serving spelling at cutover. Declaration fixtures reject the retired form |
| INV-12 | Load passes the tap, and recall/Identity records the boot identity read | TRACE-LOAD makes load the untapped trace head. HARNESS-BOOT records the read without copying or filing it. Seeded-boot fixtures check both boundaries |
| INV-13 | sqlite.rs shape, recall, position_of and identity_of read the old event/field tables | STATE-READ replaces shape, recall, save frontier and boot identity queries with the new schema. The retired-reader census leaves no serving old-table reader |
| INV-14 | The #76 seeding turn admits live system input, empty first load and the none engine | ADMIN-SEED, verified boot and TYPES-CONFIG retire all three with their writer and readers. Whole-path cutover fixtures refuse old or empty starting state |
| INV-15 | save_points.rs publishes one-file images and recovers finished room artifacts | Section 5 and ADMIN-SAVE/RECOVERY require three-file A/B binding, healthy-capture Commit and indexed selection. Publication fixtures refuse broken or unconfirmed artifacts |
| INV-16 | Diagnostic preload and replay depend on the old tables and election | STATE-MODE and the analysis-state amendment preserve a separate diagnostic binding/catalog. Mode fixtures refuse serving preload and cross-mode adoption |
| INV-17 | ToolInterrupt maps to unload, while MemberDies cells permit serving | Typed fault interruption and replacement serving lifecycle cells distinguish control requests from component loss. Fault fixtures prove no serving continuation after loss |
| INV-18 | Missing snapshot replies drain late bytes and leave a live channel available for retry | R20-6 retains retry and the loaded snapshot allowance. PERSIST-CLEANUP first confirms abort on a held capture, including FinishedHeld. Expiry fixtures prove filing and retry work after cleanup and failed cleanup halts |

## 7. Integration dependencies and measured values

The original operator questions are settled by section 0. The authoritative
analysis-state amendment is this repository's copy. WeaverAnalysis mirrors it. R20-6 and
R20-7 supply the persistence failure outcomes, with the protocol guards and tests above.
No pending operator branch authorizes two implementations.

Spec 06's composite returns Staged under R20-16. The next ordinary append consumes its
pinned tail, and the complete Generated answer confirms placement. No serving prefill
exchange or diagnostic permission is owed.

The mandatory boot duties consume Spec 05 BOOT-REPAIR and Spec 07 plan_current for the
committed management account. Spec 08 is a later selection consumer, not a
strategy-validation stage of load. These type/schema dependencies must be implemented
with the foundation before serving cutover. Runtime orchestration remains outside this
document under FOUNDATION-SCOPE.

Production STE100 artifacts, the reviewed system prompt and measured limits remain
cutover evidence. Their methods are in the owning Specs. Protocol doubles and document
checks do not provide that evidence. No deadline promises to preempt a blocked kernel
filesystem call. [CLASS-7, PROMPT-2, PERSIST-5, R20-12]

[cutover]: #1-applicability-and-the-serving-cutover
[persistence-contract]: persistence-contract-amendments.md
