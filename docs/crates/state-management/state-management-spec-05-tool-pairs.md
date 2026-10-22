# State management Spec 05 - Tool-call pairs

**Status:** MERGED.

**Landing PR:** #241

**Version:** 0.9 **Date filed:** 2026-10-20 **Document ID:** state-management-spec-05
**Parent:** state-management-primitives-PRD

## 0. What this document is

This Spec serves PAIR-1 through PAIR-4, CLASS-3, STORE-2, STORE-5 and the pair joins in
READ-1 and READ-4. The [PRD][prd], [October 18 R4][rulings] and [foundation][foundation]
govern it. It closes the tool-statement and inward-call seams in [Spec 02][s02]. R4
makes pair fates unclassified metadata and repairs loaded open pairs through the trace
before service. R3 permits no halt dump. The old P05-1 proposal is therefore resolved by
those rulings, not left to the coder.

The baseline is 2eb5c5a2e47f89309631327ff121393d393be85c. Message contains a Vec of
ContentBlock. ToolCall contains name and arguments. The gate sees ToolExecution
{name,arguments,clock_ms}, correlated by the organ channel's ExchangeId, not an
application call ID. Execution is serial. The sole registered external tool is sh, with
the merged shell grant and confinement from #175 through #177. This Spec retains those
shapes and that confinement.

Every replacement below applies at [FOUNDATION-TRANSITION][transition]. Until cutover
its previous serving contract remains applicable. This includes each owning-document
amendment when inserted there. The schema and port additions in section 7 are review
deltas to the foundation and must be integrated before the single document PR can clear.
No independent partial deployment is authorized.

## 1. Send identity and the admitted pair record

**PAIR-IDENTITY.** Define CallId {run:String,sequence:DecimalU64,call:DecimalU64} in
weaver-types::state_flow. Sequence names the original message.assistant, or a certified
loop.instruction for the review lane below. Call is the ordinal among its recovered
ToolCall blocks, excluding Text and UnrecoveredCall. Also retain the original content
index as call_block. Equal name/arguments never make equal calls. StatementRef is
{block_id:SourceBlockId,binding:DecimalU64}. Spec 02's adjacent Text rule supplies it,
including a corrected statement's new source where repair was necessary. Reason: a
return belongs to one emitted request, not to whichever matching command happens to be
pending. [PAIR-1, PAIR-2, STORE-5]

Inward recall uses the same CallId domain after the recovered external-call count.
Enumerate recall bindings by original Text index then binding ordinal. Its call_block is
that Text index. Reserve an ordinal for every accepted recall binding, including one
whose operation shape requires Spec 02 RECALL-REFUSAL. Such a refusal never renumbers
a later request. An invalid classifier result supplies no accepted recall binding or
CallId. Spec 02 forbids a recall binding from also serving as an external
ToolCall statement. Both kinds are handled in source-content order, with binding order
breaking an inward tie. No call ID is added to the gate payload. The harness binds the
active CallId to its outgoing ExchangeId before sending. [READ-3, PAIR-3]

**PAIR-OPEN.** Add flow/pairs.rs in the harness. For every conforming recovered request,
including requests that a plan break, stop or round bound prevents executing, author
tool.call.started with the typed PairStarted payload below. It replaces that kind's
current deferred name/arguments object in serving. It means the communication opened,
not that an OS command ran. Wait for both the statement's filed receipt and the
PairStarted receipt before dispatch. A correctly rejected statement goes through Spec 02
repair first and grants no execution. Missing statement association is a recorded
protocol refusal with no pair, never a guessed send. Reason: a later repair must not
replace the identity of an already executed request. [PAIR-1, CLASS-3, CLASS-9, SEAM-2]

An accepted recall binding with invalid operation shape takes RECALL-REFUSAL before
pair creation. Its typed refusal retains the reserved request identity without a
PairStarted, PairEnded or fabricated subject. The accepted selected statement remains
an ordinary filing. Only conforming recall requests enter the pair ledger. [READ-3,
PAIR-1, CLASS-10]

PairStarted is {call_id,call_block,statement,lane,request,retry_of}. PairLane is
external, recall or review. PairRequest is externally tagged
external:{name:String,arguments:String}, recall:{subject:String}, or
review:{setup:ReviewSetupRef,work:WorkRef,command:String}. Spec 07 owns ReviewSetupRef
and WorkRef. The trusted caller supplies command as the exact script sent in
ToolExecution.arguments. lane has type PairLane and request has type PairRequest.
retry_of is CallId or null. The two tags must agree. Source attribution names the
original request block. The pair entry is metadata about that request and carries no
additional STE100 statement. [PAIR-1, PAIR-3, STORE-2]

