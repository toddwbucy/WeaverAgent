# weaver-harness Spec: dependable-flow amendment

**Status:** MERGED.

**Landing PR:** #387

**Version:** 0.42 **Date filed:** 2026-10-21 **Parent:** weaver-harness-PRD

## 0. What this document is

Replacement text for harness Spec 2, 3, 4, 6, and 7, with instruments for section 8.
This unit includes the store/read and seed/boot clauses. Its classifier near side is
buildable against a declared double. The production STE100 duty and tool pairing remain
dependencies, not implied implementation. Source anchors use 2eb5c5a2.

## 1. Ownership, authoring, and capacity, destinations sections 2 and 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-OWNERS.** Add flow.rs::FlowCoordinator to the serving recording context. Run
owns that context once enter has authored load. The coordinator owns the ledger,
Ste100Port, StateSeam, and the sole HaltLatch. Move the optional Run.state slot into it.
Delete lifecycle's try_clone split between Tee and StateSeam. The channel has one writer
and one outstanding exchange, including controls. The coordinator alone receives
receipts. StateSeam is neither Clone nor a public stream. Diagnostic Record and state
access remain separate variants. Reason: a partial file cannot interleave with another
ask. [SEAM-1, SEAM-2, FLOW-5, SCOPE-1]

**HARNESS-RECORD.** Record::submit consumes every serving tap outcome before returning a
sequence. Its FlowInbox holds immutable selected copies and failure notifications. It
owns no failure latch. On observing a failure, the coordinator sets HaltLatch before any
further decision. Every model request, tool dispatch, accepted-input placement, cut,
save, and success reply first drains that inbox and tests HaltLatch. A discarded author
return cannot discard a tap failure. [FLOW-1, FLOW-2, FLOW-4]

FlowInbox::accept(copy: AdmittedCopy, budget: &mut RetentionBudget) takes ownership of
the copy and returns Accepted, Deferred or IntakeFailed {copy, detail}, returning the
whole copy on failure. It reserves an inbox slot, never ledger entries. Record keeps a
returned copy in its failure slot without allocating. The coordinator's drain alone
installs ledger entries, reserving and validating the complete copy before transferring
any block. That installation also returns the whole copy if it fails. author_and_wait
drains after each Record attempt and before another author call or return to a caller.
This drain installs only copies from completed trace admission and latches failures. It
neither authors nor reduces held port completions. Only IntakeFailed latches tap_failed
at copy_intake. Deferred is owned waiting under HARNESS-BACKLOG and permits
completion-record progress. No second failed intake can overwrite the held copy. Copies
still queued at that latch remain accounted. Backpressure before trace admission remains
CapacityPending, not this failure. Reason: the tap-to-ledger handoff cannot lose a
selected block or leave a partly accepted copy hidden behind an ignored author result.
[FLOW-2, FLOW-4, SEAM-1]

**HARNESS-BACKLOG.** RecordingContext owns RetentionBudget, shared by Record and the
flow ledger. It charges every selected nonterminal block once, including queued copies
and in-flight requests. A move from inbox to ledger transfers the reservation. Maxima
are the measured FlowBacklog block and byte limits. They are memory bounds, not
receipt-age deadlines. The operator ruled bounded waiting on 2026-10-20. Reason: recent
emitted material remains in the SPU context tail while filing lags, and its trace record
supplies the reconstruction source. Qualify this slack in turns and tail occupancy, not
a wall-time promise. [FLOW-2, FLOW-4, CLASS-6, CLASS-8, R20-2]

The fields are limits:FlowBacklog, blocks:u64, bytes:u64,
reservations:HashMap<SourceEventKey,CopyReservation> and
waiting_copy:Option<AdmittedCopy>. SourceEventKey is {run:String,sequence:u64}.
CopyReservation is {shared_bytes:u64,blocks:Vec<BlockCharge>}, and BlockCharge is
{block:u64,bytes:u64}. Charge canonical envelope and context bytes once per copy plus
each exact raw block. PairCopy charges one block. Zero-block selection has no charge.
Checked sums and fallible reservations precede transfer. Release a block only after its
filed or correctly_rejected disposition is Recorded. Release shared bytes after the last
block. Duplicate reservation/release is a protocol failure, not an extra allowance.
[FLOW-2, STORE-5, SEAM-1]

| Condition | Result and obligation |
| --- | --- |
| Complete copy fits both limits | Reserve all fields, accept once |
| Complete copy does not fit remaining room | Hold it whole in the single waiting_copy slot, set BacklogWaiting |
| waiting_copy can now fit | Transfer its complete charge to the inbox, clear slot and waiting state |
| Copy alone exceeds a declared maximum | Keep it whole and wait, no truncation or immediate queue_limit halt |
| BacklogWaiting and another content-producing call | Return Busy with the caller's input retained before a new content record, no second admitted copy |
| BacklogWaiting and mandatory excluded completion/measurement record | Permit normal trace authoring so classification and receipts can progress |
| Recorded terminal disposition | Release its reservation, reconsider waiting_copy |
| Allocation/identity failure | Preserve the copy, component failure through copy_intake |
| Existing decision_wait expires while the next model request is blocked | queue_limit component failure, account waiting_copy and the blocked request |
| Halt | Freeze all charges and retain the single waiting/failure copy for its account |

FlowInbox::accept distinguishes Accepted, Deferred and IntakeFailed. Deferred transfers
ownership to the fixed waiting_copy slot and is not failed intake. Record may return
Recorded once the trace event and this whole-copy ownership are secured. It never waits
recursively inside authoring for that copy's filing. The next content-producing
operation uses FlowCoordinator::ready_for_content, which returns Ready or BacklogWaiting
and pumps the existing sockets while waiting. No new generation, dispatch or admitted
control content passes that guard. Already received caller input remains owned before
trace admission. The classifying and filing of existing obligations continue, including
their excluded result records, with no second content-author ticket. [FLOW-2, CLASS-6,
CLASS-8, R20-2]

When BacklogWaiting blocks the next model request, arm the existing decision_wait from
that first block and never reset it on progress or another probe. Record a state.flow
backlog_wait sample with the held-copy identity and retained gauges, and set lag_alarm.
When room returns record backlog_resumed, clear that forced alarm and proceed. A
trace-capacity wait retains the original sample ticket. At expiry record the elapsed
wait in the terminal account even if the normal sample could not land. queue_limit is a
reason only at that expiry. The gate's tool clock and persistence timeout are not
replaced by this wait. [CLASS-8, FLOW-4, FLOW-5, COST-4, R20-2]

Qualification includes the one-copy waiting/failure reserve, metadata, maximum
classifier expansion, one encoded filing and the held caller input. The source charge is
not total RSS. No source is truncated or partly filed to meet the budget. Fixtures fill
the queue while classifier replies still need trace records, then free it through
receipts. They must progress, preserve exact copies and never tap twice. Removing the
content guard, allowing a second wait slot, resetting the bound or halting immediately
at the limit must fail a fixture. [FLOW-2, FLOW-4, CLASS-6, FALS-5, R20-2]

**HARNESS-AWAIT-AUTHOR.** Replace every serving Author call with the common
RecordingContext::author_and_wait path below. Run delegates to it, and Ports borrows the
same recording context. Author::author takes kind, subsystem, turn, AuthoredSource,
causal_parent: Option<Sequence>, and payload, and returns RecordAttempt or
SubmitFailure. AuthoredEvent holds those six members unchanged. Ordinary callers pass
None, accepted input supplies the offered sequence. Remove the existing author
boundary's hardcoded None for this field. author_message keeps its outer
UnlicensedMessage refusal. The helper calls resume until Recorded, servicing transport
while pending. It cannot recursively author. The baseline author census is the current
source's finite set of author, author_message and submit calls, including wrappers and
discarded results. Diagnostic calls are excluded by the explicit converter boundary. The
implementation act refreshes that finite list against its source revision. The caller
audit and helper tests enforce handling. A result-type change alone cannot stop let _
from discarding a result. Reason: synchronous nested callers still need a defined
progress path. [FLOW-2, FLOW-3, CLASS-8]

**HARNESS-RECORD-LIFETIME.** record.rs::RecordingContext owns Author, Record,
FlowCoordinator, RetentionBudget and the completion slots for one serving run. Its
method is author_and_wait(&mut self, ports: &mut PumpPorts, event: AuthoredEvent) ->
Result<Sequence, AuthorFailure>. Both resume_author_and_wait and
resume_claimed_author_and_wait also return Result<Sequence, AuthorFailure>, preserving
their existing ticket, claim and transport arguments. AuthorFailure is a move-owned
result containing the RunFailure and an optional returned original AuthoredEvent. It is
not Clone. A returned original owns no live recorder ticket and has no trace sequence,
and remains the caller's custody. An absent returned original means the context retains
that exact original in an existing author slot, or the original has already reached
Recorded; it never means the original was discarded. RunFailure retains its existing
failure, wait and nonfatal admission reasons, not event custody. An already-latched
fatal cause remains immutable and takes precedence over later admission reasons.
PumpPorts borrows the currently standing lifecycle, gate, decode and legacy-label
transports and PumpIo. Each absent transport is None. The state and STE100 transports
are the coordinator's. The pump never reads a second handle for one transport. Reason:
enter authors load before the current Run constructor, and leave authors unload after
releasing members. Neither site can require a fully standing Run. [FLOW-2, FLOW-3,
FLOW-5]

Construct this context when enter establishes the serving recorder and common clock.
Move it, without reinitializing its clock, tickets, latch or slots, into Run after load.
Ports borrows it while a loop has the seat. Keep it through the last unload and drain.
Diagnostic recording retains its separate path. The context's phases are these,
independent of the lifecycle's existing positions. [FLOW-2, FLOW-3, FLOW-5, SCOPE-1]

| Phase | Available work | Exit |
| --- | --- | --- |
| Preflight | State open and load, trace-capacity wake, lifecycle and enter deadline. No decode, tool or user input | Move the same context into Run after Recorded load |
| Entered | Partition activation, template checks, boot, then serving as each port stands | Clean leave after filing barriers and capture, or first halt |
| Releasing | Existing release and shutdown evidence, including HARNESS-OPEN's private owed account. No new content or filing. Remove a transport only after its final completion is consumed | Unload, bounded drain where required, Closed |
| Failed | HARNESS-HALT owns terminal permits, partial transports, the first failure account and HARNESS-OPEN's private owed account. Settle or abandon that account before retiring its transport and authoring unload | Failed exit under the halt deadline |
| Closed | No authoring, port reduction or reactivation | Terminal |

In a clean release, the coordinator remains present after the state transport closes.
Require an empty obligation set and no held state completion before that intentional
close. Only nonselectable shutdown kinds remain legal. A later content attempt is an
integration failure, never an accepted copy on a closed store. An unexpected EOF before
that close remains a component failure. No retired port is polled, recreated or silently
treated as a successful answer. [FLOW-4, FLOW-5, SEAM-2]

The helpers in authorship.rs build AuthoredEvent before submitting. For serving, message
construction returns Result<AuthoredEvent, UnlicensedMessage>, and fault, refusal,
recall, tool-result and unrecovered-call construction return the event or their existing
construction failure. They take no recorder and cannot submit recursively. The context
alone invokes Author::author and Record::resume. Retire author_identity from serving
with the seeding turn. Existing diagnostic wrappers remain on their diagnostic arm.
[FLOW-2, FLOW-3, PROMPT-1, STORE-8]

Every ignored-result site in the disposition table must propagate its failure reason or
enter the already-latched teardown while preserving any returned original in its
caller's owner. Neither a void helper nor an is_ok test can resume a
decision after failed or pending authoring. Refactor report_pressure to return an
optional prepared fault, and submit it from the owning context. Set its reported flag
only after Recorded. Retire record_enter_ask's record-failed-then-author-fault fallback
in serving. HARNESS-READ records the boot answer before seating it and failure takes the
enter refusal path. [FLOW-3, FLOW-4, FLOW-5, PERSIST-6]

Split the common pump into transport progress and event reduction. Transport progress
reads at most one complete pending completion per source and stores it in that source's
owned slot. Sources are lifecycle, gate, decode, label classifier, STE100, and state. A
full completion slot stops reads from that source. It may finish a partial outgoing
frame, observe deadlines and terminal faults, and wake on trace capacity. It cannot
author or release a completion into model work. Reduction occurs only after the pending
author completes. Every completion's required trace record must complete before its
effect, including a ledger transition. Thus a pump-generated observation never competes
for the single pending Event. A second submit is a caller error. When no fatal cause is
already latched, it latches protocol_failed as the first fatal cause without discarding
the returned Event. When a fatal cause is already latched, that cause remains unchanged
and takes precedence over the later caller error. If the context's held caller original
or immutable first-fatal original prevents taking another original, return that new
original in AuthorFailure. Never replace either earlier original, clone an event merely
to fit an error value, allocate an overflow queue, or invent a ticket. Keep the first
latched failure unchanged; a later rejected call returns that failure with its own
untouched original. This return grants neither retry nor continuation. [FLOW-2, FLOW-3,
FLOW-4, SORT-4]

Ownership transfer and recording admission are separate facts. An original accepted into
the one pending author keeps its allocated ticket and original wait through
CapacityPending and resume; no returned original may duplicate that pending owner. An
early owner hold remains unadmitted and may occupy the context's existing single held
slot. Its caller takes it unchanged before another attempt. If a resumed pending author
is withdrawn because an existing account owns admission, return that original through
the same move-owned result when the held slot is occupied, after releasing only that
attempt's unadmitted preparation and bookkeeping. A withdrawal assigns no trace sequence
and renews no wait. Existing mandatory account settlement continues; no return permits
skipping it. [FLOW-2, FLOW-3, FLOW-4, FLOW-5]

**HARNESS-HELD-BOUNDARY.** New owner admission, release, Enter rollback,
member-capture preparation and Closed refuse while the held caller slot or the
template-blocked caller slot is occupied, including an original held after release
began. Return a recoverable Busy refusal when the held original is the blocking
condition; do not latch protocol_failed for that condition alone. The separate
second-submit caller-error rule above remains unchanged. [FLOW-2, FLOW-3, FLOW-5]

Use one shared private held-input predicate and reason across these admission roads.
The Busy reason identifies the occupied caller-held or template-blocked slot; if both
are occupied, both originals remain owed. The method's existing result family may carry
that private reason, but may not translate held input into WrongProof, MissingHead or
TemplateBlocked. In particular, completed_template_enter cannot claim a template
failure merely because an ordinary caller original is held, and install_place must
return the same offered proof with Busy rather than claiming it mismatched. Genuine
incomplete/rejected certification retains its own TemplateBlocked reason. Neither Busy
nor TemplateBlocked by itself creates a trace refusal or a new wire vocabulary.
[FLOW-2, FLOW-3, CLASS-8]

Held input at begin_releasing, begin_enter_rollback, close_after_drain and
prepare_member_capture returns recoverable Busy, not a new fatal ProtocolFailed. Other
invalid phase, missing proof, wrong bound or unmet protocol preconditions keep their
existing rules. A held
refusal leaves phase, pending ticket, exact originals, resources and original deadlines
unchanged; it neither arms a replacement wait nor starts release or capture. Taking the
exact originals only removes this hold. A subsequent attempt must still pass all
ordinary phase, proof, ownership and deadline checks; it is not a promise that retry
succeeds and it grants no ordinary author retry permission. [FLOW-2, FLOW-3, FLOW-5]

