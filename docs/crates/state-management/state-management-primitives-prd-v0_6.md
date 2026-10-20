# State Management Primitives - PRD

**Status:** MERGED. Requirements change only on a return from the reviewer seat or
an operator ruling the changed clause names.
**Version:** 0.6
**Date filed:** 2026-10-16
**Document ID:** `state-management-primitives-PRD`
**Editorial:** ASCII, no semicolons, prose at 88 characters.

## 0. Authority and reading

This document charters state management for WeaverTools' agent. It states the
requirements of each primitive and the obligations at its seams. It does not choose
representations, group the eventual specs, or prescribe their build order. The
operator has elected wall compaction as the bring-up path and dependable trace-to-store
flow as a prerequisite. Those elections are requirements, not a schedule invented
by this charter. [H5.3, H9, G6]

The lock-and-first-spec handoff of 2026-10-16 takes precedence over the earlier
returns and closes the gate wait. After this lock, no requirement changes without a
return from the reviewer seat or an operator ruling, which the changed clause names
by its date. [L1]

The operator's copy-and-partition clarification follows the fourth review return and
clarifies the tap without changing the trace-head boundary. The sources otherwise,
in descending authority, are the fourth, third, second, and first review returns of
2026-10-16, the operator's earlier tee guidance of that date, the State Management
PRD handoff of that date, and the frozen [State Management Primitives, graphable
v0.14][paper]. The Guiding Statement and Working Rules frame the program's separation
of mechanic and motive. They are not requirement authorities for this PRD. Existing
state management documents and code are suspect inventory evidence, not requirements
for this charter. [G7, V5:0, V4:0, C2, H0, H9, G0]

The operator questions are settled. The fourth return adds the cut wait bound,
deferred prefetch, declaration validation, trace duties, and falsifiers. The open
lists name only spec work or measured values.
The first-spec inventory is carried in the [foundation inventory table][inventory].
It is implementation evidence, not a second authority for requirements. This charter
governs the revised target. [V5:6, V4:1, V4:6]

[inventory]: state-management-foundation-Spec.md#6-owning-amendments-and-enforcement

[paper]: ../../project/graphable-state-management-primitives-v0_14.md

Source tags use these names:

| Tag | Source |
| --- | --- |
| L1 | Section 1 of the lock-and-first-spec handoff, bounded gate wait and PRD lock |
| Dn | Item n in the fourth review return |
| V5:n | Section n of the fourth review return |
| Bn | Ruling n in the third review return |
| V4:n | Section n of the third review return |
| An | Answer n in the second review return |
| Pn | Pending-item ruling n in the second review return |
| V3:n | Section n of the second review return |
| Cn | Correction n in the first review return |
| Rn | Ruling n in the first review return, subject to later returns |
| REV0 | First review return section 0, seats and authority |
| REV4 | First review return section 4, plan and forward flags |
| REV5 | First review return section 5, now settled by P1, P2, and P3 |
| Hn | Section n of the operator's handoff, including its subsections |
| P:path | Node `loop-state-mgmt/path` in graphable v0.14 |
| G0 | Operator's instruction that old state management documents and code are suspect |
| G1 | Tee guidance, corrected invariant and permitted transport control messages |
| G2 | Tee guidance, trace as sole source and tee failure as agent failure |
| G3 | Earlier tee admission and exclusion guidance, subject to C1, A3, V3:3, D3, and G7 |
| G4 | Tee guidance, tee yields, classifier labels, loop carries |
| G5 | Earlier trace custody guidance, superseded for this PRD by A1 and A5 |
| G6 | Tee guidance, prerequisite scope and instruction to proceed to this PRD |
| G7 | Operator clarification after the fourth return: the trace runs whole, the tee copies, the partition selects from the copy, and the unselected copy is discarded |

Each numbered requirement gives its source and reason. A source that gives no reason
is marked accordingly. Open-for-spec lists distinguish a value owed to measurement
from an implementation choice. No operator question remains open in this version.
No candidate in the paper becomes binding merely by appearing in this document.
[H3, H4, H7, H9]

## 1. The capability and its timing

**CAP-1.** State management comprises retain, sort, and place. A primitive performs
its mechanic and holds no motive. The consumer declares the criterion and the loop
composes the primitives. Reason: the same parts must compose into different agents
without embedding a consumer's purpose in a primitive. [H0, P:ladder/builder,
P:after/capability]

The chartered parts are the tee, the store that performs retain, the outbound sort,
the serve-side sort, the classifier that performs model-based sort, and the renderer
that performs place. The partition is a declaration applied by the tee, not a
separate primitive. The loop, trace, engine, and SPU are parties at their seams.
[H1, H5.1, H6, P:walk/rung-6]

The deferred prefetch organ is an instance of serve-side sort at rung six, not an
additional mechanic. It fills candidate bins from retain while the decoder works,
with release governed by place, N, and marks. [D2, P:ladder/step-3]

**CAP-2.** The verbs are fixed. The tee taps and writes nothing. The partition admits.
The loop carries. The store writes and reads. Retain holds through the store. Sort
reads, labels, joins, and selects, and no sort writes the store. Place lays the argument
head and tail and reports N, and never drops. Reason: admission, carrying, filing,
selection, and arrangement have distinct duties with one operating content feed.
Seeding changes the pre-agent origin, not the store's ownership of writes. [H1,
P:rulings/sort-twice, G4, V3:3, B1]

**CAP-3.** State exists only during a pass, is not readable afterward, and belongs to
nobody. During operation, residue is admitted material copied off the tap, which the
trace also carries. The seeded system prompt and its end mark are the sole pre-agent
exception to that origin, accounted for by the seed's A and B in the first trace head.
The buffer and saved buffer entries are intermediate computation owned by the engine.
They are neither residue nor state, since they outlive a pass. Reason: the operating
agent receives the tap's copy, while the operator creates its initial holdings before it
exists. Sink durability remains the trace's duty, and engine buffer bytes remain
different from retained content. [B1, A5, H3, P:terms-drawn-in]

**CAP-4.** Context enhancement from an output reaches a later cycle, never the current
pass. One cycle is the floor for its use through state management. The emission goes on
the trace, the tee copies, the partition admits from that copy, and the classifier
labels or correctly rejects the admitted material. The loop carries accepted material to
the store. With automatic retrieval enabled, the next inbound turn queries filed history
under READ-2. Filing may lag further while recent output remains in the argument.
Reason: a pass never sees its own output through state management, and the next argument
can only use what has been filed by then. No context changes during a pass. Cuts obey
MARK-4. [G7, P1, P3, A3, H5.10]

Correctly rejected model emissions follow CLASS-9. Retaining recent output in the
argument does not license a cut that discards material still owed to the store.
In normal flow, classifier delay is measured rather than treated as component
failure. At a cut, the declared wait bound under CLASS-8 and MARK-4 ends the wait.
The gate check has its own bound under GATE-1. [L1, D1, A3, P3, V3:4]

**CAP-5.** Every arrangement offered by these primitives loads from its declaration
without an edit to the harness. A required harness edit identifies a new mechanic,
which must be chartered. Reason: the admission test distinguishes composition of
existing mechanics from construction of another one. [H6, P:ladder]

## 2. The trace and the dependable flow prerequisite

**FLOW-1.** The trace is the complete record of everything that enters the agent,
everything that happens inside it, and everything that leaves it. After load, the tee is
the single tap and the only feed through which content enters state. Seeding under
PERSIST-5 is the only write outside that feed: an operator tool performs it before the
agent exists, and A and B in the first trace head account for it. Reason: every
operating-agent addition comes from the auditable trace, and the seed has a bounded
origin that the first trace identifies. [G2, B1]

**FLOW-2.** Every completed agent content event passes one tap before external handoff.
The trace runs whole and unbroken through that point. The tee takes a copy of its
traffic, and the partition selects from the copy. What is selected goes into state
management under the downstream filing rules. The rest of the copy is discarded. Nothing
is removed from the trace itself. The trace head records the deployment tuple, is never
tapped, and never enters the store. After the tap, transport may carry a content record
but no party adds agent content to that record. Transport control messages on the
external socket are permitted. Reason: every retainable event has one opportunity to
enter state management without reducing or altering the complete trace. The tuple is
trace-only deployment information. [G7, V3:3, G1, H2]

This invariant fixes the content boundary. It does not elect a queue layout, a
thread, or the representation of the external transport. The complete trace leaves
through the admin gate. The tee's copy does not divert, modify, or add to that
through-line. Configuration and tuple records do not enter the store. [V3:3, A3,
G1, H2]

**FLOW-3.** No partition reduces the trace floor. The trace holds the assembled
argument at each position, the sampling seed, sampler state, artifact identity,
emission, act, loop decisions, and measurement payload. Reason: the operator's record
must remain complete independently of the deployment's retention choices.
[H3, P:rulings/tee, P:walk/rung-4, B1]

**FLOW-4.** Tee or classifier component failure is agent failure. The agent halts
loudly, on the trace. A classifier that crashes, returns no result, or returns a result
that does not parse has failed. A failed carry or filing that loses an item owed to the
store also breaks the one flow and halts the agent. An admitted item that the classifier
correctly rejects is not lost and is not tee failure. In normal output flow, a slow
result is late, not absent. Wherever waiting blocks the model, the declared bound
applies. A result still pending past the gate-check bound or the cut wait bound is
missing under CLASS-8, and this halt applies. Delay under the applicable bound remains a
measurement, never failure. Reason: ordinary lag is measurable, but a pending result
must not hold the model forever. [L1, D1, G2, R5, A3, P3]

**FLOW-5.** There is no running-while-detached mode, no live reattach-and-catch-up
mode, and no save-point-only check that lets decisions continue. Partial sends,
failed filings, and other losses of items owed to the store fall under FLOW-4.
Correct downstream rejection and a connected classifier's lag are not detached
operation. Reason: a broken feed loses items, while the valid rejection and filing
pause rules preserve their declared disposition. [G2, R5, A3, P3, V3:4]

**FLOW-6.** The operating state-management flow starts at the tap. The store holds the
seeded starting content and material from that tap under the partition and downstream
filing rules. The save-point hash chain verifies the dump under PERSIST-3. Whether a
trace line later lands at its sink is outside this PRD. Reason: state management owns
its starting holdings and the tap's copy, not the trace's later delivery or durability.
Sink durability remains owed in the [trace charter][trace-charter], section 9. [B1, A5,
A1]