**PAIR-PROJECTION.** Extend the trace registry with one complete `pair` projection for
tool.call.started and tool.call.completed. It copies the entire canonical payload at
virtual block index zero. The source_path is the empty path, naming the payload root.
This projection produces PairCopy, a separate AdmittedCopy variant from StatementCopy.
It is permitted only for those kinds. The tee still selects by kind, channel and origin
and never examines the fate to decide admission. [PAIR-3, TAP-1, TAP-4, SEAM-2]

Both kinds use channel tool. Started has origin model for model-requested calls and loop
for review calls. Completed has origin tool for a gate answer and origin loop for a
harness or inward outcome. Retain subsystem harness/gate as attribution of who supplied
the answer. A load-time coverage check requires the whole pair projection for every such
tuple and all model Text statement blocks in the enabled serving tool path. Failure is
StateManagementInvalid at partition, forbidden_projection with the missing tuple in
detail. The check belongs to the loader, never the tee. [TAP-2, TAP-3, PAIR-1]

Review calls use the instruction's sequence, call zero and call_block zero. Its
StatementRef names that one-Text instruction. Only the trusted pair port can construct
this lane, on an explicit caller request with a filed certified instruction, current
plan, setup identity and WorkRef. Review uses the loaded sh clock, never retries
automatically, and shares every receipt, terminal and stop/leave rule here.
Model-emitted sh calls retain the external lane and cannot become review evidence by
returning review-shaped JSON. [PLAN-2, PAIR-1, October 18 R6]

message.tool_result remains the model-facing grant record. It is not a second state
filing carrier. The loader refuses its projection, and neither it nor recall is
selectable as retained content. The pair's whole ToolOutcome is held from the completed
record instead. There is no tool.pair_outcome kind and no classified terminal statement.
Reason: one terminal fact must not produce a second return in state or depend on a
language classifier. [PAIR-3, STORE-2, October 18 R4]

## 2. Outcome mapping and rejection

**PAIR-END.** Replace the serving tool.call.completed payload with PairEnded
{call_id,execution,fate,reason,outcome,loaded_from}. execution is an ExchangeId or null.
fate is completed or failed. reason is null for completed and a closed PairFailure for
failed. outcome is the existing complete ToolOutcome or null. loaded_from is
StartIdentity only for abandoned_at_load, otherwise null. This record is the only state
terminal carrier. Completion is metadata with its full block, not a new classification
request. [PAIR-1, PAIR-3, STORE-2, October 18 R4]

| Source and outcome | Pair fate and reason | Held block and model-facing evidence |
| --- | --- | --- |
| Gate Result, including nonzero shell exit | completed, null | Whole ToolOutcome, existing granted message.tool_result |
| Gate Refused | completed, null | Whole refusal ToolOutcome, grant records the refusal |
| Gate Errored | completed, null | Whole infrastructure ToolOutcome, grant records the error |
| Gate Killed by clock | failed, timeout | Whole Killed outcome with any partial attachment, never a successful return |
| Gate Killed by cancel | failed, cancelled | Whole Killed outcome, existing stop rule decides delivery |
| Gate Killed by unload | failed, unload | Whole Killed outcome, marked eligible for an explicit later re-ask |
| Gate Killed by flow_failure | failed, flow_failure | Terminal trace account only after latch, no filing on the broken flow |
| Harness closure Killed by fault | failed, gate_fault | Whole indeterminate outcome, never automatically retried |
| Harness Refused because no tool grant | failed, not_granted | Whole harness refusal, no gate exchange, model grant preserved |
| No dispatch due to pass purpose, repair exhaustion, plan break, round limit, stop, leave or absent gate | failed, not_dispatched with cause | No fabricated gate return, visible model notice where the turn continues |
| Inward recall read result | completed, null | ToolOutcome::Result containing the complete encoded read result, recall event and grant |
| Valid opened inward recall operation refusal | completed, null | ToolOutcome::Refused naming EmptySubject or the typed read refusal, no empty-history success |
| Open pair found on load | failed, abandoned_at_load | No outcome or model turn, current trace event names loaded_from |

PairFailure is externally tagged timeout:{}, cancelled:{}, unload:{}, flow_failure:{},
gate_fault:{}, not_granted:{}, not_dispatched:{cause}, or abandoned_at_load:{}. The
cause is pass_purpose, repair_exhausted, plan_break, round_limit, stop, unload or
no_gate. Those are metadata codes, not text passed through the classifier. All non-null
outcomes retain their exact received content. A partial attachment remains marked
partial and never satisfies a completed pair. [PAIR-1 through PAIR-4]