An already latched fatal cause remains first and is returned before a nonfatal held
reason. Existing clock/deadline and independent failure checks still run where required;
Busy must not mask an observed failure, renew a bound or revive Failed. Taking an
original in Failed does not make these healthy boundary APIs available. Independently
bounded fatal teardown remains required under HARNESS-HALT-BOUND: the existing context
or caller evidence owner retains each untaken or returned original, settles only real
owed work, and closes/exits failed at the original cap without claiming successful
Closed or successful Left. No held-input recovery wait extends that cap, and no original
is dropped just to satisfy close. [FLOW-2, FLOW-3, FLOW-5]

Existing admitted owners may complete their exact private continuations; held input
grants none of its own. This rule adds no halt-account member, fabricated ticket,
implicit submission, overflow queue or successful close that loses an original.
[FLOW-2, FLOW-3, FLOW-5]

Every caller handles both ownership outcomes explicitly. A returned original goes back
to that caller's existing event or stopped-evidence owner; a caller may not use an error
conversion that drops it. In particular, no From<AuthorFailure> for RunFailure is
permitted: propagation with ? must not silently discard a returned original.
Compile-time negative pins must prove that AuthorFailure is not Clone and RunFailure
does not implement From<AuthorFailure>. Input offered/refusal/turn/message authoring,
mark-refusal authoring, repair authoring and retained retries preserve their original
stage and source identities. They mark an attempt started only when the context actually
takes custody, and never rebuild a returned event from current state. After failure,
returned custody joins the caller's bounded teardown under the existing deadline. It
neither enlarges the fixed halt-account schema nor adds a trace record. Halt reporting
still names only the real pending ticket; an unaccepted original must not be represented
as a ticketed AuthorAtHalt. [FLOW-2, FLOW-3, FLOW-5, FLOW-6]

The first fatal account uses the trace writer's fault reserve. It may bypass an event
that has no trace identity, after stopping the tap. Record the pending ticket in the
account. If capacity later permits the event, it is trace-only with a stopped tap.
Unload uses its distinct reserve. Deadline expiry is never held behind a trace write. No
recursive error-report authoring is allowed. [FLOW-4, FLOW-6]

**HARNESS-AUTHOR.** AuthoredSource is BoundaryInput(uid), ManagementInput(Cause), Model,
Tool, Loop, or Runtime. Only input.offered and message.user accept BoundaryInput,
compared against Run.operator to yield operator or external. Missing serving peer uid is
a boundary failure. The table covers TRACE-REGISTRY. A new kind requires an explicit
row. No payload may choose channel or origin. [TAP-1, TAP-5]

| Kinds | Channel | Origin |
| --- | --- | --- |
| input.offered, message.user | input | operator or external from boundary UID |
| plan.event from root control | input | operator from ManagementInput |
| plan.event effect, plan.break, state.plan.measurement | internal | loop |
| message.assistant, model.output, model.unrecovered_call | output | model |
| message.tool_result | tool | tool |
| tool.call.started | tool | model for a model request, loop for a licensed review request |
| tool.call.completed | tool | tool for a gate answer, loop for a harness or inward outcome |
| tool.return_rejected | tool | tool |
| loop.instruction, sort.decision, state.sort.measurement, step.outcome, state.mark, state.mark.position, state.placement, model.request | internal | loop |
| model.measurement, model.field, score, state.flow, state.store.measurement | measurement | runtime |
| classify.request, classify.output, ste100.result | internal | runtime |
| load, unload, session.closed, turn.started, turn.closed, fault, elision, refusal, flush, recall, save_point, message.restored, message.system | internal | runtime |

The serving path never authors message.system. Replace WIND_DOWN_REQUEST's message.user
with certified loop.instruction. Remove the live seeding acknowledgement with #76.
Restatement, compaction, and step instructions enter the template inventory before their
consumers ship. [FLOW-8, CLASS-9, review S2]

**HARNESS-LOOPS.** Keep engine.rs as the sole author of accepted conversation input. The
frame boundary records input.offered and obtains its conformance result, but does not
also author message.user. It mints a run-local AcceptedInput token holding input_id,
offered sequence, kernel dialer UID, exact Text bytes and accepted bindings. Only the
serving frame path can construct it. The token is consumed once when the engine opens
its turn and records message.user with the offered sequence as its causal parent. The
new source ID comes from that record, and only an identical Text block can reuse those
bindings. Reason: FLOW-1 and FLOW-8 require one attributed content origin, and GATE-1
requires that the checked bytes reach the model. [FLOW-1, FLOW-2, FLOW-8, GATE-1,
CLASS-4, STORE-5]

Change the serving Ports::turn argument from Vec<Message> to Vec<Contribution>.
Contribution is a harness-owned public enum with private payload constructors:
Input(AcceptedInput), Instruction(TemplateExpansion), and Held(HeldMaterial).
Ports::accepted_input transfers the pending token once. Ports::instruction takes a
registered template ID and its typed substitutions and returns TemplateExpansion or a
typed operation refusal. HeldMaterial comes only from the typed read adapter. No
constructor accepts an arbitrary role or a fabricated source identity. Diagnostic replay
keeps its separate API and cannot mint any of these serving capabilities. [FLOW-8,
STORE-8, PROMPT-1, CLASS-2, SCOPE-1]

| Contribution | Engine obligation before append | Refuse |
| --- | --- | --- |
| Input | Open one turn, author message.user once with BoundaryInput(uid), then consume its tap outcome | Foreign run, consumed token, altered bytes or scheme, second input in the same turn |
| Instruction | Expand certified template, author loop.instruction once with Loop origin, then consume its tap outcome | Unregistered template, wrong slot type, missing slot, changed inventory digest |
| Held | Record recall provenance and place the exact stored span, with no new filing | Wrong source, incomplete span, altered bytes |
| Raw Message or caller-selected role | No serving constructor exists | System, assistant and tool-result impersonation through the loop delta |

A follow-on decode round may contain instructions and held material without a new Input
token, within the same externally opened turn. Instructions alone open no new turn. A
contribution cannot open an external turn without AcceptedInput. Only the engine's
emission and granted tool-return paths author assistant and tool-result messages. The
stored prompt is seated by boot, never supplied by a loop contribution. [FLOW-8,
PROMPT-1, STORE-8]

| Current consumer at 2eb5c5a2 | Required replacement at cutover |
| --- | --- |
| bin/worker/dev_loop/mod.rs::drive, contribution, reentry_text | Remove SYSTEM_PROMPT and prompt copies in re-entry, consume the accepted-input token, use certified instruction expansions and HeldMaterial |
| engine.rs::Ports::turn and run_turn | Accept Contribution, author each new source at its sole licensed path, never reauthor a Held contribution |
| engine.rs::Ports::recall and state.rs::Recalled | Return typed ReadEntry via Result, require positive last_turns for conversation, remove pairs[content] reconstruction |
| engine.rs::Ports::session_shape and state.rs::SessionShape | Keep the typed aggregate over new-schema rows, return a typed read result, never interpret a failed seam as empty history |
| engine.rs::wind_down | Use the certified wind-down instruction, never message.user |

The loop stays compiled under bin/worker/dev_loop/. Its template inventory has a
load-checked digest. Retire the compiled system prompt, prompt copies at re-entry and
quote-and-truncate recap builders. Loop-authored content is admitted only through
certified, typed template expansion and its own trace event. Stored content uses
HeldMaterial, never a reconstructed raw Message. These are input invariants, not a
required strategy or pass sequence. The operator ruled this on 2026-10-20. [FLOW-8,
CLASS-2, PLACE-4, PROMPT-1]

The pass for contribution licensing certifies origin, single use, exact checked or held
bytes and template identity. It does not certify the truth of content or a loop's
motive. Evidence is the offered/source/recall identity and template inventory recorded
on the trace. Every row above is a perturbation case in engine tests, including a
changed user byte, a duplicate prompt, a reused token and an uncertified recap. [FLOW-1,
FLOW-8, GATE-1, STORE-8]

## 2. Classifier port and input check, destination section 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-CLASSIFY.** Define a crate-private Ste100Port trait in flow/classifier.rs:
readiness() borrows a descriptor, submit(Ste100Request) returns Submitted or
Busy(request) or PortFailure, and try_result() returns Pending or Completed(Ste100Reply)
or PortFailure. One request may be outstanding. The adapter retains partial transport
internally. It is polled only when ready, except the initial nonblocking call. Its
counter starts at 1 in its own domain and increments only on Submitted. Counter
exhaustion is protocol_failed. Target, result, and codes are TYPES-CLASSIFIER. Reason: a
missing result, a pending result, and rejection are three different facts. [CLASS-4,
CLASS-6, CLASS-8, FLOW-4, R10]

Completed transfers the reply once and releases the adapter's request slot. The table
below is the coordinator's state, not a second transport acknowledgment. The coordinator
retains the reply in ResultHeld and submits nothing further until ste100.result is
Recorded. No new acknowledgment method or wire message is needed. A caller that bypasses
this state is a protocol error. [FLOW-3, CLASS-8, SORT-4]

| Coordinator state | Event | Next state and action |
| --- | --- | --- |
| Idle | submit valid request | InFlight, own request ID |
| InFlight | second submit | Busy with ownership returned |
| InFlight | no complete result yet | Pending, unchanged |
| InFlight | matching Accepted or NotSte100 | ResultHeld until ste100.result recorded, then Idle |
| InFlight | crash, terminal no_result, malformed or mismatched reply | Failed, latch classifier_failed |
| ResultHeld | second submit | Busy(request), keep held reply |
| ResultHeld | unsolicited reply or trace failure | Failed, latch protocol_failed or trace failure |
| Idle | unsolicited result | Failed, latch protocol_failed |
| Any active state | blocking bound expires | Failed, latch gate_bound, cut_bound or decision_bound |
| Any | external halt | Closed, abandon outstanding request with account |
| Failed or Closed | submit | Refuse locally, no reopening |

The double uses a nonblocking UnixStream pair and a scripted queue of exact replies. A
test can hold readiness forever, send one result in fragments, close the peer, or return
any terminal variant. It checks one outstanding request and exact scheme agreement. The
test-only fixture-read/1 dictionary comes from TYPES-CLASSIFIER. Production scheme_id,
dictionary and artifact digest come from FlowConfig.scheme, checked against the adapter
at load. No label-score answer satisfies this trait. The production adapter uses Spec
02's CHANNEL and PORT, preserving this near side. [CLASS-2, CLASS-7, CLASS-8, R10]

The production bootstrap carries the declared model binding unchanged. Spec 02's
MANIFEST-LOCATOR makes its artifact the exact manifest file. The SPU acquires it and its
referenced files. The worker does no directory discovery or model-file preopening and
passes no extra artifact descriptor across exec. Map admission refusal to
StateManagementInvalid using Spec 02 DECLARATION's table and bounded envelope rule.
Retain the checker code and exact detail when the complete encoded lifecycle refusal
fits its existing bound. Do not infer the optional path from checker prose. An
over-bound envelope remains a channel fault, without truncation or a substitute
lifecycle refusal. It keeps EnterFailure::AfterLoad's existing refusal and cleanup
behavior and the original load deadline. [CLASS-2, CLASS-8, PERSIST-6, SEAM-2]

Validate bindings for a Text block without normalizing its bytes: target and scheme,
nonempty ordered statements, valid UTF-8 spans, one or more nouns in each binding within
that binding's statement_span, condition nullability, and verb dictionary membership.
Author each complete Ste100Reply as ste100.result before using it, including template
replies during enter. The load event precedes those replies and records their input
inventory, not future results. The model result target exists even when the partition
did not select that source. Correct rejection then authorizes repair but no filing.
Invalid result shape is a component failure. [CLASS-2, CLASS-9, STORE-3, SORT-4]

**HARNESS-RECALL-REFUSAL.** The model association and repair owners implement
Spec 02 RECALL-REFUSAL before constructing an inward pair or query. The accepted
association account alone supplies the reserved CallId, original Text index, exact
StatementRef and shape facts to TYPES-RECALL-REFUSAL. An Accepted Text reply with a
zero-noun binding fails HARNESS-CLASSIFY before that account exists, even if another
binding shares its statement_span and has a noun. No guessed subject or unchecked
caller vector may construct the operation refusal. [READ-3, CLASS-2, CLASS-8, CLASS-9]

Author the existing refusal kind through HARNESS-AWAIT-AUTHOR, with internal/runtime,
subsystem harness, the same open turn and causal_parent equal to the referenced
statement's message.assistant sequence. This payload is unselected and creates no
filing. The original accepted Text still follows its ordinary selected-copy path. Retain
the exact refusal and its event before the first fallible admission, and preserve
Recorded sequence and continuation state before another recoverable wait. Return the
private disposition once, after Recorded, under the original Decision bound. [FLOW-2,
FLOW-3, CLASS-8, STORE-3]

The repair account keeps Recorded refusals inspectable alongside valid requests without
releasing a refused CallId as a request candidate. A correction containing a ToolCall
or UnrecoveredCall requires one Recorded correction_call disposition. An empty
correction requires its exact Recorded empty_correction refusal under Spec 02
EMPTY-CORRECTION. A correction with defective Text and no attempted call requires no
correction_call disposition. For every failed correction, complete all required
refusal records, including every applicable RECALL-REFUSAL, and every required valid
pair's actual CompletedPair, including opening commits and the terminal Filed receipt,
before charging the one failed correction. Preserve the original call identity and any
corrected statement source independently. Spec 02 CANCELLED-CORRECTION owns the
complete-attempt boundary across Stop/leave: retain exact correction identity and
count once when all required facts exist, before fallible continuation or extraction,
without reopening any cancelled grant or cap continuation. Recorded PairEnded alone
is not a completion receipt. Cancellation and phase permissions follow
RECALL-REFUSAL and HARNESS-RECORD-LIFETIME; no new filing after foundation halt or
renewal of the original wait is authorized. No new terminal reserve,
placement grant, final-answer policy or classifier meaning is introduced. [CLASS-9,
CLASS-10, FLOW-4, FLOW-5, OUT-1]

**TEMPLATE-CHECK.** At enter, enumerate all harness text templates that can reach state,
including wind-down and restatement. Put the inventory identity, scheme and checker
digests in the first load event under TRACE-LOAD. After that event, validate the
semantic partition and activate the recorder. Stand the STE100 duty through the worker's
existing after-load organ start sequence. Check the inventory before the boot content
read and before opening the user channel. Each template is Spec 02's TemplateSpec, with
typed technical_name or step_id slots. The request carries its canonical compact JSON
and a template target. An accepted reply must have bindings:[], unlike accepted Text
replies. Certification proves grammar inclusion for every legal substitution, not
sampled examples or arbitrary certified fragments joined together. Author each exact
reply as a turnless ste100.result in inventory order. Retain those replies as
TemplateCheckTranscript and derive the certificate from them. No result is predicted in
load and no result event precedes it. A failed partition or template check uses
EnterFailure::AfterLoad and its existing refusal and rollback path. A partition failure
uses the preflight recorder without activating a content tap.

The pass certifies only conformance of that inventory and its substitution rule. Its
evidence is the exact inventory bytes, scheme and checker digests, recorded replies, and
accepted template IDs. It does not prove safety or arbitrary runtime strings. Missing
certificate or incompatible scheme refuses enter. A correctly reported runtime template
defect records state_template_rejected, fails that operation, and never invokes the
component halt. Spec 02's TEMPLATES defines the substitution proof and certificate
method. Its production grammar artifact remains a cutover dependency. [CLASS-2, FLOW-8,
review S2]

**HARNESS-INPUT.** Check a request before granting Ports or starting a turn. Keep the
existing world request grammar, {text:String, role?:String}, under parse_request. The
role member must be absent. The former role system is refused, and this unit adds no
role user spelling to the world grammar. Construct internal Role::User with one Text
block from request.text with source index zero, not a new multiblock world format.
Author input.offered with the original base64 frame before decoding/parsing its octets,
including when that parsing will refuse the request. Check that exact text with input_id
and block zero. Correct rejection never authors message.user. Acceptance mints the
AcceptedInput token. HARNESS-LOOPS opens the normal turn and authors message.user
exactly once, reusing bindings only for identical text and scheme. Its causal_parent is
input.offered's sequence, so the recorded input_id joins the accepted result to its
source block without changing Message. [GATE-1, CLASS-4, FLOW-3]