[trace-charter]: ../weaver-trace/weaver-trace-PRD.md

**FLOW-7.** The prerequisite is to make this one data flow dependable. Retrieval,
the STE100 classifier, and the rest of state management are not prerequisites for
that repair. Reason: the reliability of the feed must be established before the
capabilities that depend on it. This charter states the target and does not claim
that the prerequisite is implemented. [G6]

**FLOW-8.** Nothing the loop authors reaches the argument without its own trace event
first. This includes the restatement request under CLASS-9, the compaction request under
STRAT-2, and step instructions under PLAN-4. Each event passes the tap, and the
partition decides admission from the copy under the same rules as any other event. A
loop instruction is not gate input, because the loop is not an outside sender. The
assembled argument is still recorded under FLOW-3. Reason: the loop's instructions must
be explicit in the record before they shape a decision, without creating a new retention
feed or requiring an inference from the assembled argument. [D5, G7, H1]

### Open for spec

- How the harness learns that an admitted item reached and was accepted by the
  store, and how it detects a failed or incomplete carry. The halt is settled.
  [G2, G4]
- How the agent halts and records tee failure without letting failure reporting
  restart the data flow or allow another decision. The reporting mechanism must
  preserve the trace through-line. [G1, G2]
- How a loud failure reaches the operator and user without bringing down the host.
  The state-management report is authored to the trace. Delivery and durability of
  that report at the sink belong to the trace charter. [H7.2 item 8, A5]
- How store acceptance distinguishes pending filing from a lost carry, and how cuts
  wait within the declared cut bound under MARK-4. [D1, G2, P3, V3:4]
- Detection and reporting of classifier component failure, distinguishing a valid
  defect report and a pending result from a terminal missing or unparseable result.
  Normal output lag alone is not failure. At a gate check or cut, a result pending
  past the applicable declared bound is missing. [L1, D1, R5, A3, P3]
- The loop-authored instruction event and its ordering before placement, including
  restatement, compaction, and step instructions. Admission remains the declaration's
  decision on the copy. [D5, G7]

## 3. The tee and the partition

The trace is the through-line and runs whole to external handoff. The tee takes a
copy of completed agent content traffic at its tap. It applies the declared partition
to that copy, yields the selected material, and discards the unselected copy. Nothing
is diverted or removed from the trace. The trace head remains outside the tap. The
residue seam yields the admitted material to the loop, with no store interface or
carry owned by the tee. [G7, V3:3, H1, H2, G1, G4]

**TAP-1.** The partition selects from the tee's copy by envelope only: event kind,
channel, and origin, never content. Event kind is an envelope field. Payload paths shape
what is copied onward and never decide whether an item is admitted. The tee applies only
the validated declaration and has no admission policy of its own. It knows nothing of
STE100 or downstream rejection. Reason: admission is a declared mechanic on the copy,
while content classification is a separate duty and the trace remains whole. [D3, G7,
C1, A3, G3, P:rulings/tee]

**TAP-2.** The partition and tapped field set live in the agent configuration file,
are read at load, and can change between loads without a rebuild. Reason: retention
is the operator's declaration rather than an election hidden in code.
[H5.6, H6, G3]

**TAP-3.** The loader refuses to load a partition that admits measurement payload,
`save_point`, or `message.restored`. A valid declaration excludes all three. The tee
then applies that declaration and nothing else, selecting from its copy without
inspecting content or applying hard-coded exclusions. Configuration and the deployment
tuple never enter the store, and the trace head is never tapped. All belong on the
trace. Reason for measurement exclusion: management measurements must not leak into the
argument. Reason for restored messages: reinjection must not file the holdings again.
Reason for save points: filing a copy of state puts the state inside itself. The
configuration and tuple describe deployment, not retained content. The system prompt
enters through the seed and is read at boot under PROMPT-1. Boot does not copy it
through the tee or file it again. No carve-out copies turnless `message.system` whole
regardless of the declaration. There is no rejected-emission kind. [D3, G7, C3, R2, A3,
B1, B2, B4, G3, H3, P:rulings/tee]

**TAP-4.** The tee yields the material selected from its copy without rendering a
store-ingest frame or owning the carry. Classification labels that material, where
applicable, and the loop carries it across the store's contract. Discarding an
unselected part of the copy never removes anything from the trace. Reason: the tee
copies, the partition admits, the classifier labels, and the store's own contract
governs filing. [G7, G4, H1, H5.1]

**TAP-5.** The PRD proposes a starting field set for spec review: event kind, channel,
and origin for admission, source event identity and turn order for attribution, the
statement and source block needed for filing, and call identity and pair status for
tool communications. TAP-3's mandatory exclusions govern this set. This is a
proposal, not the final field list. Reason: those fields serve admission, span
attribution, ordered retrieval, and pair completion.
[C1, H5.1, H5.6, H5.7, H7.2 item 6, G3]

### Open for spec

- The final tap fields and their representation in configuration. [H7.2 item 6]
- How event kind, channel, and origin implement the declared partition, and how the
  loader proves that the declaration excludes the mandatory event kinds and
  measurement payload. The validation includes payload projection without a runtime
  content test or tee-owned policy. Task input accepted at the gate follows the copy
  path. The seeded prompt is read rather than copied at boot. [D3, G7, C1, B2, B4, B5]
- The load refusal and its diagnostic representation for an invalid partition. [D3]
- The trace-head boundary and the configuration and tuple representation, so the
  deployment record remains on the trace and outside retained content. [V3:3]
- Payload projection that preserves source blocks, spans, and mark boundaries.
  Recording the boot argument must preserve FLOW-3 without refiling the restored
  system prompt. [H5.1, V3:4, B2]
- The admitted-copy interface and its failure result, distinct from the store's
  ingest representation. [G2, G4]

## 4. The carry and the store seams

**SEAM-1.** The store is a member of state management, built as its own crate behind
its own contract and Unix socket. The loop carries admitted items in the format that
contract states and does not know which store implements it. Reason: every store
has its own interface, and a generic read-and-write service would fit none well.
[H5.1, P:rulings/store]

**SEAM-2.** After load, the write seam carries an admitted item, its source attribution,
and the classification required for filing. The store performs the write and exposes the
result needed to enforce FLOW-4. Neither the tee nor a sort performs that write. Reason:
admission authorizes retention, while the store alone owns filing and can establish
whether it succeeded. The loop does not carry a correctly rejected block to this seam.
Model-emission restatement follows CLASS-9. The pre-agent seeding exception is chartered
at the persistence seam under PERSIST-5 and does not transfer write ownership from the
store. [H1, H5.1, G2, G4, A3, B1]

**SEAM-3.** The read seam carries a structured query and returns held content with
its index fields, turn order, and source spans. The store performs the read. The
serve-side sort reads over one query seam, with retain as its first party. Reason:
selection must not depend on a store's private representation or grow a second
harness path when another permitted source offers material. [H5.1, H5.8, H6,
P:after/capability]

**SEAM-4.** A future outside organ reads the trace rather than inheriting the store's
partition. An organ triggered by measurement runs on the management plane. Its
signal never enters residue or the argument. Reason: the trace is the complete
record, while retained holdings reflect an election the outside organ did not make.
The source constraint in FLOW-1 still applies to anything saved for state management.
[H6, P:after/capability, G2]

The contract roles below name obligations, not filenames or wire formats. Their
representations are for the specs. Each row refers to the requirements that state
its reason and source.

| Contract role | Parties and crossing | Governing requirements |
| --- | --- | --- |
| Completed content record | Trace authoring to external handoff, whole and unchanged, with a tee copy at the tap and the trace head outside it | FLOW-1 through FLOW-3 |
| Admitted copy | Partition selects from the tee's copy, unselected copy is discarded, selected material and attribution reach the loop | TAP-1 through TAP-5 |
| Partition validation | Declaration to loader, yielding a valid partition or refusal before operation | TAP-1, TAP-3 |
| Loop instruction | Loop to its own trace event before place can put the instruction in the argument | FLOW-8 |
| Classification | Loop to classifier on SPU and back, item and scheme to bindings and spans, a valid defect report, or component failure | CLASS-1 through CLASS-10 |
| Store write | Loop to store, admitted and labeled material to accepted filing or failure | SEAM-1, SEAM-2, FLOW-4 |
| Store read and query | Serve-side sort through loop to store and back, subject or mark query to content and attribution | SEAM-3, READ-1 through READ-5 |
| Outbound selection | Emission through outbound sort to act and trace authoring | OUT-1, OUT-2, SORT-4 |
| Placement input | Serve-side sort to renderer, selected material and argument head | SERVE-1 through SERVE-3, PLACE-1 |
| Engine argument | Renderer through loop to engine, argument and valid N or loud fit failure | PLACE-3 through PLACE-6 |
| Save and load | Loop, store, configuration custody, and trace, from a dump request to verified starting artifacts | STORE-7, PERSIST-1 through PERSIST-4 |
| Seed creation | Operator tool to the store's persistence interface, creating schema and the STE100 system prompt with its end mark, then producing an ordinary save point | PERSIST-5 |
| Boot read | Loaded store through its read contract to place, system prompt through its end mark before the user channel opens | PERSIST-6, PROMPT-1 |
| Gate input check | Sender, gate, and classifier, yielding conforming content or refusal and a restatement request, with refusal on the trace | CLASS-4, GATE-1 |
| Plan and step events | Operator or harness through trace and tee to store, then condition selection to place | PLAN-1 through PLAN-6 |
| Marked span | Seeded prompt boundary or recorded runtime boundaries in the store, then address query to a whole span for sort and place | MARK-1 through MARK-6 |
| Cut readiness | Store filing status and decoder stop through the loop, yielding a permitted cut and valid N or a halt at the declared wait bound | CLASS-8, MARK-2, MARK-4, PLACE-5, PLACE-6 |
| Deferred prefetch | Live-emission signal on the management plane to serve-side sort, store reads to candidate bins, selected candidates to place between passes | PREFETCH-1 through PREFETCH-3, DEFER-5 |
| Prompt material | System prompt from seed and boot read, then STE100 task prompt through the gate, trace, and tee, with end marks for placement | PROMPT-1, PLACE-1, PLACE-2 |

### Open for spec

- The store's own read and write calls, including how accepted filing is reported.
  [H7.2 item 5, G2]
