# Bounded worker turn and final answer composition

Date filed: 2026-10-24

Issue: #146. Branch: `code/state-worker-final`, based on
`docs/state-rejected-return-wire` at
`49c1f07d7c02e52f4334e21e202270c2dcf5ba42` (#325).

This act composes one accepted input through task placement, generation, repair,
recorded outbound selection, inward/external requests, result placement and a
Recorded clean turn close followed by its exact world reply. The developer loop
owns the choices and the 32 execution-round limit. The active serving dispatcher
still selects the existing implementation.

## Authority and author assessment

Implementation authority is merged main `f8ca92ef`, including the merged state
management Specs. The inherited #325 raw-rejection proposal is not implementation
authority: this act neither implements its string carrier nor elects that proposal.

| Implements | Author assessment |
| --- | --- |
| Spec 02 EFFECTIVE-EMISSION and REPAIR-STATES | Conforms for the joined healthy and correction paths. Answer text uses selected effective statement spans, joined by LF, including corrected source identities. The opaque decoder emission is never answer material. Existing failed correction accounting is retained. |
| Spec 05 PAIR-STATES, RETURN-CARRY and PAIR-LEAVE's round limit | Conforms in the exercised paths. Every original conforming call opens and receives a terminal filing receipt. A nonempty execution batch counts once, including two calls. After 32 batches the next generated pass can answer, but all its calls receive not_dispatched/round_limit before close. |
| Spec 08 OUTBOUND | Conforms. Sort validates and records the exact acts. The developer loop selects all conforming requests and effective Text blocks in logical order; sort chooses no dispatch, retry, answer or round policy. |
| basic-inference-loop sections 4 and 6; harness-gate contract section 2 | Conforms for this one-turn composition. A clean close is Recorded before the original gate exchange carries the answer. The harness exposes explicit transitions and retains custody; worker/dev_loop chooses the workflow. |
| Harness Stop/Observe and announce-after-record | Conforms at the exercised close boundary. Stop before close authoring cancels it; a close already being authored/Recorded settles as the single close, and subsequent Stop answers AtRest. Observe continues to use recorded turn state. |
| Spec 06 task spans | No task-end election is added. The turn close leaves the original task mark standing. Multi-turn task continuation and plan effects remain separate work. |
| Spec 09 qualification and coupled cutover | Incomplete. Scripted process instruments do not qualify language artifacts, system prompt, model, SQLite, resource limits, production cost, security or full lifecycle. |

## Ownership and workflow

`worker/dev_loop/foundation_turn.rs` holds the accepted input or its management
owner, the restatement family, cut method, round count and next result/answer
choice. Observe and Stop retain those decisions across management suspension.
Unsupported Leave/Save/directive composition returns the whole workflow still
owning its actual management connection. It does not claim completed teardown.

The loop files a task, places its current source, generates, repairs, records its
identity sort, executes every selected request in source order, places complete
results and repeats. Corrections use the existing separate retry cap. Empty
checked output can finish with an empty answer. Length completion retains the
existing world `finish: "length"` marker. A nonempty request batch advances the
execution-round count only after pair admission; the final barred batch does not
advance it or generate again. At most 32 previous rounds remain in this workflow's
retained chain, plus the final emission and its own bounded correction accounts.
The lower-level explicit ports still require a caller that follows workflow limits.

`bar_round_limit` accepts only a recorded sort and returns no request ready for
dispatch. It retains the EffectiveEmission and all original CallIds while the
existing pair owner records and files every terminal. `close_answer` accepts only
the completed pair batch and its recorded selection. No caller-supplied text,
receipt, corrected source or close outcome can replace these private owners.

The final owner retains the selected checked text, finish, original turn/exchange,
returned author failure, Recorded sequence, exact frame and tracked send. It uses
one existing loaded Decision wait through authoring and complete delivery; it
neither refreshes that wait on backpressure nor grants completion on a queued
send. A successful send without its clock observation is a TraceFailed author
failure, preserving the sent receipt without granting completion. Trace or socket
failure retains custody and the first failure for the unfinished teardown owner.

A Stop arriving before close authoring owns the stopped close. Once clean close
authoring has begun, management finishes that exact transaction and pending world
send before judging Stop at rest. There is one immutable close, not a second
stopped record. This follows the same owner rule as the existing refusal-close
join. No new event, timer, schema, queue or serialized wire variant is introduced.

The complete-loop Stop instrument also exposed a pre-existing composition stall:
management made an outer blocking poll after a local pair transition, before any
world close existed. With the real pump, nothing could wake that transition and
Decision expired. Management now makes that outer wait only for the Recorded
close's owed gate send; engine/pair/repair owners service their own waits first.
The simulated-clock parent cases did not expose this stall.

## Verification

The instruments exercise actual worker startup, classifier, gate, decoder,
recorder and management sockets with scripted model and state-member peers. The
external fixture runs no shell command. The authoritative artifact inventory and
real production decoder remain outside this evidence.

The 20 new process cases cover all/subset answer selection, UTF-8 text, multiple
bindings in one block, empty output, length completion, corrected text, a two-call
inward round, a two-call external round, 32 two-call rounds plus two barred calls,
held terminal filing, trace and gate backpressure, first trace failure, final
Decision expiry, Stop before and after close, and a clock failure strictly after
an actual whole send. Three complete-loop cases also resume after Observe,
settle Stop during a two-call round, and retain an unsupported Leave with its
actual management owner. The long case requires 64 reads and result filings, 66
terminal pairs, exactly 33 ordinary generations, and no 34th generation. A turn
close must not add a task-end mark.

The final source passed lock, formatting, strict harness lint and **2,030 harness
unit/integration/doctests (22 doctests included)**. The ordinary suite reports 281
ignored cases: **263 delegated process cases passed serially**, and 18 inherited
ignored cases remain. The delegated groups are gate 46, startup 17, input 19,
Observe 14, Stop 16, generation 21, emission 33, correction 19, inward 24, return
13, external 21 and final workflow 20. The admin fixture binary was rebuilt for
those joins; the eight privileged admin startup instruments were not run.

The corrected final family passed **five idle runs and five loaded runs**, 20
cases per run, **200/200 cases**. Load was four low-priority SHA256 workers;
each consumed 42.78-42.79 CPU seconds during the five loaded families. This is
measured fixture repetition, not production capacity qualification.

The review controls produced two distinct results. Reintroducing a clock sampled
before request receipt, with a 2 ms delay to expose that ordering, reproduced the
exact store-measurement clock refusal in the round-limit case. Planner F3,
removing only AnswerWork's sent_at guard, **survived all 20 final cases**. That
individual guard is not independently pinned. Source reading explains the extra
defense: a failed send-clock observation exits pump_once with an error; the input
or management owner latches TraceFailed and Stopped before AnswerWork can advance
again. No malformed owner state was injected to manufacture a mutation failure.
The original ten author mutations remain evidence at bb02de61, not rerun claims
for this correction; the review discovered that old fixture's unreliable clock.

All **673 source/configuration hashes were restored and verified**, followed by
another **20/20 restored final cases** and lock/fmt/diff checks. Current manifest
SHA256: `249aa8a61d60ae5f5953698f731f497611e098c51f8367cfb9fc93a572b01313`.
Current local evidence is `/tmp/state-146-worker-final-clock-review/`, including
gates, repetitions, load accounting, controls and restoration. The initial sandbox
attempt could not start sudo and is retained separately, not counted as a test.
Original author evidence remains `/tmp/state-146-worker-final/`. The private
target is `/var/tmp/weaver-state-146-worker-final-target`; the PR body records the
new immutable deposit and exact committed source.

The first author runs retained two fixture failures: the new recall modes were
not admitted by the scripted member, and final Decision expiry was incorrectly
asserted as a lifecycle Wait rather than DecisionBound. Build/lint attempts also
recorded an exhaustive-match update, a private-helper visibility correction and
a lazy bool closure lint. They are not passed instruments or perturbations.

## Planner review dispositions

Review 5638580573 at bb02de61 found one fix and nine batch rows. The new head
requires a Planner regrade; author evidence is not that grade. The current
correction routes the concurrent final-workflow responder through step_live,
which samples its clock after each complete request is decoded. Simulated-clock
responders retain their caller-supplied reading. Production observation validation
is unchanged. The test-only send probe now uses one atomic load with the same
Sent/Observed alternatives as sent().

| Row | Disposition |
| --- | --- |
| 1 | Corrected responder; idle/loaded repetition counts and exact-source rerun evidence are recorded above. Earlier one-off green author gates remain historical evidence, not proof that the old responder was reliable. |
| 2 | Open before activation: model-side Finish::Stopped currently refuses the answer election instead of following the existing clean-close road. No elected replacement behavior is claimed. |
| 3 | Current policy is explicit: a round-limit pass answers from all its selected checked Text, including request statements whose calls are barred. Thus the fixture answers Read/Recall request text while recording not_dispatched/round_limit; it claims no dispatch. Clarifying basic-inference-loop section 6 remains owed. |
| 4 | Refused still combines answered refusal and unfinished election. The exact finish result keeps custody; splitting the progress states remains owed. The child session layer rejects next_input while the underlying owner cannot finish. |
| 5 | Stop's gate-send pumping still belongs to the outer management special case. Moving that duty into StopWork, including pre-close waiting, remains owed. |
| 6 | Observe currently derives Active/Idle from the recorded open_turn. After the close records it can report Idle while world delivery remains owed. The semantic review remains open; no changed election is made here. |
| 7 | selected_answer still erases answer_text's RepairError. Exact fault/detail propagation remains owed. |
| 8 | AnswerWork and StopWork still duplicate close-and-carry ownership. A shared owner and uniform complete-send observation remain owed. |
| 9 | Corrected: the test probe observes status once with the same alternatives as sent(). |
| 10 | Unavailable self-check skills remain disclosed; no invocation is claimed. |

## Open boundaries and review carries

- #323 at 89e301d5 was regraded by the Planner in 5638369691: 0 fix, 7 batch,
  1 file, and marked ready. Its residual-owner Busy test/internal repair nuance,
  retirement assertion/factoring, frozen WrongReceipt/delivered tests, record
  cleanup and unavailable self-check skills remain open. The reported OUTPUT
  cleanup-bound issue is #326; its source counterevidence is pending review,
  and no fix to that mechanism is claimed here.
- #313 now includes the fourth parallel timestamp fixture. Loaded-run residency
  EAGAIN failures remain separate failures; passing serial runs do not close them.
- #324 at 5fc27ead was regraded in 5638375910: 0 own fix, 10 batch,
  ready. Its queued-world-input obstruction, loaded schema clock/certification,
  cancellation ordering and other batch findings remain. This workflow is not
  safe for cutover until those routing/containment joins are complete.
- #325 still needs operator election and merge, followed by paired producer and
  reader implementation. Its inherited multiline rejection behavior remains held.
- Multi-turn task reuse, serve selection/cuts, plan/forward flags, boot repair,
  S4/S5/S10, Save/capture/restore, failed-run teardown (#316), author migration,
  qualification and coupled cutover remain owed under #146.
- The required self-check skills remain unavailable in this environment as
  previously disclosed. This record is an author assessment, not a Planner,
  CodeRabbit, security or production qualification grade.

This PR is submitted as a draft for independent review. The operator alone merges
and deploys. No CodeRabbit review is requested.
