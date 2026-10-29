# Parent verdicts for checked-turn flag cancellation

**Version:** 0.1, 2026-10-29.
**Parent:** WeaverTools Working Process

This act answers #389's AD1-AD7 in Planner review 5693707553. The branch is
`code/state-flag-boundary-verdicts`, stacked on #391 at
`2ab08d1721c9d760710cba1466cc169f5816eecc`. It advances issue #146's worker composition
evidence. Production selection and runtime behavior remain unchanged. No broad
completion checkbox closes.

Implements: Spec 07 PLAN-OBSERVATION, harness amendment HARNESS-PLAN-OBSERVE and
HARNESS-READ, and harness Spec section 3's Stop and rollback Leave paths. Author
verdict: conforms within the measured cases. This adds test observations over existing
behavior.

## Boundary and observations

The two existing process cases use a checked turn with a pending flag read. Stop and
rollback Leave cancel that read, retain the original input and source identities, and
record one sample before the original turn closes. The real gate fixture now exposes
both management verdicts to the parent test.

For rollback, the parent requires the gate's Lower marker and the absence of a received
world-close frame. The gate records such a frame if one arrives before Lower, so an
erroneous close cannot pass through a child-only assertion. The gate exits after
answering Lower. This pin covers traffic before that acknowledged Lower, not traffic
sent to an already closed gate.

For Stop, the gate records any Lower, including one received in Stop mode. After Stop
settles, the parent sends a private fixture fence on the same ordered socket and waits
for an atomic acknowledgment file. Every preceding worker packet must reach the
observer before that acknowledgment. The parent rejects a Lower marker before, during
or after the wait. Sending and acknowledgment share one two-second test bound. This
fence exists only in the fixture and changes no production wire or management port.

The decoded world close must carry stopped, its fixed reason and the captured run and
turn. TurnAborted also names the captured turn. The two trace-order assertions share
one sequence parser, and the gate observations use the captured fixture root.

## Review dispositions and surface census

| #389 row | Disposition |
| --- | --- |
| AD1 | Parent requires no world-close file before acknowledged rollback Lower |
| AD2 | Gate records erroneous Lower and the parent waits for an ordered Stop acknowledgment before returning |
| AD3 | Parent checks decoded close kind and original run, as well as reason and original turn |
| AD4 | TurnAborted compares the retained original turn rather than a repeated fixture literal |
| AD5 | Stop close checks use CheckedTurn.root |
| AD6 | Both trace-order checks use trace_sequence |
| AD7 | Renamed seated, which has runtime readers. Kept _turn because its only read is cfg(test) and runtime retains it for custody. No dead-code allowance added |
| AD8 | Process limit retained: code-review and verification-before-completion skills are unavailable in this session |

The surface census covers the sole gate fixture producer, Stop's exercise consumer,
rollback's finish consumer and both callers of test_checked_source. The only runtime
source edit is the private seated field rename at its constructor, readers and update.
The author read the complete diff and its consumers twice. This is a manual author
check, not the unavailable skill invocation, a Planner grade or security disposition.

## Controls and validation

Four compiled faults exercise the new observations through the process cases. A
rollback fault sends a real world-close packet before Lower. A Stop fault sends a real
Lower after the tracked world close. Both fail in the parent. With just the respective
new parent check removed, each faulty implementation passes without a child failure.
These two counterchecks demonstrate the earlier observation gaps. Separate wrong-kind
and foreign-run faults fail at the parent's decoded world-close assertions.

All 812 Rust/configuration hashes restore after the controls. The initial wrong-kind
log classifier expected a serde Value on both sides of assert_eq, while Rust prints the
right-hand string directly. The test had failed at the intended assertion. The original
log and classifier are retained, and the corrected classification reads that same log
without another build. No runtime or test correction was needed for that mismatch.

The pinned nightly-2026-02-13 uses --locked, CARGO_INCREMENTAL=0 and private target
`/tmp/weaver-state-146-serving-organ-loss-target`. Cargo runs without root. Compiled
process tests run in an owned delegated systemd scope with the real admin binary.
The final gates pass nine ordinary flag tests, all 24 worker_plan_ process identities,
strict all-target harness lint, formatting, lock, three DocsMirror tests and the diff
check. Before/after hashes match for all 812 tested Rust/configuration files.
Publication checks the committed blobs against those hashes and retains the snapshot.

The scope is the ordinary flag tests, all 24 worker_plan_ process identities, strict
all-target harness lint, formatting, lock, DocsMirror and diff checks. The full harness
package suite is not rerun for this fixture correction and private field rename. #391's
full-suite results remain evidence for its head alone. Nested child passes are not
counted again, and the gate rejects any child panic or failed test summary.

#388 AC6, #383 Y1 and the inherited review register remain open. A passing family does
not explain the earlier intermittent cleanup failure. #391's Planner review 5693929433
now grades its head zero fix, with AF1/AF2 and AE1-AE8 carried for the Diagnostic act.
#390 still owes Diagnostic interruption composition. #384 and #387 remain proposed
elections whose dependent implementation waits for the operator, review and merge.
These fixtures qualify no production language artifact or deployment tuple.

The planned work was to make the two hidden gate outcomes observable in the parent.
The controls showed both original tests could pass those faults, and the restored
tests reject them. Keep the ordered acknowledgment when judging a still-running child,
and check the concrete assertion output before classifying a protective failure.

Scratch evidence is `/tmp/state-flag-boundary-verdicts-20261029`. The publication body
names its committed snapshot and verified deposit under the authorized #146 evidence
directory.