- The query and result representations and how source and admission status travel
  with candidates over the single query seam. [H5.1, P:owed/prd item 23]
- The encoding of the admitted copy through classification into the store's write
  contract. [G4, H7.2 item 3]

## 5. The store: retain, filing, and persistence

The store performs retain. It holds admitted residue and answers reads over its own
contract. It does not elect what the next decision needs. Its place in the circuit
is after admission and classification on the operating write side, and before
serve-side sort on the read side. The pre-agent seed uses the persistence seam.
[H1, H5.1, B1, P:walk/rung-5]

**STORE-1.** The elected implementation is rusqlite with SQLite in memory only during
service. No turn waits on disk for a store operation. Reason: the program builds the
most capable agent for one user on one machine, and the in-memory store keeps disk
latency out of each turn. [H5.1]

**STORE-2.** The agent's admitted statements are held as STE100. The verb is the key,
the noun is the value, and the governing condition is a third field. All three are
retrievable. A tool return is held as the block of its request's pair. The classified
STE100 request supplies the verb and noun, and the return is not classified. A return
without a matching request is rejected under PAIR-1. Reason: the meaning protocol
supplies the filing structure, and the return answers the action already named by
its request. [R4, H5.1, H5.5]

**STORE-3.** Each verb and noun pair points to the stored block and its source span.
The pairs are the index and the block is the content. A verb over a compound object
is one action with several values, each pair retaining its span pointer. Reason:
no separate reason for these schema guarantees is supplied by the handoff. [H5.1]

**STORE-4.** The store is indexed on the noun as well as the verb. Writes file by verb
and queries enter by noun. Reason: an index on the verb alone turns every noun query
into a scan. [H5.1]

**STORE-5.** The store keeps turn order, source and admission status, and the pairing
of every tool return to its call. Reason: retrieval must return a subject's actions
in order, and a return is meaningful as the second half of its communication.
[H5.1, H5.7, H5.8]

**STORE-6.** Retained holdings are not bounded by the decoder's context length. The
argument is bounded, and sort selects what is placed from the holdings. Reason:
a growing record changes the selection task, not the model's context capacity.
[P:walk/rung-5, H5.8]

**STORE-7.** On unload the whole store dumps in SQLite format, accompanied by its
configuration copy and manifest under PERSIST-1. Load verifies all three under
PERSIST-3 before reinjection. Store and dump use one technology. Reason: holdings
survive unload and reload without adding disk latency to each turn, and all saved
starting places use the same load procedure. [H5.1, H5.4, A1, A2]

**STORE-8.** Reload creates no new content origin. It restores the verified seed and
subsequently filed holdings. Neither the boot read nor `message.restored` files those
holdings again. Reason: the seed accounts for initial content, the tee is the only
content feed after load, and restoration must not duplicate either. [B1, B2, G3]

The dump floor restores holdings, including the system-prompt span, not a live engine
buffer or an interrupted pass. Buffer-level suspend and restore is deferred. Every
agent starts from a seed, and every load reads the system prompt already in its
store. A changed system prompt requires a new seed and a new agent. [H5.4, H5.9,
B1, B2, B3]

### 5.1 Save points and the session hash chain

The persistence seam takes an on-demand save request or an unload request through the
loop to the store's own contract. It returns the dump artifact and the information
needed to bind it to configuration and record the event on the trace. On load, that
binding is checked before the holdings are used. Configuration custody and trace
authoring remain duties outside the store's content write. Before an agent exists,
the operator tool uses the persistence seam to construct its seed. The seam owes the
same ordinary save-point result, not a second load format. [R2, R3, H1, B1]

**PERSIST-1.** The operator may request a save point at any time. It writes three
files in one directory: the SQLite dump, a configuration copy containing the dump's
hash A, and a manifest containing A and the configuration's hash B. The order is
fixed: hash the finished dump to obtain A, write A into the configuration copy, hash
that finished configuration to obtain B, then write the manifest with A and B. Reason:
the files bind a saved state to its configuration without asking a file to contain
its own hash, and let the operator return to a saved starting place. [A1, R2]

**PERSIST-2.** For a save point taken during agent operation, the trace records the
request, time, file location, A, B, and confirmation. It does not carry the dump or
its contents, and none of the save-point record goes into the store. Seed creation
precedes the agent's trace and is accounted for by A and B in its first trace head
under PERSIST-5. Reason: the trace names the starting place and configuration without
filing state inside itself or inventing a trace before the agent exists.
[A1, R2, C3, B1]

**PERSIST-3.** Every load starts a new trace, including restart, reload, and rollback to
a save point. Every agent is born as a save point under PERSIST-5. The loader verifies
the dump against A in the manifest, the configuration against B in the manifest, and A
inside the configuration against A in the manifest. Any mismatch is loud failure, and
the agent does not load. The trace head records A and B along with the deployment tuple.
Reason: each load checks both artifacts and their binding, and records the same
identifiers as the source save point. The first trace records A and B even though seed
creation preceded the agent and its trace. [A1, R3, B1]

**PERSIST-4.** An unload is a save point taken at shutdown. It writes the same
three-file directory and uses the same load procedure. The outgoing trace records A, B,
and the saved location. A trace loading that save point records those same A and B. Save
points can support a tree of recorded starting places. Agent continuity within that
provenance is governed by PERSIST-7, not by a shared configuration name. Reason: all
saved starting places have one form, and the trace can identify both the state and the
configuration on each side of a save and load. [A1, A2, P2, R3, B3]

### 5.2 Seed, boot read, and continuity

**PERSIST-5.** Every agent is born as a save point. A fresh agent cannot start with an
empty store. Before the agent exists, an operator tool creates the store schema and
inserts the STE100 system prompt with its end mark. It then dumps the store and hashes
the dump to obtain A, writes configuration containing A and hashes that file to obtain
B, and writes the manifest containing A and B. The result is an ordinary save point,
loaded through PERSIST-3 with no first-load special case. Seeding is the only state
write that does not pass the tee. Reason: the exception occurs before the agent exists,
is performed by an operator tool rather than the agent, and is accounted for by A and B
in the first trace head. All three conditions bind. The tool requests creation and
insertion at the store's persistence interface. The store still owns its writes. [B1,
B5, H1]

**PERSIST-6.** Load verifies the save point and puts the store in RAM. The first action
after load is to read the system prompt through its end mark from the store and place it
as the argument head. Only then does the user channel open. This is a read, not a write:
the prompt does not pass the tee and is not filed again. The assembled argument is
recorded under FLOW-3. Reason: the agent must have its stored system prompt before
accepting user input, while replay must see what the model received. This boot read is
mandatory regardless of the automatic-retrieval setting in READ-2. [B2, H1]

**PERSIST-7.** An agent remains one agent while each session loads the dump left by its
preceding session. A new seed starts a new chain and a new agent. To change the system
prompt, the operator authors a new seed. No configuration carries a system prompt to
replace the one in a loaded store. Reason: continuity follows the inherited dump, and a
changed system prompt changes the seeded agent rather than creating two competing prompt
sources. [B3]

### Open for spec

- Tables, field types, indices, and block and span representations that satisfy
  STORE-2 through STORE-5. Preserve the send's statement and attribution alongside
  the return block, and support mark-addressed reads. [H7.2 item 2, R4, V3:4]
- The hash algorithm, digest representation, filenames, and manifest encoding. The
  three files, hash order, and three load checks are settled. [A1, A2]
- Coherent dump creation, configuration copying, publication, verification, and
  reinjection, including interrupted writes and pending filings when a save is
  requested. Dump custody does not reopen sink durability in this PRD.
  [A1, A2, A5, P3]
- Trace representations of A, B, saved location, confirmation, and save-point origin.
  Configuration and tuple records never enter the store. [A1, V3:3]
- The operator seeding tool's interface to schema creation, STE100 prompt insertion,
  end-mark creation, and ordinary save-point publication. Its packaging and mechanics
  are for spec. No empty-store boot mode or second save-point format is owed.
  [B1, B5]
- Source and admission attribution for seeded content, distinct from runtime trace
  events, without inventing a trace event before the agent exists. [B1, H5.1]
- The boot query through the prompt's end mark, placement, and user-channel readiness
  sequence. Failure surfaces must not open the user channel before successful boot
  placement. [B2, H7.2 item 8]
- Representation of the seed origin and predecessor dump in session lineage,
  including saved branches, under the continuity rule in PERSIST-7. [B1, B3]

## 6. Sort: common rules and the two positions

**SORT-1.** Sort reads, labels, joins, and selects under a declared criterion. It may
be mechanical or use a model on the SPU. It never writes the store or elects
retention. Reason: the partition owns admission and the store owns filing, so a
selection must not become a second retention door. Classification may correctly
reject an admitted block before filing, under CLASS-10, without changing the
partition or writing the store. [H1, H6, A3, P:rulings/sort-twice]

**SORT-2.** Sort may run at any crossing under the same duty boundary. The residue
path includes classification after admission and before filing. Reason: the
classifier needs that crossing to label the admitted item without moving content
inspection into the tee. [H5.5, H6, G4]

**SORT-3.** An external input is not dropped unless sort has a declared criterion
permitting that drop. The drop is recorded on the trace. Place never performs it.
A tool return is never partly dropped. Reason: a silent drop makes an undeclared
content decision, and half a return breaks its pair. [H5.2,
P:ladder/step-3]

A drop from the next argument does not remove an admitted item from the store's
write obligation. Selection for placement and retention remain separate decisions.
Correct downstream rejection under CLASS-10 ends that block's filing obligation.
That is distinct from selection for placement. [H1, H5.2, G2, A3]

**SORT-4.** The loop and harness author each sort decision and step outcome as a new
trace event when it happens. That event then passes the tap like any other. No event
crosses the tap twice, and nothing is written back into an earlier event. The loop and
harness supply mechanical review closure in its new event, and sort labels the step from
that event. The partition decides retention from the tee's copy, with no sort-to-store
write. Reason: each later selection or outcome must be accountable at its own time in
the record and use the one operating feed. [D4, G7, P:walk/rung-6, H1, G1, G2, REV4]

### 6.1 Outbound sort

**OUT-1.** Outbound sort reads emission e and selects the act Y. Its input contract
receives the emission and criterion, and its output contract supplies the selected
act and the decision for recording. Reason: the decoder owns the emission and the
agent's declared criterion determines the act. [H3, P:walk/rung-3]