When admin Spec section 3.0's current-position table requires NoResidency for the
request, follow Spec 02 INPUT-NO-RESIDENCY after recording its input.offered and
before parsing or check submission. The table's S4/S5 Unloading rows keep their
precedence over the fact that an SPU is dead. Use the typed
WorldRequest/NoResidency lifecycle refusal and its causal parent, retaining the exact
request and close through backpressure. This road creates no classifier exchange or
AcceptedInput. All already-admitted work retains its existing accounting and failure
rules. [GATE-1, FLOW-2, FLOW-3, CLASS-8]

| Input state | Event | Next state and trace or wire action |
| --- | --- | --- |
| Idle | Serialized check intake receives next queued frame | Offered, assign input_id and start gate deadline |
| Offered | input.offered recorded, shape and dialer accepted | Checking, submit exact Text at block zero |
| Offered | Bad input shape | Refused, record refusal/Input(invalid_input_shape) |
| Offered | Admin section 3.0's current-position table requires NoResidency before parsing or check submission | Refused under INPUT-NO-RESIDENCY, record refusal/Lifecycle(WorldRequest, NoResidency) |
| Offered, Checking or Accepted | Quiesce or operator leave | Cancel this input operation, revoke any token, send unnamed stopped close, account for any outstanding check through its result or bounded teardown |
| Offered or Checking | Component failure or deadline | Halt, no input reaches model |
| Checking | Accepted reply recorded | Accepted, disarm gate_wait and mint the single-use token, no turn yet |
| Checking | ste100.result author returns Pending | Hold exact reply and ticket, remain Checking, no token |
| Checking | NotSte100 | Refused, record ste100.result and refusal/Input(non_ste100) |
| Accepted | Copy selected, bytes and scheme match | Ledger ready_to_file without a second check |
| Accepted | Component failure | Halt and revoke the token, no input reaches model |
| Refused | component failure | Halt, no refusal success fabricated |
| Refused | Trace records complete | Answer refused, return Idle, no turn opened or closed |

For shape or language rejection, the reply uses existing
Payload::Frame(TurnFrame::carry(...)) with UTF-8 JSON
`{"kind":"refused","reason":"Restate the input in STE100."}`. There is no turn or run
member because no turn opened. The fixed response is not filed and is not a model
restatement instruction. Input offered while a turn runs remains queued by existing turn
ordering, with its check deadline starting when handed to the check, not when a dialer
first queued. Within the check, classifier queue delay counts. Diagnostic binding never
enters this table. [GATE-1, FLOW-3, COST-4]

For leave or component failure, use Spec 02 INPUT's unnamed stopped close and reason.
The input token is never granted after cancellation, even if a pending check later
accepts. Keep that exchange in the coordinator until its recorded disposition or the
foundation's bounded teardown. Use Spec 02 INPUT's exact deadline-disarming rule. The
input fixture exercises leave before submit, while checking and after acceptance but
before token consumption, then delivers a late valid reply. Removing the
cancelled-operation guard or authoring message.user before token consumption must make
it fail. [GATE-1, CLASS-8, FLOW-4]

## 3. Ledger, channel, and waits, destinations sections 2 and 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-LEDGER.** File in trace sequence and original block-index order. Later results
may be held, but no later selected source overtakes an earlier obligation. Zero
projected blocks create a count-zero observation and no ledger entry. Text projection
never sends tool blocks here. [STORE-5, TAP-4, FLOW-4]

| State | Event | Next state or action |
| --- | --- | --- |
| absent | Selected Text, no reused check | awaiting_classification |
| absent | Selected accepted input, identical checked bytes | ready_to_file |
| absent | Selected PairCopy, valid licensed pair payload | ready_to_file as PairFile, with no classifier request |
| awaiting_classification | Valid acceptance recorded | ready_to_file |
| awaiting_classification | Correct rejection recorded | correctly_rejected, no filing |
| ready_to_file | Earlier obligations disposed and channel Idle | sending, freeze frame and ID |
| sending | Partial write or WouldBlock | Retain bytes and offset |
| sending | Full request sent | awaiting_receipt |
| sending | Any reply before full send | protocol_failed, halt |
| awaiting_receipt | Matching filed and valid observation recorded | filed, acceptance established |
| awaiting_receipt | Matching refused | filing_refused, halt |
| any channel state | Unsolicited, wrong-ID, malformed, or duplicate reply | protocol_failed, halt |
| any pending entry | Another source sets HaltLatch | abandoned, preserve prior state and commit uncertainty |
| any pending entry | Bound expires | Halt with wait reason and blocked stage |
| any transport | EOF or terminal write failure | carry_failed, halt |

The ledger owns a typed StatementFile or PairFile. Only statement entries can become
correctly_rejected. PairFile uses file_pair and the same source order, single writer,
request counter, partial-write handling and committed receipt. A parse failure of a
copied pair is a component fault. It cannot become a conformance rejection. [PAIR-3,
STORE-2, SEAM-2]

Filed and correctly_rejected are terminal. No reconnect or resend resolves an unknown
commit. The state request counter is distinct from classifier and input counters. The
channel's control state table is in the store/read and persistence amendments. [SEAM-2,
FLOW-5]

**HARNESS-PUMP.** Extend exchange_wake and lifecycle wait helpers with state I/O, STE100
readiness, label-score readiness, and trace capacity wake. All serving waits on
StateSeam use this pump: ask_shape, ask_identity_within, ask_recall, ask_recall_within,
ask_replay, ask_grants, restored, and snapshot continuations. Removed serving asks fail
locally as specified by the read amendment. Ports::classify keeps its API and 30-second
bound but replaces recv_answer_by with the common pump. No gate or cut deadline can
disappear inside that retained call. [CLASS-8, FLOW-4]

The pump owns each transport's only read cursor and its partial-frame buffer.
ClassifyChannel::recv_reply_within and recv_answer_by in channel.rs, and
StateSeam::await_line_until, called by await_answer in state.rs, cannot read their
descriptors after the pump has taken ownership. Their serving replacements wait on the
source's completion slot. exchange_wake, wait helpers, engine decode receives and gate
receives use the same rule. A nested author_and_wait may fill a slot but cannot consume
the caller's answer. Tests hold a full trace queue while each source answers, then prove
exactly one reduction and one receipt after capacity returns. [FLOW-2, FLOW-4, CLASS-8]

Service lifecycle and terminal failures first, then one bounded work unit per ready
source in rotation. Process buffered complete frames before polling. Do not read another
completion from a source whose slot is full. Reduce held completions when the pending
author slot is empty, and stop reduction while its next required record waits for
capacity. The timeout is the nearest armed absolute deadline across enter, leave, gate,
cut, decision, state control and legacy label waits. A ready source cannot reset
deadlines or prevent their check before and after each work unit. No harness thread or
async runtime is introduced. [CLASS-6, CLASS-8, FLOW-4]

**HARNESS-CLOCK.** Capture CLOCK_MONOTONIC origin_ns before state open at enter, and
give that same origin to Author and the pump. Record clock_source and origin_ns once in
load under Spec 09's CLOCK. Envelope monotonic_ns remains elapsed from that origin.
Cross-process observations use raw readings of the same Linux clock, never serialized
Rust Instant values. Under HARNESS-ENTER-WINDOW, the inherited admin load budget
starts at ADMIN-ENTER-WINDOW's exact dispatch anchor and its original EnterWindow
crosses Enter. Each stage uses only the time remaining before that same deadline.
Gate wait starts at check intake. Cut wait starts at the between-turn filing barrier,
after the preceding turn closed and before
any decode work for the next turn. decision_wait comes from state_management.ste100 and
starts when an output check, required send/terminal receipt or an ordinary placement
measurement blocks the next decision. It ends only when that barrier's disposition is
recorded. Spec 02's DECISION-BARRIER leaves it disarmed while the decoder generates a
correction or the gate executes a tool. Each armed barrier has one deadline, never
renewed by partial progress. All three use declared elapsed-ns with checked addition.
Delay below the bound is only a measurement. Pending at the bound is missing. The gate's
tool execution clock remains its only clock. [CLASS-8, GATE-1, MARK-4, R7]

PumpIo is a crate-private test interface for both now() and wait(descriptors,
absolute_deadline). now returns monotonic nanoseconds. Production uses
clock_gettime(CLOCK_MONOTONIC) and poll(2), recomputing the remaining timeout on EINTR.
Checked arithmetic rejects overflow. The fake implementation advances the same numeric
clock and returns scheduled readiness or Timeout together. A fake clock never attempts
to wake a real poll. Also exercise the real poll with a short test-only deadline and an
outer test watchdog. [CLASS-8]

**HARNESS-ENTER-WINDOW.** The replacement Serving receiver retains the
required original EnterWindow and its existing exchange and descriptor custody. Sample
received_ns from the common raw clock after the complete packet/descriptor receive and
before JSON decoding. Structural/encoding violations of TYPES-ENTER-WINDOW follow the
existing malformed-envelope channel fault; all received descriptors remain accounted
for on that road. A structurally valid began_ns later than received_ns is malformed
Enter, not an allowance to wait until that anchor: use the existing before-load
Malformed refusal on its valid exchange, without state open or a fabricated load.
The same clock domain is a producer/consumer precondition, not a promise that arbitrary
foreign timestamps can be detected or translated. [CLASS-8, FLOW-3, SEAM-2]

After decoding, and before and after every admission/boot work unit, compare the
current raw reading with the retained deadline. At now >= deadline_ns a well-formed
Enter is expired even if receipt was earlier. Queue, dispatch, parsing and constructor
delay all consume the window. An expired Enter permits no new state-open/decode-Open
byte, ordinary boot work, template grant or Ready. It is an elapsed wait, not by itself
a component halt or configured-fit failure. Do not turn it into boot_failed,
StateManagementInvalid, a new refusal code or a renewed interval. [CLASS-8, FLOW-3,
PLACE-4]

Before load, expiry follows the existing unadmitted/before-load cleanup and leaves no
load or refusal record manufactured to explain a run that never stood. Account for and
close only this unadmitted Enter's owned descriptors; retain the lifecycle position for
its actual before-load rollback outcome. After load, retain the same context, original
window and all actual owed author/read/pair/Open/process evidence in the entered
bracket for explicit rollback. An expired exchange produces no substitute lifecycle
answer. Admin owns its existing unanswered-load result and directs rollback under
ADMIN-ENTER-WINDOW. Actual component failure still follows HARNESS-HALT with its first
cause; expiry cannot erase it. Neither path fabricates Released, LastWord, process
exit, Unload or successful cleanup. [CLASS-8, FLOW-3, FLOW-5, PERSIST-6]

Initialize the private Enter bound from the validated original began_ns/deadline_ns,
without reconstructing deadline_ns from a later trace origin or sampled remainder.
Keep the trace's separately captured origin_ns and elapsed envelope semantics unchanged.
A trace origin observed after deadline means expired Enter, not a reversed original
interval. Preserve original bounds across every owner transfer. Only the actual
accepted rollback Leave may invoke its existing independent cleanup authority; it
never rewrites Enter to that allowance or admits a late Ready. Any already owed
account is retained or settled under its existing applicable bound and disposition,
not silently discarded to make expiry look complete. [CLASS-8, FLOW-3, FLOW-5]

The existing harness Spec's rollback-Leave rule applies even if the worker sent
Ready before the deadline and admin received it at or after the deadline. Admin's
late-answer refusal does not turn that Ready into an accepted load. Once the actual
forced Leave with rollback true is accepted, the worker follows rollback by the
directive, regardless of its own entered position or earlier Ready send. No new
ordinary work is admitted after that acceptance; retain and unwind actual owed work,
run no wind-down, take no save point, and release only what stood. Where a load
bracket stands, its existing terminal rules apply with the load's cause. The accepted
Leave supplies its existing cleanup allowance, never a new Enter window.
[FLOW-3, FLOW-5, PERSIST-6]

| Enter observation and worker position | Event | Required disposition |
| --- | --- | --- |
| Before load; original window expired | Current time reaches the retained deadline | No load or substitute refusal; retain and close actual unadmitted resources under existing before-load rollback. |
| Load bracket stands; boot has not answered Ready | Original window expires | Keep the same entered context and owed work for explicit rollback; no new ordinary boot work or Ready. |
| Ready was sent before the cap; worker is Entered; admin receives it late | Actual forced Leave with rollback true is accepted | Follow the existing any-position rollback rule; no new ordinary work after acceptance, no wind-down or save point, and no renewal of Enter. |

| Claim | Required instrument | Protective change that must fail |
| --- | --- | --- |
| TYPES/ADMIN/HARNESS-ENTER-WINDOW | Canonical closed carrier and real seqpacket/process fixture with original admin anchor, delayed preparation/carry/parse and actual descriptor ownership | Default a missing window, re-add sampled remaining time at receive, restart at trace origin or lose transferred descriptors |
| Original expiry | Exact-deadline receive, before first work byte, after owner admission and late Ready controls, with actual rollback/account custody | Treat expiry as malformed or boot_failed, accept late Ready, renew the cap or fake completed cleanup |
| Shared clock and bound | Real poll with outer watchdog plus controlled raw clock at wake/decode, including EINTR | Treat readiness as timely completion, accept a future anchor or wait on another full duration |

**HARNESS-BARRIERS.** prepare_cut(required_ids) consumes FlowInbox and waits for pending
authoring and each required source's disposition. prepare_save() freezes its own trace
boundary under HARNESS-CAPTURE, consumes FlowInbox and waits for pending authoring and
each owed block through that boundary. Both require filed or correctly_rejected, never
a maximum sequence alone. Unknown selected IDs are protocol_failed. Save freezes
admission at a turn boundary, includes wind-down output, and uses the declared cut_wait
as its filing-pause bound even when wind_down_bound is absent. The earlier of leave and
filing-pause deadlines wins. No snapshot or long read starts while a gate/cut barrier is
armed. A save completes or fails before another model turn starts. [MARK-4, PERSIST-1,
PERSIST-4, CLASS-8]

**HARNESS-CAPTURE.** The capture frontier is a harness-local trace boundary, distinct
from the member's captured SourcePosition. The recording context retains the greatest
sequence whose author operation has returned Recorded in its current run, including
enter, serving and leave. Every such completion advances this watermark through the same
recording owner, including load, Boot and Read recall, ordinary authors,
classification/state measurements and flow observations. Partition selection,
RecordPurpose and the reducer that receives the completion do not change this rule.
CapacityPending has no sequence and advances nothing. Preserve the admitted sequence
before any later fallible clock or lifecycle check. The watermark proves trace
admission, not filing or sink survival. [FLOW-2, FLOW-3, FLOW-6, PERSIST-4]

At the between-turn boundary, freeze new content admission and settle any pending
content author before choosing the frontier. prepare_save freezes the current run and
that Recorded watermark once, after SavePointBeginning has been carried. The caller
cannot supply a smaller sequence or substitute a loaded run's position. The successful
Recorded load supplies an initial watermark even when this run has filed no content.
All blocks of an event at the frozen sequence lie within the boundary. Admission stays
frozen until capture completes or its required cleanup proves that it can be released.
[FLOW-2, PERSIST-1, PERSIST-4, STORE-5]

