# State management boot pair repair owner

Date: 2026-10-22
Branch: code/state-boot-pair-repair
PR: #247
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

Each old-run pair reached in the initial snapshot receives a current-run
tool.call.completed in returned order, with its original CallId and
failed/abandoned_at_load. The event has
Origin::Loop, no turn, execution or outcome, and the complete verified loaded_from.
A current-run call cannot be relabelled as an old abandoned call. The check is
per pair. In an old/current mixed snapshot, the prior old terminal may already
be committed before the later current-run call halts repair. Its correct receipt
remains owned and no final recheck or repair grant follows. The normal author,
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
Borrowed account views expose both read outcomes, committed terminals and the exact
withdrawn unstarted event. They cannot mint repaired evidence.
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
read/filing under its existing bounds. It starts no later terminal or recheck.
As with begin_task, cancelling a completed but untaken result revokes its grant.
Already recorded reads and committed terminals remain in the cancelled account.
Calling advance, cancel or take without a repair owner is a protocol caller defect.
An old
loaded call has no serving turn or executing gate exchange, so this owner invents no
stop/not_dispatched outcome, ToolCancel, tool clock or save point.

A failed flow retains the owner and cannot file another terminal. The read owner keeps
its existing failed-channel cleanup responsibility. This primitive returns local
refusal/cancellation evidence for the lifecycle adapter. It does not claim to implement
post-activation failed-enter rollback. The existing begin_enter_rollback method handles
the separate refused-partition path and is not reused as a substitute for that work.

## Instruments and measured validation

The original seventeen tests use the actual load/template/boot-read owners, state
sockets, recorder,
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
- A single current-run call rejected before its terminal record or store write.

The #247 Planner follow-up adds ten tests. They cover the exact Enter-capped
terminal window and expiry attribution, an actual receipt Recorded at or after
Decision expiry during bounded work, cancellation between two committed-terminal
steps, release refusal in initial/between-leg/completed states, a mixed old/current
snapshot, and cancellation after both successful and refused completion. Account
views are checked against actual reads, committed receipts and withdrawn events.
No-owner operation use is pinned as a caller defect.

Three unchanged receipt association predicates are tested with inspection copies
from a real committed terminal. The wrong turn, changed pair and changed source
sequence never replace a sealed receipt or simulate a reachable member echo. The
changed pair's raw payload is changed together so its structural check still passes.
These fixtures pin defensive checks without weakening read or filing validation.

The post-admission Busy case reuses #245's per-instance coordinator hook. Normal
synchronous admission already checks the real Busy predicates. The hook allocates
no competing operation or request and is described as a defensive returned-Busy
probe. Foreign read and filing checks remain redundant with the private purposes
and each owner's admission guard. No artificial owner is used to claim otherwise.
The small predicate extraction preserves production checks. Broad withdrawal,
receipt-timing and owner-hook consolidation remains batched to avoid unrelated churn.

Follow-up source and recipes are prepared. Compilation, strict lint, formatting,
lock, the joined full suite and compiled mutations remain pending root's serial
lane, including joining the #245 hook before Cargo. The baseline results below do
not validate these new edits.

Compile-time ambiguity pins require PriorBootOpen and RepairedBootPairs to remain
non-Clone and non-Default. Additional pins forbid StartIdentity-to-prerequisite and
pair-vector-to-result conversions. The production prerequisite has no constructor.
Olympus used private target /tmp/weaver-146-boot-pairs-target and the pinned toolchain.
The prior joined baseline passed 1356 tests with 18 inherited ignored cases.
All 17 focused boot tests, strict all-target harness clippy, formatting and lock passed.
All nineteen protective mutations compiled, failed their intended test and passed after
exact-byte restoration. All six ownership probes reached the intended E0283 ambiguity,
and the restored baseline check passed.

The first compile found an iterator borrow retained across a mutable fault call. A
local validation scope fixes its lifetime, and an unused test import was removed.
The first lifecycle receipt-transfer mutation survived the original bounded checkpoint
fixture. A new fixture starts from the actual Recorded receipt, counts the single
transfer's clock samples and crosses Leave at each one. That mutation now fails with
the exact missing-owner assertion and passes restored. Initial failures and the survivor
remain in the evidence. Seven earlier protective cases and twelve corrected/final cases
make the nineteen total. The final full suite includes the added transfer test.

Two independent author readings covered the owner and its joined read/marks/receipt
boundaries without a remaining concrete finding. Logs, recipes, type probes and source
hashes are deposited on the shared bulk store under
weaver-testing/state-management-146-olympus-2026-10-21/boot-pairs. These are author
checks, not a Planner grade, language/hardware qualification or serving activation.

## Remaining integration

The checked Open owner and its actual prerequisite construction remain owed. The full
boot coordinator must sequence Open, this repair, held plan summary and user gate, and
perform post-activation failed-enter rollback. A real member dump with multiple open
pairs, receipt failure at each position and reload of the original selected artifact
remains an integration instrument. The scripted member proves consumer mechanics,
not SQL commit semantics, adoption behavior or save-point publication exclusion.

No new boot protocol election was found. Coupled serving cutover, lifecycle composition,
production classifier/prompt/corpus qualification and measured limits remain open.
This branch descends from #245, preserves exact #190 ancestry and inherits its
qualified coupled-cutover hold. The operator alone merges. This PR is submitted as
a draft and must not activate the incomplete replacement.