**OUT-2.** Changing Y alone does not alter the engine buffer. If a change is carried
into the next argument at position k, reuse ends no later than k. Reason: an edited
argument cannot reuse entries computed for different content at that position.
[P:walk/rung-3, H3]

### 6.2 Serve-side sort

**SERVE-1.** Serve-side sort receives the new external input and results from the
query seam. It labels, joins, and selects the material for the next argument, then
hands it to place. It uses the same label scheme as filing. Reason: the input and
retained actions can be joined only if their labels agree in meaning.
[H5.5, H5.8, P:rulings/classify-on-write]

**SERVE-2.** Serve-side sort selects within the declared tail budget, preserves the
required tool pairing, and applies the deployment's strategy criterion. Place
arranges the selected material and reports whether it fits. Reason: selection and
arrangement are different duties, and neither can silently discard external input.
[H5.2, H5.3, P:walk/rung-6]

**SERVE-3.** Sort labels a step closed from the recorded result that it passed the
mechanical review setup, tests and gate, and can name its outcome. The loop and
harness own review and firing the next step. The model does not close a step by
claiming completion. These labels support cuts at step boundaries and placement of
later outcomes and forward flags. Reason: a cut requires a named boundary, and the
return defines closed as passed review rather than a model assertion. [REV4, H5.3,
P:walk/rung-6]

### Open for spec

- Criterion and label representations at the sort seams, including the shared label
  scheme's detailed verb dictionary. [H7.2 item 3, H5.3]
- What counts as a structured match and how selected blocks and tool pairs are
  supplied to place. [H7.2 item 7, H5.7]

## 7. The classifier and the meaning protocol

The classifier performs sort with a model on the SPU. Its input contract carries the
item and declared label scheme. Its output contract carries the verb-to-noun binding,
governing condition, and span attribution needed for filing or matching, or a valid
report that a classified block is not STE100. It labels and never files. A terminal
missing or unparseable result is component failure. A pending result is lag during
normal output flow. A gate check and a cut each have a declared bound, past which
a pending result is missing. [L1, D1, H5.5, G4, R5, A3, P3, P:walk/rung-6]

**CLASS-1.** The meaning protocol has four properties: standardized, short declarative
sentences, classifiable, and a verb bound to its noun under a governing condition.
The elected instance is STE100 with a closed set of computing verbs. Reason: regular
emission makes the clause and its action mechanically findable and its labels
finite, so the same structure supports filing and retrieval. [H5.5, H6,
P:rulings/meaning-protocol]

**CLASS-2.** STE100 is both the decoder's emission contract and the product's election
for filing by verb. The verb set is closed and fixed at load. Keys are generated from
that list without a model. The classifier finds each sentence's verb and binds it to the
noun acted on, with its condition and pointer to the span. Reason: the narrow parsing
job bounds its cost. Output filing is owed before the cut under CLASS-6. Gate input
checking must finish before the input reaches the model under GATE-1. [D6, H5.5, P3]

**CLASS-3.** Classify-on-write ships for the model's own STE100 output, including tool
requests. A tool return is not classified and completes its pair by matching its
send. Reason: a tool request is model output, while the return's role is completion
of the already identified communication. [H5.5, H5.7]

**CLASS-4.** All input through the gate requires STE100 under GATE-1. The input check
runs ahead of the model, and the same classifier and label scheme serve input and
output. Claude Code is the test driver. A separate seam remains for a safety classifier
ahead of or behind the STE100 classifier. Reason: STE100 is the condition of exchange,
so both sides can be filed, retrieved, and checked by the same scheme. Claude Code is
identified by the operator as writing STE100 well. [B5, H5.5]

**CLASS-5.** The protocol targets technical, long-horizon, machine-to-machine work,
where meaning and intent must stay stable between AI agents. Free conversation is
not a target. Reason: the narrow domain permits a controlled verb scheme and keeps
it stable over time. [H5.5, H8]

**CLASS-6.** The classifier's C_tail is a temporal filing budget, not the decoder's size
limit. Output classification and filing must complete before a cut drops material still
owed to the store. Correctly rejected model blocks owe no filing under MARK-4. Normal
output lag alone is not component failure. The loop bounds its filing wait at a cut
under CLASS-8 and MARK-4. The input check at the gate is on the turn's critical path,
since input cannot reach the model before the check passes. It has its own declared
bound under GATE-1. Its delay is recorded as wall time under COST-4. A result pending
past either applicable bound is missing, while delay under that bound remains a
measurement. The spec measures input and output classification together, including
gate-check delay and how far filing lags and for how long. Reason: recent output is
already available in the argument, whereas gate input must wait for checking. Growing
filing lag shows an overloaded tuple, and bounded waits keep a hung classifier from
blocking the model forever. [L1, D1, D6, P3, V3:4, B7, H7.1 item 2]

**CLASS-7.** Several decoder candidates are measured under one declared tuple for
STE100 emission fidelity under load and over a long task, acceptance of STE100 input,
and use of recall verbs. The best performer on that evaluation is selected. If it
needs refinement, one fine-tune covers all three. Reason: these are three uses of
one protocol, not independent behaviors. Each rejection under CLASS-9 counts against
STE100 performance. Cross-rejection correlation and original/correction pairing
belong to Weaver analysis, not to this classifier. [H7.1 item 3, A4]

**CLASS-8.** A classifier crash, missing result, or unparseable result halts the agent
loudly on the trace under FLOW-4. A correctly reported input defect is not component
failure. In normal output flow, a pending result is lag rather than failure merely
because time has elapsed. Wherever a wait blocks the model, it has a declared bound. The
gate conformance check has its own bound. At a cut, the loop waits in the decoder pause
for filing up to the cut wait bound. A result still pending past either applicable bound
is missing, and FLOW-4 halts the agent. Delay under the bound stays a measurement and is
never failure. Both bounds and their units are spec values set by measurement. Reason:
normal lag is evidence the operator needs, but a hung classifier must not hold either a
gate check or a cut forever. [L1, D1, R5, A3, P3]

**CLASS-9.** When the classifier correctly reports that a model-emitted block is not
STE100, the loop sends that block back to the model with an instruction to restate it in
STE100. The loop authors the restatement request as its own trace event before it
reaches the argument under FLOW-8. The corrected statement is an ordinary emission that
follows the same trace, tee, classification, and store path. A valid correction is
filed. After the measured retry cap with no valid restatement, the agent fails loudly to
the user. Reason: the model's emission contract is STE100, and restatement repairs a
correctly detected emission defect with the loop's instruction explicit in the trace.
[D5, A3, A4, R6, H5.5, G2]

**CLASS-10.** A model emission reaches the trace as a normal emission. The tee copies
it, and the partition selects from that copy. The classifier is the downstream filter at
the store's door. If it correctly reports that a block subject to STE100 classification
fails STE100, the loop does not carry that block to the store. The rejected original
remains on the trace, as does a later ordinary corrected emission. There is no
rejected-emission kind and no training-pair duty inside the agent. Reason: valid
rejection keeps nonconforming content out of retained state without hiding it from the
trace or moving content inspection into the tee. Unclassified tool returns still follow
their request's pair under STORE-2. [G7, A3, A4, R4]

The filter rejects a non-STE100 block regardless of whether it is a careless emission
or attempted prompt injection. STE100 conformance is not itself a safety decision:
the classifier binds verbs, nouns, and conditions rather than judging the intent of a
well-formed instruction. The safety-classifier seam remains separate. An operator
could declare an agent whose task material is injection content, but that mechanism
is outside this first release's STE100 input contract. [A3, H5.5, H0]

### 7.1 Input at the gate

**GATE-1.** STE100 is the condition of interacting with the agent. All input through the
gate, including the task prompt, must conform. Nonconforming input is refused at the
gate, never reaches the model's argument, and is returned to its sender with a request
to restate it. The refusal is recorded on the trace. The receiving model does not
rewrite refused external input. Accepted input reaches the store only through trace, tee
copy, partition, and the store's filing contract. Tool returns answer the agent's own
requests and are not gate input. They stay unclassified under STORE-2. The gate check is
on the critical path of each inbound turn and has its own declared bound. Its delay is
recorded as wall time under COST-4. Delay under the bound is never failure. A
conformance result still pending past that bound is missing, and FLOW-4 halts the agent
loudly on the trace. The bound and its unit are spec values set by measurement. Reason:
both sides exchange statements under one scheme, the model must wait for conformance
before receiving input, and a hung check must not hold the turn forever. Gate
conformance checking is not a safety decision. [L1, D6, G7, B5, R4, H1, G2]

The gate receives the sender's input and returns acceptance or a refusal requesting
restatement. Its classifier seam supplies the conformance result before the model
receives content. Its trace seam records refusal, and accepted content follows the
ordinary trace-and-tee retention path. Checking input before the model does not
create another retention feed. Where the check sits relative to the gate remains a
spec decision. The world-content classifier in DEFER-4 is the later rung that may
relax the input requirement. [B5, H1]

A stock encoder of the BERT class without task training is an operator-named strong
candidate for the classifier. It is not elected. The spec measures candidates and
chooses the model. [H5.5, H7.2 item 3]

### Open for spec

- Measured value: the classifier's C_tail and lag with both directions active,
  assessed against filing before cuts, alongside gate-check wall time on the input
  critical path. No inside-decode-cycle deadline is elected. [D6, P3]
- Measured value and unit: the declared wait bound at a cut. The bound applies in the
  decoder pause and a result pending past it is missing. [D1]
- Measured value and unit: the separate gate-check bound. A conformance result pending
  past it is missing. [L1]
- Measured election: the decoder evaluation and selected model, with any refinement
  covering the whole protocol and counting each rejected emission. [H7.1 item 3, A4]
- Measured value: the restatement retry cap. [R6]
- The detailed verb dictionary, recall verbs, and classifier model. [H7.2 item 3]
- Result formats separating accepted bindings, a valid STE100 defect, and component
  failure. Pending work and a terminal missing result must be distinguishable without
  treating normal output lag as failure or extending a gate check or cut beyond its
  applicable declared bound. [L1, D1, A3, P3]
- Restatement request and result handling sufficient for the loop's retry cap.
  The loop's request has its own trace event before placement. Original/correction
  pairing for analysis is outside the agent. [D5, A4, R6]
- Placement of the input conformance check relative to the gate and reuse of its
  result for filing, preserving the pre-model check and trace-and-tee content feed.
  Refusal, return to sender, and a request to restate are settled. [B5]
