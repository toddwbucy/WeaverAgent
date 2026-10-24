# Completed and returned boot rollback custody

This slice advances #146 on code/state-boot-rollback-custody, above exact #270
51a5490722ede066b18d28f042b1bd47630f2b7d. It answers #267 Planner review
5624358977 row 1, also carried by #269 review 5624951821. It remains dormant.

## Authority and behavior

Implements: persistence contract PERSIST-ENTER; HARNESS-OPEN-LIVE and the Open state table;
HARNESS-PLAN's mandatory boot read; PLAN-SUMMARY-EVIDENCE; and
HARNESS-SAVE-TIME's failed-enter rollback/no-capture rule. These merged clauses
require truthful completed evidence and cleanup under the original bounds.
Approved but unmerged #260, #263 and #268 are not implemented here.

A completed boot read remains successful evidence after Stop or Leave. Ordinary
extraction refuses once future Enter use is revoked and retains the original
transaction for cleanup. Neither beginning rollback nor settling its account
rewrites that completed read as cancelled. Unfinished cancellation retains its
existing disposition. Completed effect refreshes still return their exact past
outcome after Stop revokes future cache use.

Successful BootPlanSummary extraction now transfers the original boxed summary
transaction, including its repaired pairs, Recorded read, rollback seal and
frozen account minimum. A shorter held Leave travels with that proof even if its
registered wait later retires. The read and repaired-pair accessors borrow the
same retained evidence.

The consuming restore_boot_plan_summary_for_rollback adapter validates actual
context/read/Open identity, the exact committed-cache Arc, BootHead/run and the
original registered Enter. Foreign or stale proofs, missing or replaced Enter,
wrong phase and competing work return the same proof and existing first failure.
Acceptance transfers the original owner back for cleanup, revokes ordinary
success, and freezes the existing Enter/Leave/halt minimum. It samples no clock,
admits no new request or record, and cannot clear the first failure.

The adapter operates in Entered or Failed while state remains standing. Existing
release and halt methods still perform bounded disposal. Successful placement
consumes the only owned boot proof into PlaceEngine; its borrow cannot satisfy
the consuming adapter. A separately owned foreign proof cannot replace it.
Installed-proof disposal remains placement's existing teardown duty.

## Validation

Sixteen new actual-owner tests cover both release/extraction orderings, preserved
successful evidence, unchanged unfinished cancellation, held and shorter Leave,
original-bound expiry, exact foreign/stale proof refusal, competing filing,
retained first failure and real transport retirement through Unload/drain/Closed.
Actual placement tests exercise Stop, Leave and JoinLeave between extraction
and installation, then return the exact rejected proof through rollback. The
unstopped installed proof and completed effect are positive controls.

Author gates pass: six completed-boot tests, eight returned-boot tests, two
placement/boot tests, the existing completed-effect control, full harness 1,694
passed with 18 inherited ignores, trace 160, harness all-target Clippy with
warnings denied, formatting, lock and diff checks. The private target is
/tmp/weaver-146-boot-rollback-custody-target. Gates ran serially on the host
socket lane with lock first and --locked.

All 19 protective mutations compiled and failed behaviorally; their exact
restored controls passed. The separate installed-borrow perturbation failed
with the expected E0308, because the adapter requires owned BootPlanSummary
rather than a borrow. Its restored control passed. That type rejection is not
counted as a behavioral mutation.

The first focused run passed five tests and failed one setup assertion: the
public committed summary is Pending while its completed transaction is still
owned. The fixture now checks that boundary and the retained populated private
cache separately. Production behavior was unchanged by this correction. All
initial logs, source reviews, probe recipes and restored hashes are retained.

Independent source and join reviews cover the six changed source paths and the
published parent. Production changes are confined to context_plan_summary.rs;
the placement-aware admission and dual Decision/Cut rules remain unchanged.
Author validation and scripted peers establish owner custody, not a Planner
grade, a live SQL circuit or production qualification.

## Remaining work

#269's two later Planner fix rows remain separate work: returning cancelled
reload inputs/confirmed Staged custody and taking cancelled native mark jobs
without blocking later turns. This slice does not close those findings.

The actual lifecycle/residency aggregator, Ready/Left responses, worker/dev_loop
callers, installed teardown composition, qualified language/model/prompt
artifacts and measured limits remain required. Existing serving stays active
until the reviewed and qualified coupled cutover. No merge, deployment or #146
completion follows from this draft.