Drain every selected source through that boundary to filed or correctly_rejected,
including queued copies and blocked carry. A selected source from the current run
beyond it is protocol_failed at barrier. A maximum sequence alone never discharges a
source obligation. An untapped observation needed to finish filing, capture or cleanup
may be Recorded after the freeze. It advances the live watermark but never changes the
frozen frontier, admits content or resets a deadline. A later capture chooses the then
current watermark. Existing author serialization, phase and failure restrictions still
apply. [FLOW-3, FLOW-4, TAP-3, SEAM-2, CLASS-8]

The member's image position remains its last committed statement or pair source, or
null for a seed without operating content. Neither recall nor an operation observation
creates that position. The capture watermark is not a new wire field, image position,
operation ID, store counter or claim that untapped trace records are in the dump.
STATE-OBSERVE independently determines which operation metadata precedes capture and
therefore belongs to the image. No party rewrites the image to include the snapshot's
own observation. [PERSIST-3, PERSIST-4, STORE-5, COST-2]

**HARNESS-SAVE-TIME.** At the serving SavePoint handler, require TYPES-SAVE-TIME's
window and compare its duration with loaded persistence.save_wait. Its began_ns cannot
be later than the receiving pump's raw clock reading. Retain the absolute values while
the current turn runs. An expired queued save follows PERSIST-EXPIRY, not a component
failure. At the boundary carry SavePointBeginning before prepare_save. The pump includes
the same deadline while carrying that frame, authoring the barrier, filing and receiving
the save continuations. [PERSIST-1, PERSIST-4, CLASS-8]

For a leave, start no save timer in hold_leave, drain_gate or wind_down. After
lower_gate, before the requested save record and prepare_save, create the capture window
and carry Beginning with the actual shared leave window. Capture is skipped on rollback
or a latched flow. If the shared tail is already exhausted, use the existing
failed-leave/seal path with no save claim. Never create a fresh tail or an invalid
zero-length window to attempt capture anyway. [PERSIST-4, FLOW-5, admin Spec 3.0,
harness Spec 6]

After Published, complete release_capture and the published trace record within the
capture window. On demand answer SavePointTaken there. On leave carry
SavePointCaptureComplete before disarming only the capture window, then continue
release, unload and Left under the existing shared tail. A partial completion write
retains its bytes and does not disarm the window. A force join keeps the same capture
and can shorten but never restart a deadline. The measured wait values remain
qualification obligations. [PERSIST-4, FLOW-5, COST-4]

For a refused pre-Commit leave capture, follow foundation PERSIST-LEAVE-REFUSAL.
Only proven member cleanup and Recorded refusal permit the worker's matching
SavePointCaptureRefused acknowledgement. It ends no Leave. Preserve the shared tail
for its carry even when cleanup finishes after the capture window. Admin's refusal-only
wait uses that same original tail and its original outer deadline as foundation
PERSIST-LEAVE-REFUSAL specifies. Capture expiry grants no late save success. Keep the tail
through release and the real final response, Left without a save report for a forced
leave or
SavePointNotTaken at S9 for a leave that stayed graceful. A refused capture never sends
CaptureComplete. Admin loss does not renew a wait or create delivery evidence.
[PERSIST-4, FLOW-5, admin Spec 3.0, harness Spec 6]

The fixture drives a graceful S4/S5 longer than save_wait with either lifecycle bound
absent and proves no save timer or Beginning exists yet. It then lowers the gate and
checks the original tail plus the fresh capture window. Other cases fragment
Beginning/Complete, queue an expiring on-demand save, and delay a leg without allowing
its anchor to reset. Removing each phase/correlation guard must break its assertion. The
same fixture exercises R20-6 timeout/retry and R20-7 broken-publication outcomes, with
no snapshot-missing halt or recovery publication. [CLASS-8, PERSIST-4, FLOW-5]

**HARNESS-CONTROL-FAILURE.** A dead peer or malformed control answer remains component
failure. A missing snapshot or finished answer enters PERSIST-CLEANUP before any later
state ask. The sole owner keeps the barrier, queues abort on the original request and
capture IDs, and waits for matching aborted under the existing two-second small-control
bound from abort enqueue. Matching late snapshot, finished or capture refusal answers
are validated and their observations recorded, but discarded as save results. They
neither reach admin nor restart the cleanup bound. Partial writes retain their offsets.
[PERSIST-4, CLASS-8, FLOW-4, COST-2, R20-6]

A matching admin Aborted before Commit follows the same member cleanup ownership.
Before snapshot is enqueued, finish owed authoring and filing without starting capture.
With snapshot or acknowledge still outstanding, or a held image awaiting ImageChecked,
abort on the original exchange. After a timely finished exchange completes, use fresh
release_capture. Preserve a pending confirmation author's original event and ticket,
but send no Commit after cancellation. Keep the existing small-control cleanup bounds.
Admin preparation refusal names Published, unless the worker already retained an
Answer or Finished refusal. The first retained attribution wins. [PERSIST-4, SEAM-2]

RefusingCapture owns that attribution and the incomplete cleanup/accounting. It is
entered from explicit refusal, from cancellation before any member request, or after
the abort/release proof. Only completed cleanup and a Recorded refusal permit
CarryCaptureRefused. A first admin Aborted may cross the worker's own refusal, including
the acknowledgement carry. Retain it without a second cleanup, refusal record or
acknowledgement. A second Aborted is protocol failure. Foundation's actor rows own the
exact transitions and After Commit treatment. [PERSIST-4, FLOW-4, SEAM-2]

While Aborting, only aborted proves Ready and permits barrier release and the final
refusal disposition. On demand that is SavePointNotTaken and operator retry. On leave,
PERSIST-LEAVE-REFUSAL keeps the accepted lifecycle outcome owed. Failed carry,
dead peer, missing or invalid aborted, cleanup refusal or failed room-file deletion
is component failure. R20-6 still forbids halting merely for
the original image/finished timeout. The snapshot allowance is the loaded
persistence.snapshot_wait from TYPES-CONFIG, initially declared as 120 seconds with no
silent compiled fallback. The finished allowance stays two seconds. Cleanup neither
extends the save/leave windows nor permits publication after expiry. A valid explicit
save refusal remains an operation result. [PERSIST-4, CLASS-8, FLOW-4, R20-6]

Component failure names the halt category, not a separate FlowHaltReason. A refused,
malformed or wrongly correlated cleanup uses protocol_failed at state_reply. An expired
cleanup wait uses control_bound at barrier. EOF or failed carry uses carry_failed at
state_reply or state_send, according to where it is observed. A member delete failure
must not produce aborted and follows the observed EOF or missing-result case. All use
the same HaltLatch and failed-run recovery, with no retry or successful leave. Reason:
the category decides whether serving stops, while the reason records what broke.
[FLOW-4, FLOW-5, CLASS-8]

| Save-control state | Event | Action and next state |
| --- | --- | --- |
| Beginning or Barrier, leave | First matching admin Aborted | Retain Published and its code/detail, settle partial Beginning and owed authoring/filing, no snapshot, RefusingCapture |
| AwaitImage, AwaitChecked or AwaitFinished, leave | First matching admin Aborted | Retain Published and its code/detail, keep barrier, abort original request/capture, Aborting |
| AwaitPrepared or AwaitRecorded, leave | First matching admin Aborted before Commit | Retain Published and its code/detail, settle admitted confirmation without Commit, release_capture after the finished exchange, ReleasingRefusedCapture |
| AwaitImage or AwaitFinished | Original result bound expires | Keep barrier, enqueue correlated abort, Aborting |
| Aborting | Expected late snapshot, finished or capture refusal for the original leg | Validate and record observation, discard result, remain Aborting |
| Aborting | Matching aborted | Retain cleanup proof, RefusingCapture |
| ReleasingRefusedCapture | Matching capture_released | Retain cleanup proof, RefusingCapture |
| RefusingCapture | Member holdings remain and no cleanup is pending | Abort the original request before completed finished, or fresh release_capture afterward, Aborting or ReleasingRefusedCapture |
| RefusingCapture | Required cleanup and admitted authoring/filing settled | Record retained refusal, release barrier once Recorded, CarryCaptureRefused on leave or SavePointNotTaken on demand |
| CarryCaptureRefused | Whole acknowledgement carried | RefusedLeave under the unchanged shared tail, forced Left or graceful S9 as foundation specifies |
| CarryCaptureRefused | Partial carry or WouldBlock | Retain exact bytes and offset under the unchanged shared tail |
| CarryCaptureRefused | Admin disconnected | Retain undelivered acknowledgement, RefusedLeave under the same shared tail, no invented response |
| Any pre-Commit refusal state, leave | First matching admin Aborted crosses worker refusal | Retain it without replacing attribution, another cleanup or another acknowledgement |
| Any pre-Commit refusal state, leave | Second admin Aborted or wrong capture | Protocol failure, no successful Leave |
| Aborting | Partial carry or WouldBlock | Retain bytes and deadline, remain Aborting |
| Aborting | Cleanup bound expires without aborted | Latch control_bound at barrier, no further ask or successful leave |
| Aborting | Refused, malformed or wrongly correlated cleanup | Latch protocol_failed at state_reply, no further ask or successful leave |
| Aborting | EOF or failed carry | Latch carry_failed at state_reply or state_send, no further ask or successful leave |
| ReleasingRefusedCapture | Partial carry or WouldBlock | Preserve original release bytes/deadline, remain ReleasingRefusedCapture |
| ReleasingRefusedCapture | Expiry, invalid result, failed carry or dead peer | Use the same small-control failure categories as the abort rows, no successful leave |

These are the foundation's capture-state projections, not a second channel owner or
timer policy. During Aborting, no ordinary file/read enters the channel until aborted.
Tests delay the snapshot answer in CaptureHeld and the finished answer in FinishedHeld,
then prove a subsequent filing and retried save both succeed without wrong_phase. They
also exercise a late explicit capture refusal, wrong capture ID, deletion failure and
silent cleanup.
Admin Aborted is exercised in every pre-Commit phase and crossing each refusal state,
including the no-member-request case and the fresh release after finished. No callback
may infer cleanup from receipt of Aborted. CaptureRefused after capture expiry can be
timely under the original shared-tail and outer leave deadlines, and grants no save
report. After either remaining deadline expires, Unanswered and the Open marker remain.
A still-graceful orphan stays in S9.
Removing the abort, correlation, barrier or cleanup-bound guard must fail its case.
[SEAM-2, PERSIST-4, FLOW-4]

## 4. Halt and lifecycle, destinations sections 3 and 7

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-HALT.** HaltLatch owns the first reason, source, outstanding entries and
pending author ticket. It revokes decisions before trace reporting. FlowInbox only
notifies it. Add Event::FlowFailed and Response::EndServingWithoutSave. For serving,
replace every MemberDies cell below with that same response. Keep diagnostic cells on
their old table. Split LegMissed from MemberDies where they shared an arm. [FLOW-4,
FLOW-5, October 18 R3]

| Position | Serving MemberDies or FlowFailed | Old response retired |
| --- | --- | --- |
| Enter | Refuse enter and rollback | No serving confirmation |
| S2, S3 | EndServingWithoutSave | ServeOnSeamRetired |
| S4, S5 | EndServingWithoutSave | LeaveGoesOnSeamRetired |
| S6 graceful, S9 | EndServingWithoutSave on member death or flow failure | StaysS9MemberDead. StopInS9 on a missing result remains the merged R20-6 retry |
| S6 forced, S10 | EndServingWithoutSave on member death or flow failure | ForcedMissToS7 on member death. Missing-result behavior retains R20-6 retry |
| S7 | EndServingWithoutSave, finish bounded release | MemberUnconfirmedAtRelease |

StateSeam EOF/send/parse handlers, control adapters, take_save_point, and member release
reporting must dispatch these events instead of retiring silently. Remove the table's
dead_code rationale that MemberDies bypasses respond. Gate/decoder events keep their
existing outcomes unless they also produce a flow component failure. [FLOW-4, FLOW-5]

| Halt phase | Action | Next phase |
| --- | --- | --- |
| Running | First failure latches, stop tap and decision permissions | Latched |
| Latched | Cancel unconfirmed save, retain ledger, capture and pending author account | Reporting |
| Reporting | Author fault/StateFlowFailed and one refused save_point for an unconfirmed pending capture through reserved entries where trace works | Interrupting |
| Interrupting | Send ToolInterrupt{by:flow_failure}, await gate containment within halt deadline | Closing |
| Closing | Close open turn Stopped/fault, answer caller stopped if reachable, lower gate | Releasing |
| Releasing | Settle or abandon HARNESS-OPEN's owed account under its earlier bound before retiring the decode transport; release SPU, close state and classifier, then author unload with end_reason component_failure | Draining |
| Draining | Bounded trace drain, return ChannelFault::Closed | Failed worker exit |
| Any halt phase | Further failure | Preserve first cause, append available evidence, never restart teardown |

**HARNESS-RELEASE-ABSENT.** Retain whether the harness
actually created an SPU child for this run through healthy rollback and Failed
cleanup. A typed classifier admission refusal before SPU fork and a real failure
before SPU fork both leave that fact false. A successful SPU fork makes it true,
including later exec, Load or Open failure and a child already reaped. Port
retirement never resets it. Do not start an SPU merely to obtain a release outcome.

At the foundation cutover, every new Unload still carries `release`. Use the
paired trace amendment's TRACE-RELEASE-ABSENT optional `release.spu`: omit it only
when no SPU child was created; otherwise retain the
exact existing Confirmed/Unconfirmed rule. Retain the member's independent actual
Closed/Unconfirmed/absent disposition. Absence creates no Release exchange, death
fault, new terminal permit, successful Left or additional time. A healthy refusal
still waits for explicit rollback and uses its actual forced/cause facts. A real
halt still preserves its first failure, original cap and component-failure
account. Existing bounded cleanup and trace-drain duties are unchanged. [FLOW-3,
FLOW-4, FLOW-5]

HARNESS-OPEN supplies one additional trace-only author permit for an Open already
sent before cancellation or failure. It may finish that exchange's owed boot placement
account while trace and the earlier enter/teardown bound permit. It is not ordinary
author admission, a new decoder request or permission to initialize the journal. Keep
the first failure and the existing terminal obligations. This account uses ordinary
writer capacity, never one of TRACE-CAPACITY's seven reserved terminal entries. It does
not delay mandatory terminal work. An unavailable account does not delay expiry or
clear the latch. [FLOW-3, FLOW-4, FLOW-5, PERSIST-6]

ToolInterrupt is amended in types and harness-gate contract. A live gate reports
FlowFailure only after containment. Fault retains its existing gate-death meaning and
indeterminate outcome. Neither is automatically rerunnable under the unload rule. The
halt uses existing containment, not a second tool-execution clock. A missing trace
report does not clear the latch. No snapshot, successful Left, or worker-written admin
marker is permitted. [FLOW-5, PAIR-2, R3]

After a valid Commit, retain the capture facts in the halt account without inventing a
refused or published outcome. R20-7 forbids admin from finishing the broken publication.
Ending the worker does not answer that separate question. [FLOW-5, PERSIST-4, R3]

**HARNESS-HALT-BOUND.** Set one absolute teardown deadline at the first latch using the
existing 150-second post-turn leave allowance. If a leave deadline is already armed, the
earlier deadline wins. Every interrupt, lower, release and trace-drain wait uses only
the remaining time. Run no wind-down, model decode or ordinary filing after the latch.
At expiry, stop waiting, close descriptors and exit failed. Use the existing gate-death
containment and cgroup cleanup duties where a gate cannot prove containment. Never
fabricate a contained tool result to finish sooner. The existing 30-second tools
containment bound is capped by the remaining teardown time, and admin's later escalation
remains its own custody duty. This is a shutdown bound, not a second clock choosing the
outcome of a running tool. [FLOW-4, FLOW-5, CLASS-8, COST-4]