For a not-dispatched notice, the harness uses the same private grant-construction
pattern as its existing not-granted refusal, and records who spoke. It does not pretend
a gate exchange occurred. Tools remaining after the first interrupted call are each
closed through this row, instead of disappearing at the stopped dispatch batch.
UnrecoveredCall has no recovered send identity and keeps its existing recovery account
without a fabricated pair. [PAIR-1, STORE-5]

An outbound identity choice and the plan check can bar execution, but cannot erase a
conforming recovered send. Open every such pair and then close each barred request as
not_dispatched/plan_break. A missing tool grant instead uses not_granted with its
complete refusal outcome. All request and terminal receipts precede a healthy turn
close. No gate exchange is opened for either case. A correctly rejected block with no
accepted request statement remains outside this ledger under Spec 02. [PAIR-1, PAIR-3,
PLAN-5, CLASS-10]

Purpose and cancellation checks run after send association and before execution.
pass_purpose records that the caller supplied a purpose which bars model-requested
dispatch. The primitive checks that capability and elects no pass schedule. A completed
repair-cap disposition uses repair_exhausted for the original pass's remaining
conforming sends. A stop or leave already accepted takes precedence, using stop or
unload. A broken flow permits no further filing under FLOW-4. Refusing execution is
never a reason to lose a known conforming pair. [PAIR-1, CLASS-9, PLAN-1, STRAT-2,
FLOW-4]

**PAIR-MATCH.** A gate answer is eligible only for the one live execution's exact
ExchangeId, opener, close position and ToolOutcome payload, while its CallId is opened
and its send receipts are held. The gate's first-outcome rule decides races among
result, clock, cancel and interrupt. The harness does not compare arrival time to a
second timeout. The matching pass certifies exchange identity and disposition, not the
return's truth or side effects. Evidence is the recorded send, file receipts, exchange
mapping and exact outcome. [PAIR-1, PAIR-2, PAIR-3]

A valid late answer for an exchange the harness already closed by fault is authored as
tool.return_rejected before it can be discarded by the lower path. Keep that closed
exchange/CallId mapping through lower and gate reap. A duplicate known close uses the
same kind with duplicate_return. An unexpected exchange has reason unexpected_exchange
and call_id null unless its mapping is known. A malformed organ frame is a protocol
fault, not a fabricated tool return. Preserve the merged fault and teardown behavior
after recording the available raw evidence. [PAIR-2, PAIR-3, FLOW-3]

RejectedReturn is {call_id:CallId-or-null,execution:ExchangeId,reason,
raw_envelope:RawValue}. It records the entire received envelope with no normalizing
parse/reserialize. The kind uses channel tool, origin tool, and is forbidden by the
partition validator. A duplicate or late response never changes a stored terminal. No
new unknown_call or wrong_tool payload case is invented for a wire that carries neither
name nor application call ID on its close. [PAIR-2, PAIR-3, TAP-1, TAP-3]

## 3. The coordinator and the single clock

**PAIR-STATES.** The harness owns one coordinator and the state channel owns the filing
order. Table rows are test cases. All pending records retain their source identity,
copied bytes and receipt state. A stop changes dispatch eligibility, never rewrites an
already recorded fate. [PAIR-1, PAIR-4, FLOW-4]

| State | Event | Action and next state |
| --- | --- | --- |
| Candidate | Statement accepted with unique association | Author started, wait for both receipts, Opening |
| Candidate | Correct statement rejection | Return the correlated Ste100Reply not_ste100 result to the owning RepairLedger, no dispatch and no opened pair yet |
| Candidate | Stop or leave before accepted send | Cancel candidate, preserve source disposition, no fabricated pair |
| Opening | Both receipts recorded locally | Opened |
| Opening | Dispatch barred by purpose, repair cap, plan, stop, leave or round limit | Finish the healthy opening, then author the mapped not_dispatched terminal, Ending |
| Opened | Eligible external dispatch | Bind ExchangeId, send ToolExecution, Executing |
| Opened | Eligible inward dispatch | Run pure constructor and bounded state read, ExecutingInternal |
| Opened | No grant or dispatch barred by purpose, repair cap, plan, round limit, stop, leave or no gate | Author mapped failed terminal, Ending |
| Executing | Gate's matching terminal answer | Author PairEnded from mapping, Ending |
| Executing | Stop or leave | Send existing cancel/interrupt once, wait for outcome/containment under existing rule |
| Executing | Gate death or interrupt escalation | Follow merged containment, author gate_fault terminal when possible, Ending |
| ExecutingInternal | Read result or explicit read refusal | Author mapped terminal, Ending |
| Ending | Terminal filed | Completed or Failed, immutable |
| Completed or Failed | Later gate close | Rejected-return trace, no new filing |
| Any pending state | Filing refusal, broken carry or applicable blocking bound | Foundation halt, preserve outstanding state, no dump |
| Any state | Existing flow halt | Stop dispatch, containment/trace account only, no new state write |

