# Harness filing receipt custody

Date: 2026-10-21. Status: implementation under review.

This slice follows [PR #206][base]. It completes the caller-facing disposition
handoff from [HARNESS-LEDGER][harness]. A registered consumer receives the original
filing only after the real member receipt, its recorded measurement and the recorded
terminal flow event. Correct rejection remains a separate disposition. The existing
serving path remains active under FOUNDATION-TRANSITION.

## Implemented boundary

The ledger owns one registered receipt set. Registration names exact retained
sources, never an arbitrary sequence watermark or a historical row. A consumed,
unknown, foreign or repeated source cannot produce a new commit witness. The set
retains no second history after its caller takes or cancels it.

The context reserves the consumer before authoring. It binds the recorder's actual
sequence before any classifier or filing reduction. A selected copy waiting on
backlog capacity retains the same reservation until installation. An unselected
event or an empty projection produces no committed source.

At terminal recording, custody moves from the ledger into the registered result.
The committed result retains the exact original typed filing and request identity.
The rejection result retains its classifier evidence and never becomes acceptance.
Consumer custody is distinct from still-owed flow charges. Cancellation removes the
consumer's claim without cancelling a filing, replaying a request or undoing a commit.

The transaction uses an already armed decision or cut bound. It does not elect a
new duration, renew a deadline or disarm its caller's barrier. The same recording
context checks all earlier lifecycle bounds. Its pending author and result belong
to this transaction until explicitly returned.

A pending subscription prevents retirement of its borrowed wait. Once every
disposition was recorded before the original deadline, the enclosing owner may
retire that wait and collect the result later. A replacement bound cannot authorize
the old operation. An enclosing wait that remains armed still governs other work.
Cancellation releases only this subscription's retirement guard.

## Validation

The harness suite passes 795 tests with zero failures and 17 inherited ignored
instruments. All 22 added tests run. Eleven context tests use the actual recorder,
Unix stream state channel and STE100 packet socket with scripted member and checker
replies. They prove the harness receipt protocol. They do not qualify a production
classifier or constitute a new SQLite commit test.

The context tests cover exact source identity, mixed commit and rejection, deferred
whole-copy installation, withheld receipts, terminal trace pressure, cancellation,
completion custody and original deadlines. The ledger tests cover atomic source
registration, unchanged filing allocation, whole pair files, correct rejection,
frozen custody, cancellation, arithmetic and release failure. Compile-time checks
keep grants non-Clone and non-Default and make all five consumer entrypoints
accessible from sibling flow modules.

Ten compiled protective mutations fail their intended assertions. Saved baselines
and logs are in /tmp/state-receipt-mutations-final/. Independent review checks the
assertions and exact restoration, separately from the drafter's assessment. The
source timestamp test distinguishes final recording from the earlier author time.

The private target is /tmp/weaver-state-receipt-target. Final suite and lint logs
are /tmp/state-receipt-final-tests.log and /tmp/state-receipt-clippy.log. All-target
harness clippy at -D warnings, formatting, the lock gate, editorial/link checks and
git diff --check pass. No dependency, wire or production declaration changes.

## After-action

Review caught borrowed receipt exposure before transfer and after cancellation.
The result now exposes only facts until its complete, uncancelled handoff. Review
also found that a pending caller could retire its wait, and that an already timely
completion needed to survive deliberate wait retirement. Both paths now have tests.

The receipt remains intact if its recording timestamp is missing. Capture cannot
enter while the filing consumer still owns its result. A sibling-scope compile
check catches consumer methods accidentally made private to the context module.
The large receipt variant stays inline in its reserved slot to avoid adding a box
allocation at terminal transfer. Independent review found no remaining defect in
this component. The Planner's PR grade remains separate.

## Integration still owed

This result proves source disposition. It does not confer tool dispatch, plan
closure, mark allocation or permission to append a Message. [Spec 05][pairs] still
requires the pair owner to combine both commits with the accepted request and its
dispatch conditions. [Spec 08][sort] still requires a private template or tool-result
grant when constructing fresh instruction or result material.

Whole-generation checks and repair, the pair owner, marked placement and their
serving callers remain owed. The read owner still waits on [draft PR #189][recall]
for the serving recall payload. No new wire vocabulary or production classifier
qualification is part of this slice.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/206
[recall]: https://github.com/toddwbucy/WeaverAgent/pull/189
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[pairs]: ../crates/state-management/state-management-spec-05-tool-pairs.md
[sort]: ../crates/state-management/state-management-spec-08-sort-selection.md