Normal leave still reads grants through the sole owner and compares it with enter. At a
flow halt, do not send a grants ask behind an unsettled filing or on a failed channel.
The unload then carries the existing GrantSurface::Unreadable value and the fault
account names the unavailable state seam. It never substitutes the old enter reading as
a fresh observation or blocks teardown on that read. [FLOW-3, FLOW-5]

| Source | Reason and point | Required correlation in the account |
| --- | --- | --- |
| Projection or reportable copy allocation error | tap_failed, tap | Source event, source block only when known, detail from TapFailure |
| FlowInbox reservation or conversion fails | tap_failed, copy_intake | Entire unheld_copy identity set, source event, no invented ledger acceptance |
| State EOF, zero write, terminal I/O error, oversized unsent filing | carry_failed, state_send or state_reply | Outstanding request and source if any, exact byte progress, uncertain commit where appropriate |
| Matching selected filing refusal | filing_refused, state_reply | Request, source, refusal code/detail, commit refused |
| Classifier crash, terminal no_result, malformed/mismatched result or transport error | classifier_failed, classifier | Outstanding STE100 request and target, input_id or source where applicable |
| Unsolicited classifier reply | protocol_failed, classifier | Received request when decodable, no fabricated outstanding target |
| Malformed, duplicate, premature or wrong-ID state reply | protocol_failed, state_reply | Expected and decodable received request, source if known, never mark filed |
| Gate, cut, decision or undisputed control bound expires | Corresponding bound reason, barrier | Wait stage, original start/deadline and observation time, source or input where known |
| Whole-copy backlog still blocks next model request at decision_wait expiry | queue_limit, barrier | Waiting copy, original decision deadline and current backlog sample, never an immediate-full failure |
| Harness counter overflow | protocol_failed, counter | Counter domain in detail, current source/request where known, no wrapped ID |
| Busy misuse (excluding the HARNESS-HELD-BOUNDARY recovery refusal), invalid ticket or unknown barrier source | protocol_failed, author or barrier | Pending author or supplied source identity, detail names the failed check |
| Trace admission, render, append, queue or sink failure | trace_failed, author | Pending author where present, source event only after admission. Keep existing StreamWriteFailed fault case |

R20-6 keeps missing snapshot/finished answers on the retry path after PERSIST-CLEANUP.
The control-bound row applies to missing aborted, not the original image/finished
timeout. HARNESS-CONTROL-FAILURE assigns the cleanup halt reasons: protocol_failed for
refused or malformed cleanup, control_bound for its expired wait, and carry_failed for
EOF or failed carry. Each is a component failure with the same halt and recovery path.
Retain received code and detail where available. The first failure wins. Later
observations cannot replace its cause or revive the flow. [FLOW-4, FLOW-5, CLASS-8,
October 18 R3]

A correctly reported NotSte100 never enters this table. A refusal of a filing is a
broken caller/store contract, distinct from classifier conformance. [CLASS-8, FLOW-4]

**HARNESS-HALT-ACCOUNT.** flow/report.rs owns the serialize-only FlowHaltAccount carried
as FaultReport.account. The floor's RawValue carrier is unchanged. All fields below are
present, with JSON null for absent options, and counters use DecimalU64. The fault's
ordinary trace envelope supplies the reporting run and turn. No account creates a new
source identity. Reason: an operator must be able to distinguish an owed block, an
unrecorded event and an uncertain commit without guessing from the halt code. [FLOW-3,
FLOW-4, FLOW-5, COST-5]

| Field | Type and meaning |
| --- | --- |
| reason | FlowHaltReason, closed spellings from the failure table |
| point | tap, copy_intake, author, state_send, state_reply, classifier, barrier, counter or teardown |
| detail | String naming the failure, including a received refusal code when present |
| observed_ns | Option<DecimalU64>, first latch's raw CLOCK_MONOTONIC reading, null only if that read fails |
| source_event | Option<{run:String,sequence:DecimalU64}>, a recorded event |
| source_id | Option<SourceBlockId>, a known block in that event |
| input_id | Option<DecimalU64>, an offered input not yet assigned a source block |
| request | Option<RequestAtHalt>, the outstanding request |
| received_request | Option<RequestAtHalt>, only an identity decoded from the offending reply |
| wait | Option<WaitAtHalt>, the blocked wait when one stands |
| outstanding_blocks | Vec<BlockAtHalt>, every pending ledger entry in filing order |
| queued_copies | Vec<CopyAtHalt>, selected inbox copies not yet installed in the ledger |
| unheld_copy | Option<CopyAtHalt>, a selected copy that failed the whole-copy intake |
| pending_author | Option<AuthorAtHalt>, the event still without a trace sequence |
| outstanding_pairs | Vec<OutstandingPair>, as HARNESS-ACCOUNT defines |
| capture | Option<CaptureAtHalt>, factual capture progress only |
| flow_sample | The final FlowObservation under HARNESS-ACCOUNT and Spec 09 FLOW-COUNTS |
| secondary | Vec<SecondaryFailure>, empty in the first fault and accumulated for unload |

FlowHaltReason is tap_failed, carry_failed, filing_refused, classifier_failed,
gate_bound, cut_bound, decision_bound, control_bound, protocol_failed or trace_failed or
queue_limit. RequestAtHalt is {domain,id}, with domain state or ste100 and id
DecimalU64. WaitAtHalt is {stage,began_ns,deadline_ns}, with stage gate, cut, decision
or control and raw monotonic DecimalU64 readings. A bound reason requires its own stage
and observed_ns at or past deadline_ns. Other failures may retain the armed wait without
claiming expiry. received_request is null when no complete identity can be decoded.
Source and event identities must agree where both are present. A template target has
neither source_id nor input_id, and detail names its template ID. [CLASS-8, GATE-1,
FLOW-4]

queue_limit requires stage decision, the same active decision_wait deadline and a whole
copy still waiting for space at expiry. It never means ordinary lag or an immediate
full-queue refusal. The pending waiting copy appears in unheld_copy on the halt account,
while the last flow sample retains waiting_copy. [R20-2, CLASS-8, FLOW-4]

**HALT-ACCOUNT-SIZE.** The terminal reserve is seven queue entries, not seven fixed-size
byte buffers. Its first fault contains one finite account bounded by the configured
ledger block/byte limits plus the one waiting copy, one pending author event, one
request/reply identity and the finite active-pair ledger. At latch, freeze those sources
and use the actual escaped JSON lengths in a checked counting-serializer pass to
determine account_bytes before allocation. Refuse arithmetic overflow as trace failure.
Do not truncate identities, split one account into more terminal entries, or apply the
control-answer byte limit to the trace. [FLOW-3, FLOW-5, COST-2]

Qualification measures the worst permitted ledger with maximal escaped source identities
and the largest admitted pending event, including serialization of both fault and unload
accounts. Account allocation failure is trace failure and preserves the original halt,
never permission to keep serving or fabricate a complete account. The terminal-reserve
test includes this largest fixture and asserts one record per reserved entry. A pass
proves that bound for the qualified inputs, not an unconditional allocation guarantee
under process exhaustion. [FLOW-5, CLASS-6, COST-2]

BlockAtHalt is {block_id,content_type,state,request_id,bytes_sent,frame_bytes,commit}.
content_type is statement or pair. state is the prior ledger state, one of
awaiting_classification, ready_to_file, sending or awaiting_receipt. request_id and
frame_bytes are nullable DecimalU64, and bytes_sent is DecimalU64. A frame's count
includes its delimiter. commit is not_requested before any byte is sent, not_committed
while only an incomplete request has crossed, unknown after the complete request without
a valid matching result, committed after a fully valid filed result, or refused after
its matching refusal. A valid result held for trace authoring may establish this
evidence before granting a decision. A malformed observation or wrong identity
establishes neither committed nor refused. The failed entry remains in this snapshot
even when its next state is abandoned. No status permits resend. [SEAM-2, FLOW-4,
FLOW-5]

CopyAtHalt is {source_event,partition_rule_id,content_type,sources}. sources is the
ordered Vec<SourceBlockId> from the held copy. The ledger, queued_copies and unheld_copy
are disjoint. The original copy stays held until reporting/teardown and is never
retried. AuthorAtHalt is {ticket,event}, with ticket DecimalU64 and event holding
AuthoredEvent's six members: kind, subsystem, turn, source, causal_parent and payload.
The optional members are null. Kind/subsystem use trace spellings and payload keeps its
original JSON. source is externally tagged boundary_input:u32 or management_input:Cause,
or the unit string model, tool, loop or runtime. The nested event has no sequence and
cannot be mistaken for admitted residue. [FLOW-2, FLOW-3, FLOW-6]

CaptureAtHalt is {capture_id,state,commit_sent,published}. Its ID is DecimalU64. state
is the harness's PERSIST-SAVE state in snake case, from queue_boundary through
completing. Allocation enters queue_boundary even while requested-event authoring is
pending. commit_sent records complete Commit transmission, never a prepared artifact or
attempted send. published is nullable StartIdentity and is present only after a valid
Published answer. These facts identify the broken capture under R20-7. SecondaryFailure
is {reason,point,observed_ns,detail} using the same definitions. Later failures append
only that evidence and cannot change the first snapshot. If a trace failure starts the
halt, use StreamWriteFailed with reason trace_failed. Other listed failures use
StateFlowFailed. The final unload carries the same account through TRACE-HALT, including
secondary evidence, without another fault. Failure to render or deliver this report
never delays the latch or restarts filing. [FLOW-3 through FLOW-6, PERSIST-4, October 18
R3]

## 5. Evidence, pass statements, and enforcement, destinations sections 4 and 8

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-EVIDENCE.** Keep lifecycle.rs's load before SPU and classifier start.
TRACE-LOAD records admin's stack, verified starting identity, scheme and template
inventory, plus the member identity returned on open. Keep the engine's existing
Generation request/measurement carriage for effective model and sampling facts. Spec 09
EVIDENCE-SOURCES names the external build and host observations that join these records.
The harness opens no build manifest, probes no assigned device for qualification and
accepts no RunEvidence over a serving port. A missing external evidence artifact cannot
become a serving refusal or halt. Reason: qualification reads the trace after the fact,
while serving records what each component knows when it acts. [FLOW-2, FLOW-3, COST-5,
SCOPE-1, October 18 R9]

**HARNESS-ACCOUNT.** FlowObservation fields are phase, optional source_id/input_id,
optional request_id, pending_count, oldest_age_ns, optional elapsed_ns, and optional
blocked_stage, plus Spec 09 FLOW-COUNTS' required sample_start_ns, sampled_ns,
pending_bytes, filed_count, filed_bytes, rejected_count, input_pending, input_started
and input_completed. It also carries retained_blocks and retained_bytes and
pending_turns as DecimalU64, backlog_waiting and lag_alarm as bool, and waiting_copy as
Option<CopyAtHalt> under HARNESS-BACKLOG and Spec 09. DecimalU64 encodes all counters
and durations. Phases are copied, classification_started, accepted, correctly_rejected,
file_started, filed, and wait_finished, repair_requested, repair_exhausted, sampled,
backlog_wait and backlog_resumed. The two repair phases require source_id,
failed_corrections and retry_cap. All other phases refuse the last two fields.
blocked_stage, when present, is gate, cut, decision or control. Classifier bindings live
in ste100.result, not this measurement. Oldest age starts at copy intake. Filing latency
starts at first write byte. Sample intervals start at open confirmation, use
FLOW-COUNTS' transition/tick/boundary rules and count receipts only once. Sampling has
no lag timeout. A halt account includes the final flow_sample and preserves each
outstanding entry's prior state and pending or unknown commit status. It also carries
outstanding_pairs:Vec<OutstandingPair>. Each entry has call_id, state from PAIR-STATES,
execution:ExchangeId-or-null, opening_source:SourceBlockId, opening_filed:bool,
terminal_source:SourceBlockId-or-null and terminal_filed:bool. An unrecorded pending
start stays in the author-ticket account. A failed flow never files another terminal or
attempts a dump. [PAIR-1, FLOW-4, October 18 R3, R4] [CLASS-6, COST-4, FALS-5, FLOW-4]

**HARNESS-OBSERVE.** StateChannel keeps the exact operation and queue/send timestamps
with its one outstanding request. It samples receipt time when the complete answer is
available, validates TYPES-OBSERVATION and authors StateStoreMeasurement before
releasing that completion. The author event owns a converted copy, while the held
completion keeps the exact received answer. The source's completion slot stays occupied
until the pending-author slot records it, so there is neither a second request nor a
duplicate receipt while trace capacity is full. A malformed observation uses the
existing protocol-failed path. Unavailable counters are valid and do not prevent the
ordinary response disposition. [FLOW-4, COST-2, COST-5]

The open observation is held until TRACE-LOAD and partition activation, then recorded
before the boot read. Validate member_observation and convert it into
load.state_member_observation, preserving unknown start facts. The harness never copies
that measurement back through the tee. The store retains its own metadata under
STATE-OBSERVE. The response field never becomes model content. On pre-load failure use
existing diagnostic reporting. A lifecycle halt remains terminal and never waits for a
new OS read or another trace slot. Raw sample clocks and envelope elapsed clocks share
HARNESS-CLOCK's origin. [FLOW-2, FLOW-5, TAP-3, COST-4]

The owner schedules the next FLOW-COUNTS tick from the last sampled timestamp plus the
loaded observation_interval, checked for overflow. It arms a tick only while filings or
input checks are pending. A tick is a wake for a measurement, not an operation deadline.
The pump handles due terminal deadlines first and coalesces missed ticks under trace
backpressure as Spec 09 states. No timer resets gate, cut, decision or lifecycle bounds.
[CLASS-6, CLASS-8, FALS-5, COST-4]

The harness/state fixture delays both partial response writes and trace capacity, then
checks exactly one measurement and receipt. It also returns a valid filed payload with a
wrong operation ID, missing observation or reversed clock, proving that no acceptance
grant escapes. An unavailable counter must still allow the receipt. A zero-receipt
interval must appear while work is pending. Removing any of those checks must change its
assertion, not merely the number of log lines. [SEAM-2, FLOW-4, COST-5, FALS-5]

**HARNESS-PASS.** Partition validation certifies finite tuple and projection syntax,
registered kinds and exclusions, and no overlapping selection. Evidence is the exact
partition and state-copy/1 registry in load. It certifies no model accuracy or retrieval
coverage. Template certification has the separate, limited pass statement above. No test
double is production qualification. [TAP-3, CLASS-4]