Before opening the next gate execution, the previous exchange has answered and required
pair records have filed. A send/terminal receipt wait uses Spec 02's measured
decision_wait while it blocks the next decision. It does not clock tool execution.
During leave the earlier existing leave deadline wins. During boot the remaining load
deadline wins. No partial write or unrelated readiness resets any deadline. [PAIR-4,
CLASS-8, FLOW-4]

**TOOL-CLOCK.** Keep ToolExecution.clock_ms and the gate's existing kill and containment
completion rules in harness-gate contract section 2. The harness has no independent
expiry decision. Keep the gate's current maximum of 60000 ms, and check a deployment's
chosen clock is positive and no greater than that maximum at load. This is an existing
tool bound, not a newly measured choice in this Spec. Qualify the chosen value and its
measurement reference under the actual command workload. [PAIR-1, PAIR-2]

Add `[state-management.tools.sh]` with clock-ms:u64 and measurement-ref:String. Both are
required when tool-set elects sh. Refuse unknown names, a clock for an unelected tool, a
missing policy or an out-of-range clock as ConfigInvalid naming the key. Retry
scheduling and attempt limits belong to the caller. Retire
engine.rs::TOOL_CALL_CLOCK_MS. The offered shell schema is rendered from the loaded
clock instead of the process-global LazyLock, and its fixed prose is in the
load-certified template inventory where it can enter state. [PAIR-2, CLASS-2, FLOW-8,
TAP-2]

Move the shared shell maximum into the floor's tool vocabulary so admin validation and
gate refusal consume one constant. The gate retains its refusal for an invalid wire
value, even after load validation. No change is made to ShellGrant, the fixed command
set, output confinement, network denial or the merged tool registry. The complete
ToolOutcome the gate returns is the pair block, including its existing explicit
output-bound notice. State performs no further truncation. [PAIR-3, PAIR-4]

**RETRY.** retry_of is an optional lineage reference supplied by the caller when opening
a fresh request. It must identify a filed terminal external pair from this run, with the
exact same name and arguments. It cannot name itself, an open pair or an ancestor from
another run. The new request gets a new CallId and gate exchange. A changed request has
null retry_of. The primitive validates lineage, not whether, when or how often a tool
should be retried. It has no retry timer or automatic replay. [PAIR-1, PAIR-2, STORE-5,
R20-12]

**COMPLETED-PAIR.** After the terminal receipt, the pair port returns a private
CompletedPair holding {started:PairStarted,ended:PairEnded,
terminal_source:SourceBlockId,receipt:FiledReceipt}. FiledReceipt is the harness-private
grant {request_id:DecimalU64, block_id:SourceBlockId}, created from a fully validated
StateFrame::Filed and its recorded observation. It is not another wire frame. Only the
pair coordinator can construct it, after matching both commits and the terminal mapping.
A gate response, model text or read record cannot mint this grant. When closure uses a
review pair, it additionally requires lane review, completed fate and
ToolOutcome::Result under Spec 07. A category permitting closure without review uses
attest_closure instead, never a fabricated pair. [R20-17] [PAIR-1, PAIR-3, PLAN-2]

**RETURN-CARRY.** The port retains terminal source and completion grants until
acknowledged or unloaded. take_result(CallId) returns a borrowed complete result grant
or NotReady, UnknownCall or AlreadyDelivered. It selects no model pass.
acknowledge_delivery(CallId,placement_receipt) requires actual completed returned
coverage from Spec 06 before marking delivered. Cancellation of a placement leaves the
grant available. The caller may instead fetch the whole held pair when the original
request is no longer in the argument. Spec 08's result-unit constructor refuses a
missing kept request. No operation edits a terminal fate or drops part of a return.
[PAIR-4, STORE-5, PLACE-4, R20-12]

The result pool is session-local placement bookkeeping. Unload discards it only after
sources are held, and reload reads immutable pairs. It is not a retry queue, durable
delivery field or second state feed. The caller decides whether to place a raw review
return or a later outcome statement. [PAIR-3, PERSIST-6, DEFER-2, R20-12]

## 4. Storage, reads and boot repair

