# State management consolidation review evidence

Date: 2026-10-21.
Branch: code/state-gate-interrupt. Landing PR: #215.

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
the multi-call stall. The [#215 review][review] also requests the inherited #217
tests and Spec readings. This record answers those findings without reopening the
earlier gate batch/file dispositions. It does not close epic #146.

## Implements and author grades

Implements: [Spec 05] PAIR-STATES and RETRY, [harness amendment] HARNESS-READ and
HARNESS-BARRIERS, and [state amendment] STATE-PAGES and STATE-CHANNEL as consumed
by the harness.
These are bounded author grades for the corrected paths, supported by the final
restored-source suites and protective mutations below. They do not grade the complete
serving implementation. The Planner's grade and third-party review remain owed.

| Owning section | Grade and implemented correction |
| --- | --- |
| Spec 05 PAIR-STATES | Conforms in the external execution owner. Serialize effective pair openings around the preceding terminal commit and give each blocking send/terminal barrier its own absolute window. |
| Spec 05 RETRY | Conforms in lineage validation. Accept an exact same-run external predecessor whose terminal is filed, including one retained in the active batch. |
| Harness HARNESS-READ | Conforms in paged acquisition. Exclude unrelated pair, input and author owners and distinguish each read protocol leg before releasing or exposing a result. |
| Harness HARNESS-BARRIERS, read admission | Conforms at this entry. Refuse an external read while Gate or Cut is armed, preserving the existing critical barrier. |
| State STATE-PAGES and STATE-CHANNEL, harness consumer | Conforms in page/refusal handling. Preserve the acquired cursor through a valid later page refusal and reject refusal codes from another protocol leg. |

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

## Retained gate findings

The [posted review][review] keeps these earlier gate findings outside this fix act.
They retain their existing dispositions and are not reported as resolved here.

| Review item | Existing disposition |
| --- | --- |
| 1. send_answer deadline before first try_send | File |
| 2. Deadline or unsendable result becomes Closed with successful exit and no line | Batch |
| 3. Control crossing a refused Tool opens a second Close | File |
| 4. answered_tools grows with each ordinal/call | Batch |
| 5. Duplicate late-control rule | Batch |
| 6. WouldBlock polling uses POLLIN and an unbounded None wait | Batch |
| 7. Unused Tools contain/wait_empty methods | Batch |
| 8. Dated banner in the gate interrupt evidence record | Batch |

## Validation and remaining scope

The restored suites passed on the combined checkout:

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

Local evidence is under /tmp/weaver-pr215-review. The foundation-results.json,
spu-results.json and gate-delegated-results.json files record their completed gates.
mutations-pair/ and mutations-read/ each retain results.json, restorations.json and
individual logs. Final harness results are in harness-results.json, with
harness-test.log, harness-clippy.log, fmt.log and diff.log. busybox-build.log records
the pinned fixture build. These are local evidence, not production qualification.

All 475 source hashes match /tmp/weaver-pr215-review/source-sha256.json after the
final gates. The manifest's SHA-256 is:

```text
28ed4f7ade98e91fcac07b6c1bf1318fb7c93b8f4bdf1457109737b191bfc964
```

Earlier slice counts remain historical evidence in the map. Independent source
readings do not replace the Planner's grade or third-party review.

The operator authorized consolidation onto main. Runtime activation and deployment
remain held until the coupled foundation cutover, and the old serving path stays
active. Inward pair execution and boot repair, complete journal/place ownership,
plan and forward-flag ports, serve-side selection, lifecycle/author migration,
failed-run teardown and the complete cutover remain owed. Draft Specs #189, #192 and
#213 are not implementation authority. OPEN-FACTS still lacks the identity-prefix
open-measurement carrier. Production language artifacts, the separately reviewed
system prompt, measured limits and the required host instruments remain separate
qualification obligations. Keep weaver-state, worker/dev_loop orchestration and
SPU classification. Only the operator merges.

[map]: state-management-implementation-plan-2026-10-20.md
[Spec 05]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[harness amendment]: ../crates/state-management/weaver-harness-Spec-amendment.md
[state amendment]: ../crates/state-management/weaver-state-Spec-amendment.md
[pair-review]: https://github.com/toddwbucy/WeaverAgent/pull/216#pullrequestreview-5604724227
[review]: https://github.com/toddwbucy/WeaverAgent/pull/215#pullrequestreview-5607348857
