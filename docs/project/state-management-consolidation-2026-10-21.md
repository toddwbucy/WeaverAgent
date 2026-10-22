# State management consolidation review evidence

Date: 2026-10-21.
Branch: code/state-gate-interrupt. Landing PR: #215.

The operator merged #215 at a42d1f02 on 2026-10-21 after its final review.
The validation sections below retain their original tested revisions. The later
Spec merges #189, #192 and #213 authorize the next implementation acts, tracked
in [issue #146](https://github.com/toddwbucy/WeaverAgent/issues/146).

## Integration and authority

The operator authorized integrating main into the #215 root and retargeting
the PR to main. The integration inputs are:

| Parent | Exact revision |
| --- | --- |
| Consolidated #215 | fe4d193b73e52e83491feed43ccdd1c19c08ce96 |
| Main after #184, #185 and #198 | 50a009448ef18a18cb0121966a10323565a63d87 |

#220, #219, #217 and #216 were merged down into #215. The #218 producer/consumer
wire is already incorporated through merge 32a57b57, which retains #217 at bdfa3a76
and #218 at 8b387eff as parents. #218 was closed as already incorporated. The main
integration preserves that ancestry. The [implementation map][map] records each
original slice and its evidence separately from this combined revision.
The review fixes follow the history-preserving main merge in a separate commit.

The merged Specs authorize these corrections. The [#216 review][pair-review] reports
the multi-call stall. The [earlier #215 review][review] requests the inherited #217
tests and Spec readings. Those fixes and their validation reached 2ef9657. The
[next review][continuation-review] adds the continuation and pump corrections below.
This record does not close epic #146.

## Implements and author grades

Implements: [Spec 05] PAIR-STATES and RETRY, [Spec 06] MEASURE-DELTA,
[harness amendment] HARNESS-READ, HARNESS-BARRIERS, HARNESS-AWAIT-AUTHOR,
HARNESS-RECORD-LIFETIME, HARNESS-PUMP and the measurement portion of HARNESS-PLACE,
and [state amendment] STATE-PAGES and STATE-CHANNEL as consumed by the harness.
Grades are bounded to these ports and supported by the current harness gates and
protective mutations. Grades tied to 2ef9657 describe that earlier tested revision.
Fresh Planner and third-party review remain separate.

| Owning section | Grade and implemented correction |
| --- | --- |
| Spec 05 PAIR-STATES and RETRY | Conforms at 2ef9657 within external execution and lineage validation. Serial openings, separate windows and filed active-batch predecessors. |
| Harness HARNESS-READ | Conforms within the tested correction. Retain accepted page/release progress before a recoverable post-validation wait. |
| Harness HARNESS-BARRIERS, read admission | Conforms at 2ef9657 within this entry. Preserve armed Gate/Cut and unrelated owner barriers. |
| State STATE-PAGES and STATE-CHANNEL, harness consumer | Conforms at 2ef9657 within page/refusal handling. Preserve acquired cursors and check exact protocol legs. |
| Spec 06 MEASURE-DELTA and harness HARNESS-PLACE, measurement owner | Conforms within the tested corrections. Save recorded-author successors before yielding and check the clock before consuming a reservation. |
| Harness HARNESS-AWAIT-AUTHOR and HARNESS-RECORD-LIFETIME, shared author admission | Conforms within the tested correction. Retain a valid original through initial lifecycle wait and repeat admission checks before its later submission. |
| Harness HARNESS-PUMP, held gate completion | Conforms within the tested correction. An already held gate reply is not fresh progress and does not permit another read. |

## Current continuation review

The restored corrections after 2ef9657 pass 1,167 harness tests, with 18 inherited
ignored instruments. The harness all-target lint, workspace formatting, lock and
diff checks pass. Cargo used the pinned toolchain, --locked and the private target
/tmp/weaver-pr215-target. Only the harness changed in this correction. The earlier
full suites, GPU run, 22 mutations and manifest below remain historical evidence.

All 13 new protective mutations compiled and failed their intended assertions.
Every restored named test passed, with exact byte and SHA-256 restoration.

| Protective group | Compiled mutations rejected | Restored tests passed |
| --- | ---: | ---: |
| Measurement author and reservation handoffs, final check | 3 | 3 |
| Read accepted continuation, resumption and final bound | 3 | 3 |
| Held Gate receive progress | 1 | 1 |
| Shared author callers, readmission, invalid input and fatal priority | 6 | 6 |

The ten new tests cover intermediate pages and release acknowledgements, all three
measurement author purposes, measured/refused/cancelled result custody, all five
input/repair author callers, admission rechecking and fatal priority. The real-socket
pump fixture checks false progress, exact completion custody and the original Leave
bound. It does not claim the old bug independently caused a spin.

Current evidence is under /tmp/weaver-pr215-review-round2. harness-results.json
records the final gates. mutations/ holds results.json, restorations.json and logs.
pre-format/ retains the initial successful tests/lint and the formatting-check
failure. Formatting was corrected before the mutations and final full gates.
All 475 source hashes match source-sha256.json after the final run. Its SHA-256 is:

```text
f4985fc07e518612339ce51013ec659d668ca4a36eddb8ce07145ba3d9a41fa6
```

The read owner saves accepted progress before the post-validation clock check. A
recoverable lifecycle wait resumes that continuation without replaying the answer,
issuing another request or losing the final release. Measurement saves each recorded
author's successor state before its post-work check, and checks the clock before
settling the decode reservation. Original read and enclosing measurement bounds remain
authoritative.

Shared author admission retains an unchanged event only after its admission checks
pass. An initial lifecycle wait leaves an explicit deferred-admission flag. Resumption
repeats those checks before submitting the event, so refused authors do not become
admitted content. The pump also distinguishes a gate reply already held in its slot
from new receive progress when another descriptor is ready.

| Swept site | Current disposition |
| --- | --- |
| Read accepted answer before post-validation live check | Retained continuation implemented and regression-pinned. |
| Measurement recorded author before post-work live check | Purpose-specific successor stored first and regression-pinned. |
| Measurement reservation settlement before final live check | Final check moved before settlement and regression-pinned. |
| Template acceptance before live check | Safe. Transcript and pending-request state already advance before the check. |
| Outbound fault author before live check | Safe. fault_source already retains the recorded result. |
| Repair's initial author call | Shared admission correction implemented and regression-pinned. |
| Input Offered and Refusal author calls | Same correction covers both callers, with regressions. |
| Input TurnStarted and MessageUser author calls | Same correction covers both handoffs, with regressions. |
| Capture refusal before Abort admission | Unchanged from main 50a00944. Filed separately as [#223][capture-followup], with no runtime reproduction claimed in this sweep. |

The five initial review sites and five additional author callers are accounted above.
The capture-file diff from main changes only entry guards, not the inherited refusal
transition. Its separate regression must preserve the original deadline ownership.

## Pair execution and retry

The context has one pair receipt wait. Previously it could begin a later opening
while the preceding send still owned that wait. The common pump's successful send
then retired the shared bound and left the later opening unable to advance.

The execution context now opens only its first unfinished effective pair. The pure
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
next call's send wait. Their callers check the live boundary before taking custody.
A subsequent Leave still wins at the next boundary, with the owner and its wait
intact. This prevents a resumable lifecycle interruption from losing the next wait
or reviving the preceding terminal window.

RETRY requires a filed terminal external predecessor with the same run, name and
arguments. It does not require the predecessor's batch to be archived. The lookup
therefore checks completed entries in the active batch as well as closed batches.
An observed answer without a terminal receipt is still ineligible. The caller's
lineage is retained in the new pair opening without rewriting the sealed effective
emission or reusing the predecessor's CallId or exchange.

The two-call context regression drives the first send through the common pump,
holds its terminal receipt, and completes both calls in source order. It checks
that execution can outlast retired windows, a new call gets a distinct full send
window, and the record order is opening/terminal for each call. The retry fixture
rejects the unfinished active predecessor, accepts it after the terminal receipt,
and checks the new exchange and original effective metadata.
The lifecycle fixtures sweep each clock sample around the terminal receipt handoff
and force a Leave boundary immediately after a matched answer. Both must resume with
the required wait retained. They are named
terminal_handoff_keeps_the_next_wait_across_each_leave_boundary and
matched_gate_answer_installs_its_terminal_wait_before_leave_can_yield.

## Read admission and refusal legs

The external begin_read port now also refuses an owned pair, an interrupted input
handoff, waiting content, a completed author, a pending recording barrier, or a
pending flow record. An armed Gate or Cut bound also refuses admission, even when
its initiating owner has no pending channel operation. HARNESS-BARRIERS excludes an
unrelated read until that critical work finishes. A Busy refusal preserves the
selector and turn and starts no read, state request or deadline. Existing owners
and their bounds remain intact through the refusal.
No production inward caller uses this port. Inward execution still needs an explicit
pair-owned read entry that carries the owning pair instead of borrowing external
admission.

Read accepts query refusals without acquiring a cursor. read_busy, unknown_snapshot
and wrong_offset on that initial leg reveal inconsistent channel ownership and are
protocol failures. Next accepts unknown_snapshot, wrong_offset, result_unavailable
and answer_too_large. Query selection errors and result_too_large cannot describe
that already acquired immutable result. Release must acknowledge the exact snapshot.
A refused or mismatched release is a component failure.

The later allocation and answer-bound refusals are intentional. STATE-PAGES says a
valid later refusal retains the cursor. The member's page encoding can report
result_unavailable, and STATE-CHANNEL requires answer_too_large for an oversized
complete control answer. Either valid Next refusal requires exactly one release
using the original read deadline. A failed seam closes instead of guessing cleanup.

The added fixtures exercise refusal codes on their proper and improper legs,
preserved pair/author custody, armed Gate/Cut barriers, and a real interrupted input
handoff that resumes after Busy. Page continuity cases use otherwise valid repeated
and skipped offsets. Filtered Span cases check binding ordinal uniqueness and order.
The selected-source classifier interruption fixture isolates the pending flow record
while the author barrier is empty. The input-turn guard shares other reachable
ownership guards and remains an explicit redundant defense. Mutation checks must
distinguish that redundancy from the seven independently exercised Busy protections.

No separate pending_author guard is needed. Healthy author ownership also retains
the pending event tested by barrier.pending. An author-only state has latched
RecordFailure, which begin_read's initial live check rejects. The existing
failure_before_admission_keeps_original_but_no_invented_sequence fixture covers
that failure state without inventing an admitted sequence.

## Gate findings and followups

The [posted review][review] assigned the earlier dispositions below. Gate findings
1 and 2 are tracked in [#222][gate-followup]. The dated banner is corrected in this
round. The remaining batch/file items are not reported as resolved.

| Review item | Existing disposition |
| --- | --- |
| 1. send_answer deadline before first try_send | File, #222 |
| 2. Deadline or unsendable result becomes Closed with successful exit and no line | Batch, #222 |
| 3. Control crossing a refused Tool opens a second Close | File |
| 4. answered_tools grows with each ordinal/call | Batch |
| 5. Duplicate late-control rule | Batch |
| 6. WouldBlock polling uses POLLIN and an unbounded None wait | Batch |
| 7. Unused Tools contain/wait_empty methods | Batch |
| 8. Dated banner in the gate interrupt evidence record | Corrected in this round. Date retained, stale status removed. |

## Historical validation at 2ef9657

These restored suites passed for 2ef9657. They do not validate the current corrections:

| Suite | Passed | Ignored |
| --- | ---: | ---: |
| Shared types, default | 127 | 0 |
| Shared types, config | 182 | 0 |
| Trace | 138 | 0 |
| State | 193 | 8 |
| Admin | 498 | 37 |
| Gate | 75 | 56 |
| SPU, cuda and gguf | 299 reported | 0 |
| Harness, all targets and doctests | 1,157 | 18 |

All corresponding all-target clippy gates passed, including both shared-types
feature modes and SPU with cuda,gguf. Final workspace formatting, lock and diff
checks passed. The private Cargo target is
/tmp/weaver-pr215-target. The SPU suite exercised 298 tests. Its optional
the_pair_reports_its_pace test returned a runtime SKIP because WEAVER_MEASURE_PACE
was unset, which Cargo counted as an additional pass. Real GGUF, native safetensors,
large sharded and two-card/split fixtures ran. This is no measured-pace or production
qualification claim.

The delegated gate lane passed 31 tests: 28 library tests and three served-binary
tests. Three internal helper result lines do not add tests to that total. The lane
used /tmp/weaver-pr215-busybox/busybox, built from the pinned source by
deploy/build-busybox.sh. No artifact was deployed.

All 22 protective mutations compiled and failed their intended assertions. Every
restored named test passed. Each runner verified byte-for-byte restoration and equal
original/restored SHA-256 values before continuing.

| Protective group | Compiled mutations rejected | Restored named tests passed |
| --- | ---: | ---: |
| Pair ordering, new windows, three lifecycle-yield sites, receipt, attribution and retry | 9 | 9 |
| Seven Busy guards, refusal legs, two valid later refusals, page offsets and binding ordinals | 13 | 13 |

The first harness --lib preflight reported 1,102 passed, one failed and 18 ignored.
The new fake-pump fixture lacked writable readiness. It now supplies POLLOUT using
the actual descriptor, and its focused restored test passed. The mutation against
serialized opening still reproduces the original stall. No production code changed
after that preflight. The final full restored harness suite and its all-target lint
passed. The failed preflight is not counted as a final passing run.

Historical evidence is under /tmp/weaver-pr215-review. The foundation-results.json,
spu-results.json and gate-delegated-results.json files record their completed gates.
mutations-pair/ and mutations-read/ each retain results.json, restorations.json and
individual logs. Final harness results are in harness-results.json, with
harness-test.log, harness-clippy.log, fmt.log and diff.log. busybox-build.log records
the pinned fixture build. These are local evidence, not production qualification.

All 475 source hashes matched /tmp/weaver-pr215-review/source-sha256.json after that
revision's final gates. This is the old manifest, not the current source snapshot.
Its SHA-256 is:

```text
28ed4f7ade98e91fcac07b6c1bf1318fb7c93b8f4bdf1457109737b191bfc964
```

Earlier slice counts remain historical evidence in the map. Independent source
readings do not replace the Planner's grade or third-party review.

## Remaining scope

The operator authorized consolidation onto main. Runtime activation and deployment
remain held until the coupled foundation cutover, and the old serving path stays
active. Inward pair execution and boot repair, complete journal/place ownership,
plan and forward-flag ports, serve-side selection, lifecycle/author migration,
failed-run teardown and the complete cutover remain owed. Specs #189, #192 and
#213 are now merged authority. Their recall/boot, sampling/version and empty-correction
implementations remain owed. OPEN-FACTS still lacks the identity-prefix
open-measurement carrier. Production language artifacts, the separately reviewed
system prompt, measured limits and the required host instruments remain separate
qualification obligations. Keep weaver-state, worker/dev_loop orchestration and
SPU classification. Only the operator merges.

[map]: state-management-implementation-plan-2026-10-20.md
[Spec 05]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[Spec 06]: ../crates/state-management/state-management-spec-06-marks-renderer-engine.md
[harness amendment]: ../crates/state-management/weaver-harness-Spec-amendment.md
[state amendment]: ../crates/state-management/weaver-state-Spec-amendment.md
[pair-review]: https://github.com/toddwbucy/WeaverAgent/pull/216#pullrequestreview-5604724227
[review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5607348857
[continuation-review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5607907692
[gate-followup]: https://github.com/toddwbucy/WeaverAgent/issues/222
[capture-followup]: https://github.com/toddwbucy/WeaverAgent/issues/223
