# Entered Diagnostic replay interruption

**Version:** 0.1, 2026-10-30.
**Parent:** Issue 146, state management implementation.

Implements: weaver-harness-Spec section 6.2 DIAGNOSTIC-INTERRUPT and its
entered management/death rules, section 6.1.1 DIAGNOSTIC-SETUP-ROLLBACK's
handoff, weaver-diagnostic-Spec sections 3 and 5, and
weaver-harness-spu-decode-contract sections 2, 3 and 5's consumer duties.
Merged #393 and #398 supply authority. Admin Spec section 3 and the
admin-harness contract supply existing cleanup, cause and deadline rules.

The branch code/state-diagnostic-interruption stacks on exact #411
3fd316de01782fa125707bbcf3ba150d3b1ef97a. Merge a59b32f4 retains corrected
#410 at 610cd986d508f2697cc1218f95958977df200a54. It does not squash away
that parent. #408's merged WorkRef election remains inherited through main.
This act advances #146 section E and implements #412 R1/R2. Review and
operator merge remain required. No broad #146 checkbox closes here.

## Behavior and ownership

The entered Diagnostic owner retains management connections from setup
through replay, post-pass idle and Leave. Selecting Ready grants replay even
if another peer has unread replies. Accepted first requests and exact replies
remain owned. Once cleanup fixes Left, an unrelated unread reply cannot turn
that completed outcome into worker failure when delivery expires.

A Stop with no open turn answers AtRest and retains the pass. A Stop against
an open turn retains its original peer, exchange and cause. Forced Leave,
rollback, joining force and graceful-drain expiry use the same cancellation
owner. Its original 60,000 millisecond interval begins at acceptance and
covers partial request sends, terminal settlement and turn-close recording.
Progress and later requests do not renew it. Graceful Leave finishes only
the current turn under the original optional drain bound. An absent bound
remains unbounded until completion or joining force.

An unsent ReFeed can be abandoned before it starts. Once a segmented request
starts, its original series must finish before the one matching Cancel.
The owner validates RefeedInterrupted's exact turn, consumed below path
length and checked consumed + 1 <= resident <= capacity. It retains that
terminal through AtRest and recorded turn close. A complete ReFed that wins
the race retains its truthful generation records. An interrupted prefix
never becomes a Generation, parser input or certificate.

Earlier Field, Column and closing-event obligations retain their admitted
bytes and order. A new replay.closed cannot be admitted after an accepted
open-turn Stop or any entered Leave. An already admitted closing event keeps
its original outcome. No later turn or resident edit runs after interruption.
A successful Stop follows decode settlement and the one Recorded turn close.
Requester loss retires delivery alone.

One idle decode report can be held behind the earlier writer. It records
before the next report is received. The idle observer does not steal an
active decode reply. Decode EOF is distinguished from a failed management
wait. Observed death after ReplayOpened records under the existing failure
bound where applicable, and child custody remains with cleanup. Post-pass
idle death retains the run, reaps the SPU and makes later Leave forced.

Cleanup retains the strict pre-Ready Released requirement and the entered
Unconfirmed release rule. An owned kill makes the final Leave forced. A live
typed refusal does not invent an organ death. Failed record or unsettled
cleanup grants no Left. Management stays available through actual child reap.

The Diagnostic recorder gains a managed submission form in the same act as
its Spec section 5. It validates and renders once, retains the original
partial-write offset, invokes management before writes and advances sequence
only after a whole line. Admission refusal never touches the sink. Wait/write
failure latches the existing WriteFailed. This is a mechanism for #393's
existing policy, not a new event, field, queue or deadline. Ordinary submit
retains its existing failure behavior.

| Surface | Responsibility and caller |
| --- | --- |
| diagnostic_setup | Owns peers, selected outcomes, original bounds and cleanup. |
| diagnostic_replay | Owns the entered turn, active decode and cancellation. |
| diagnostic_idle | Owns post-pass reports, death accounting and child reap. |
| ReplaySeat / replay::drive | Walks recorded work and checkpoints before later work. |
| send_replay_managed | Distinguishes unsent work from an already started series. |
| StateSeam::ask_replay_managed | Retains the parked answer deadline and retires a missed direction. |
| Author event constructors | Stamp facts once before the recorder wait. |
| Record managed doors | Reject Serving use and retain first failure. |
| Diagnostic Recorder | Retains one canonical line and offset through pressure. |
| engine::Ports replay helpers | Test-only legacy walker adapter, no production selection. |
| canonical Diagnostic admission | Hands the entire run to the managed owner. |

## Evidence and controls

Final source passes 2,301 ordinary harness tests/doctests, 19 Diagnostic
cases and 13 explicit owner tests. All 92 unique Diagnostic worker scenarios
pass in 93 top-level invocations because one substring filter also matches
its longer scenario name. Terminal summaries and the complete name census
were checked separately, excluding nested child summaries. The ordinary
harness suite reports 1,059 ignored cases and explicitly skips known #331.
Both changed-crate strict all-target lint gates, lock, formatting, diff and
three DocsMirror checks pass. Both real models report one Field, zero
complete Generations and one turn close. No model fixture skips.

The final runtime and test sources match the 1,209-file tested snapshot.
Only this project record and the Spec's Landing PR header change after the
code gates. Their final checks and commit reconciliation are deposited.
Pinned nightly-2026-02-13, --locked, CARGO_INCREMENTAL=0, debug=0 and jobs=8
use /var/tmp/weaver-state-diagnostic-interruption-target and /var/tmp/wd146i.
The full workspace/device suite is not claimed from these affected gates.

