# Worker terminal and refusal composition

Date filed: 2026-10-24

Issue: #146. Branch: `code/state-worker-terminal-composition`, based on
`code/state-worker-session` at
`a863c36c33fa102af7b8fcb1efcc33e75dc1a1d2` (#328). The single stack remains
rooted at #312. Existing serving remains selected.

The dormant worker now closes a completed task/read or decoder refusal through
its original world exchange. A natural model Stop can finish a checked answer.
An unsuccessful loop election reports ElectionRefused and retains its owner
while continuing to service management. Reusing a TurnKey is rejected before
consuming the next accepted input, allowing the caller to supply a fresh key.

## Authority and author assessment

Implementation authority is merged main `f8ca92ef`. The inherited #325 proposal
is unelected and unimplemented. No wire, trace field, limit or new lifecycle
outcome is introduced by this act.

| Implements | Author assessment and evidence |
| --- | --- |
| Harness amendment section 9 HARNESS-OPERATION-CLOSE | Conforms in the exercised task/read and decoder cases: the complete account precedes a Recorded stopped close with reason Refused and absent cause; the exact original world exchange must finish sending. The worker elects this response, not every primitive. |
| Harness amendment section 6 HARNESS-READ-CALLERS | Conforms for the exercised task read: exact InvalidSelector code/detail remain visible, release and operation measurement precede turn.closed, and no recalled-content event is invented for a refused read. |
| basic-inference-loop sections 4 and 6; harness-gate section 2 | Conforms for exercised successful, refused and cancelled world replies: checked Answer spans, natural model Stop and full tracked delivery. Round-limit answer policy remains an open inherited question. |
| Spec 02 INPUT/REUSE and Spec 06 BOUNDARIES | Conforms at turn admission: an already used key cannot consume the accepted input; a fresh key can use that same owner and continue the existing task. |
| Harness Stop/Observe and HARNESS-PUMP/HARNESS-CLOCK | Conforms for exercised pressure, deadline and failed-election joins: management remains serviceable, Stop settles its operation before close, and the close retains the original exchange and an existing decision cap. Accepted-input Stop policy and complete post-Stop account handoff remain open. |
| Spec 09 qualification and coupled cutover | Incomplete. This is dormant composition with scripted member/model peers, not production qualification or activation. |

## Ownership and caller audit

`WorldClose` shares the previous answer and Stop author/send ownership. Its
private constructors derive the turn and exchange from the actual input owner.
It retains returned authors, requires Recorded before staging a world close, and
requires a full send with source-port, exchange and send-clock evidence before
reporting completion. A newly Recorded close returns control before blocking so
management can queue the Stop answer even when the world socket is full.

Stop captures an already active Decision bound before cancellation can retire it.
It does not install a Decision wait over a live read's StateControl wait. The
existing operation settles first; closing restores a captured cap if that wait
has been retired, or uses the existing close allowance when no cap was captured.
The inherited-deadline test expires the original cap after retirement and requires
failure without a Recorded close or staged world send.

The loop calls close_operation_refusal only at OperationRefused. That port first
preserves the existing correction refusal path, then admits an actual retained
TaskCompletion::Refused or GenerationCompletion::Refused. A generic failed
emission/election is not a fabricated completed refusal. It stays ElectionRefused,
can answer Observe and accept Stop, and cannot next_input until its real owner
finishes. The new session case refuses an uncertified repair family, observes
Active and then completes Stop through the same owner.

Natural model Stop is distinct from operator cancellation. Checked output with
Finish::Stopped may supply the selected answer and a clean turn close, while the
model.output record preserves Stopped. Management Stop still cancels/account-settles
the operation and closes with the actual directive cause.

| Changed surface | Every affected caller |
| --- | --- |
| WorldClose | close_answer and close_operation_refusal elect outcomes; EngineStage::Answer advances the shared owner; StopWork cancels/settles before advancing it; management reads its Recorded/send state. |
| ElectionRefused | Workflow progress/advance retains and services input or management; Session maps and advances it; next_input refuses it; the session fixture explicitly exercises recovery through Observe/Stop. |
| accepts_turn | start_task, Workflow::begin and Session::start_turn share the admission check. The private context retains its independent identity enforcement. |
| Failure detail | Workflow and Session only use operation_detail as fallback for refused/failed owners. Completed success has no stale refusal detail; the independent loop election detail remains retained. |
| Session advance | Input, Management and Workflow are advanced in place. Only checked ownership moves take state; this is not a complete unwind-safety or allocation-failure qualification. |

## Verification

Lock, formatting, strict harness clippy and diff checks passed. The full suite
passed **2,033 tests**, including 22 doctests. Its **299 ignored** comprise
281 separately exercised delegated cases and 18 inherited ignores. All **281
delegated composition cases passed serially**, including 24 inward, 21 final,
16 session and 17 Stop cases. The admin test executable was built in the same
target for the actual Observe/Stop process cases.

After the full gates, only two test files and a next_input doc comment changed:
the Stop deadline instrument refreshes the real management liveness boundary and
asserts DecisionBound; the session instrument rejects an unexpected ElectionRefused
immediately and times its management assertion before the child watchdog. Runtime
code is unchanged. Strict clippy and the 33 session/Stop cases passed again; after
the final targeted mutations, all **54 final/session/Stop cases** and lock/fmt
passed on the restored source. The source-difference record preserves this scope.
All **676 source/configuration hashes** were restored and verified.
Final source manifest SHA256: `c6477ab0dbf46d6e65cc7ed68f2283d16d71d69d6df32425c90c3cb8c6b837bb`.

Six new delegated cases cover natural model Stop in final/session composition,
task/read refusal followed by a valid turn, decoder refusal followed by a valid
turn, failed repair election followed by Observe/Stop, and an inherited Stop
deadline. Existing duplicate-key, pressure and detail assertions are strengthened.
The Stop gate-pressure test rejects any blocking wait before its admin answer is
queued. All fixtures retain the existing exact input, original CallIds and
StatementRefs, member/decode receipts, and trace-before-use checks.

Eight compiled protective mutations were detected across ten targeted tests:
reject natural model Stop; withhold completed refusal close (both read and decode);
bypass Recorded authoring; treat queued world sends as complete (answer and Stop);
forget the inherited Stop deadline; allow reused turn identity; stop observing
management after a refused election; and block on the world gate before queuing
the Stop answer. Failure excerpts identify the intended assertion or behavior.

T5 initially survived because the instrument retained a stale context clock after
retiring the old wait. The corrected instrument uses the management path's live
observation; removing the captured bound then wrongly succeeds and fails the test.
T2/T7 initially reached the fixture watchdog; stronger assertions now fail at the
missing refused close or Observe answer. Those first outcomes remain retained;
the final T2/T5/T7 rerun is separate evidence, not a relabeling of the first run.

Initial evidence is retained: fixture mode wiring and count expectations were
corrected without changing validators. The first full gate run exposed the new
Stop helper arming Decision during an outstanding recall read; the pair owner
correctly refused the foreign wait. Capturing without arming fixes that regression.
A first local delegated attempt was prevented by sandbox sudo restrictions; the
isolated delegated rerun used the established host test lane. No failed attempt is
counted as a passing gate or mutation detection.

Evidence is under `/tmp/state-146-worker-terminal/`; the PR body names the exact
published tree and verified immutable deposit. The pinned toolchain, private
`/tmp/weaver-state-146-worker-session-target`, --locked and serial Cargo/Git
mutations were used. The required code-review and verification-before-completion
skills remain unavailable in this environment as previously disclosed. These are
author checks, not skill invocations, an independent Planner grade or security
qualification.

## Planner rows and remaining work

Parent #328 was cleared at a863c36c by review 5638816596 with 0 fix/10 batch;
the Planner marked it ready. Its automatic Codex code review has no findings.
This child addresses selected batch work without changing that reviewed head.

| Review rows | Current disposition |
| --- | --- |
| #327 review 5638580573 rows 2, 4, 5, 8 | Natural model Stop, distinct failed-election progress and shared answer/Stop close custody are implemented and exercised here. The close owner pumps its owed gate send, while management queues its answer before the first blocking wait. This does not establish absence of every local-transition CPU spin. |
| #328 review 5638632959 row 1 | Task/read and decoder refusals now finish their world reply; failed repair election continues management. Generic emission refusals, owed returned-owner completion and full lifecycle disposition remain open. |
| #328 rows 2, 3, 6 | Used-key preflight and recovery, in-place advance and conditional detail fallback are implemented. The latter two have the source/case scope above, not a blanket panic-safety claim. |
| #328 rows 4, 5 | Still activation blockers: accepted-input Stop needs the existing operator ruling, and finish/next_input can drop an owed cancelled-generation account. #316 remains open. |
| #327 rows 3, 6, 7 | Still open: round-limit answer selection policy, Observe while a world close is owed, and selected_answer losing RepairError through .ok()?. |
| #328 rows 7, 9 | Shared progress helpers, unreachable-arm cleanup and independent enforcement claims for redundant guards remain open. Centralizing accepts_turn does not retroactively pass historical S1/S2/S3 controls. |
| Shared fixture and skill rows | #313's correction is inherited from #327/#328 and awaits stack merge. Required unavailable self-check skills remain disclosed. Historical F3 send-clock guard survival is not relabeled as a detected control. |

#325 still needs an operator election and merged authority. #326's source
counterevidence remains under review; no account fix is claimed here. #323/#324
queued-world-input/tool-answer routing, loaded schema certification/clock,
selection/cuts, plan/forward flags, Save/restore, full Leave/failed-run teardown,
held-input/author migration, production language/model/store artifacts, system
prompt, limits, security disposition and coupled cutover remain owed. No checklist
item closes. Only the operator merges/deploys and controls CodeRabbit.