Measure gate/cut/decision bounds with both classification directions, store receipt
delays, and the largest qualified Text blocks under a declared tuple. Record queue,
compute, and transport separately. Freeze chosen bounds before held-out runs, then
inject a hang to prove they expire. Measure committed filing throughput and lag under
the one-exchange assumption. Neither number is elected here. [CLASS-6, FALS-5, COST-4]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| HARNESS-OWNERS | Compile pin and socket-peer test | Add a second stream writer |
| HARNESS-RECORD | Ignored-return caller and full-slot tests | Lose tap failure or overwrite pending Event |
| HARNESS-AWAIT-AUTHOR | Saturated real trace writer with lifecycle and classifier ready | Recursively author, block poll progress, or discard Busy event |
| HARNESS-RECORD-LIFETIME | Caller disposition plus pre-Run load, boot refusal, ignored-result and post-release unload fixtures | Reset the context, poll a retired port, seat an unrecorded prompt or continue after ignored failure |
| HARNESS-BACKLOG | Delayed receipts with small test limits, compound copies and terminal trace delay | Release charge before Recorded, accept a partial copy, exceed a running limit, lose the failed copy or halt solely on the age alarm |
| HARNESS-AUTHOR | Exhaustive kind/source fixture table | Take origin from payload or accept missing UID |
| HARNESS-CLASSIFY | Scripted double, all table rows, zero-noun Accepted and target mismatches | Treat Pending as rejection, accept an empty noun vector or a label score as bindings |
| HARNESS-RECALL-REFUSAL | Actual recorder pressure and lifecycle waits, mixed original/correction identities and cap fixtures | Admit a second event, lose an owed receipt, return before Recorded, dispatch a refused ID or charge several attempts for one correction |
| TEMPLATE-CHECK | Inventory and substitution certificate fixtures | Serve with missing certificate or unlisted template |
| HARNESS-INPUT | Existing {text,role} accepted/refused frames and later-turn assembly | Place one rejected block or open a refused turn |
| INPUT-NO-RESIDENCY | Actual SPU exit, two later world exchanges, held trace capacity and graceful Leave | Accept or classify a later request, send before its refusal records, lose its input.offered link, duplicate organ death, invent a Released confirmation, or replace an S4/S5 Unloading refusal with NoResidency |
| HARNESS-LEDGER | Every table row, real commit with lost receipt | Accept send success or resend uncertain commit |
| HARNESS-PUMP | Busy decoder, label wait, asks, writer pressure, nearest deadline | Restore a blocking recv or starve one source |
| HARNESS-CLOCK | Fake clock/wait pair plus real poll watchdog | Reset anchor on progress or omit a nearer deadline |
| HARNESS-BARRIERS | Wind-down with pending file and no wind_down_bound | Save before receipt or wait forever |
| HARNESS-CONTROL-FAILURE | Every cleanup state, exact reason/point assertions, explicit size refusal and later filing/retry | Halt on the save timeout alone, skip abort, resume before aborted, mislabel the halt reason, publish a late result or continue after cleanup failure |
| HARNESS-HALT | Every lifecycle cell and halt phase with live tool containment | Serve on member death, label interrupt unload, save, or return Left |
| HARNESS-HALT-BOUND | Fake clock plus real-poll watchdog with silent gate, SPU and sink | Restart a phase deadline, run wind-down, or wait past the absolute end |
| HARNESS-HALT-ACCOUNT | Table-driven failures through the real coordinator, canonical fault/unload corpus and failed-inbox reservation | Relabel malformed receipt as carry loss, lose an unheld block, mark a partial send committed, omit unknown commit, assign a sequence to pending authoring or replace the first cause |
| HARNESS-ACCOUNT | Rebuild and interval trace checks | Omit binding evidence or double count a receipt |
| HARNESS-PASS | Invalid partitions and mismatched certificate | Treat a double's pass as production evidence |

A circuit fixture must come from an unmodified real recorder trace produced at the
pinned baseline, with generator command, commit and raw-file SHA recorded beside it.
Synthetic new fields are a separate derived fixture with an explicit transformation. Use
Text plus ToolCall plus UnrecoveredCall, refused input, and seeded boot cases. No
captured fixture or runtime test result is claimed by this documentation pass.

## 6. Read and boot integration, destinations sections 2, 6 and 6.1

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-READ.** flow/read.rs adapts the old asks to TYPES-READ on the same owner. Read
readiness uses selected source-ID obligations, never a private store commit counter. For
mandatory fresh history, use prepare_cut's bounded filing pause before read, then
release that barrier and start the read deadline. Keep the merged small ask bound of 2
seconds per exchange, 120 seconds for the image-adoption/open exchange, each capped by
the enclosing load/leave budget. Boot prompt reads retain the serving identity allowance
of 2 seconds. The 600-second parked-ask allowance stays diagnostic-preload-only. The
full paged read also has one absolute deadline from the original read request, using the
applicable allowance, so many pages do not restart it. [SEAM-3, CLASS-8]

Reassemble the tagged QueryResult before handing content to place. Require entries for
content selectors, including subjects and statement, open_pairs only for that selector,
marks only for marks, and span_preview only for span_preview with exactly the requested
span/through. Require plan_current only for that selector, plan with the exact requested
PlanRef, and plan_flags with PLAN-FLAG-READ's exact plan/after echo and count sum. The
three plan results cannot construct HeldMaterial or a cut grant. Require observations
only for its selector, with exact run/after echo and the ordered records and source
nullability from TYPES-OBSERVATION-READ. A supplied nonnull through must echo exactly.
Its result cannot construct HeldMaterial, SelectedUnit or a cut grant. The outer
response observation remains separate from the queried metadata. A wrong variant is
protocol failure, never empty history. Preserve each entry's pairs, including exact
whole return blocks. Author existing recall through HARNESS-RECALL, retaining the
selector, snapshot_id, original source and binding identities, selected span occurrences
and, for boot, the complete seated Message. No recalled content is filed again. An
initial refusal acquires no snapshot and sends no release. After receiving a valid first
page, retain its snapshot_id and send release exactly once on success or a valid later
operation refusal, using the read's remaining absolute deadline. A failed seam closes
the channel instead. Closure drops the member's cursor. The adapter never guesses an ID
from the request counter. Reason: release acts only on an acquired snapshot, and cleanup
cannot restart the read's bound. [STORE-8, READ-1, FLOW-3, SEAM-3, CLASS-8]

The query itself creates no ledger entry. A model's inward recall call separately
records its request and terminal pair under Spec 05. That terminal may contain the
complete read result, but its nested held statements keep their original source
identities and are not parsed into fresh statement filings. The new pair records that
the recall occurred. The recall event and placed message.tool_result remain excluded
from admission. [STORE-2, STORE-8, READ-3, PAIR-1, PAIR-3]

The read-client fixture covers an initial refusal, a later page refusal after snapshot
acquisition, success and a failed seam. It observes respectively no release, one
release, one release and channel close. Removing the acquisition guard, releasing twice
or granting cleanup a fresh deadline must fail that fixture. These cases supplement the
full-result and wrong-variant instruments. [SEAM-3, CLASS-8, FLOW-4]

For an observations read, recall retains the exact typed selector and snapshot_id, with
sources and spans empty. These are operation identities, not content provenance. No
observation is placed by the read adapter. [SEAM-3, FLOW-3, R20-19]

For a successful plan_flags read, visit after then through.block_id and deduplicate
exact ContentRefs in first-encounter order for recall.sources, as TRACE-RECALL-SOURCES
defines. If after equals through.block_id, retain exactly one source. Every source has
empty binding_ordinals, and recall.spans is empty. The selector and snapshot_id retain
their ordinary typed meanings. The counts appear only in the later plan observation.
Release the snapshot once and never retain a read cursor just to produce a measurement.
[PLAN-3, SEAM-3, FLOW-3, COST-4]

**HARNESS-RECALL.** flow/read.rs converts one completely reassembled and checked
QueryResult into the trace-owned RecallRecord defined by [TRACE-RECALL][recall-shape].
The adapter copies the exact submitted ReadSelector and acquired snapshot_id. It applies
TRACE-RECALL-SOURCES' complete traversal, ordinal union, first-encounter source order
and occurrence-preserving span order. It never treats source identity, binding ordinal
or returned-vector position as interchangeable. Reason: recall provenance must describe
the checked result before a consumer can use it. [FLOW-3, STORE-3, STORE-5, SEAM-3]

The adapter accepts only HARNESS-READ's matching selector/result pair. It checks the
step_successor reference against the requested plan and previous_step, and its
instruction against the same StatementRef, before the provenance walk. It retains
original pair opening and terminal source identities, including a null terminal for an
open pair. Every boot identity and byte check in TRACE-RECALL-BOOT runs against the
loaded state_start and the complete system_prompt entry. The boot Message uses the
existing canonical Message shape. The conversion into the trace's checked raw field
preserves those bytes and adds no trace dependency on another crate. [STORE-3, PAIR-3,
MARK-3, PLAN-4, PROMPT-1, PERSIST-6, SEAM-3]

Read is used for every successful nonboot query, including later system_prompt queries
and all metadata selectors. Boot is used only for HARNESS-BOOT's first content read.
Both author internal/runtime recall without a turn, under the existing turnless rule.
Only boot includes held content bytes. Both variants stay excluded by the partition
and construct no filing. Metadata reads retain their existing restriction against
HeldMaterial, SelectedUnit and cut-grant construction. [FLOW-3, TAP-3, STORE-8,
MARK-3, PERSIST-6]

A successful Read or Boot recall advances HARNESS-CAPTURE's live Recorded watermark
exactly as another trace completion in that run does. It does not advance the member's
SourcePosition or any already-frozen capture frontier. A refusal that authors no recall
has no recall sequence to contribute, while its separately Recorded operation
measurement follows the same watermark rule. [FLOW-3, PERSIST-4, TAP-3, STORE-5]

| Read state | Observation | Provenance obligation |
| --- | --- | --- |
| Awaiting full result | Valid page, more pages remain | Hold bytes, author no recall, keep the original deadline |
| Awaiting full result | Initial or later valid refusal | Keep its typed refusal, perform existing acquired-snapshot cleanup, author no recall |
| Checking | Complete result matches the selector and source rules, and HARNESS-READ has completed snapshot release | Construct exactly one immutable Read or Boot record |
| Recording | CapacityPending | Retain that record and result, service the pump, do not requery, refile or rebuild an empty success |
| Recording | Recorded | Snapshot release is already complete under HARNESS-READ. Expose the checked result or seat BootHead |
| Any read state | Malformed result, failed seam or applicable bound | Existing failure account and latch, no inferred successful recall |

Recording retains the read's original deadline and the existing pump's stop/leave
handling. A recorded result cannot be recorded a second time on resume. Snapshot release
remains HARNESS-READ's single acquired-cursor obligation, completed before recall
construction or recording for both Read and Boot. A valid empty result records an empty
success with its real selector and snapshot ID. A refusal does not. This state table
adds no clock or release exchange. [FLOW-3, FLOW-4, SEAM-3, CLASS-8]

A pass certifies faithful provenance for the completed typed result and recording before
its consumer. It does not certify selection quality, permit a metadata result to be
placed, or establish that the query contained every relevant fact. [FLOW-3, READ-1,
SEAM-3, FALS-4]

**HARNESS-READ-CALLERS.** The retained serving inspection methods have these signatures
in engine.rs, with their result types re-exported from lib.rs:

```text
Ports::recall(&mut self, last_turns: NonZeroU64)
    -> Result<Vec<ReadEntry>, ReadFailure>
Ports::session_shape(&mut self)
    -> Result<SessionShape, ReadFailure>
```

NonZeroU64 is std::num::NonZeroU64. ReadEntry is TYPES-READ's shared type. ReadFailure,
defined in flow/read.rs, is Refused {code:ReadRefusalCode, detail:String} or FlowFailed.
The latter returns only after the common failure latch is set. Correct operation refusal
preserves the exact store code and detail. It never becomes None or an empty successful
vector. recall submits conversation with the positive bound, completes paging and
snapshot release, then records provenance before returning. session_shape uses the typed
shape control. Both drive the common pump and its existing read deadline. Reason: the
old public readers must have a concrete replacement, not leave each Rust caller to
invent a failure convention. [SEAM-3, READ-1, READ-5, FLOW-4]

Keep SessionShape {runs:Vec<RunShape>} and RunShape
{run:String,kinds:Vec<(String,u64)>}. Group shape rows by run in first-appearance order
and keep each run's kinds in the member's returned order. Counts retain their decoded
u64 value. These are inspection results, not placement grants. Neither ReadEntry nor
SessionShape is convertible into Contribution or HeldMaterial by a public constructor. A
loop cannot edit returned data and then present it as held content. The private material
adapter below retains that authority. [STORE-5, STORE-8, FLOW-8]

Diagnostic replay retains its separate Recalled representation and port. [SEAM-3,
READ-5, SCOPE-1]

Rust consumer fixtures must observe distinct empty success, populated success, typed
refusal and latched failure. A cross-run fixture keeps equal turn ordinals distinct.
Compile pins forbid constructing HeldMaterial from inspection data. Removing the
positive bound, typed-failure mapping or private constructor must fail its respective
fixture. These are read-interface checks, not proof that a selected history contains
every fact needed for a decision. [READ-1, READ-5, STORE-8, FALS-4]

**HARNESS-MATERIAL.** flow/material.rs owns the private HeldMaterial constructor and
Spec 08's UNIT adapter. A read result is usable only after full paging, typed validation
and recall provenance. Check ReadBinding ordinals are unique and increasing, offsets are
valid in the complete decoded text, and every attachment refers to one of the included
bindings. subjects and conversation must have the complete binding sequence from zero,
while statement must have exactly its requested ordinal. Other filtered reads preserve
their original ordinals. A malformed response is a component failure, not an empty
history. [SEAM-3, FLOW-4, STORE-3]

Build one complete unit before returning it to sort. Preserve the requested text
interval, whole raw pair sources and canonical ToolOutcome content. The adapter can
reconstruct floor Message records for held assistant and tool traffic, but cannot mint
the harness's ToolResult grant, an AcceptedInput, a TemplateExpansion or a dispatch
capability. The private pure outcome-format helper in tools.rs is shared with live
completion. Only the existing completion door still constructs ToolResult. Reason:
reading held traffic and authoring new traffic require different authority. [FLOW-8,
STORE-8, PAIR-3, PLACE-1]

Every unit includes Spec 06's SourceMapEntry array. Map each exact stored Text interval
to its new canonical interval, each external/review request to its whole ToolCall, each
recall request to its complete request Text, and each return to its whole ToolResult.
Preserve repeated span locations, and require the ordered distinct mapped materials to
equal sources. Neither a missing map nor a plausible equal-text substitution is usable.
SourceMapEntry is local to the unit. Only place allocates Message occurrence IDs.
[STORE-3, MARK-6, PAIR-4, PLACE-4]

Sort receives immutable SelectedUnit and read/measure adapters only. Its source coverage
test is Spec 08's COVERAGE. Place verifies result-unit request coverage under Spec 06's
COVERAGE-JOURNAL, never substitutes a unit silently. A valid opened pair cannot form a
complete history unit and returns SelectionRefused.incomplete_candidates. Unknown source
kinds, mismatched intervals or fabricated source references fail the adapter's
validation. The serving loop cannot choose arbitrary Message roles to repair the result.
[SORT-1, SERVE-2, PLACE-4, PAIR-4]

Before enter confirmation, HARNESS-PLAN obtains plan_current after boot repair has
settled its receipts. It may run before or after the final empty pair recheck. Both
results must describe that same repaired store before loaded is reported. Selection has
no load-time strategy under HARNESS-SELECTION-INPUT. These reads create no filing and
share the load deadline. [PLAN-1, PLAN-6, PERSIST-6]

**HARNESS-SELECTION-INPUT.** Selection criteria and candidate reads are explicit
per-call inputs to Spec 08. Enter checks no strategy, reminder list or automatic
retrieval setting. The foundation still verifies the seed, prompt, pair repair, mark
catalog and committed plan-summary reads needed by those primitives. [READ-1, PERSIST-6,
R20-12]

**HARNESS-MARKS.** The explicit begin_task port takes an AcceptedInput and returns
TaskScopeRef or a typed operation refusal. No input receipt invokes it automatically. It
performs the following operation when requested. flow/marks.rs is the single serving
allocator/coordinator for Spec 06's marks. It reads marks through HARNESS-READ,
retaining the returned next ID and open path only for the current operation. It
serializes mark-producing operations with the plan coordinator, so another operation
cannot allocate between that read and its filing receipt. Allocation is committed only
by the store effect. A cancelled pre-authoring proposal consumes no ID. A lost receipt
halts rather than reusing an uncertain ID. Reason: mark identity is held state, not an
independently persisted harness counter. [MARK-1, MARK-5, MARK-6]

