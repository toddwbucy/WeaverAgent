# Rollback listener composition

Date: 2026-10-23. Branch: code/state-rollback-listener-composition.
Parent: #294, d35ed52a833c5124f7b1b900c9b0eb9232212f12.
Advances Issue #146 composition. No full checklist item closes.
Operator alone merges/deploys; existing serving remains active.

## Connected path

The actual accepted rollback now consumes its coordination listener. Its measured
Grants/trace wait services that listener and one secondary dial through the common
pump. Observe answers InTransition; JoinLeave and another Leave are OutOfOrder for
a rollback. An out-of-order Enter retains and closes its received descriptors
without adopting them. Original state-channel identity and the numbered read remain
unchanged. The listener and any unfinished secondary answer travel into the owned
release-preparation handoff beside the accepted Leave and both boundary readings.

Primary admin input, a secondary dial and the already accepted Leave peer have
separate pump roles. Secondary and extra packets cannot trigger boot/placement
cancellation. The original Leave connection is monitored again: an unexpected
second frame stays held, without becoming another accepted directive. Its EOF is
retained and reported without stopping rollback. The original cause, forced outcome
and absolute Leave bound remain unchanged. This addresses #294 Planner finding 2
and composes the Grants-wait portion of #291's listener carry.

The common clock immediately before nonblocking accept anchors the existing two-
second silent-read allowance, capped by the original Leave. A delayed reduction
cannot restart it. Only that exact read expiry or a secondary-peer transport failure
is recoverable locally. Listener/state/trace failure and original Leave expiry still
stop. Actual listener hang-up is now recognized even where accept reports EAGAIN;
the joined shutdown instrument exposed that gap. Clock/bound validation retains
accepted custody, and a tracked whole send with a failed clock observation retains
Sent and latches the first failure instead of discarding it as a dead peer.

Answers are staged once and remain owned under backpressure. A pressured secondary
answer does not hold the state read, and preparation keeps the pending answer and
its real send witness for the final lifecycle owner. There is one secondary slot,
not an unbounded connection queue. No new duration, wire field or trace event is
introduced. Kernel root-credential verification still precedes every payload.

## Authority and evidence

Implements harness Spec 6; admin-harness contract sections 2, 3 and 4; admin Spec 3's
rollback/JoinLeave rows; HARNESS-CLOCK and HARNESS-PUMP. Author assessment: conforms
for the accepted rollback's measured-read wait, role isolation, response mapping,
original windows and retained custody. Terminal seal, backlog sweep and Left are
still unimplemented here; this is not whole-section serving conformance.

Fourteen new joined scenarios cover Observe while Grants is pending, rejected joins
and second Leave, original silent acceptance timing, failed/malformed peers, actual
credential rejection, both Enter handles, send pressure, listener loss, original
Leave expiry, two clock failures and a retained extra original-peer frame. Thirteen
root-credential scenarios re-execute inside user namespaces; the remaining
scenario uses the host caller's actual credential. No authentication bypass or
running deployment is used. The 36 response/rollback scenarios pass together.

Final gates: 1,909 harness passes with 18 inherited ignored; 160 trace passes; strict
all-target harness Clippy, formatting, lock and diff checks; three DocsMirror tests.
Private Cargo target: /tmp/weaver-146-boot-composition-target. Seventeen compiled
protective mutations fail their named tests and all restored tests pass. E0507
rejects providing the owned listener through a shared borrow; the restored 36 joined
tests pass. All 244 harness Rust hashes match after restoration. Nested child
summaries are excluded from top-level counts. These are author checks, not a new
Planner/security or production qualification. Prescribed review/verification skills
were unavailable; direct source/sequence reviews are retained in the evidence.

The initial control set retained one survivor: the clock test could reach ordinary
success when the intended failure did not happen. The test now requires its failure
branch, and the final mutation fails. Those logs and the earlier fixture/initializer
corrections remain in the deposit. The parent #294 Planner grade has zero fix rows
and seven of seven controls; its unavailable Codex review is not a security pass.

## Remaining composition

A timed-out or lost current read still needs the accepted-Leave Failed cleanup
handoff carrying Unreadable (#294 finding 1). Pending initial Grants/refusal accounts,
bounded LastWord, terminal release/Unload/drain/Left, success gate/Ready and the actual
admin/listener/worker root remain open. Every later wait must keep servicing the owned
listener; terminal seal/backlog processing is not granted by preparation. No-SPU
terminal representation still awaits #285 review/merge. The initial Grants timeout
ruling and other authority holds are unchanged. Other #294 batch findings remain
tracked. Production artifacts, prompt, limits, stack/memory and qualified coupled
cutover remain owed; no successful Left or Issue #146 completion is claimed.

After-action: test accepted lifecycle identity, real authenticated dials, measured
state work and transport pressure in one path. Retain unexpected input without
promoting it to authority. Require negative tests to reach their failure branch;
checking a failure only if it occurs can conceal a surviving mutation.
