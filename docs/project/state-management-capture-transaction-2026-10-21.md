# Harness and member capture transaction

Date: 2026-10-21. Status: implementation under local review.

This slice is stacked on [PR #204][base]. It implements the harness/member exchange
in [PERSIST-SAVE, PERSIST-BEGIN and PERSIST-EXPIRY][foundation], the correlated
cleanup in [PERSIST-CLEANUP][persistence], and the capture portion of
[HARNESS-BARRIERS, SAVE-TIME and CONTROL-FAILURE][harness]. It uses the existing
state-flow/1 channel, recorder and common clock. The current serving path remains
active under FOUNDATION-TRANSITION.

## Implemented boundary

The lifecycle caller carries SavePointBeginning before entering this port. The
recording context requires a between-turn boundary and takes the last sequence it
recorded itself. A caller cannot supply a smaller frontier. The coordinator drains
all owed sources, waiting copies, receipts and observations before it permits Snapshot.
The context blocks new content, turns and input transactions during the capture.
The coordinator blocks new reads and classifier checks while existing filing drains.

Snapshot, Acknowledge, Abort and ReleaseCapture use the sole state channel. Acknowledge
and Abort retain the original request and capture IDs. ReleaseCapture spends a fresh
request ID. Each answer becomes usable only after its exact state.store.measurement
records. The original recording timestamp survives an interrupted channel release or
a later pending author. The context owns a typed result until its caller takes it.

The capture hold remains after the final member answer. The lifecycle caller must
record the final save disposition, consume the answer and explicitly finish the
capture before another turn or filing can start. Cancellation before Snapshot removes
only the capture hold after the caller records refusal. Existing filing remains owned.

## Time and failure

The loaded save and snapshot allowances come from the recorded declaration. The
original save window includes carry and remains the permission limit for later
publication stages. Snapshot starts its wait at actual enqueue. Acknowledge starts
its two-second wait at enqueue. Original leg waits are capped by the save window and
observe any earlier Leave. Filing uses the declared cut bound until the real barrier
settles. No transport progress starts a new deadline.

A missing Snapshot or Finished starts correlated Abort and retains the barrier.
Cleanup has its own two-second allowance from enqueue. It can run after the save
window expires, while the original publication permission stays expired. An earlier
Leave still wins. Every late original answer is validated, measured and discarded,
including one already recorded but held behind a later author. It cannot renew or
disarm the cleanup wait. Timely explicit refusal remains distinct from a missing
result and becomes visible after cleanup.

A parent cancellation after timely Finished uses a fresh ReleaseCapture, as the
foundation requires. Its cleanup retains the original publication window and observes
the earlier Leave, even if permission to publish has expired. If the timely Finished
proof still waits behind another author, the coordinator accepts one release intent
and starts the same cleanup bound at that enqueue. Its original queued timestamp
survives the wait for the prior author/proof and reaches the eventual channel
measurement. It never substitutes original-ID Abort for this case.

Malformed, wrongly correlated, refused or missing cleanup and failed carry remain
component failures. Capture identity and phase remain available to the halt census.
The context checks the externally supplied halt capture ID against its retained owner.
The lifecycle caller still owns publication phase, Commit and published evidence.
No halt starts a dump or substitutes an artifact for a protocol outcome.

## Validation

The full harness suite passes 750 tests with zero failures and 17 inherited ignored
instruments. All 38 new capture tests run. They use real Unix sockets, actual recorder
capacity and the common deterministic clock. They cover the successful exchange,
both missing-result legs, explicit refusals, late observations, cleanup failure,
partial progress, the real filing drain and a new capture after cleanup.

Timing regressions cover clock expiry during enqueue, an earlier Leave, a release
bound equal to the outer save bound, and timely recorded results held behind either
an interrupted release or a later author. A later pending author also blocks an
already cached result. Expiry of publication permission does not relabel a timely
member result as missing.

Twenty guard-removal probes compile and fail their intended assertions. They cover
the real drain, frontier, ownership, original-result discard, timing anchors,
pending authors, cleanup bounds and the distinction between a recorded Finished and
a recorded refusal. Both probed sources are restored byte for byte. Independent
review checks the source boundary, test counts and evidence.

The private target is /tmp/weaver-state-capture-target. The harness gate command is
cargo test -p weaver-harness --locked. The local evidence is
/tmp/state-capture-baseline-tests.log and the per-probe results and logs in
/tmp/state-capture-mutations-final/. The restored capture run passes 44 tests with
zero failures and one inherited ignored member instrument. Its log is
/tmp/state-capture-restored-tests.log. All-target harness clippy at -D warnings,
formatting, the lock gate, editorial checks and git diff --check pass. The lint log
is /tmp/state-capture-clippy.log. Independent review finds no remaining defect in
this boundary and verifies the baseline, probes and exact restoration.

## After-action

The planned component covered the filing barrier, capture replies and timeout
cleanup. Review exposed a cancellation after timely Finished while a later author
still held the result. The coordinator now accepts one release intent under the
existing cleanup bound and preserves its enqueue timestamp until dispatch. The
regression uses the public resumption port. The lifecycle integration must use this
owned transaction and its typed results rather than rebuild ownership from socket
progress.

## Integration still owed

This port does not create the admin lifecycle wire, publish artifacts or decide when
the loop should save. The paired lifecycle change must connect Beginning, Image,
ImageChecked, ImageFinished, Prepared, Commit, Published and final outcomes to this
member exchange and the admin publisher. That act must also carry force/leave
containment and verified boot under the coupled cutover contract.

The serving recall and snapshot measurement amendments remain separate review items.
The member sampling change is not implemented from an unmerged Spec. These tests
exercise synthetic observations and actual local sockets, not production store-size
qualification, save allowances or disk performance. Production STE100 artifacts,
template certificates and the separately reviewed system prompt remain owed.

[base]: https://github.com/toddwbucy/WeaverAgent/pull/204
[foundation]: ../crates/state-management/state-management-foundation-Spec.md
[persistence]: ../crates/state-management/persistence-contract-amendments.md
[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