- The gate refusal response and trace representation, without treating refused input
  as accepted content or adding a rejected-emission kind. [B5, A3]
- One classifier instance or several under the same label scheme, or a faster model,
  chosen from measurements of input and output together. [H7.2 item 4, P3]

## 8. The tool-call pair lifecycle

A tool-call pair is one communication with a send and a return. Its lifecycle is a
requirement across trace, admission, store, and serve-side sort. It is not a new
writer or an independent state feed. [H5.7, G2]

**PAIR-1.** Every opened pair reaches an end and no pair stays open. The following
fates bind both the store and trace. Reason: a return must be joined to the action
that requested it, and a missing return must have a recorded outcome. [H5.7, R4]

| Fate | Trigger | Store | Trace |
| --- | --- | --- | --- |
| Opened | Model emits the send in STE100 | Send is filed | Send is recorded |
| Completed | Return arrives within that tool's timeout | Return is the block of the send's pair | Completion is recorded |
| Failed | Tool timeout expires without a return | Pair is marked failed | Failure is recorded |
| Rejected | Return has no eligible matching send | Return is never admitted | Return is recorded as rejected |

**PAIR-2.** A late return to a failed send is rejected, not used to reopen or complete
the failed pair. The loop may try again under its retry policy. Reason: the operator
can compare failed sends with later rejected returns, identify overrunning tools,
and adjust their timeouts. [H5.7]

**PAIR-3.** Returns are matched to their sends without content classification. They
answer the agent's requests and are not gate input under GATE-1. Rejected returns never
enter the store, and tee admission remains envelope-only. Reason: pairing completes the
communication without adding a content-reading duty to the tee or classifying the return
block. [B5, R4, H5.5, H5.7, G3]

**PAIR-4.** A return is an indivisible selection unit. Sort may drop it only whole,
under a declared criterion, and the drop is traced. A failure to deliver an already
admitted pair item or lifecycle update is tee failure, not a tool timeout. Reason:
half a return breaks its pair, and a broken retention flow is agent failure.
[H5.2, H5.7, G2]

### Open for spec

- Pair identity, matching, lifecycle representation, and how an unmatched return
  is marked rejected before admission using event kind, channel, and origin. No
  matching location is elected here. [C1, H5.7, G1, G3]
- Per-tool timeout values and retry policy, with deployment values identified as
  such. No values are elected by this charter. [H5.7]
- How open pairs reach a recorded end on interruption or unload. The requirement
  that no pair stays open is settled. [H5.7]

## 9. Structured retrieval and the next argument

**READ-1.** First-release retrieval runs as structured queries in rusqlite. The
query holds the subject and returns every action applied to it in turn order, with
each action's span. Filters by verb and condition and joins across pairs are
available. Reason: retrieval is the write run backwards, entering by noun where
filing entered by verb. [H5.8]

**READ-2.** Each deployment declares whether automatic retrieval is enabled and an
injection budget that is a share of its single decoder C_tail. When enabled, each
inbound turn has its subject's filed history queried by the harness without the model
asking. If history exceeds the injection budget, sort selects under a declared
criterion, such as recency or the current step, and records the selection drop on
the trace. Place lays the selected history in the next argument. Reason: automatic
retrieval addresses the failure of a recorded fact not reaching a decision, while
the operator controls its use and its share of the available context. [P1, H5.8]

**READ-3.** The closed verb set includes a small number of recall verbs. Each is an
internal tool call whose action is already decided. The model supplies only the
subject, and the call runs the noun query and returns the actions on it. These verbs
add reach and never replace the harness-driven floor when enabled. They do not become
a correctness guarantee when the operator disables that floor. Reason: model-elected
recall supplements automatic retrieval rather than making it depend on remembering
to ask. [P1, H5.8]

**READ-4.** Retrieval returns the ordered history, and selection and placement still
obey the single C_tail and indivisible-return rules. Retrieval does not authorize
silent truncation by place. Reason: the store's holdings have no context bound,
while the next argument does. [H5.2, H7.1 item 1, P:walk/rung-5,
P:walk/rung-6]

**READ-5.** Retrieval can fetch a whole marked span from the store by its mark and
return it for placement at the tail. The mark fixes the span's start and end, and
sort and place preserve that span as a whole. Reason: a marked span outside the
window has an exact address for reminder at the tail. [P1, V3:4]

### Open for spec

- The deployment enable control, injection budget as a share of C_tail, and declared
  selection criterion. No enable default or budget fraction is elected here. [P1]
- What counts as a subject and a match, including normalization, compound subjects,
  and cases with no match. No semantic-nearness mechanism is elected here.
  [H7.2 item 7, H5.8]
- The recall verbs and the read calls they invoke. [H7.2 items 3 and 5]
- How the inbound subject and ordered history reach the serve-side sort under the
  shared label scheme when the floor is enabled, without model-elected recall becoming
  its guarantee. The mark-address query uses the existing read seam. [H5.5, P1, V3:4]
- How a whole marked span and the subject-history selection share the injection
  budget. A span that cannot fit is not partly placed. [P1, V3:4, H5.2]

## 10. The plan and forward flags

State management holds the plan, files forward flags, and selects statements by
condition. Its input seam receives operator plan instructions, permitted initial
plan output, operator updates, and harness closure events through the trace and
tee. Its read seam returns the next step's instructions and statements conditioned
on that step. Serve-side sort hands them to place for the next argument. The loop
and harness own review, enforcement of the plan, and progression. No additional
primitive or store writer is chartered. [REV4, H1, G2]

**PLAN-1.** The operator supplies a plan, or general steps and an instruction to make
a plan. The step-by-step plan is filed as STE100, one statement per step, with a
stable ID for each step. Step IDs never renumber. Initial model-authored steps follow
the normal emission, classification, and filing path. Reason: no separate reason
for the plan representation or stable-ID guarantee is supplied in the return.
[REV4, H1, H5.5]

**PLAN-2.** A step closes only when it passes the mechanical review setup, tests and
gate. A model assertion of completion does not close it. The loop and harness own
the review and fire the next step on closure. State management receives the closure
as an event on the trace and labels the step closed. It does not judge the work.
Reason: the return gives the closed label the concrete meaning of passed review.
[REV4]

**PLAN-3.** A forward flag is a statement whose governing condition names the stable ID
of a later step. The classifier labels it, the loop carries it, and the store files it
through the normal path like any other statement. No new store field is needed. Reason:
the agent can retain something it finds now for the later step that needs it by using
the existing condition field. [REV4, H1, H5.1]

**PLAN-4.** When a step closes and the loop fires the next step, serve-side sort selects
every statement whose condition names that next step. The loop authors any step
instructions it supplies as their own trace event before they reach the argument under
FLOW-8. Place lays the selected statements beside that step's instructions in the next
argument. This is reminder at the tail, armed in advance. Reason: a fact found at step
three for step 14 must reach the decision at step 14 when step 13 closes. The loop
supplies progression and records its instructions, sort selects by condition, and place
lays the result. [D5, REV4, H5.10]

**PLAN-5.** The plan is locked. Work that deviates from the plan or does not fit its
categories is rejected, and the agent returns to the user with what it found and
asks how to address the break. The agent never edits its own locked plan. The loop
and harness enforce this boundary under the declared plan. The store holds the
plan and labels, and does not decide whether work fits the user's purpose. Reason:
no separate reason for the locked-plan election is supplied in the return.
[REV4, H0, H1]

**PLAN-6.** A plan change is an operator update to the database through the one feed.
Discussion may precede the change. The update itself enters on the management plane,
is written to the trace, is admitted by the partition, and is written by the store
across its contract. There is no direct plan-edit path into the store. Existing step
IDs do not renumber. Reason: an operator update uses the same source of truth and
filing boundary as every other retained item. [REV4, G2, H1]

The plan is retained task material, called state in the return's phrase "the plan
is state". Across passes it is residue under CAP-3. System prompt, task prompt, and
work are held in that order. The system prompt supplies the argument head, while
plan steps and forward flags are task material selected for the tail. Step closure
also supplies a mark under MARK-1. [REV4, V3:3, V3:4, A5]

### Open for spec

- Stable step identity, step ordering without renumbering, plan categories, and
  update and closure representations within the existing schema and verb scheme.
  Numeric adjacency of IDs is not elected as progression. [REV4, H7.2 items 2 and 3]
- The initial plan handoff and point at which the supplied or requested plan becomes
  locked, preserving the distinction between authorized creation and operator-only
  updates. [REV4]
- The loop and harness seam for mechanical review results, next-step identity, and
  rejection of work outside the plan. State management receives events and performs
  declared labeling and selection, with no review authority of its own. [REV4]
- Condition matching for step IDs and the selected bundle of step instructions and
  all matching flags passed to place. If that bundle cannot fit C_tail, PLACE-4's
  loud failure governs. No flag truncation or chunking is elected here. [REV4, H5.2,
  H7.2 item 7]
- The operator-update call through the store's contract, with source attribution and
  admission status preserved. The instruction to update must come through the trace.
  [REV4, H5.1, H7.2 item 5]

## 11. System Prompt

The system prompt is a separate document, owed, with its own PR process. It is itself
written in STE100. This section charters its duties and its role in the seed and boot
read, not its wording. The prompt lives only in state as agent content, never in the
configuration. The reviewed source document is what the operator seed tool inserts.
Its appearance in the traced argument records what the model received, not another
configurable prompt source. [B1, B2, B5, B10, V3:5]

**PROMPT-1.** The trace head records the deployment tuple needed to recreate the run for
diagnostic replay, including the loaded save point's A and B, and is never tapped. The
system prompt is the first item in state, inserted in STE100 with its end mark by the
seed tool before the agent exists. It is the first item read at each load, through that
mark, and place lays it as the argument head before the user channel opens. Nothing is
copied through the tee or filed again by that boot read. The task prompt subsequently
enters through the gate in STE100, is held after the system prompt with its end marked,
and work follows. Configuration and tuple never enter the store. Reason: the agent
starts with its rules in state and reads them before accepting task input, while the
traced argument supplies the replay record. [B1, B2, B5, V3:3]

**PROMPT-2.** No agent deploys without a system prompt meeting this section's
requirements. The prompt document is reviewed through its own PR process. The deployment
gate checks that document against the required duties below. Reason: the return elects a
conforming system prompt as a deployment gate. No separate reason for that gate is
supplied. [V3:5]