For a requested begin_task when no active task remains, wait for the input's
message.user filing, read marks, then author state.mark with its registered begin-task
template and the exact begin_task context. Use the next two span IDs for task and
prompt. The event has the current turn, channel internal, origin loop and one role-user
Text block. Consume its tap outcome through the ordinary classifier and ledger. Do not
begin generation or a dependent plan operation before its filed receipt. For an existing
open task no begin_task is authored. This wait uses the decision barrier and the same
read deadline as other controls, never a new mark clock. The input must therefore be
covered by the partition. [MARK-1, PLACE-2, CLASS-8, FLOW-8]

The tap's ContextBlock becomes SourceContext only on block zero, preserving its path and
raw bytes. After statement acceptance, strictly decode the licensed context and
construct the corresponding StatementEffect. Ordinary statements receive null
context/effect. A correct template rejection records the existing template fault and
fails this operation visibly without a mark or a component latch. A malformed context,
bad source association or refused filing is a broken component contract. Unselected
required mark content indicates an invalid loaded partition, never permission to issue a
direct store update. [TAP-4, MARK-5, CLASS-8, SEAM-2, review S2]

| Mark coordinator state | Observation | Action and next state |
| --- | --- | --- |
| Idle | Caller requests begin_task with accepted input | Wait for its filing under decision barrier, AwaitInput |
| AwaitInput | Exact input filed receipt | Submit marks read on the sole state owner, Reading |
| Reading | Valid marks result, current task absent or terminal | Allocate task/prompt proposal, author certified state.mark, Authoring |
| Reading | Current task open | Return its TaskScopeRef, Idle |
| Reading | Explicit read refusal | Record operation refusal, return it to caller, Idle |
| Authoring | CapacityPending | Retain exact event, pump existing work and deadlines |
| Authoring | Recorded with selected Text/context | Ordinary classification/filing, AwaitReceipt |
| AwaitReceipt | Correct template defect | Record operation failure, return it to caller, Idle |
| AwaitReceipt | Exact filed receipt | Expose new TaskScopeRef, Idle |
| Before authoring | Stop or leave | Cancel proposal with no allocation, lifecycle owns continuation |
| After authoring | Stop or leave with healthy flow | Account and finish the owed filing, no dependent generation |
| Any wait | Component failure or applicable bound | Foundation latch, no retry or direct mark write |

Spec 07 invokes the remaining mark helpers through its classified plan effects. Before
closure it freezes a source frontier and waits for every disposition through it. Only
filed content and terminal filed pairs may enter a preview or finalized span. Correctly
rejected blocks are absent. The read adapter preserves the preview tag and never
constructs a closed-span or CutReady grant from it. A preview is evidence for the review
tool, not an instruction to mutate state. [MARK-3, MARK-4, MARK-5, PLAN-2]

**HARNESS-BOOT.** Follow PERSIST-LOAD once per enter: opened, untapped load, semantic
partition validation and recorder activation, STE100 duty start and template checks,
first content read system_prompt, recall evidence, cached BootHead, decoder open with
that head first, pair repair, then user gate. BootHead is a Message plus SpanRef and end
byte offset. The state member can open before load because that check is structural and
emits no agent content. The worker starts no classifier before load. Remove the old
implicit ask_recall boot-tail path and #76 seeding-turn branch. A second mandatory boot
query is an integration error in the harness, not a store-side ban on later explicit
recall. Run Spec 05's BOOT-REPAIR through open_pairs, tool.call.completed, PairCopy and
file_pair, then require its second empty query before gate open. It uses the remaining
load budget and never edits during adoption. [PERSIST-5, PERSIST-6, PROMPT-1, STORE-8]

Boot recall Recorded establishes BootHead before decoder Open. HARNESS-OPEN consumes
that private provenance to bind the one Open exchange and its boot placement. The
placement cannot create BootHead retrospectively. Only its successful Recorded and
checked-fit boundary permits journal initialization, boot mark readiness and later
enter stages. Pair repair and its second empty query remain required before the gate.
[PROMPT-1, PERSIST-6, PLACE-1, FLOW-3]

**HARNESS-SAVE.** After prepare_save, relay the member snapshot result as
SavePointImage. On SavePointImageChecked relay acknowledge, then send
SavePointImageFinished. Validate Prepared A/B against that capture. Author save_point
confirmed and send Commit only while the latch is clear. Await Published, author
save_point published, release_capture, and answer the original directive. If latch sets
before Commit, send Aborted if possible and halt. After Commit, take no new capture and
preserve that boundary in the failure account. This is the persistence contract's
confirmation protocol, with R20-7 broken-publication handling. [PERSIST-2, PERSIST-4,
FLOW-5]

Save-induced delay to a queued next turn is recorded as wall time, from the boundary
freeze until resume or halt. Capture RAM and read snapshot RAM are measurements too. No
claim that a between-turn pause has zero elapsed cost is made. [COST-2, COST-4]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| HARNESS-CAPTURE | Same-content captures after enter/load, ordinary authors, Read/Boot recall and coordinator observations, pending-ticket, post-Recorded interruption and post-freeze fixtures | Skip one author path, advance before Recorded, move the frozen boundary, admit later content or substitute the trace watermark for image position |
| HARNESS-READ | Real member pagination and query-result trace checks | Place partial bytes, restart deadline per page, or file a recall |
| HARNESS-RECALL | Every selector/result pair, null and populated plan snapshots, repeated prefixes, whole pairs, filtered ordinals, empty/refused reads, held or failed release and pending recorder fixtures | Walk metadata in another order, renumber bindings, deduplicate spans, invent a source for the seed, record a partial/refused read or before Released, expose before Recorded or author twice after a wake |
| HARNESS-RECALL, plan_flags | Equal and distinct after/through.block_id fixtures | Emit duplicate sources for equal identities, reorder distinct sources or add a binding ordinal or span |
| HARNESS-MATERIAL | Filtered binding, whole-pair, current-result and stored-span fixtures plus private-constructor pins | Renumber a binding, omit an attached return, duplicate the live request or mint a live grant from a stored record |
| HARNESS-SELECTION-INPUT | Explicit criterion and complete unit fixtures | Invent a default strategy or accept an incomplete unit |
| HARNESS-MARKS | One input/task fixture, stop/leave rows, concurrent plan-operation and template-defect cases | Start generation before mark receipt, reuse an uncertain ID, skip input coverage or turn a correct defect into a halt |
| HARNESS-BOOT | Seeded member plus recorder/decoder double, boot recall before Open and placement Recorded before journal/mark readiness | Duplicate boot read, omit end mark, create BootHead from Opened, refile prompt or open gate early |
| HARNESS-SAVE | All three-party transitions with fault injection | Send Commit with latch set, use edited draft, or answer before Published |

## 7. Place and decode facts, destination sections 5 and 6

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-RELOAD.** The engine exposes Spec 06's CUT-RELOAD and owns its fixed protocol
and sole mutation reservation. CUT-RELOAD through STAGED-APPEND define its local types
and state table. New material uses the standing classifier and ledger. Held units retain
their receipts. The caller receives Staged, not a resident reload. Every next-append
constructor consumes that private grant and prepends its pinned tail. No caller can
bypass it, replace it with a second stage or borrow diagnostic ReFeed. The full delta is
measured again with new input, and Generated confirms the final resident count.
Enforcement covers absent classifier, each stop/leave state, empty suffix, added-input
overflow, bypass and duplicate consumption. [CLASS-8, MARK-4, PLACE-5, R20-15, R20-16]

**HARNESS-PLACE.** flow/place.rs implements Spec 06's PLACE-PORT, journal, capacity
checks and cut coordinator. It holds canonical messages, source occurrences and SPU
counts, never a usable token vector or a tokenizer. HARNESS-OPEN's opaque trace evidence
may contain token IDs, but grants no token-level operation. The engine remains the
sole decoder writer and the pump its sole reader. A placement request retains its
exact delta and ordered granted schemas until append or refusal. The existing
shell_schema producer supplies the
schemas, recorded with the effective grant in load and in the assembly's evidence. No
family formatting occurs here. [PLACE-1, PLACE-3, PLACE-5, FLOW-3]

**HARNESS-OPEN.** The boot owner holds the existing recorded BootHead, exact Open
request, decode reservation, source-port identity and original absolute enter bound.
It uses the sole pump reader and requires the complete request send and one complete
matching Opened before accepting evidence. Echoed session must equal the held request
and trace session. Validate all four counts, prefix_tokens no greater than capacity,
terminal_tokens equal to one, and the SPU-OPEN closed box shape. Check each input token
in measurement.prefix_token_ids as u32, its checked vector length against prefix_tokens,
and exactly one identity-prefix
interval from zero through the
decoded rendered string's UTF-8 byte length. Reject unknown, missing or duplicate
members. Preserve both raw boxes unchanged without creating a usable token-vector
cache. The harness has no renderer and must not compare rendered text with the
unframed BootHead Text. Producer fixtures prove rendering/token/prefill truth.
[PROMPT-1, PERSIST-6, PLACE-1, PLACE-5, SEAM-2]

Check the declared head/tail budget and effective generation reserve with Spec 06's
checked arithmetic, including generation_limit greater than zero for foundation
serving. Zero is valid evidence of the resolved limit but fails this configured fit;
it follows the truthful placement then boot_failed path below, not protocol failure.
A valid Opened describes a prefix already seated, whether or not that configured fit
succeeds. Retain its exact evidence and author the assembly-zero placement with the
existing boot recall sequence as causal_parent. Keep the reservation and active boot
ownership through recorder backpressure. A shared author ticket has this one purpose
and cannot be consumed by an input, repair or other owner. Save the returned Recorded
sequence and next continuation before any recoverable lifecycle check. No wake authors
the event twice or consumes a proof before retaining its next state. No stage restarts
the original enter bound. [FLOW-3, PERSIST-6, PLACE-1, PLACE-3, COST-4]

**HARNESS-OPEN-LIVE.** Live admission for this owner means RecordingPhase::Entered,
an active coordinator, a clear first-failure latch, no observed stop or leave for
this enter, and the exact original Enter bound still owned and unexpired at the
common clock sample. Check it before sending the first Open byte and again before
releasing journal or boot-mark readiness after Recorded. Once cancellation or
failure revokes this owner's readiness it cannot regain admission on a later wake.
The private trace-only teardown permit below does not satisfy this predicate.
[FLOW-3, FLOW-4, FLOW-5, PERSIST-6]

Only Recorded, successful fit and HARNESS-OPEN-LIVE install the checked journal facts
once. Use StatePersistenceInvalid stage boot, case boot_failed for a failed configured
fit, after the truthful boot placement, and run failed-enter teardown. An SPU typed
Open refusal follows the existing refusal account and failed enter, with no placement.
A wrong source, session, reply arm, malformed evidence or inconsistent count is a
protocol failure, not a fit refusal. It grants no accepted placement or readiness.
An answer exceeding DECODE_MESSAGE_BOUND is component failure. Trace encoding or
sink errors follow the existing trace failure path. Allocation failure grants no
readiness, and this
carrier adds no process-wide allocation-failure recovery. TRACE-PLACEMENT
states the distinct record-size qualification obligation; the recorder has no
per-record byte ceiling to borrow here. Never truncate evidence or retry Open.
Later explicit reads
cannot replace the existing BootHead or open the residency again. [PROMPT-1, PERSIST-6,
PLACE-1, PLACE-3, FLOW-4]

| Boot owner position | Observation | Retained work and next disposition |
| --- | --- | --- |
| Prepared | HARNESS-OPEN-LIVE | Send the one held Open, AwaitOpen |
| Prepared | Stop or leave | No Open, existing failed-enter cleanup |
| AwaitOpen | Partial send or partial answer | Retain exact exchange under the original bound |
| AwaitOpen | Valid complete Opened | Retain boxes, counts, fit result and completed open wait, AwaitRecord |
| AwaitOpen | Typed refusal or invalid answer | Existing refusal or failure account, no accepted placement |
| AwaitRecord | Recorder pressure or recoverable lifecycle wait | Retain exact event, ticket and fit result |
| AwaitRecord | Recorded, fit and HARNESS-OPEN-LIVE | Install journal facts once, continue enter |
| AwaitRecord | Recorded but failed fit | Record boot_failed, failed-enter teardown |
| Any sent-Open position | Stop, leave or first failure | Revoke readiness and retain only bounded exchange/accounting obligations |
| Any waiting position | Applicable bound, failed seam or failed trace | Account available facts, dispose without readiness, existing teardown |

Before any Open bytes are sent, cancellation withdraws the unsent reservation. A
partial send remains owned by the existing partial-message cleanup. It is neither
restarted nor accepted as complete without complete-send proof. After a sent Open,
cancellation does not undo a seated prefix. Drain the existing
exchange where its channel and remaining bound permit. A valid completion after
cancellation but before that bound may record the same boot placement as a trace-only
teardown obligation, even
when ordinary author admission is closed in Releasing or Failed. This permit is private
to the already-sent Open and its recorded boot source. It permits no ordinary content,
filing, generation, mark readiness or journal grant. It uses ordinary writer capacity
without spending or delaying reserved terminal work. The earlier of the original enter
bound and any active leave/halt bound limits both exchange and recording. Preserve the
first failure, do not renew a bound, and release the residency under existing cleanup.
In both Releasing and Failed, finish that account or declare it incomplete before
retiring its decode transport and before authoring unload. At its bound or a failed
seam/sink, abandon the remaining account and continue terminal cleanup without
inventing placement. This ordering does not postpone reserved terminal work that
precedes Releasing in HARNESS-HALT.
An unanswered Open or failed trace leaves an incomplete account and never a fabricated
placement, successful enter or successful Left. No new Open cancellation directive,
reconnect, prompt read or retry is introduced. [FLOW-3, FLOW-4, FLOW-5, PERSIST-6,
SEAM-2, COST-4]

On an explicit prepare_prefix request, prepare the exact logical prefix under Spec 06's
PREFIX-PREPARE. With no pending rebuild, an ordinary append uses the last confirmed
resident count as N. A cut or pending rebuild instead uses the latest still-resident
completed boundary at or before its logical endpoint. Retained Messages keep their IDs
when recomputed, while new units and assistant answers receive new IDs. The read-only
measure adapter prepends that rebuild exactly once and submits MeasureDelta through the
pump with that N and the exact proposal. A measurement response must match the cached
prefix/capacity and the expected resident/keep facts. The complete proposal passes the
universal C_tail check and the generation reserve check before any append. A bad caller
occurrence refuses stale_position before the ask. A response inconsistent with a serial,
unchanged residency is a protocol failure. A count that merely exceeds a limit is the
typed placement refusal and leaves the resident sequence unchanged. [PLACE-3, PLACE-4,
PLACE-5, FLOW-4]

Track native mark positions from a committed scope effect and the scope's observed
argument boundary, not the last stored member. Boot and whole-span replays use Spec 06's
distinct registration rules. Register only after the licensing operation completes, then
author state.mark.position. Mark registration, source annotation and resident changes
advance the journal revision and invalidate old PreparedPrefix grants. Recheck that
revision before consuming a placement grant or a cut grant. [MARK-1, MARK-6, PLACE-5]