**PAIR-FILE.** Add the shared PairFile {block_id,source,partition_rule_id,source_path,
admission,raw_block,scheme_id,pair} and the state-flow/1 operation file_pair with field
filing. The common request identity and filed/refused answer remain the foundation's.
pair is externally tagged started:PairStarted or ended:PairEnded, and must match the
copied event kind and exact raw payload. Admission is admitted. No binding list or Text
field is invented for this metadata. The member verifies scheme, source, expected open
state and statement reference, then commits atomically before its filed receipt. Refusal
rolls back and is a flow failure. [SEAM-2, PAIR-3, PAIR-4, STORE-2, October 18 R4]

PairFile includes scheme_id, equal to the open/store scheme as StatementFile does. The
pair metadata's source is a new current-run event even when its CallId names an older
run during boot repair. A source may be filed only once. PairFile preserves source_path
as the empty path and block_id.block as zero for the whole-payload projection. It has no
parser-side route from an unmatched return to ended. [PAIR-3, STORE-5, TAP-4]

**PAIR-SCHEMA.** The [store-owned catalog][state-catalog] defines content_type,
pair_event, tool_pair and the pair_statement/pair_open indexes. They belong to the
initial state-content/1 catalog before its first cutover. No smaller draft catalog is
accepted and no runtime ALTER converts old agents. Statement and seed writes set
content_type statement. Pair writes set pair and statement_text NULL. Every admitted
item shares content_block's global commit order, so position, shape and save-frontier
reads include pair events. Counter blobs use the foundation's 8-byte big-endian
encoding. [STORE-3, STORE-5, PAIR-1, PERSIST-4]

On started, resolve StatementRef to exactly one committed statement_binding, check its
source, lane and binding shape, insert content_block and pair_event at its ID, then an
opened tool_pair. The harness owns the Spec 02 adjacency/repair proof, which cannot be
reconstructed from the Text-only statement filing. On ended, find exactly one opened
CallId, validate the closed outcome mapping, insert content_block and pair_event at its
ID, and update that row's fate/reason/terminal_event. Pair source block must be zero and
its kind must agree with record_type. Zero updated rows, duplicate source, absent
statement or a second terminal refuses the transaction. No SQL update runs merely
because the harness sent bytes. terminal_event points to the actual admitted terminal
block, never a model claim or inferred timeout. [PAIR-1, PAIR-3, SEAM-2]

A recall opening requires exactly one held noun, a null governing condition and exact
subject equality with that noun. Nonempty membership in a compound noun list is
insufficient. These are mechanical checks on the stored binding, not reclassification.
An invalid opening refuses invalid_pair and rolls back the whole transaction. Apply
the same shape and identity checks during saved-image adoption. This makes explicit
the existing RECALL and binding-shape obligation, without admitting invalid-operation
requests into the pair schema. [READ-3, PAIR-4, PERSIST-4, CLASS-2]

The terminal transaction uses the following guarded update after inserting its content
and pair_event rows. The member requires exactly one changed row, otherwise it rolls
back those inserts as well. reason is canonical PairFailure JSON or NULL. No statement
changes terminal rows after this transition. [PAIR-1, PAIR-2]

```sql
UPDATE tool_pair
SET fate = :fate, reason = :reason, terminal_event = :terminal_event
WHERE run = :run AND sequence = :sequence AND call = :call AND fate = 'opened'
```

Add file_pair refusal codes unknown_pair, already_terminal, invalid_pair,
statement_missing and duplicate_pair to the file refusal family. A conformance rejection
has already disposed a statement before this point and is never represented by a store
refusal. The store verifies structure and references, not STE100 meaning. [PAIR-4,
CLASS-8, SEAM-2]

**PAIR-READ.** Add required pairs:Vec<ReadPair> to the foundation's ReadEntry, empty for
a statement with none. ReadPair has call_id:CallId, statement:StatementRef,
lane:PairLane, request:PairRequest, fate:PairFate, reason:PairFailure-or-null,
opening_source:PairReadSource and terminal_source:PairReadSource-or-null. PairLane and
PairRequest are PAIR-OPEN's closed types. PairFate is opened, completed or failed.
PairReadSource is {source:EventSource,raw_block:RawValue}, using the foundation's exact
EventSource fields. Opening is a complete PairStarted payload, terminal is a complete
PairEnded payload and is null only while opened. The request's verb and noun therefore
lead to its terminal block through pair_statement. ReadPair has no SQL ID. A completed
return is selected whole with its request reference. A failed pair exposes its fate and
any partial attachment without pretending a successful return. [READ-1, READ-4, PAIR-3,
PAIR-4]

For each returned statement, the store obtains its pairs with the following fixed query.
Internal IDs stay inside the store. It resolves each source from the selected
content_block rows and reconstructs StatementRef from the binding's block source and
ordinal. No caller supplies SQL. [READ-1, SEAM-3]