**PROMPT-3.** The STE100 system prompt must teach all nine duties below. They are
elected requirements, including the three additions accepted by B10. Reason: these are
the model behaviors needed by the filing, retrieval, plan, and compaction contracts.
Teaching them does not transfer enforcement from the loop and harness to the model.
[B10, B5, V3:5, H5.5, H5.8, REV4]

| Duty to teach | Governing requirement and source |
| --- | --- |
| Emit STE100 with the closed verb set, including tool requests | CLASS-1 through CLASS-3, H5.5 |
| Use recall verbs to reach history | READ-3, P1 |
| Flag material forward with a statement whose condition names a later stable step ID | PLAN-3, REV4 |
| Keep to the locked plan, never edit it, and return to the user when work does not fit | PLAN-5, PLAN-6, REV4 |
| Treat a step as closed only when mechanical review passes | PLAN-2, REV4 |
| Restate a block in STE100 when the loop returns it as rejected | CLASS-9, A3 |
| Accept and use STE100 input and placed history | CLASS-4, CLASS-7, B10 |
| Produce the initial step-by-step STE100 plan when the operator instructs it | PLAN-1, B10 |
| Produce the STE100 summary when the loop requests wall compaction | STRAT-2, B10 |

The seed tool requests prompt insertion and its end mark through the store's
persistence interface. At boot, the store reads the prompt and place lays it. The
system prompt itself performs none of admission, filing, review, or placement.
Changing it requires a new seed and therefore a new agent under PERSIST-7.
[B1, B2, B3, H1]

### Open for spec

- The prompt document's location, deployment check, and reference to the reviewed
  STE100 wording. The separate prompt document and its PR remain owed. [B10, V3:5]
- Prompt and task representations and end marks: prompt insertion through the seed,
  task input through the gate, and boot reading without refiling. [B1, B2, B5]
- The deployment tuple representation and trace-head boundary needed for replay.
  This charters the record, not Weaver analysis's diagnostic mode. [B1, V3:3, R7]
- Boot query and placement representations, including the existing trace floor for
  the assembled argument. No configuration prompt or live prompt replacement is
  chartered. [B2, B3]

## 12. Marks

A mark is the identity of a span in retained state and records its boundary. The
system prompt, task, subtasks, and plan steps have marks, and marks nest. Their
positions in the argument can move. Marks are not a new primitive or another value
named N. The seed supplies the system-prompt end mark. After load, the input seam
receives span-boundary and closure events through the trace and tee, and the loop
carries them across the store's contract. The read seam supplies a marked span for
sort and place. The cut seam supplies a boundary that place relates to the engine's
valid prefix. [B1, B8, V3:4, H1, G2]

**MARK-1.** The store holds marks for the system prompt, task, each subtask, and each
plan step. A subtask's mark sits within its task, and a step's closure under PLAN-2
is a mark. The loop and harness supply closure from mechanical review, not from a
model claim. Reason: the nested boundaries identify spans at the depth where work
changes and where earlier content must remain addressable. [V3:4, REV4]

**MARK-2.** A cut lands at the end mark of the last unchanged span. It keeps the prefix
through that mark and recomputes forward. Changing subtask four keeps everything through
subtask three. Changing the task keeps only the system prompt, using its end mark. Marks
provide the permitted cut positions. N remains the one position from which the next pass
recomputes, never past the valid prefix. Reason: the cost of a change follows its depth
in the stack, and changed content cannot keep its old computation valid. [B6, V3:3,
V3:4, H3]

**MARK-3.** A marked span outside the window can be fetched by its mark from the store
and placed again at the tail, whole. The mark fixes the span's start and end. Reason:
reminder at the tail gains an exact handle rather than relying on a model to recover
the span boundaries. The representation of both boundaries remains for spec.
[V3:4, P1]

**MARK-4.** No cut may drop material still owed to the store before that material is
filed. A model block correctly rejected by the classifier owes nothing to the store: its
disposition is on the trace, so it may be cut without waiting for filing. External input
that fails STE100 never enters the argument under GATE-1. A cut occurs between passes at
a natural decoder stop, when no other compute runs, with valid N. In the preceding
pause, the loop waits for the filing owed by retained material up to a declared bound. A
result still pending past that bound is missing under CLASS-8, and FLOW-4 halts the
agent instead of allowing the cut or continuing the wait. The bound and its unit are
spec values set by measurement. A filing wait in the pause is not a decoder stall, but
added wait is wall time under COST-4. Reason: valid material must remain recoverable,
rejected material has a recorded disposition, an active pass must not see context
change, and a hung classifier must not hold the cut forever. [D1, B7, B9, B5, V3:4, P3,
H5.10]

**MARK-5.** After load, mark creation and closure take the one feed: trace, tee, loop,
and store. The seed tool inserts the system-prompt end mark before the agent exists.
Sort reads the resulting boundaries and selects spans, and place lays selected content
and reports valid N. Neither marks nor cut decisions let sort write the store or place
manipulate engine entries. Reason: the handle composes with retain, sort, and place,
with only the elected pre-agent seeding exception. [B1, V3:4, H1, G2]

**MARK-6.** A mark identifies a span in state. That span's position in the argument can
move as cuts and reminders occur. The spec must keep span identity distinct from
argument position. Reason: a retained span remains addressable even when its current
placement changes. No identity encoding is elected here. [B8]

### Open for spec

- Mark identity, nested scope, and how an end mark fixes both boundaries of the span
  it addresses. Distinguish the task-prompt end from the enclosing task's work span.
  Keep identity distinct from argument position and both distinct from choosing N.
  [B8, V3:3, V3:4]
- Boundary and closure event representations, store write and mark-address read
  calls, and preservation of marks in the whole-store dump. [V3:4, H5.1]
- Mapping a stored span to its current argument position and a valid engine prefix,
  including spans that have been placed again at the tail. [V3:4, H7.2 item 1]
- The loop's cut-readiness check: decoder stopped, required filing complete, and no
  competing compute at the cut. Filing uses the preceding pause, so the condition
  cannot prohibit the classification needed to finish that filing. Correctly rejected
  material owes no filing, and its recorded disposition must be distinguishable from
  pending work. A result still pending past the declared cut bound is missing, and
  the loop halts under FLOW-4. [D1, B7, V3:4, P3]
- Measured value and unit for the cut wait bound, shared with CLASS-8, and its
  enforcement and trace representation in the pause. [D1]
- Whole-span fit and overlap with the injection budget. Place never truncates a span,
  and the existing loud fit failure remains the floor. [V3:4, P1, H5.2]

## 13. The renderer: place and the engine boundary

The renderer is hosted in the harness and performs place. Its input contract takes
selected material, the pinned argument head, the declaration's bounds, and the engine
facts needed to report valid reuse. Its output contract supplies the complete next
argument and N to the loop for the engine. It does not select or retain material.
[H1, H7.2 item 1, P:rulings/per-turn-line, P:walk/rung-6]

**PLACE-1.** Place lays the pinned argument head first and the moving tail after it.
The argument head is the system prompt from state, through its end mark. The task
prompt is held after it and belongs to the tail, followed by selected work, plan
steps, forward flags, retrieved spans, and new external input. The trace head is the
deployment tuple and is never the argument head. Reason: state begins with system
prompt, then task prompt, then work, while the tuple records the deployment for
replay. [V3:3, V3:4, REV4]

**PLACE-2.** A task change cuts at the system prompt's end mark, keeps that prefix, and
recomputes forward with the changed task prompt. It needs no restart and creates no new
session by itself. A data-plane emission cannot edit the system prompt, change
deployment configuration, or edit the locked plan. Operator plan updates follow PLAN-6.
Reason: the task is content after the pinned system prompt, so changing it need not
replace the system prompt or the deployment. Changing the system prompt requires a new
seed and a new agent under PERSIST-7. [B3, V3:3, REV4, H6, P:after/budget]

**PLACE-3.** Each deployment declares one decoder C_tail for that model on that
hardware. Every strategy and every fail-loudly check uses it. The argument has the
declared argument head and tail bounds, with C = C_head + C_tail. The spec measures
C_tail against model capacity and supporting memory. Reason: a capable model must not be
held to an arbitrarily smaller limit. No 32K input limit is elected by the paper's
document token budget. [H5.2, H7.1 item 1, H6, P:after/budget]

**PLACE-4.** If an input exceeds C_tail, the default is loud failure. Sort may drop
it only under a declared criterion with the drop on the trace. Place never drops,
truncates, or chunks it. Reason: silent loss makes an undeclared content decision,
and half a tool return breaks its pair. Chunking is outside this program's scope.
[H5.2, H8, P:ladder/step-3]

**PLACE-5.** Place reports N with the argument. N never exceeds the prefix for which
the engine holds valid reusable entries. Reason: an N that is too low costs a
recompute, while an N that is too high reuses invalid computation. No candidate
longest-common-prefix expression in the paper is elected by this charter.
[H3, H7.2 item 1, P:rulings/per-turn-line]

**PLACE-6.** The cut keeps up to N and recomputes forward. There is no middle swap
that changes a position and leaves later entries valid. State management reaches
the buffer through the argument and N, not by manipulating its entries. Reason:
the engine buffer is valid only as a prefix and belongs to the engine. Every cut
also obeys the mark and filing conditions in MARK-2 and MARK-4.
[V3:4, H5.3, H6, P:walk/rung-1, P:after/cut]

### Open for spec

- Measured value: decoder C_tail for the declared model and hardware, with the
  argument-head allocation and model capacity accounted for. [H7.1 item 1]
- The exact expression for N and the engine's evidence of valid reuse, including
  model, adapters, position treatment, non-text inputs, block boundaries, and an
  emitted token whose entry may not yet exist. [H7.2 item 1,
  P:rulings/per-turn-line]
- The renderer's input and output representations and the operator and user surface
  for a loud fit failure that does not bring down the host. [H7.2 item 8,
  P:walk/rung-6]

## 14. The four first-release strategies

**STRAT-1.** Leave in place, reminder at the tail, cut at the step boundary, and wall
compaction ship in the first release. No strategy requires saved buffer entries.
There is no shipped default. Each deployment declares its strategy as a sort
criterion and supplies thresholds such as the compaction fraction. Compaction acts
only on material after N. Everything reserved before N, the system prompt and the
task prompt where it is reserved, is untouched by any cut (the operator's ruling of
2026-10-18, R11). Reason: the operator elects a criterion for its task, and the
primitives hold no motive. [H5.3, PLACE-5, P:walk/rung-6]