| Placement state | Observation | Action and next state |
| --- | --- | --- |
| Ready | Valid prefix grant and selected units, no decode exchange | Arm decision barrier, measure rebuild plus units, Measuring |
| Ready or Recording | Prefix grant became stale | Trace stale_position, no append or Flush, Refused |
| Measuring | Matching DeltaMeasured within budgets | Hold measured candidate, author state.placement, Recording |
| Measuring | Matching count over budget | Trace typed fit refusal, preserve session, Refused |
| Measuring | MalformedDelta refusal | Trace the decode refusal and Placement.invalid_span, Refused |
| Measuring | NotOpen, OutOfOrder, wrong answer or component failure | Existing decoder/protocol failure, stop serving |
| Recording | CapacityPending | Retain exact record/candidate, service pump and original deadline |
| Recording | Recorded and no cancellation | Return PlacementGrant, disarm preparation wait, Reserved |
| Reserved | Caller submits grant to engine append port | Recheck revision, send exact delta/schemas, Generating |
| Reserved | Caller cancels or grant becomes stale | No append, preserve owed rebuild, Ready |
| Generating | Existing complete Generated | Keep rebuilt IDs, add new Messages and the assistant occurrence, clear staged rebuild/carry, register completed span placements, Ready |
| Before append | Stop or leave | Consume any outstanding measure, discard candidate, no append |
| Any model-blocking placement wait | Applicable bound expires | decision_bound or cut_bound, foundation halt |

During Generating the existing generation/cancel/tool-round protocol applies, not the
placement deadline. Before a cut, Spec 06's CUT-WAIT table takes the candidate through
drain, quiescence, measure and the chosen Flush or suffix Elide. It owns one deadline
for all those phases. No separate decision clock restarts it. A measured append may run
between tool rounds, while a smaller keep, Flush or Elide requires the between-turn
CutReady grant. Returned Flushed or Elided counts must match the planned cut before the
journal changes. [MARK-4, PLACE-5, PLACE-6, CLASS-8, October 18 R7]

After a confirmed cut, retain the unchanged logical prefix and its rebuilt Messages in
RAM, discard later physical boundaries and mark positions, and stage caller-supplied new
units separately. A subsequent operator cut can shorten this waiting logical prefix
using only still-resident physical boundaries. A later explicit append request is
measured with its supplied input. Phase cut records the confirmed mutation, while phase
append records the later proposed argument. A failed fit cannot silently discard either
rebuild or carry. Unload drops these intermediates after the normal filing barrier.
[MARK-2, MARK-4, PLACE-5, STRAT-3, PERSIST-6]

The pass certifies a complete, attributed proposal whose size fits the declared budget
under actual SPU facts, and whose retained prefix is valid. It does not certify
selection relevance. The evidence is the held source map, SPU reply, current journal and
recorded placement. A correctly refused fit or render never sets the flow latch. Preview
mark results cannot supply CutReady. [PLACE-4, PLACE-5, MARK-6, FALS-4]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| HARNESS-PLACE | Real family/session fixture with source-map capture | Count tokens in harness, reorder schemas, use different delta after measure or place on refusal |
| HARNESS-OPEN | Shared SPU-OPEN corpus, fixed/tunable facts, failed fit, wrong session/port/reservation, partial send/answer and malformed boxes | Guess an effective value, wrap arithmetic, substitute source/answer, retokenize or grant without complete correlation |
| Boot recording | Real recorder pressure and lifecycle interruption at admission and Recorded settlement, successful and failed fit, cancellation before/after send, silent/oversized reply and failed trace | Lose or duplicate evidence, steal another author's ticket, consume before retaining continuation, renew a bound, emit placement on invalid answer or grant readiness after cancellation |
| Placement wait | Pump test with writer pressure, delayed measure and stop/leave | Restart the deadline, block the pump or append after cancellation |
| Cut boundary | Multi-round turn fixture and invalid/stale occurrence corpus | Mint CutReady mid-turn, use preview as an end mark or accept a returned count mismatch |
| Logical prefix | Repeated marks, unplaced review metadata, empty answer and successive-cut fixtures | Infer native position from final stored member, reuse a removed boundary or append rebuild twice |

## 8. Plan coordinator, destinations sections 5, 6, 6.1 and 8

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-PLAN.** flow/plan.rs owns the private grants AcceptedPlanUpdate,
AuthorizedProposal and ClosureEvidence. It shares HARNESS-MARKS' one allocator and
HARNESS-OWNERS' pump, not a second store writer or thread. Runtime trace attribution
adds ManagementInput(Cause), constructible only from the verified root coordination
handler. It licenses plan.event/input/operator with cause uid zero. Runtime loop
emissions license plan.event/internal/loop, plan.break and state.plan.measurement. No
TurnFrame or model message can construct ManagementInput. [PLAN-5, PLAN-6, FLOW-1,
FLOW-8]

The boot summary must reflect any boot-repair receipts before loaded is reported.
[PERSIST-6, PLAN-2]

The management owner implements Spec 07 PLAN-ADMISSION-REFUSAL before sending a
pre-admission PlanUpdate refusal. It retains the original opener, exact refusal and
private trace claim beside the suspended input owner. Its RefusalRecord::Lifecycle
uses TYPES-PLAN-CONTROL's LifecycleAsk::PlanUpdate. The implementing act completes
that floor variant, its codec and typed consumers with this author path. Use the
loaded decision wait from the complete opener's receive reading for accounting and
delivery, independently of the active operation's unchanged bounds. A competing
author finishes only through its original continuation. Never borrow that author's
ticket or admit the rejected
plan after it becomes idle. EOF keeps the accounting duty. Failed accounting retains
the complete management owner through teardown. After Recorded, lost or expired
delivery creates no retry and no new foundation failure. [PLAN-6, FLOW-1, FLOW-4]

At boot, after pair repair and before enter confirmation/user admission, read
plan_current through HARNESS-READ. At HARNESS-RECALL's Recording/Recorded
transition, cache PlanSummary by projecting plan, status, document_sha256 and
last_source under PLAN-SUMMARY-EVIDENCE. Snapshot release is already complete.
Preserve the exact nullable turn. Do not reconstruct missing facts from the
current run, trace sequence, model text or a newly hashed plan. The member owns
stored provenance validation. The harness checks strict shape, result association
and any already-held trusted expected facts. Without such an expectation, boot
cannot independently detect a well-shaped false member digest or turn. A refused
or malformed boot read cannot establish enter success and follows failed-enter
rollback. The boot read consumes the existing load budget and is not a filing.
[PLAN-1, PLAN-6, PERSIST-6, SEAM-3]

After every plan-effect receipt, refresh the committed cache through that same
read and Recording/Recorded transition before the control answer or next
generation. Require the returned last_source to equal the effect's exact
SourcePosition, including nullable turn. Where an expected operator document
digest for that revision is already held through trusted input or prior committed
evidence, compare it too. Do not obtain a new source lookup or derive the expected
value from this response. An update in progress retains its previous receipt
under the existing InTransition convention. Observe/show reads this committed
cache only and never reports null or a proposed plan as an accepted update.
[PLAN-1, PLAN-6, FLOW-3, SEAM-3]

If this required post-receipt refresh refuses, fails, is malformed or disagrees
with a held trusted expectation, latch component failure through the foundation
path. The old cache may remain prior evidence but is not a successful committed
Observe/show result. Do not answer PlanUpdated or grant another generation. Do
not roll back the committed effect or report null or a proposed plan as accepted.
Only a successful refresh replaces the cache. [PLAN-1, PLAN-6, FLOW-4, SEAM-3]

A root PlanUpdate is accepted only at rest with no save, leave, other plan operation or
open review. Validate the exact document digest and strict shape before authoring its
operator event. Snapshot the current plan and marks, then hold the allocator through
preflight, authoring, classification and effect receipt. During this interval no turn,
step activation, cut or save can obtain that grant. Stop/leave cancels an uncommitted
operation but still accounts every already admitted block. A committed receipt is never
undone. [PLAN-6, MARK-5, FLOW-4]

| Coordinator phase | Result | Action and next phase |
| --- | --- | --- |
| Idle | Root update accepted | Begin decision deadline, read plan/marks, Reading |
| Reading | Matching snapshots | Check expected revision and typed structure, author operator plan.event, Input |
| Reading, or Input during preflight | Valid PLAN-PREFLIGHT-READ refusal | Finish acquired cleanup and admitted dispositions, record read_refused with exact code/detail, release allocator grant, keep prior cache, answer refusal, Idle |
| Input | Every selected statement filed | Preflight source-backed effect, Effect |
| Input | Correct rejection | Finish dispositions, record/refuse non_ste100, Idle |
| Effect | Valid private grant | Expand certified template, author/classify/file effect, AwaitReceipt |
| Effect | Ordinary invalid proposal | Trace PlanUpdateRefused, Idle, no effect |
| AwaitReceipt | Matching committed receipt | Refresh summary through Recording/Recorded, check held expectations, answer PlanUpdated, Idle |
| AwaitReceipt | Required post-receipt refresh refused, failed, malformed or mismatched | Foundation latch, no successful stale Observe/show, PlanUpdated or generation |
| Any pre-receipt phase | Correct template defect | Trace state_template_rejected and refuse operation, no halt |
| Any phase | Component failure, refused filing or deadline | Foundation latch, no success answer |
| Any phase | Admin disconnect | Continue already accepted operation under its original bound, retain trace outcome |
| Any phase | Stop/leave | No new effect after cancellation, finish admitted dispositions under existing shutdown rules |

The root coordinator applies Spec 07 PLAN-PREFLIGHT-READ only before admitting an
effect. It retains the exact read leg and its recorded operation measurement, and
records RefusalRecord::Lifecycle with ask plan_update, case read_refused, the original
read_code/detail and no rejected_blocks. Its causal_parent is the refusing leg's
Recorded measurement sequence. Keep the same author ticket through pressure and bind
its Recorded completion before any fallible continuation. Do not author recall for a
refused query, guess a snapshot or parent, or substitute a later release measurement.
[PLAN-6, FLOW-3, SEAM-3]

An initial refusal sends no release. An acquired cursor is released once within the
original bounds before the operation refusal completes. Existing source dispositions
still settle. Release the allocator grant with operation settlement, after acquired
cursor cleanup, admitted dispositions and the Recorded refusal, before answering.
No new effect is admitted, the previous committed cache remains evidence
and becomes usable after those duties, and this valid operation refusal alone latches
no component failure. A failed release, invalid leg/code/identity, malformed response
or observation, dead seam or applicable expiry keeps its existing failure account.
After a committed effect, the required summary refresh retains its explicit foundation
latch on refusal. Do not route that refresh through the pre-effect rule. [PLAN-6,
CLASS-8, FLOW-4, SEAM-3]

Every model-blocking phase consumes Spec 02's single decision deadline, without reset on
intermediate reads or receipts. Tool execution disarms that decision wait and uses the
gate clock. Authoring uses the existing ticket/resume discipline and never recursively
authors while pending. Correct conformance results are operation outcomes, never
component faults. The table extends the serialized lifecycle reducer, not admin Spec
3.0's definition of a run's state. [CLASS-8, PLAN-6]

**HARNESS-PLAN-PORTS.** Implement Spec 07 PLAN-PORT in flow/plan.rs. Its public
operations request activation, predicate checking, work-reference construction, review
assessment binding, category-authorized attest_closure, closure and break recording.
Each returns its typed result after the source/receipt conditions in Spec 07. No method
selects a next step, launches a review, interprets a review script or starts a decode
pass. The caller supplies ReviewAssessment and the primitive checks its association with
a held completed review pair and unchanged WorkRef. attest_closure requires
review_required false and the current WorkRef, without a fabricated pair. close rechecks
the category before filing either form. Model and gate content have no constructor for
this trusted operation capability. [PLAN-1, PLAN-2, PLAN-5, R20-12]

The operation owner implements PLAN-PORT-REFUSAL and PLAN-PORT-RECORD. Keep the
accepted request, finalized read/refusal or template-rejection evidence, source
dispositions, original Decision deadline and any allocator grant together until
settlement. Apply PLAN-PORT-REFUSAL's selector-specific domain mappings, retaining the
exact nonnull read_code for read-derived invalid_target and work_changed too. A code
that the selector/leg cannot produce keeps its foundation failure path. Distinguish
local allocation exhaustion from a member's counter_exhausted read reply. A correctly
rejected effect/outcome template records its existing
state_template_rejected fault, then the exact plan-operation refusal. Neither is a
foundation halt solely because the template correctly refused. [PLAN-2, PLAN-6,
CLASS-2, CLASS-8, FLOW-3, FLOW-4]

Use the common paged read owner and its finalized refusal after acquired-snapshot
release. Record the typed operation refusal before exposing its completion. Bind the
refusing leg's measurement parent before another read or release can replace it. Reuse
the same author event/ticket through pressure, and retain the first failure plus owed
custody through Stop, leave and failed-run handoff. An operation cancelled during this
accounting grants no continuation. Correct resource refusal preserves the prior
committed plan/cache and releases the allocator only after owed work settles. A refused
or failed mandatory post-effect refresh still latches foundation failure and exposes
no stale success. These ports do not retry, select a successor or call record_break
because a resource refused. [PLAN-2, PLAN-5, PLAN-6, SEAM-3, FLOW-3, FLOW-4]

**HARNESS-PLAN-OBSERVE.** The explicit observation port implements PLAN-OBSERVATION
without scheduling reads on each transition. It checks plan and receipt identity,
same-run clocks and the once-per-assembly count before recording the caller's sample. It
has no store write or progression handle. A typed read refusal is unavailable, not zero.
[PLAN-3, COST-4, R20-12]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| HARNESS-PLAN | Root update and source/receipt lifecycle rows, held receipt/digest mismatches, delayed Recording/Recorded, boot refusal and failed post-receipt refresh | Mint model authority, answer before commit, ignore held expectation, cache before Recorded or serve stale successful Observe/show after refresh failure |
| PLAN-ADMISSION-REFUSAL | Root management refusal through trace pressure, earlier owners, delivery pressure and failed-run handoff | Lose a request or ticket, cancel the original turn, grant a plan operation or send before Recorded |
| HARNESS-PLAN-PORTS | Private-grant, exact work/pair and PLAN-PORT-REFUSAL settlement fixtures | Close on model prose, reuse evidence, start review without a caller request, return refusal before Recorded, lose read/source custody, renew Decision or release an allocator early |
| HARNESS-PLAN-OBSERVE | Explicit call, receipt and duplicate sample corpus | Invent zero or count one assembly twice |

## 9. Selection, destinations sections 5, 6 and 8

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**HARNESS-SORT.** flow/sort.rs implements Spec 08's typed selection and identity ports.
It accepts caller-supplied criteria, immutable source grants and PreparedPrefix. It owns
no strategy, automatic retrieval schedule, pass order or filing handle. The harness
records its exact decision and measurement before releasing a selection grant. Typed
operation refusals retain their source and PlacementCode and do not latch a component
failure. [SORT-1 through SORT-4, SERVE-1, PLACE-4, R20-12]

**HARNESS-OPERATION-CLOSE.** A caller may return an operation refusal through the
existing stopped turn closure. Author turn.closed with reason Refused and absent cause,
then render the corresponding world close through lifecycle.rs::render_close. A refused
pre-turn input has neither run nor turn fields. These are response mechanics, not a
policy that every primitive refusal automatically ends a turn. [FLOW-3, CLASS-8,
PLACE-4, R20-12]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| HARNESS-SORT | Spec 08 protocol and source/fit fixtures | Give sort a write handle or release a result before its record |
| HARNESS-OPERATION-CLOSE | Existing close grammar, turn and pre-turn cases | Invent TurnClose::Stopped or add run without a turn |

## 10. Open elections and dependencies

Spec 02 owns production classifier transport, template proof and repair. Spec 05 owns
tool association, terminal metadata and R4 open-pair closure. Their production artifacts
and measured values remain required before first-release deployment. Marks, renderer and
retrieval injection are later consumers of this unit's ports. Sink durability stays in
the trace charter. Bounds remain measured values. R20-1 through R20-20 settle the
operator items from this review. Code acceptance and production qualification remain
owed.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover

[recall-shape]:
weaver-trace-Spec-amendment.md#61-serving-recall-representation
