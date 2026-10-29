# Flag cancellation at a checked turn

**Version:** 0.1, 2026-10-29.
**Parent:** WeaverTools Working Process

This test-only act answers #388 AC2 in Planner review 5693179258. The branch is
`code/state-plan-flags-cancel-turn`, based on #388 at
`252ed148de0e1233cfea214ae466dd0a25b88cce`. It advances issue #146's worker composition
evidence. No broad completion checkbox closes and production selection stays unchanged.

Implements: Spec 07 PLAN-OBSERVATION, harness amendment HARNESS-PLAN-OBSERVE and
HARNESS-READ, and harness Spec section 3's Stop and rollback Leave paths. These conform
within the measured scope. The only implementation-file addition is a
cfg(test) accessor for the retained rollback turn and source identities.

## Composition and control

The normal SQLite member and real worker first complete a root plan update. A real
fixture gate then supplies an input, the worker checks it and opens its turn, and an
explicit flag sample begins while the member withholds its response. The two new
cases deliver Stop and rollback Leave at this boundary.

Both require one Cancelled result, read cleanup under the original bound and one
Recorded sample. They compare the original gate bytes, turn key and seated source
before and after management. The sample follows turn.started. Exactly one turn.closed
for that turn follows the sample. The Stop case requires TurnAborted for that key and
checks the decoded world reason received by the gate. The Leave case carries the
sample and original input through the consuming rollback handoff, verifies the gate's
Lower, then requires closed cleanup, Left and one unload with no second sample.

A compiled control suppresses cancel_read only while a checked turn is open. Both new
cases fail with Measured where Cancelled is required. The normal turn-boundary case
still passes. All 809 Rust/configuration hashes restore. Final gates run on the
restored sources, and their before/after hashes match.

## Review dispositions

| #388 row | Disposition |
| --- | --- |
| AC1 | The preparation loop rejects any state other than Pending or its target and includes the reached state and detail in failures |
| AC2 | Two process cases cover checked-turn Stop and Leave, with a compiled cancellation control |
| AC3 | The success case checks FoundationTurn::turn and source before returning it to service |
| AC4 | Gate failures include the mode and packet, and cancellation cases handle their real world close or Lower |
| AC5 | Preparation loops sleep 1 ms per lap while the member responder works |
| AC6 | Carried: consolidation of the five responder/thread guards spans the existing fixture family |
| AC7 | Removed the redundant in-loop Pending assertion, retaining the first boundary check |
| AC8 | Process limit retained: code-review and verification-before-completion skills are unavailable in this session |

The author read the complete four-file source diff and its Stop/rollback consumers.
This is a manual author check, not a skill pass or Planner grade. #383 Y1 remains
unresolved: a passing family does not explain the earlier intermittent cleanup failure.

## Validation and limits

The pinned nightly-2026-02-13 used --locked, CARGO_INCREMENTAL=0 and private target
`/tmp/weaver-state-146-serving-organ-loss-target`. Cargo ran without root. Compiled
process tests ran in owned systemd scopes with Delegate=yes. The real admin test
binary was supplied and its hash recorded.

| Gate | Result |
| --- | --- |
| Lock, rustfmt, strict all-target harness clippy | Pass |
| Ordinary flag tests | 9 passed, 10 ignored process entries |
| Delegated worker_plan_ family | 24 distinct cases passed, including all 10 flag cases and actual admin/SQLite |
| Checked-turn cancellation control | Compiled, both new cases failed as intended |
| Source restoration and final-gate hashes | All 809 match |
| DocsMirror and diff checks | 3 tests passed, clean diff |

Nested child passes are not counted again. The gate script also rejects a nested child
panic or failed test summary. The full harness package suite was not rerun for this
test-only change. Parent Planner results remain evidence for that parent, not this head.
Scripted language fixtures do not qualify production artifacts or a deployment tuple.

The first development run exposed two fixture errors: it expected a world close before
rollback Lower and searched a serialized base64 carrier for plain text. Rollback closes
the retained turn after Lower, and the corrected Stop assertion decodes the frame.
The failed log is preserved. No runtime change was needed. The lesson is to read the
existing close and Lower ordering before composing an assertion across them, and to
check nested child failures independently of a cleanup owner's terminal verdict.

Scratch evidence is `/tmp/state-plan-flags-cancel-turn-20261029`. The publication body
names its committed snapshot and verified deposit under the operator-authorized #146
evidence directory.

## Next composition boundary

The read-only Diagnostic audit found that a complete route must handle both the new
Enter shape and paired Observe facts, including Observe during replay. It must retain
the original peer and descriptors, return pre-load refusals to before-Enter service,
and preserve Diagnostic replay and no-state Leave. An initial-success adapter alone
would leave those consumers on the old wire. This act changes none of them. #384 and
#387 remain proposed Spec elections, and their dependent implementation waits for the
operator's ruling, review and merge.
