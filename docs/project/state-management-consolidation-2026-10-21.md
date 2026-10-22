# State management consolidated implementation

Date: 2026-10-21. Landing PR: #215.
Reviewed source: 8acc5f8877e9b9fd2068b420cab5812602d4fe5f.

The foundation, trace and additive harness roots are on main through #184, #185,
#198 and #215. The [implementation map][map] names their primitive boundaries.
[Issue #146][completion] tracks the remaining implementation and qualification.
Runtime activation remains held for the coupled foundation cutover.

## Authority and scope

The merged Specs authorize the consolidated ports. Spec amendments #189, #192 and
#213 authorize the recall/boot, observation/2 and empty-correction implementation
work tracked by #146. Their merge does not establish completion of their consumers.
The empty-correction implementation and its review are recorded in
[the dedicated record][empty-correction].

The consolidation retains the original merge ancestry, including #217 at bdfa3a76
and #218 at 8b387eff joined by 32a57b57. The paired wire and its complete consumers
remain one deployment boundary. Closed child PRs are not separate landing targets.

Implements: [Spec 05] PAIR-STATES and RETRY, [Spec 06] MEASURE-DELTA,
[harness amendment] HARNESS-READ, HARNESS-BARRIERS, HARNESS-AWAIT-AUTHOR,
HARNESS-RECORD-LIFETIME, HARNESS-PUMP and the measurement portion of HARNESS-PLACE,
and [state amendment] STATE-PAGES and STATE-CHANNEL as consumed by the harness.
The grades below concern these private ports; they do not grade serving integration.

| Owning section | Scoped author grade and implemented behavior |
| --- | --- |
| Spec 05 PAIR-STATES and RETRY | Conforms within external execution and lineage validation: serial openings, separate windows and filed active-batch predecessors. |
| Harness HARNESS-READ | Conforms within read continuation: accepted page/release progress survives a recoverable post-validation wait. |
| Harness HARNESS-BARRIERS, read admission | Conforms within external admission: armed Gate/Cut and unrelated owner barriers remain intact. |
| State STATE-PAGES and STATE-CHANNEL, harness consumer | Conforms within page/refusal handling: acquired cursors and exact protocol legs are preserved. |
| Spec 06 MEASURE-DELTA and harness HARNESS-PLACE, measurement owner | Conforms within measurement custody: recorded-author successors are saved before yielding, and liveness is checked before consuming a reservation. |
| Harness HARNESS-AWAIT-AUTHOR and HARNESS-RECORD-LIFETIME | Conforms within shared author admission: a valid original survives initial lifecycle wait and admission checks repeat before submission. |
| Harness HARNESS-PUMP, held gate completion | Conforms within held-result handling: an already held gate reply is not fresh progress and does not permit another read. |

## Pair execution and retry

The execution context opens only its first unfinished effective pair. The pure
ledger retains its general opening port. PAIR-STATES requires the previous exchange
to answer and its required records to file before the next gate execution. It does
not require speculative overlapping openings.

Each call's opening and send share one measured decision_wait. Successful atomic
send retires that window, so it does not time tool execution. The matching answer
starts the terminal receipt window. Once that terminal commits, a later call starts
a new send window. Progress inside either window retains its absolute deadline.
The existing load or leave deadline still wins when earlier. This introduces no tool
clock, dispatch policy or automatic retry.

Two custody handoffs install their next wait without an intervening lifecycle yield:
the matched gate answer to its terminal wait, and the filed terminal receipt to the
next call's send wait. Their callers check liveness before taking custody. A later
Leave wins at the next boundary, with the owner and its wait intact.

RETRY requires a filed terminal external predecessor with the same run, name and
arguments. It does not require that predecessor's batch to be archived. Lookup
checks completed entries in active and closed batches. An observed answer without
a terminal receipt remains ineligible. The new opening preserves caller lineage,
the sealed effective emission, and the new CallId and exchange.

The regressions drive both calls through the common pump and complete their records
in source order. They hold a terminal receipt, check distinct full send windows, and
sweep Leave boundaries at both custody handoffs. Retry fixtures reject an unfinished
active predecessor and accept it only after its terminal receipt.

## Read admission and refusal legs

The external begin_read port refuses an owned pair, an interrupted input handoff,
waiting content, a completed author, a pending recording barrier, or a pending flow
record. An armed Gate or Cut bound also refuses admission, even when its initiating
owner has no pending channel operation. Busy preserves the selector and turn and
starts no read, state request or deadline. Existing owners and bounds remain intact.
Inward execution still needs an explicit pair-owned read entry; no production inward
caller uses this external port.

Read accepts query refusals without acquiring a cursor. read_busy, unknown_snapshot
and wrong_offset on that initial leg reveal inconsistent channel ownership and are
protocol failures. Next accepts unknown_snapshot, wrong_offset, result_unavailable
and answer_too_large. Query selection errors and result_too_large cannot describe
that acquired immutable result. Release must acknowledge the exact snapshot. A
refused or mismatched release is a component failure.

A valid later refusal retains the cursor. The caller releases it exactly once under
the original read deadline. A failed seam closes instead of guessing cleanup.
Accepted page and release progress is stored before a post-validation clock check,
so lifecycle resumption neither replays an answer nor loses the final release.

The fixtures cover refusal legs, exact ownership barriers, a real interrupted input
handoff, repeated/skipped page offsets and unique ordered binding ordinals. The
input-turn guard is an explicit redundant defense. Healthy author ownership also
retains the pending event tested by barrier.pending; an author-only failure is
rejected by the initial live check. The tests do not invent that state as healthy.

## Measurement, author and pump continuations

Measurement stores each recorded author's successor before its post-work check and
checks liveness before settling the decode reservation. Original read and enclosing
measurement bounds remain authoritative through recoverable lifecycle waits.

Shared author admission retains an unchanged event only after admission checks pass.
An initial lifecycle wait leaves an explicit deferred-admission flag. Resumption
repeats the checks before submission, preserving refused-event custody. The repair
Instruction and all four input author handoffs use this path. Template acceptance
and outbound fault continuations already retain their successor state before yielding.

The pump distinguishes a gate reply already held in its slot from new receive
progress when another descriptor is ready. The real-socket fixture verifies exact
completion custody and the original Leave bound. It does not claim that the former
behavior independently caused a spin.

## Verification and retained followups

The final #215 continuation revision 8acc5f8 passed 1,167 harness tests with 18
inherited ignored instruments. Harness all-target clippy, workspace formatting,
lock and diff checks passed. Cargo used the pinned toolchain, --locked and private
target `/tmp/weaver-pr215-target`. Those results validate that revision, not later
implementation changes. The [final review][continuation-review] grades that exact
head and records a clean Codex pass. Independent Executor readings are not a
substitute for those reviews.

All 13 continuation mutations compiled and failed their intended assertions. Each
restored target passed, with exact byte and SHA-256 restoration: three measurement,
three read, one held-Gate and six shared-author/caller guards. The prior combined
crate/device validation and 22 pair/read mutations are preserved in Git at
[2ef9657][prior-validation], rather than duplicated as current validation here.

The final #215 evidence was recorded under `/tmp/weaver-pr215-review-round2`.
This is a host-local staging location, not a durable bulk-store deposit. Its
`harness-results.json`, mutation logs and source manifest identify the checked
revision. All 475 manifest entries matched after those gates. The manifest SHA-256:

```text
f4985fc07e518612339ce51013ec659d668ca4a36eddb8ce07145ba3d9a41fa6
```

The inherited capture refusal-to-Abort continuation remains tracked in
[#223][capture-followup]. The continuation sweep did not reproduce a runtime failure
there and does not close that issue. Its regression must preserve deadline ownership.

The final Planner grade retains a measurement test obligation: a mutation that adds
a second live check after reservation settlement survives while the pre-settlement
check remains. The code has no yield at that point; the owed test must let the first
check pass and expire Leave at the inserted second check. This is a batched pin gap,
not a newly demonstrated runtime defect.

The [gate review][review] and final Planner grade retain these gate categories:

| Finding | Followup |
| --- | --- |
| send_answer deadline before first try_send | [#222][gate-followup] |
| Deadline/unsendable result becomes Closed with successful exit and no line | [#222][gate-followup] |
| Control crossing a refused Tool opens a second Close | Retained gate review item |
| answered_tools grows with each ordinal/call | Retained gate review item |
| Duplicate late-control rule | Retained gate review item |
| WouldBlock polling uses POLLIN and an unbounded None wait | Retained gate review item |
| Unused Tools contain/wait_empty methods | Retained gate review item |

The final review also retains #216 findings F3, F4, F6, F8 and F9, #217 findings
F4-F8, and the #198 batch items. This record does not close those dispositions.

## Remaining implementation boundary

Inward pair execution and boot repair, complete journal/place ownership, plan and
forward-flag ports, serve-side selection, lifecycle/author migration, failed-run
teardown and the coupled cutover remain owed. The merged recall/boot and observation
Specs require their implementation consumers. OPEN-FACTS still lacks the
identity-prefix open-measurement carrier. The empty-correction record names only
its completed private transition, not these integrations.

Production language artifacts, the separately reviewed system prompt, measured limits
and required host instruments remain separate qualification obligations. Keep
weaver-state, worker/dev_loop orchestration and SPU classification. The old serving
path remains active until qualified cutover. Only the operator merges or authorizes
deployment; #146 remains open.

[map]: state-management-implementation-plan-2026-10-20.md
[completion]: https://github.com/toddwbucy/WeaverAgent/issues/146
[empty-correction]: state-management-empty-correction-2026-10-21.md
[Spec 05]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[Spec 06]: ../crates/state-management/state-management-spec-06-marks-renderer-engine.md
[harness amendment]: ../crates/state-management/weaver-harness-Spec-amendment.md
[state amendment]: ../crates/state-management/weaver-state-Spec-amendment.md
[review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5607348857
[continuation-review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5608104965
[prior-validation]: https://github.com/toddwbucy/WeaverAgent/blob/2ef9657/docs/project/state-management-consolidation-2026-10-21.md
[gate-followup]: https://github.com/toddwbucy/WeaverAgent/issues/222
[capture-followup]: https://github.com/toddwbucy/WeaverAgent/issues/223