| Strategy | Required behavior | Reason | Source |
| --- | --- | --- | --- |
| Leave in place | Keep the material at its existing position | Avoid a change now, with continued conditioning | H5.3, P:walk/rung-6 |
| Reminder at the tail | Select retained material, including a whole span by its mark, and lay it again at the tail | Put it beside the decision, at the cost of a duplicate | H5.3, V3:4, P:walk/rung-6 |
| Cut at the step boundary | Keep the prefix to the boundary, lay the needed outcome and following material, and recompute forward | Remove intermediate work from the next argument through a valid cut | H5.3, P:walk/rung-6 |
| Wall compaction | At a declared usage fraction, run one deciding pass for a summary, cut to N with everything reserved before it kept, and lay the summary as the new tail | Prove the primitives compose end to end at their simplest | H5.3 |

**STRAT-2.** Wall compaction is the bring-up path. The loop authors the compaction
request as its own trace event before it reaches the argument under FLOW-8. The summary
is a model emission that follows trace, admission, classification, and filing. Neither
the deciding pass nor sort writes the store. The cut waits for the filing owed under
MARK-4 within the declared bound and occurs at the decoder stop, or the agent halts
under FLOW-4. Reason: even the simplest composition must use the same trace and
retention path as every later strategy, including the bounded wait at a cut. [D1, D5,
H5.3, H1, G2, G4, V3:4]

**STRAT-3.** Cutting at a step boundary never deletes text from the middle of a stable
tail while pretending later entries remain valid. Needed later outcomes are selected
and placed after the cut and recomputed. A cut does not delete the admitted record
held by the store. Reason: shorter context is obtained through the one-direction cut,
not through loss of retained evidence. [H5.3, P:walk/rung-6, P:after/cut]

### Open for spec

- The representation of each declared strategy and its deployment thresholds.
  No default strategy or fixed compaction fraction is owed to this PRD. [H5.3]
- The deciding-pass and step-outcome representations at the existing sort, trace,
  and place seams. [H5.3, P:walk/rung-6]

## 15. Costs and measurement

**COST-1.** The ledger names recompute, conditioning, deciding pass, selection,
storage, and I/O. Resource cost and wall time are reported on separate axes and
never summed. Conditioning's behavioral effect is measured separately from its
execution cost. Reason: a strategy can move a cost between resources and the critical
path, and context length alone does not establish its effect on a decision.
[H3, H6, P:prices, P:prices/ledger]

**COST-2.** The trace must let the operator read, per turn, recomputed tokens,
deciding-pass tokens, selection work, I/O, and their waits. Measurements remain off
the residue path. Reason: the operator needs evidence to compare strategies without
feeding management measurements into the next argument.
[P:owed/prd item 4, H3, G3]

**COST-3.** A reminder retains the conditioning of the old context and adds a
duplicate. A cut pays recompute and any deciding pass, and does not itself prove
better decisions. Wall compaction pays for its deciding pass and rebuilding the
tail. These are cost obligations to measure, not a ranking. Reason: the benefit and
crossover depend on the task and deployment tuple. [H6, P:walk/rung-6,
P:prices/ledger]

**COST-4.** Every classifier lag is recorded on the trace as a measurement of how far
filing is behind and for how long. Gate-check delay on the input critical path is
recorded as wall time per inbound turn. A filing wait in a between-pass pause is not a
stall, but any added wait is wall time on the ledger. The trace records wait at each cut
and gate check and whether it reaches its applicable declared bound. Retrieval overflows
and selection misses are recorded, including misses after each cut, for DEFER-3 and the
falsifiers. Reason: lag shows an overloaded tuple, the gate check adds turn latency, cut
wait reveals a limit on filing, and selection misses test whether retained material
reaches the decision. These measurements stay off the residue path. [L1, D1, D6, D7, B9,
P3, P1, H3]

**COST-5.** The trace supplies gate refusal rate and CLASS-9 restatement-cap rate per
task, and store wait time per turn with disk waits identifiable, including those caused
by memory pressure. Reason: the STE100 exchange and no-disk-wait claims need evidence of
task obstruction and turn waits, not only their declarations. Measurements remain off
the residue path under TAP-3. [D7, H3]

### Open for spec

- The measurement representation on the trace and the methods for the declared
  tuple. Decoder capacity, classifier throughput in both directions, and decoder
  STE100 performance retain the specific measurement requirements of sections 7
  and 13, with output filing, cut wait, and gate-check timing distinguished under
  CLASS-6. [D1, D6, H7.1, P3, P:owed/prd item 4]
- Units and event forms for lag distance and duration, injection-budget overflow, and
  selection misses, including how a miss becomes observable. These measurements
  remain on the trace, not in residue. [D7, P3, P1]
- Trace measures for gate-check wall time and bound outcomes, cut wait and bound
  outcomes, per-task refusal and restatement-cap rates, and per-turn store waits
  with disk attribution,
  including memory pressure. Methods and encodings are spec. [L1, D1, D6, D7]

## 16. Falsifiers

The following claims are evaluated under a declared tuple. The record is the trace,
including the measurements in COST-4 and COST-5. A falsifier identifies evidence that
would defeat a claim. It does not silently replace an elected requirement. [D7]

**FALS-1.** The spec defines how the measurements below test the claims under a declared
tuple. Rate, duration, and decision-effect thresholds come from measurement and belong
to the spec. No numeric threshold is elected here. Reason: the falsifiers must test
behavior under the conditions of the deployment rather than impose numbers from the
critique. [D7, V5:5]

**FALS-2.** The STE100 exchange required by GATE-1 and CLASS-9 is falsified as a usable
task protocol if gate refusals and model restatement caps occur often enough that the
agent cannot do its task. The trace measures are refusal rate and restatement-cap rate
per task under COST-5. Reason: a mechanically enforced protocol fails its purpose if its
repair burden prevents the task. [D7 item 1]

**FALS-3.** STORE-1's no-disk-wait claim is falsified if any store operation on a turn
waits on disk, including through memory pressure. The trace measure is store wait time
per turn with disk attribution under COST-5. Reason: choosing an in-memory store does
not itself prove that a deployed turn never waits on disk. [D7 item 2]

**FALS-4.** The claim that a cut under MARK-2 and MARK-4 loses nothing that matters is
falsified if decisions after a cut miss material that was filed but not placed again.
The trace measure is selection misses after each cut under COST-4, the same measure used
by DEFER-3. Reason: filing preserves a record, but that alone does not establish that
the material reaches the decision. [D7 item 3, P1]

**FALS-5.** CLASS-6's ability to keep pace is falsified if filing lag grows without
bound under the declared tuple, if cuts reach the declared cut wait bound, or if gate
checks reach their declared bound. The trace measures are lag distance and duration,
wait at each cut, gate-check wall time, and reaching either bound under COST-4. Reason:
recent output can cover finite filing lag, but growing lag or exhausted blocking waits
shows that this tuple does not sustain the flow. The halt rule applies when a result is
still pending past its applicable bound. [L1, D7 item 4, D1, P3]

The four tests separate protocol usability, storage timing, selection after cuts,
and filing capacity. A cut can preserve every stored byte and still fail FALS-4.
A recorded lag can remain normal flow while supplying evidence against FALS-5. [D7]

### Open for spec

- Tuple, task workload, observation interval, rate denominators, and measured
  thresholds for protocol usability and sustained filing pace. The PRD elects no
  fixed refusal rate or timeout for either gate checks or cuts. [L1, D7, D1]
- Identification of a filed-but-unplaced fact needed at a decision, relating selection
  misses to the preceding cut through the existing trace floor. [D7, P1]
- Measurement of store wait and its disk contribution, including memory pressure,
  without treating a non-disk wait as proof of a disk wait. [D7]

## 17. The release boundary and deferred seams

**RELEASE-1.** The following line binds the first release and keeps later seams
available without reworking the primitives. Reason: prove the first circuit before
adding the capabilities that depend on it. The seed and boot read establish each agent's
starting state, and the gate enforces STE100 before model input. [H5.9, H6, A1, A2, P1,
P3, B1, B2, B5, D1, D2, D6, L1]

| Rung | Release | Condition for addition |
| --- | --- | --- |
| Dependable single trace-to-store flow | Prerequisite | Required before state management depends on it |
| Wall compaction | First | Elected bring-up path |
| Leave in place, reminder, cut at the step boundary | First | Declared strategy, no saved buffer entries |
| Seed, whole-store dump, and reinjection | First | Ordinary three-file save points and A/B verification for every load |
| System-prompt boot read | First | Read and place through the prompt's end mark before the user channel opens |
| STE100 input and output classification | First | Gate refusal and bounded input checks, model restatement for output, measured lag, and bounded filing waits at cuts |
| Tool-call pair lifecycle | First | Every opened pair reaches an end |
| Structured retrieval and harness-driven injection | First | Deployment enable control, injection budget, and declared selection under P1 |
| KV caching | Next | First release proves out |
| Cut and cache, cut and replace, splice and replace | Next | KV caching exists |
| Buffer-level suspend and restore | Next | KV caching exists |
| Prefetch organ, serve-side sort at rung six | Later | Measurement shows that material arriving one turn late costs decisions, independent of KV caching |
| Embedder and semantic nearness | Later | Measurement shows structured retrieval is insufficient |
| Classifier for content from the world | Later | Input other than STE100 must be taken |
| Safety classifier on input | Later | A screen is needed before or after STE100 classification |

**DEFER-1.** KV caching and buffer-level restore attach at the engine boundary.
Saved entries remain the engine's intermediate computation. The loop records the
decision to save or restore and place supplies the argument and valid N. Reason:
state management must not gain a private path into an engine-owned buffer.
[H5.4, H5.9, H6, P:terms-drawn-in, P:walk/rung-6]

**DEFER-2.** Deferred restore requires ordered, attributed material at the query
seam and a valid engine operation for any saved-entry load. The admission test asks
whether those interfaces support the arrangement without a harness edit. It does
not declare the arrangement already built. Reason: a missing operation is a new
mechanic, not a hidden duty of an existing primitive.
[H6, P:owed/prd items 18 and 19, P:walk/rung-6]