```sql
SELECT p.*, o.envelope AS opening_envelope, o.raw_block AS opening_block,
       t.envelope AS terminal_envelope, t.raw_block AS terminal_block
FROM tool_pair AS p INDEXED BY pair_statement
JOIN content_block AS o ON o.id = p.opened_event
LEFT JOIN content_block AS t ON t.id = p.terminal_event
WHERE p.statement_id = :statement_id
ORDER BY p.opened_event
```

Add ReadSelector::OpenPairs {}, yielding QueryResult's new open_pairs variant with
Vec<ReadPair>, ordered by opened_event. It uses the same bounded snapshot/paging
contract and refuses result_too_large instead of skipping pairs. QueryResult becomes an
externally tagged family that includes entries:{entries:Vec<ReadEntry>} and
open_pairs:{pairs:Vec<ReadPair>}. TYPES-READ owns the complete selector/result
association, including mark and plan results. Content selectors return entries. Each
binding in those entries is a ReadBinding with its original filed ordinal. This is an
explicit C05-2 delta to both read endpoints, not an extra unrecognized field. [PAIR-1,
SEAM-3]

```sql
SELECT p.*, o.envelope AS opening_envelope, o.raw_block AS opening_block
FROM tool_pair AS p INDEXED BY pair_open
JOIN content_block AS o ON o.id = p.opened_event
WHERE p.fate = 'opened'
ORDER BY p.opened_event
```

Conversation and ordinary span text readers select content_type statement. They do not
try to parse pair metadata as Text. A marked return uses ReadPair's whole terminal block
under the marks amendment. Shape and position still see all admitted content types.
[READ-4, STORE-5, MARK-3]

Spec 08's UNIT owns held-pair reconstruction and the fresh result-unit distinction. The
held adapter can use the pure ToolOutcome formatter, but cannot mint ToolResult. Fresh
completions retain their private grant and exact model-facing record. Replaying a held
request never starts a communication or allocates a new CallId. [PAIR-1, PAIR-3, FLOW-8]

**BOOT-REPAIR.** After verified load, template checks and the mandatory prompt read, and
before opening the user channel, query open_pairs once. For each returned pair, author
current-session tool.call.completed with failed/abandoned_at_load, execution null,
outcome null and loaded_from equal to the verified StartIdentity. It has no turn and
passes the ordinary tap, PairCopy and file_pair receipt path. No old row is edited
directly at load. Wait for every receipt and a second empty open_pairs read before enter
succeeds. All work shares the remaining load budget. Failure refuses enter and rolls
back the process set. [PAIR-1, PERSIST-6, FLOW-1, October 18 R4]

The boot pass certifies that this loaded store has no open communications before
serving, based on the query and committed terminal events. It does not prove what an
interrupted external command did. A crash during repair publishes no new dump. Another
load begins from its selected artifact and performs the same normal-flow repair again. A
changed system prompt still requires a new seed. [PAIR-1, PAIR-2, PERSIST-3, PERSIST-7]

## 5. Stop, leave and broken flow

**PAIR-LEAVE.** In S3 stop, cancel the current gate execution using the existing
ToolCancel, then close every undispatched conforming request with not_dispatched/ stop.
In S4/S5 leave, use the existing ToolInterrupt and close remaining requests with
not_dispatched/unload. In S10 forcing, use the existing lower/containment and remaining
leave budget, then file healthy-flow terminal records before a snapshot. A gate result
that won first keeps its completed fate. Gate death uses gate_fault, including the
merged S3/S4/S5/S10 containment distinctions. [PAIR-1, PAIR-2, PERSIST-4]

The coordinator names every recovered conforming call before testing MAX_TOOL_ROUNDS or
breaking the execution list. The round bound remains the merged 32 rounds, but each call
it prevents receives a terminal event. Diagnostic mode is outside this serving ledger
and gains no gate or serving pair requirement. A missing gate in serving is accounted
under the existing lifecycle policy and no_gate for undispatched requests, not an
implicit diagnostic fallback. [PAIR-1, SCOPE-1]

Before any healthy save barrier, pending pair starts and terminals count among the owed
items. A terminal not filed by its applicable blocking bound fails the flow, so no new
save point is published. On a flow halt, stop decisions and contain the active tool
under the foundation's FlowFailure rule. Record the outstanding pair states where the
trace can carry them. Do not attempt a terminal store write or a halt dump through an
already broken feed. Later boot repair closes pairs present in the last selected save
point. [FLOW-4, FLOW-5, PAIR-4, October 18 R3, R4]

## 6. What is enforced, and by which instrument

