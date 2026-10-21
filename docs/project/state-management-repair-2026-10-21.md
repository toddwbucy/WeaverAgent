# State management: repair primitive

Date: 2026-10-21. Status: implemented, draft review pending.
This is an implementation record, not a Spec amendment or cutover approval.

## Scope and authority

This slice follows Spec 02's OUTPUT-UNIT, DECISION-BARRIER, REPAIR, TEMPLATES,
INSTRUCTION-METADATA and TOOL-STATEMENT, and Spec 05's forbidden-pass pair
accounting. It builds on the consolidated harness branch in PR #198. The loop
still chooses a pass and invokes the decoder. This primitive owns evidence,
recorded instructions, correction disposition and the corresponding request account.
It neither selects a strategy nor calls a decoder, gate tool or retrieval port.

The complete legacy serving path remains active. Production language artifacts,
measured deployment values, qualified templates and caller migration remain required
by the foundation cutover. The tests below use synthetic protocol fixtures.

## Ownership and results

Repair starts from an owned OriginalRequests and the actual load-time template
certificate. Its source leases must belong to this recording context. Run, scheme,
open turn, load inventory and recorded template transcript must agree. A compiled
restatement family supplies a literal-only certified expansion. Rejected text stays
in the argument and is named by source metadata. It is never copied into a slot.

The ledger processes rejected Text blocks in original order. It records one
repair_requested event per source, then each instruction before transferring its
RestatementRequest. A selected instruction's exact recorded classifier result is
observed without consuming its filing receipt. Correct runtime rejection records
state_template_rejected and refuses the operation. The current serving declaration
requires this instruction to be selected. The watcher's unselected arm remains
defensive and is unreachable through that validated declaration. Repair does not wait
for unrelated ordinary filings.

The caller supplies each complete checked correction. A failed pass counts once,
regardless of how many blocks failed. Attempted calls in correction passes receive
recorded pass_purpose accounts before another attempt. Accepted correction text
remains filed even when its pass cannot supply the logical replacement.

EffectiveEmission preserves original generation identity and finish, substitutes
successful statement spans in original order, and joins their bytes with LF.
External requests retain their original CallId, tool name and opaque arguments.
A repaired request uses the correction's exact statement receipt. Pending request
facts are not execution grants. Multiple bindings cannot be guessed into one send.

The measured retry cap supplies the existing repair_exhausted outcome. Its trace
record precedes terminal accounting for every known conforming original send,
including one whose statement was successfully repaired earlier. A socket write
never substitutes for a committed opening or terminal receipt.

Stop and unload revoke repair success while retaining outstanding evidence. They
finish actual pending authors and required pair accounts without changing already
recorded terminal bytes. Component failure freezes the owner. None of these paths
starts a decoder or silently drops a checked correction. The sole template
certificate remains recoverable for another generation.

The decision wait uses the enclosing absolute bound and remains owned across check
completion and correction handback. No partial reply renews it. The wait ends before
the caller runs a correction decode. Ordinary filing lag remains a measurement.

## Refusal and open contract boundaries

A structural instruction refusal returns owned evidence for its trace record.
The caller must record it before reporting the operation refusal and must resume
that same author ticket if recording yields. A correct conformance result never
becomes a component halt.

An empty requested correction remains an explicit owned Spec gap. Draft PR #213
proposes a counted failed attempt, but this slice does not implement an unmerged
rule. A malformed recall binding likewise retains its actual evidence and grants
neither a guessed subject nor a fabricated refusal case. These paths are not
production qualification and cannot be treated as successful model answers.

Runtime instruction refusal can return custody without selecting a cause for an
undispatched send. The loop must preserve that account and provide the actual
later disposition. Returning custody does not mean its requests were completed.

## Verification

The full harness suite passes 952 tests, with zero failures and 17 inherited ignored
instruments. All 20 repair context tests run over the actual recorder and local sockets.
The passive repair owner has 17 tests and literal template expansion has five. The
changed shared floor passes 114 tests with default features and 158 with config.
All-target clippy passes for the harness and both floor feature modes. Formatting,
lock, diff, document text and local link checks pass.

Ten deliberately altered checks compile and fail their intended tests. Each source
file is restored byte for byte in a finally block, and the restored suites pass.

| Altered property | Observed failure |
| --- | --- |
| Certificate run identity | Wrong refusal classification, a second check still refuses the foreign run |
| Literal-only expansion | A slot-bearing family reaches the expansion result |
| Exact correction scheme | A foreign scheme passes the repair owner's check |
| One count per failed pass | Failed correction count changes by two |
| Committed terminal proofs | A correction account is accepted without its terminals |
| Actual load certificate | A foreign inventory is accepted at repair intake |
| Outstanding correction custody | Cancellation completes before the checked loan returns |
| Runtime instruction verdict | Correct template rejection no longer produces operation refusal |
| Owned Decision wait | A caller can retire the correction's original bound |
| Closed refusal family | A refusal accepts foreign fields |

Independent local review found and verified fixes for correction handoff custody,
context-owned cancellation, refusal result transfer and deadline ownership. A separate
read-only pass verified the ten mutation logs and restored guards. These are local code
and evidence reviews. The Planner grade and operator review remain owed.

## Remaining implementation

The loop integration still owes delivery of the emitted correction, invocation of
each granted restatement pass and handling of operation refusals. External tool
execution, recall reads, placement, plan/review orchestration and complete lifecycle
migration remain their own consumers. The compiled production restatement template
and language qualification are not elected by this code slice.