**DEFER-3.** An embedder and semantic-nearness retrieval attach only after a
measurement shows structured retrieval is insufficient. Their selected results
still pass through sort and place and cannot become an independent retention feed.
The measurement is the retrieval overflows and selection misses recorded on the
trace. Reason: use the structured store first, then let observed insufficiency decide
when semantic ranking is needed. [P1, H5.8, H6, G2]

**DEFER-4.** A classifier for content from the world is the later rung that may relax
the gate's STE100 input requirement. A safety classifier remains a separate seam and may
sit before or after the STE100 classifier, with drops under a declared criterion and
recorded on the trace. Reason: these additions extend input handling without changing
the sort duty or treating protocol conformance as a safety decision. [B5, H5.5, H6,
P:ladder/step-3, G3]

**DEFER-5.** The prefetch organ is deferred above the first-release line, with its
signal, query, and candidate-release seams chartered below. Add it when a measurement
shows that material arriving one turn late costs decisions. It is an instance of
serve-side sort at rung six, not a new mechanic, and its addition does not depend on KV
caching. Reason: it changes what is present at the decision, not what is retained, and
the measurement must establish the need for that earlier presence. Prefetch does not
hide retrieval latency. [D2, P:ladder/step-3, P:after/control-loops]

### Open for later specs

- The engine interface for saved buffer entries, compatibility checks, and ordered
  reconstruction, without electing a first-release cache mechanism. [H5.9,
  P:owed/prd items 18 and 19]
- How an operator keeps consumed buffer entries and the holdings read by sort for
  replay, and whether that evidence is keyed to the trace in a separate record.
  Saved entries are not residue. [P:walk/rung-6, P:owed/prd item 24]
- Restore equivalence and the resource and wall-time crossover between recompute
  and saved-entry restore. [P:owed/measurements items 20 through 22]
- Interfaces for later semantic retrieval and world-content and safety classifiers,
  preserving the existing query, sort, and place boundaries. [H6]

### 17.1 Deferred prefetch contract

The input seam receives a signal from the live emission on the management plane.
The query seam reads held content from retain through the store's contract. The
output seam offers selected candidates from bins to place for a later pass. These
are the existing sort, query, and placement duties, with their timing declared.
[D2, P:ladder/step-3, P:after/control-loops]

**PREFETCH-1.** The prefetch organ runs serve-side sort on a signal from the live
emission and fills candidate bins from retain while the decoder works. The signal is
read on the management plane as measurement payload and never enters residue or the
argument. Reason: the signal can make held material available for a coming decision
without becoming content that conditions the model. The validated partition keeps
measurement payload out under TAP-3. [D2, D3, P:ladder/step-3, P:after/capability]

**PREFETCH-2.** Prefetch reads through the existing query seam, selects under a declared
criterion, and offers candidates to place. The store owns the read. Prefetch never
writes the store, changes admission, or turns a candidate bin into another retention
feed. Reason: prefetch changes what is present at the decision, not what is retained.
Its decisions are new trace events under SORT-4. [D2, D4, H1, P:ladder/step-3,
P:after/control-loops]

**PREFETCH-3.** Candidate release uses append into a later pass's argument. The current
pass does not change. Append preserves the existing prefix, and the paper's full-length
N applies only to the extent that prefix is valid under PLACE-5. The renderer still
reports N, and its exact expression remains for spec. Release obeys the place, C_tail,
tool-pair, and mark rules, including bounded filing wait before any required cut. The
signal itself is never released to place. Reason: earlier selection can make a candidate
present at the decision turn rather than one turn late, but it cannot change active
computation, bypass capacity checks, or reuse an invalid prefix. [D2, D1, H5.10, H7.2
item 1, V3:4, P:ladder/step-3, P:after/control-loops]

### Open for the prefetch spec

- Signal representation and declared trigger on the management plane, with the
  signal excluded from the argument and residue. [D2, P:ladder/step-3]
- Candidate-bin representation, store queries, selection criterion, and release
  handoff to place under the existing read and placement contracts. [D2, H1]
- Release timing between passes, append and valid-prefix reporting, and fit with
  marks and the declared budgets. No live argument change is permitted. [D2, H5.10]
- Measurement under a declared tuple of whether one-turn-late material costs a
  decision, relating candidate availability and placement to the decision turn on
  the trace. No latency-hiding claim or KV prerequisite is elected. [D2]

## 18. Scope limits

**SCOPE-1.** A consolidating organ belongs to drey and is outside this charter.
Detection of a declaration that no longer matches the trace belongs to Weaver
analysis, outside the agent. Document processing, chunking, and free conversation as
a meaning-protocol target are outside scope. Reason for the detector's placement:
Weaver analysis is the main reader of the trace. Reason for excluding free
conversation: the controlled technical scope makes the protocol stable. No further
reason for the other exclusions is supplied by the handoff. Diagnostic mode belongs
to Weaver analysis in its separate repository and is outside this serving-operation
charter. In that replay mode the front gate is off, the trace drives inputs, and the
purpose is to inspect model distributions at each decision point. Reason: replay is
analysis, not serving operation. It grants no exception to the serving single-feed
rule. Pairing rejected originals with corrections and correlating rejections belong
to Weaver analysis, which reads the trace outside the agent. Sink durability belongs
to the trace charter and is owed there. An agent for analyzing injection material
would require a later declared mechanism outside this release's STE100 input contract.
Reason: these are analysis, trace-custody, and later input duties rather than the
state-management primitives chartered here. [H8, H5.5, R7, A3, A4, A5]

The deferred query seam does not charter drey or elect the source or admission policy
of its future offerings. The three capability properties in section 4 remain
constraints at that boundary. [H6, H8, P:after/capability]

## 19. Vocabulary

The glossary of v0.14 governs where not replaced by the authority order in section 0.
In particular, A5 changes residue's boundary and section 3 of the second return
separates the trace head from the argument head. B1 adds the sole pre-agent seeding
exception to the origin of retained content, and B8 separates span identity from
argument position. L1 bounds waits that block the model, including the gate check
and the cut wait. G7 locates partition selection
on the tee's copy. The following terms are central to this charter.
[L1, H9, V5:0, V3:3, A5, B1, B8, D1, D2, D3, G7]

| Term | Meaning |
| --- | --- |
| State | What exists only during the pass |
| Residue | Retained content from the operating tap, plus the sole pre-agent seeded-content exception accounted for by A and B |
| Trace | The operator's complete record of what enters, happens within, and leaves the agent |
| Tap | The single observation point on completed trace content from which admitted copies enter state management, with the trace head outside it |
| Tee | The primitive that copies traffic at the tap, applies the declared partition to that copy, yields selected material, and discards the unselected copy without changing the trace |
| Envelope | Event kind, channel, and origin for admission |
| Partition | The declaration selecting from the tee copy by event kind, channel, and origin, validated at load to exclude measurement payload, save_point, and message.restored |
| Retain | The function of holding residue, performed by the store |
| Store | The member of state management that owns the read and write behind its own contract and socket |
| Sort | Reading, labeling, joining, and selecting under a declared criterion |
| Classifier | The primitive performing model-based sort on the SPU |
| Place | Laying the argument head and tail into the argument and reporting N |
| Renderer | The harness-hosted primitive that performs place |
| Trace head | The deployment-tuple record for replay, never tapped or stored |
| Argument head | The pinned system prompt from state, ending at its mark |
| System prompt | STE100 content inserted first by the seed and read first at each load, living in state rather than configuration |
| Task prompt | STE100 task content accepted at the gate and held after the system prompt, outside the argument head |
| Tail | The moving argument material after the argument head |
| Buffer | The engine's intermediate computation, valid only as a prefix |
| N | The position from which the next pass recomputes, never beyond valid prefix reuse |
| Tool-call pair | The send and return of one communication, with an explicit lifecycle ending |
| Recall verb | An internal tool call with a fixed recall action for which the model supplies the subject |
| Decoder C_tail | The deployment's single measured tail capacity for its model and hardware |
| C_tail of the classifier | The temporal filing budget measured against the cut that would remove the material, with normal lag measured and waiting at a cut bounded under CLASS-8 |
| Tee failure | Component or delivery failure that breaks the one admitted flow into the store and halts the agent |
| Classifier failure | Crash, missing result, or unparseable result, including a result still pending past the declared gate or cut wait bound, distinct from normal output lag or a valid input-defect report |
| Gate-check bound | The declared limit on the input conformance check before the model receives input, with its value and unit measured in the spec |
| Cut wait bound | The declared limit on waiting for filing in the pause before a cut, with its value and unit set by measurement in the spec |
| Prefetch organ | A deferred serve-side sort that fills candidate bins from retain on a live-emission signal while the decoder works, releasing candidates for a later pass |
| Candidate bin | Material read from retain and held for prefetch selection and release, without changing retention |
| Save point | A directory containing a SQLite dump, configuration copy with dump hash A, and manifest with A and configuration hash B, created as a seed, on demand, or at shutdown |
| Dump hash A | Hash of the dump, recorded inside the configuration, in the manifest, and on loading traces and any operating session's saving trace |
| Configuration hash B | Hash of the finished configuration containing A, recorded in the manifest and on loading traces and any operating session's saving trace |
| Manifest | The third save-point file, holding A and B |
| Seed | The ordinary save point made by an operator tool before an agent exists, containing its STE100 system prompt and end mark |
| Sampling seed | The decoder's sampling seed recorded for replay, distinct from the agent's initial save point |
| Boot read | The first read after load, fetching the system prompt through its end mark for placement before the user channel opens |
| Agent continuity | A chain in which each session loads the preceding session's ending dump, with a new seed starting a new agent |
| Mark | The identity of a span in state with its boundary, distinct from its movable position in the argument |
| Filing lag | How far filing trails emitted material and for how long, measured on the trace |
| Plan | Retained STE100 steps with stable IDs, changed only by operator update once locked |
| Closed step | A step recorded as having passed the mechanical review setup, tests and gate |
| Forward flag | A retained statement whose condition names the later step that needs it |

The phrases retained state and state management do not create a second definition
of state. What the store holds across passes is residue, with the seeded initial
content's origin explicitly excepted under B1. Reload restores those holdings, and
an engine restore rebuilds intermediate computation. The plan, seeded system prompt,
task prompt, and work remain held content, not a second definition of pass state.
[B1, B2, A5, H5.4, REV4, P:terms-drawn-in]
