# Harness whole-output checks

Date: 2026-10-21. Status: implementation under review.

This component follows [PR #207][base]. It implements OUTPUT-UNIT and the checking
portion of DECISION-BARRIER in [Spec 02][protocol]. One transaction owns the original
assistant Message, its finish and turn, and every Text block's recorded disposition.
The original serving path remains active under FOUNDATION-TRANSITION.

## Implemented boundary

The transaction starts inside a recorded open turn with a caller-owned Decision
wait. It reserves the original Message before authoring. The recorder supplies its
actual sequence before any result reduction. Every Text block keeps its original
content index, including gaps occupied by ToolCall and UnrecoveredCall blocks.

Selected checks use the ordinary ledger and classifier. The coordinator retains
only results whose ste100.result record completed and whose exact source, scheme,
text and turn match the original. Unselected checks use the same classifier and
recording barrier without inventing a filing. Their requests come only from this
transaction's retained Text slots. A pending or correctly rejected sibling cannot
be omitted from the complete result.

The private CheckedGeneration is available once, after all check dispositions are
recorded. Correct rejection is a complete check result and does not halt the agent.
The grant retains accepted and rejected blocks in original order. It grants no
successful answer, dispatch, repair substitution or placement. Ordinary accepted
statements continue filing independently. Waiting for their receipts would violate
DECISION-BARRIER, so this owner does not use the filing receipt frontier.

Cancellation revokes the complete-check grant. Before authoring, it returns the
original without inventing a source. An existing author ticket resumes unchanged.
An unselected submitted check retains its exchange and original Decision wait until
its recorded result or bounded failure. Selected obligations remain with the ledger
and may continue as background filing after the output transaction is cancelled.
No cancellation deletes admitted content or replays a classifier request.

The transaction never starts a new clock or renews its borrowed bound. A pending
check prevents early wait retirement. A timely recorded complete check can be taken
after the enclosing owner retires that wait. Generic authoring, input, capture,
template certification and filing consumers cannot take its result or original.

## Validation boundary

The restored harness suite passes 822 tests with zero failures and 17 inherited
ignored instruments. All 27 added tests run. The 17 actual-context tests use the
real recorder, state stream and STE100 packet owners with scripted peers. Ten core
tests exercise the private owner. Those tests establish the harness contract, not
a qualified production classifier or a new SQLite commit.

Twelve compiled protective mutations fail their intended assertions. Independent
review verifies each test failure and the exact restored source bytes. The probes
cover original indices, selection order, complete results, cancellation, frozen
custody, recorded turn and time, unselected exchange ownership, the absence of an
ordinary-filing barrier, wait retirement and author ownership. Compile-time checks
keep the result non-Clone and non-Default and verify sibling access to its five
consumer entrypoints.

The private target is /tmp/weaver-state-output-target. Logs are
/tmp/state-output-final-tests.log and /tmp/state-output-clippy.log. Probe baselines,
results and logs are in /tmp/state-output-mutations/. All-target harness clippy at
-D warnings, formatting, the lock gate, editorial/link and diff checks pass.

The initial run rejected three fixtures with incomplete serving coverage. Those
fixtures were corrected or removed, with unselected semantics tested at the private
owner. One fixture was also corrected to observe completed send progress before
asserting AwaitingReceipt. Independent review found no remaining component defect.
The Planner grade remains owed before undraft.

TRACE-VALIDATION requires all assistant Text in a serving partition. The real
serving-context tests preserve that coverage and test sparse original Text indices
between call blocks. Unselected-block handling is exercised by the private core
instruments. No test-only route weakens the serving partition validator, and no
serving qualification claim follows for an unreachable partial partition.

## Integration still owed

RepairLedger, EffectiveEmission and the pair owner consume these complete check
results. Per-request committed receipt custody must be registered before source
retirement and composed with pair opening. It must permit a later PairStarted
filing without imposing an unrelated sibling's receipt wait. The single registered
filing consumer in #207 alone does not implement that complete composition.

Production grammar, model artifacts, the reviewed system prompt and qualification
remain required before cutover. Orchestration remains in worker/dev_loop. These
checks neither invoke the decoder nor elect repair attempts or a strategy.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/207
[protocol]:
../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
