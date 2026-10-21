# Input transaction and checked-source handoff

Date: 2026-10-21. Status: local implementation, draft submission pending.

This slice is stacked on [PR #203][base]. It implements the serving input transaction
specified by [Spec 02 INPUT and REUSE][protocol] and [HARNESS-INPUT and
HARNESS-LOOPS][harness]. The complete existing serving path remains active under
[FOUNDATION-TRANSITION][foundation]. No production language artifact or system prompt
is qualified by these fixtures.

## Implemented boundary

InputSlot owns one eligible frame, its original gate window, input ID, offered source
and classifier exchange. A busy intake returns the original frame without spending
an ID or starting its check clock. The original base64 frame records before octet
decoding and request parsing. The inherited strict request parser remains the one
parser. The replacement input path refuses every role member, including null and
user, while the existing serving path retains its previous contract until cutover.

The run's FlowCoordinator submits the exact Text at block zero through its existing
SPU socket owner. The classifier's accepted result grants reuse only after the exact
ste100.result records. AcceptedInput owns that non-Clone grant and authenticated UID.
Its private constructor and shared revocation handle prevent a late result or a token
already transferred to the caller from surviving cancellation or failure.

A correct non-STE100 result records its refusal and returns the exact unnamed refused
response. It never opens a turn or latches component failure. TYPES-INPUT supplies the
strict shared InputRefusal shape under RefusalRecord::Input. The gate's relay and its
credential admission remain unchanged.

The engine-side handoff consumes AcceptedInput once, authors turn.started and the exact
message.user, then returns its source and Message. The message's causal parent is its
input.offered sequence. Interrupted authoring retains the handoff and original ticket.
The filing coordinator pins reuse to that actual source, preserving it across unrelated
copies and a waiting whole-copy reservation. An unselected source cannot donate its
bindings to a later event. The existing filing flow carries selected content. This
handoff adds no separate read/write protocol or secondary content feed.

## Time, cancellation and failure

One declared gate_wait starts at serialized intake. Queue delay, offered authoring and
classification use that original bound. A recorded result carries the immediate common
clock sample from its Recorded reduction. Acceptance at or after the bound fails.
Timely acceptance disarms that wait before later caller work, and its cost measurement
ends at the same recorded boundary. No new clock or measured production value is added.

Cancellation before submission still records the original offer and closes unnamed.
Cancellation with a submitted check retains its exchange until the result records or
bounded teardown takes over. That recorded result ends the submitted check's wait even
if the stopped response is still waiting to be carried. It never creates a token. The
response is accounted once, including a stopped close carried after the failure latch.
An already consumed input belongs to its opened-turn lifecycle and emits no unnamed
close from this transaction.

The input gauge and cumulative counters include an eligible frame before any submission.
A failure at the next clock read still reports that ownership. Halt snapshots check the
input ID against this owner and permit a queued input beside an unrelated classifier
source request. The first failure revokes the input token before reporting. There is no
halt dump, classifier restart or continuation of decisions after a component failure.

## Validation

The affected-crate suites pass 959 tests with zero failures and 17 inherited ignored
instruments: 712 harness, 137 trace and 110 types tests, including doctests. The command
is cargo test -p weaver-types -p weaver-harness -p weaver-trace --locked. Real Unix
sockets run on the host socket lane. The final log is /tmp/state-input-final-tests.log.

All-target clippy for weaver-types and weaver-harness passes with warnings denied.
Workspace formatting, the lock gate and diff checks pass. The lint log is
/tmp/state-input-clippy2.log. The SPU CUDA lane is outside this slice.

Twenty-two behavioral probes remove or weaken a protective check. Each compiles, runs
its one named test and fails that test's intended assertion. A separate type pin makes
AcceptedInput cloneable and fails compilation at AcceptedInputMustNotClone. These are
23 probes, not 23 behavioral tests. Each restores exact source bytes. Independent review
checks the failure logs and restoration hashes. The evidence is held locally in
/tmp/state-input-transaction-mutations/ and /tmp/state-input-early-finish-probe/.

The probes cover shape refusal, cancelled late grants, source correlation, deadline
settlement, close acknowledgment, refusal recording, input accounting, author resumption
and strict refusal codecs. The final probe prevents early finish from clearing an
interrupted source handoff. The cross-source reuse probe detects an incorrect ledger
rejection, not a demonstrated unauthorized filing. The tests do not qualify production
STE100 artifacts, throughput or measured deployment limits.

## Integration still owed

These mechanics remain dormant. The coupled lifecycle change must install the serving
frame path, transfer the pending token through Ports, replace raw loop contributions
with the reviewed Contribution API, and use this handoff at the engine's sole user
source author. It must retain gate exchange correlation and acknowledge each response
only after the existing transport owner accepts it. The primitive elects no loop pass,
retrieval policy, repair schedule or deployment switch.

The template certificate, production classifier artifacts, system prompt, boot/read
adapter, output repair, capture cleanup and complete halt containment retain their own
contracts and implementation obligations. The serving recall representation and snapshot
timing amendments remain separate review items. This record does not claim the full
state-management goal or cutover is complete.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/203
[protocol]: ../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[foundation]: ../crates/state-management/state-management-foundation-Spec.md
