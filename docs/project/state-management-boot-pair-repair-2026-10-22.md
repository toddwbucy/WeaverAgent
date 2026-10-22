# State management boot pair repair owner

Date: 2026-10-22
Branch: code/state-boot-pair-repair
PR: pending assignment
Epic: #146

## Authority and dormant scope

This slice implements the repair mechanics in merged Spec 05 BOOT-REPAIR, PAIR-END,
PAIR-FILE and PAIR-READ, with HARNESS-READ, HARNESS-RECALL, HARNESS-CLOCK and the repair
portion of HARNESS-BOOT. PERSIST-LOAD and HARNESS-BOOT require checked decoder Open
before this operation and a plan summary before the user gate opens. This slice
implements neither surrounding step and grants no lifecycle Enter or gate readiness.

PriorBootOpen has private fields and no production constructor, deserializer or
conversion. Its binding names this context, its exact recorded BootHead and original
Enter bound. BootHead alone cannot call this operation. A future checked Open owner
must supply the actual missing prerequisite before this primitive becomes reachable
in production. The tests explicitly assume that one prerequisite through a named
cfg(test) constructor. They still obtain load, template and boot-read evidence through
the existing real owner paths. No test here proves decoder Open or its fit checks.

Draft #232's Open evidence carrier is not implementation authority or a dependency
of the normal repair protocol. It remains separate from this already merged repair
behavior. The existing serving path is unchanged, and no worker/dev_loop call is added.

## Read and receipt ownership

Admission requires the active load recorder and state coordinator, cached BootHead,
verified context StartIdentity, the unchanged original Enter and no open model turn or
competing owner. It consumes the private prior-Open prerequisite once. It takes no
caller pair list, load identity, raw Opened body or boolean standing in for a proof.

The first private OpenPairs read owns the original bounded read, all pages, acquired
snapshot release and ordinary recall Recorded. The exact resulting RecordedRead stays
with the transaction. An initial typed refusal has no cursor to release. A later valid
refusal releases its acquired cursor once. A malformed result or failed seam follows
the existing latch and close path. Refusal never becomes an empty successful list.

Every pair in the initial snapshot receives a current-run tool.call.completed in the
returned order, with its original CallId and failed/abandoned_at_load. The event has
Origin::Loop, no turn, execution or outcome, and the complete verified loaded_from.
A current-run call cannot be relabelled as an old abandoned call. The normal author,
tap, PairCopy, file_pair and recorded receipt subscription carry the terminal. There
is no direct state edit, replacement opening, classifier ask, gate exchange or model
return. Existing serving CompletedPair and ToolResult constructors are not used.

Each exact committed terminal receipt precedes the next terminal. The completion
checks source identity, envelope, scheme, whole payload and matching filed disposition.
The final OpenPairs query begins only after all those receipts. It must also release
and record before its empty result grants RepairedBootPairs. An initially empty list
still requires the separate second query. A nonempty recheck refuses completion and
does not initiate another repair pass or append new pairs to this batch.

Both read owners and all terminal receipts remain in the private repaired proof.
Cancellation and refusal retain their account without a success-proof accessor.
Competing reads, authors, explicit operations and lifecycle release cannot take over
its active or completed ownership slot. Taking its result does not retire Enter.

## Clocks, cancellation and failure

Both reads use their own original two-second StateControl allowance capped by the
unchanged Enter and an already active earlier Leave. Pages, release and recording do
not restart it. Retiring Leave does not extend a bound that inherited its earlier cap.

Each required terminal receipt uses the loaded measured decision_wait, capped by
Enter and an active earlier Leave. The first terminal wait installs with the retained
initial read, and each subsequent wait installs atomically with its predecessor's
consumed receipt. No recoverable clock hook splits those ownership transitions.
A timely Recorded receipt retires its Decision before later live-clock sampling, so
late consumer retrieval cannot turn completed receipt work into a timeout. The full
boot budget still remains in force. No wait helper may steal an owned Enter/Decision.

A recoverable lifecycle Wait retains the current stage, exact author/read/filing and
its bound. It is not automatically a cancellation. Explicit cancellation revokes the
final success, withdraws a staged unstarted terminal, or finishes an already admitted
read/filing under its existing bounds. It starts no later terminal or recheck. An old
loaded call has no serving turn or executing gate exchange, so this owner invents no
stop/not_dispatched outcome, ToolCancel, tool clock or save point.

A failed flow retains the owner and cannot file another terminal. The read owner keeps
its existing failed-channel cleanup responsibility. This primitive returns local
refusal/cancellation evidence for the lifecycle adapter. It does not claim to implement
post-activation failed-enter rollback. The existing begin_enter_rollback method handles
the separate refused-partition path and is not reused as a substitute for that work.

## Instruments and pending validation

Sixteen tests use the actual load/template/boot-read owners, state sockets, recorder,
ordinary tap, file_pair and receipt machinery with scripted classifier/member answers.
The prior decoder Open remains the explicit test-only assumption. Cases include:

- Multiple old-run pairs with returned order different from lexical CallId order.
- Exact StartIdentity and old CallId retention in new-run, turnless terminal events.
- Pagination, empty initial result, both read releases and the final empty recheck.
- Each terminal receipt held before the next terminal and final query.
- Initial/later read refusals, nonempty recheck, malformed result and member refusal.
- Foreign prior-Open evidence, retired Enter, once-only admission and read/author theft.
- Cancellation before a read, during acquired release or a staged author, after a
  sent terminal and at the recheck.
- Real recorder capacity retaining one recall ticket, payload and original deadline.
- Original Enter/read deadlines and an earlier Leave cap after Leave is retired.
- Forty-eight nested Leave checkpoints through read and receipt ownership transfers.
- A timely recorded terminal consumed after its Decision deadline but within Enter.
- Current-run abandonment rejected before any terminal record or store write.

Compile-time ambiguity pins require PriorBootOpen and RepairedBootPairs to remain
non-Clone and non-Default. Additional pins forbid StartIdentity-to-prerequisite and
pair-vector-to-result conversions. The production prerequisite has no constructor.
Nineteen behavioral recipes and six compile-time mutation probes are prepared outside
the repository. No mutation has run yet and no compiled failure is claimed.

Compilation, focused/full tests, lint, formatting, lock and protective mutation gates
remain pending the root's serialized lane. No independent review has completed for
this slice. This is an Executor implementation record, not a Planner grade, language
qualification, hardware qualification or permission to activate the serving path.

## Remaining integration

The checked Open owner and its actual prerequisite construction remain owed. The full
boot coordinator must sequence Open, this repair, held plan summary and user gate, and
perform post-activation failed-enter rollback. A real member dump with multiple open
pairs, receipt failure at each position and reload of the original selected artifact
remains an integration instrument. The scripted member proves consumer mechanics,
not SQL commit semantics, adoption behavior or save-point publication exclusion.

No new boot protocol election was found. Coupled serving cutover, lifecycle composition,
production classifier/prompt/corpus qualification and measured limits remain open.
The operator alone merges, and this slice remains a draft.