The dispatch pass certifies a unique conforming send and two committed receipts before
action, with a live eligible exchange or inward request. It does not certify safe side
effects, successful command exit or complete output beyond the gate's stated output
contract. The terminal pass certifies one mapped fate for that CallId, based on the
matching exchange or a named harness closure. These passes use recorded source, receipt
and outcome evidence, not elapsed-time guesses. [PAIR-1, PAIR-3]

| Claims | Instrument | Protective change that must fail |
| --- | --- | --- |
| PAIR-IDENTITY, PAIR-OPEN | Real harness generation with same-emission identical calls and mixed block types | Collapse equal calls, count UnrecoveredCall as recovered, or dispatch before either receipt |
| PAIR-PROJECTION | Valid/invalid partition matrix with real tee copies | Select model-facing message.tool_result, omit a required tuple or inspect fate inside tee |
| PAIR-OPEN, RECALL-REFUSAL | Mixed accepted recall bindings and correction passes through the real association owner | Open an invalid-shape request, invent a subject or change a later CallId |
| PAIR-END | Every row of the outcome table through real codecs and store transactions | Treat a clock kill as completed, classify a fate or hide a partial attachment |
| PAIR-MATCH | Scripted organ exchanges and real lower drain | Discard a late close without trace, accept a wrong ordinal or reopen a terminal |
| PAIR-STATES | Row-by-row coordinator fixture, including stop/leave before receipts | Skip unopened-list closures or advance a terminal before commit |
| TOOL-CLOCK | Gate integration with short fixture clock, descendants and load validation | Start a second kill clock, answer before containment or accept a clock above the gate maximum |
| RETRY | Fresh and repeated requests with changed arguments, foreign run and open predecessor | Reuse CallId or accept invalid lineage |
| PAIR-FILE, PAIR-SCHEMA | Real rusqlite transactions, forged compound/conditional recall openings and saved-image adoption | Leave half a pair row, accept a duplicate terminal, accept subject membership without exact recall shape or receipt before commit |
| PAIR-READ | Noun/condition read with complete joined outcomes and byte-limit cases | Return a partial block, skip over-large rows or detach the request reference |
| BOOT-REPAIR | Dump with several open pairs, failure at each repair receipt, reload | Serve before the empty recheck, mutate during adoption or invent a return |
| PAIR-LEAVE | S3/S4/S5/S10, round bound and mid-list gate-death integration | Drop remaining requests, save ahead of closure or publish after broken carry |
| Private grants and pure inward helper | Compile-fail and manifest instruments | Deserialize a grant, construct one from a record, link weaver-state into harness or give weaver-internal I/O |

Each behavior test must fail when its stated guard is removed or inverted. Extend
harness-tool-result-granted-not-minted to the inward read and not-dispatched private
construction sites. The loop still cannot mint ToolResult from a recorded block. The
real SQL tests include foreign-key checks, exact pair identity and both successful and
refused terminal transactions. [PAIR-3, SEAM-2]

RETURN-CARRY requires a stopped-turn fixture whose terminal receipt precedes stop and
whose result has not been appended. On the next turn it appears once, without another
call or another result event. Repeat with a refused place and a cut that removed the
request. Removing the pending list, duplicating its authoring or placing a return
without the request must fail. Purpose/cap fixtures require every known conforming
send's terminal even when it never dispatches. [PAIR-1, PAIR-4, FLOW-8, MARK-4]

## 7. Owning-document amendments and integration

**PAIR-AMENDMENTS.** These clauses are drafted for the document act, not left to
implementation. Each receives the transition sentence from section 0. Reason: the shared
fate and metadata path must exist on both sides before code uses it. [PAIR-1 through
PAIR-4, October 18 A1, R4]

