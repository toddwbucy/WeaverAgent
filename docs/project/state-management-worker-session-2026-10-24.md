# Worker session composition

Date filed: 2026-10-24

Issue: #146. Branch: `code/state-worker-session`, based on
`code/state-worker-final` at
`b0c639987dab901a987267c830c109c99deee5e4` (#327), joined with both
parents retained above the original session head `97564492`.

The worker now retains one entered serving seat across explicitly elected turns.
Each turn uses its own accepted input and caller-supplied identity. Successful
world delivery, a completed input refusal or a completed Stop permits the caller
to request the next input. An unfinished operation refusal, unsupported directive
or failed run can return its current owner through into_owner. The Planner found
unfinished-refusal, Stop and post-Stop account gaps that block activation; the
current implementation is not a complete session lifecycle. Existing serving
remains selected.

## Authority and author assessment

Implementation authority is merged main `f8ca92ef`. The inherited #325 proposal
is not authority, and its raw-return string carrier remains unimplemented.

| Implements | Author assessment and evidence |
| --- | --- |
| Spec 02 INPUT and REUSE | Conforms in the session cases: separate checked intake and original source for every input, including equal text; queued input waits for explicit handoff after the prior reply. Acceptance alone starts no task. |
| Spec 06 BOUNDARIES, MARK-FILE and MARK-READ | Conforms in the exercised composition: the actual begin_task port files each input, reads and releases the marks catalog, and reuses its open task. Two and three turns produce one begin_task mark and no invented task end. |
| Spec 06 COVERAGE-JOURNAL and ordinary PLACE-PORT | Conforms for successive no-cut appends: each model request carries only the new input; measurement retains the exact prior resident argument. Scripted model/member facts exercise the join, not production tokenization or SQLite. |
| basic-inference-loop sections 4 and 6; harness-gate section 2 | Conforms for explicit next-input handoff. The actual Completed/Refused/Aborted input owner must return its serving seat. A refused workflow election cannot bypass the underlying unfinished owner. |
| Harness Stop/Observe and HARNESS-PUMP | Conforms for the exercised idle and active management joins. Stop after acceptance needs the outstanding operator ruling; unfinished-refusal management and post-Stop account handoff remain activation blockers. Unsupported Leave and decoder failure return their current owner without claiming teardown. |
| Spec 09 qualification and coupled cutover | Incomplete. Artifact, language/model/store, resource, security and complete lifecycle qualification remain open. |

## Ownership

`worker/dev_loop/foundation_session.rs` owns either an input, its management
connection or the existing bounded one-turn workflow. `start_turn` requires checked
acceptance and a nonempty caller identity. The task primitive decides from the
actual marks read whether an open task exists; the session adds no mark allocator,
plan election, task-close policy, state read or scheduler.

`next_input` is an explicit caller election. It moves the serving seat only when
the underlying owner actually finishes. A workflow-level Refused status alone is
insufficient: a duplicate turn election retains the second accepted input, original
gate frame and operation account. The caller can transfer that complete owner to
the still-owed refusal/teardown composition. This is the exercised unfinished
election boundary, not proof that every completed Stop has discharged its account.
Planner row 5 identifies that finish can drop a retained cancelled-generation
account during handoff; that route needs an owed-account check or transfer.

Observe or Stop between turns, and before an accepted input opens its turn, uses
the existing management owner and returns the same input. During a running turn,
the existing workflow retains all repair, pair and round decisions. After a
completed Stop, the instrument starts another turn with the still-open task and
confirmed argument. It does not establish safe handoff of every owed Stop account.
The session does not reset the journal or open another state/decode session.

The one-turn workflow now reports its retained input/management failure detail
when it has no independent election refusal. This changes diagnostic visibility,
not the first failure or its ownership. All consumers of the worker-local detail
accessor are updated together.

## Verification

Twelve process cases use actual worker startup, gate, classifier, decoder,
recorder and lifecycle sockets. Model and store responses are scripted. The
fixture derives each later marks catalog from its actual earlier filed begin_task
effect and samples its operation timestamp after receiving the request. This does
not resolve the four separately filed #313 fixtures.

The cases cover two and three turns, malformed input followed by a valid turn,
an already queued next frame, Observe/Stop at idle, Observe/Stop after acceptance,
Stop during generation followed by a new turn, unsupported Leave, a duplicate
turn election, and decoder failure. They require distinct original source and turn
identities, one task mark, one member Open, unchanged recording-context identity,
the exact successive resident prefixes and full original-owner return after an
unfinished refusal or failure. The fixture's emitted world closes retain the
actual gate exchange and turn.

Lock, formatting and strict harness lint passed. The harness suite passed 2,030
tests, including 22 doctests. Its 293 ignored cases comprise 275 delegated process
cases passed separately and serially, including the 12 session cases, and 18
inherited ignores. The eight privileged admin startup instruments were not run.
Private target:
`/tmp/weaver-state-146-worker-session-target`; pinned toolchain;
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4`, and `--locked` on resolution.

At the original session head 97564492, six compiled protective mutations were
detected: replacing the original turn
identity, starting a task from acceptance alone, advancing intake automatically,
opening another task over the existing scope, losing the accepted input through
management, and hiding the retained decoder failure. Each failed at the intended
owner or trace assertion. All 675 crate/configuration files were restored by hash,
then all 12 session cases and the lock/fmt/diff checks passed again. Source manifest
SHA256 at that head:
`3b263c033eae72cdc1050724641a68762417c4c96f2081df28ba250163590fa6`.
These are historical controls, not a new mutation round for the parent join.

Logs, commands, source snapshots, mutation patches and restoration results are
under `/tmp/state-146-worker-session/`. The immutable bulk evidence leaf and exact
published revision are recorded in the draft PR after deposit verification.
The current parent join passed lock, formatting, strict harness lint, 2,030
harness tests and all 275 serial delegated cases (including 20 final and 12
session cases). Its 675-file source manifest was verified after the gates; SHA256:
`946a8229abade0708f7a90440dd77ec2678a070a70fa9ea56259497a21769bd9`.
The parent-join gates and current 675-file manifest are under
`/tmp/state-146-worker-session-parent-join/`. The final and session responders now
both use the parent's explicit step_live path; the mode-specific clock override
is removed. Other shared-fixture callers still use their earlier clock entry,
including simulated-clock callers, so this join does not close #313.

Initial evidence retains one compile failure from an unavailable direct serde
trait import (replaced by a local serialization macro), the first scripted
generation count mismatch, and the terminated diagnostic batches. The decoder
fixture initially reported two output tokens while its resident total allowed one;
the harness correctly refused it. The production count validator was unchanged.

The required code-review and verification-before-completion skills remain
unavailable in this environment, as disclosed in the parent acts. These checks
are author evidence, not those skill invocations or an independent Planner grade.

## Planner review dispositions

Planner review 5638632959 at 97564492 found 0 own fixes and 10 batch rows,
leaving the PR draft on #327's inherited clock-race gate. This join carries
#327's b0c63998 correction with its original ancestry and reruns the joined gates.
The parent's five idle and five loaded final-family passes and its surviving F3
control are parent evidence, not a new repetition round on this joined head.

| Row | Disposition |
| --- | --- |
| 1 | Activation blocker: an unfinished operation refusal leaves an open world exchange and progress never services later management. into_owner can recover current custody but is not refusal/lifecycle completion. |
| 2 | Open: reject a previously used TurnKey before consuming accepted input. The current duplicate-key instrument demonstrates the unfinished refusal, not recovery to Accepted. |
| 3 | Open: advance Input and Turn in place where possible; taking state around a panicking call can leave None and make subsequent owner extraction panic. No unwind-safety claim is made. |
| 4 | Activation blocker and operator election: Stop after acceptance currently answers AtRest and returns that same Accepted input. The test records current behavior; it does not settle the pending #255 intake/AtRest ruling. |
| 5 | Activation blocker: FoundationInput::finish can drop a retained engine/account on post-Stop handoff. Transfer any owed account to lifecycle or refuse that handoff. #316's complete failed/Stop exit remains owed. |
| 6 | Open: operation_detail can appear even on Completed. Restrict the fallback to refused/failed owners and assert exact failure text. |
| 7 | Open: share progress/management owner helpers and remove unreachable arms. |
| 8 | Partly addressed by the inherited explicit step_live path for final/session responders. Migrating every shared live responder remains #313 work; simulated-clock tests need their explicit supplied readings. |
| 9 | Open: S1/S2's redundant guards remain, enforced again by Workflow::begin/start_task and finish. S3 is unverified because its startup hit #313. No independent enforcement claim is made for those guard lines. Comments or removal remain owed. |
| 10 | Required self-check skills remain unavailable and are not claimed as invoked. |

The joined head awaits Planner regrade. The no-production-caller condition is
material to the dormant batch classification and remains unchanged.

## Remaining work

This composes repeated no-cut turns. Between-turn selection/cuts, plan and forward
flag ports, Save/restore, complete Leave/failed-run disposition, generic held-input
taking, author migration and serving activation remain open. In particular,
recovering custody with `into_owner` is not completed cleanup. #316 remains open.
The queued frame test has no external request in flight and does not fix #324's
different queued-world-input/tool-answer slot obstruction.

The read-only audit of #326 found that generation drains authors and its decode
reservation before OUTPUT, and its existing Decision guard prevents the reported
early retirement while the generation remains owned. Exact-source counterevidence
is posted at issue #326 comment 6385134403. That issue remains open for review;
this act neither claims a reproduction nor changes the frozen account deadline.

#325 still requires an operator election, review and merge before implementation.
Loaded tool-schema clock/certification and the existing review batches remain
tracked. No issue checklist closes, no security pass is claimed, no CodeRabbit
review is requested, and the operator alone merges or deploys.