The process suite uses isolated root-delegated systemd scopes, one case at
a time. It runs the actual 60-second cancellation, greater-than-60-second
graceful drain and 150-second bystander-delivery intervals. No compressed
timer stands in for those process cases. Smaller owner instruments test
original-deadline propagation and actual socket pressure separately.

Both real-model cases build their source with the actual SPU, then preload
its request and measurement through the real SQLite member. Identity and
Replay share that member's one seal. The fixture does not claim a second
unsealed Replay phase after Ready. It holds the first actual Field behind
record pressure, pauses the owned SPU, accepts Stop and Observe, resumes the
SPU and drains the trace. It requires exact Stop, graceful no-state Left,
no certificate and actual process reap. The member is directly started
without admin's run-lock descriptor, so this adds no admin-custody claim.
Raw baseline JSON, baseline SPU log, member stderr and Diagnostic trace are
exported for the final run through WEAVER_DISPATCH_EVIDENCE.

The actual artifacts are:

```text
/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf
/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct/model.safetensors
```

GPU 0 is NVIDIA RTX A6000, UUID
GPU-1bb3da64-04e7-49c0-3b6b-7ef66c6175b9, driver 615.71.09.
The private SPU binary was built from this consumer checkout with cuda,gguf.
Its SHA-256 is:

```text
b141d293d99869c2dfe1941986b3df2e541bec3c85543e7bb5949f4a11dcda3d
```

Forty-three compiled controls were caught across the recorded source
cohorts. They are not all rerun against the publication source. The exact
source, restored-source checks and failures remain in each deposit. C4 is
explicitly a two-guard control, not two independent controls.

| Cohort | Controls and watched property |
| --- | --- |
| Checkpoint 1 | U1-U7: unsent work, original bound, duplicate observer, Left, Ready, drain units and retained terminal. U1-U6 precede terminal retention. |
| Checkpoint 3 | V1-V8: exact turn, path/fullness counts, AtRest, timer renewal, graceful drain and completed Left. |
| Checkpoint 4 | W1-W4: idle decode, dead-SPU forced mode, failed fault record and management during reap. |
| Checkpoint 5 | X1-X5: started series, Cancel ordering, exactly one Cancel, unsent work and original send-pressure bound. |
| Checkpoint 6 | C1-C4: admitted clean close, settled observer and later work. C4 removes both walker exit and interruption retention. |
| Checkpoint 7 | T1-T4 and R1-R3: admitted ticket, Stop/Leave precedence and exchange, final forced release, death recording and pre-opening silence. |
| Checkpoint 8 | P1-P5: preload observer, report filing, active-exchange custody, original preload bound and no work after EOF. |
| Checkpoint 9 | D1-D2: open-turn death accounting and receive EOF retention. |
| Checkpoint 11 | MC1: omit actual Cancel while claiming sent. Real GGUF worker fails its Stop assertion after 63.99 seconds. |

All deposits are beneath the authorized root:

```text
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/
```

The prior directories are diagnostic-consumer-progress-20261030-checkpoint1
through checkpoint11. Their manifests preserve the failed attempts and exact
cohort scopes. Checkpoint 8's ordinary count is corrected by its separate
correction record to 2,301, preserving the original manifest. The final
publication evidence is diagnostic-consumer-publication-20261030.

## Review and limits

Manual self-review reads the management/record/decode transition sequences,
terminal retention, error-versus-EOF handling, first-failure latch, phase
bounds, canonical admission and the caller table above. Rework review checks
the final artifact export and source reconciliation. The code-review and
verification-before-completion skills are unavailable. These manual reads
are not claimed as those invocations or as Planner/security qualification.

This slice implements the consumer held by #411, but it does not authorize
deployment. Existing Serving remains selected. The stack still needs review,
operator merges, production artifact/prompt/limit qualification and coupled
cutover. #411 B1-B6 remain carried with their Planner dispositions. #413's
classifier EOF phase race and #414's gate-death pressure assertion remain
undiagnosed, not fixed by a passing run. Known #331 remains explicitly skipped
in the ordinary harness gate. No CodeRabbit review is requested.

## After-action

The plan was a complete entered owner rather than a receive-only Cancel
handler. Process checks exposed additional work at idle reports, partial
request sends, admitted closes, failed release and open-turn EOF. Those
corrections account for the larger slice. Deterministic peers pin order and
failure boundaries, while paired model runs pin the actual composed route.
Neither substitutes for the other or for production qualification.

Failed fixture/build attempts remain in the deposits: helper visibility,
RunId qualification, long scratch socket paths, pressure setup/scheduling
and a native model identity mismatch. The latter admitted a directory while
its baseline named model.safetensors. The corrected fixture uses the exact
file. Its stopped systemd run returned zero without a test summary, which was
rejected rather than counted as a pass. A failed long fixture also required
a panic-time reader wake so its deliberate assertion failure could unwind.

The standing continuation plan now requires raw artifact export before final
model runs and a complete process census before publication. It distinguishes
one-seal real-member evidence, scripted parked answers and actual model
execution. This keeps later review claims tied to the boundary each test ran.

Publication test deposit: 121 files plus manifest SHA-256
4f04b23ceb3f574195ae87946a8e1630b9ebe6cfb23a167f477015613d382323.
The manifest is immutable. Commit reconciliation is a separate deposit.