| Owner and clause | Replacement or addition |
| --- | --- |
| weaver-harness-Spec sections 5, 6 and 8 | Add PAIR-IDENTITY through PAIR-MATCH, PAIR-STATES, TOOL-CLOCK, RETRY and PAIR-LEAVE. Keep serial gate execution and private grants. Add the inward read grant and never-dispatched closures. Render the shell clock from the loaded declaration. |
| weaver-harness-gate-contract sections 2, 4 and 5 | Execution still carries name/arguments/clock_ms and the channel's ExchangeId alone. First outcome and containment remain the gate's. Pair fate is a harness/store account derived from that outcome, not a second kill clock. A known late close is recorded rejected before lower discards it. |
| weaver-harness-state-contract sections 2, 3, 4 and 8 | Add unclassified admitted pair metadata, atomic file_pair receipts, read joins and open-pair boot repair. The member refuses unknown pairs or a second terminal. A matched return is held whole under the request's statement binding. |
| weaver-types-PRD section 2.3 and Spec sections 2 and 4 | Add CallId, StatementRef, PairStarted, PairEnded, PairFailure, PairFile, ReadPair and tool policy. Retain ToolExecution and ToolOutcome shapes. Share the existing shell maximum constant. |
| weaver-state-Spec sections 3, 4 and 5 | Add the complete PAIR-SCHEMA, file_pair transaction and pair read joins to the serving catalog and its adoption validator. Diagnostic schema stays separate. |
| weaver-trace-PRD section 3.1 and Spec sections 3 and 11 | Extend started/completed payloads and admit one new kind tool.return_rejected. Pair terminal is turnless only during boot repair. Registry projection and load coverage are PAIR-PROJECTION. Model-facing result records remain unselectable. |
| weaver-harness-Spec section 4 and harness-diagnostic contract vocabulary | Converter rejects serving pair payloads/new rejected-return kind as outside diagnostic vocabulary. Legacy diagnostic records retain their old shapes. |
| weaver-admin-Spec sections 3.0 and 6 | Enter confirmation waits for BOOT-REPAIR. Healthy save waits for the pair frontier. Broken flow still follows foundation no-save/failed-run cells. No new lifecycle command or timeout clock. |
| weaver-internal-Spec sections 2 and 4 | Use Spec 02's pure recall-query constructor. Harness executes and accounts the read as an inward pair. No socket or model-facing tool schema belongs to the helper. |

This revises the harness-gate supplies clause that unopened remaining calls have no
further account: they still open no gate exchange, but the harness now owes their failed
pair records. It does not change the gate's meaning of its four outcomes. No
tool.pair_outcome kind or classifiable failure template is added. [PAIR-1, PAIR-3,
October 18 R4]

| Delta | Required foundation integration |
| --- | --- |
| C05-1 | Expand state-content/1 before first deployment with the content_type discriminator, nullable statement_text for metadata, pair_event/tool_pair and exact catalog checks. All types share content_block order. Add PairCopy and file_pair to both endpoints. Existing smaller draft catalogs are not accepted as production state-content/1. |
| C05-2 | Add ReadPair and the explicitly tagged QueryResult, update all reads and the seed/boot reader, and add OpenPairs. No consumer assumes the old untagged entries object. |
| C05-3 | Extend the registry's whole-payload pair projection, tuple coverage and forbidden model-result/rejected-return projections. The integrated initial-release registry is state-copy/1, replacing the draft state-text/1. |
| C05-4 | Add the new typed pair payloads and turnless boot-completion licensing. Keep message.tool_result as placement evidence, not the return filing carrier. |
| C05-5 | Add the tool clock, retry lineage metadata and common shell maximum to types/admin validation. Pair receipt waits use the reviewed C02-3 decision domain and the existing earlier load/leave bounds. |
| C05-6 | The flow failure account adds outstanding_pairs, with each CallId, coordinator state, optional ExchangeId and recorded/committed opening and terminal source IDs. Capture that account before teardown and stop dispatch immediately. It does not authorize new filing after the latch. |

The original Spec 01 and generic X01 effect array are not alternate pair interfaces. The
foundation amendments carry these deltas under FOUNDATION-TRANSITION. Runtime
integration remains owed.
[SEAM-1, SEAM-2]

## 8. Open elections and evidence

**PAIR-MEASURES.** The per-tool clock is an explicit deployment choice supported by
duration, refusal, interruption and late-return observations. Measure actual gate
execution and containment separately. Do not tune an execution clock from classifier
lag. Freeze selected values before qualification, and retain the gate maximum unless its
owning Spec is separately changed. Inward recall uses the store read bound and no tool
kill clock. [PAIR-2]

Integration of C05-1 through C05-6, the pure recall consumer, the Spec 02 tool statement
check and real boot-repair tests remain required. Spec 07 supplies review requests
through the same pair ledger and names its loop-authored send source. Its closure
consumer uses HARNESS-PLAN-PORTS, and the instruction grammar is shared with Spec 02.
The loop supplies review policy and invocation timing. No new operator choice is hidden
in a timing rule. The old P05-1 needs no additional ruling after R3/R4. [PAIR-1, PLAN-2,
October 18 R3, R4, R6]

This Spec consumes FOUNDATION-RECEIPT's loss/receipt boundary and FOUNDATION-FAILURE's
distinction between conformance and component failure. Whole-pair and boot-repair
acceptance remain implementation obligations. [PAIR-1, FLOW-7]

[prd]: state-management-primitives-prd-v0_6.md
[foundation]: state-management-foundation-Spec.md
[transition]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
[s02]: state-management-spec-02-protocol-classifier-gate.md
[rulings]: state-management-foundation-Spec.md#0-what-this-document-is

[state-catalog]: weaver-state-Spec-amendment.md
