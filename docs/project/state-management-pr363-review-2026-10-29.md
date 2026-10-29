# Consolidated foundation review corrections

Version: 0.1, 2026-10-29.

This act answers #363's Planner CX1, originally Codex on #383, and CodeRabbit's
forced-marker finding and author-test request. The starting head is
e0e12a9d36788a890feba103da8a5083a25c2835. It advances #146's existing composition
and evidence work; general code generation remains paused for stack review.

## Authority and changed surface

Implements: harness amendment HARNESS-PLAN, HARNESS-PLAN-OBSERVE and
HARNESS-AUTHOR; admin-harness contract section 3 Stop/Leave; admin Spec section
3.0 conclusion outcomes and I4; admin amendment ADMIN-SAVE. Author assessment:
conforms on the exercised paths below. No Spec election or wire change is made.

| Site | Change and remaining ownership |
| --- | --- |
| FoundationManagement flags drain | Reduce AdminDial failure through keep_dial_failure. The exact held directive survives; the original observation finishes cancellation under its existing bound. |
| FoundationManagement root plan drain | Apply the same reduction. The accepted Stop continues after the cancelled root update is fully accounted. No replacement operation or deadline is created. |
| Leave invocation conclusion | A validated final Left with forced true, no_state false and no report selects Forced after successful run cleanup. Read the final response, since an initially graceful Leave can turn forced. A completed save still selects Closed. |
| Joined Leave conclusion | The same report-less forced outcome selects Forced under the existing invocation lock and pinned marker checks. Reported saves still require verification. |
| Author census | Add a test-only non-root ManagementInput and require UnlicensedSource for forbidden plan.event sources. Production construction and attribution stay unchanged. |

The management changes are inside the shared Stop/Leave drain branches. Direct
input callers have no secondary AdminDial. Existing organ-settlement, StopWork,
answer and common-pump callers already retain that dial failure. Other transport
failures and a sent answer without its clock witness retain their prior failure
dispositions. The final answer to a disconnected Stop is retained as undelivered;
the test does not treat the terminal Dropped progress as proof that Stop ran.

Both Leave conclusions retain their existing run-end, containment, lock and
Open-marker checks. The owner still completes a validated save before cleanup;
the marker write and Unloaded still require cleanup. A joined caller does not
become another publisher. The final reply supplies termination mode rather than
the initiating invocation's forced flag. No rollback or no_state admission rule
is widened.

## Instruments and review

Two real process regressions close the admin socket at the actual bounded flags
or root-plan drain wait. Before the fix both fail with Dropped where the held
Stop must remain Pending. This is accepted-directive loss, not a demonstrated
foundation run halt. With the fix the flags case retains its original checked
input, records the cancelled measurement before the single turn close and
finishes the original world response. The root case retains its cancelled
account, releases the read/root work and creates no effect or plan.event.

A further refusal-close case fills the real gate socket, retains Stop and closes
its admin peer. It passes with the existing refusal-close handling and pins its
retained AtRest answer. It did not demonstrate a third runtime defect, so this
act changes neither settle_refusal_before_stop nor the refusal-close owner.

The admin instruments cover a report-less force join, the actual forced owner
with worker and SQLite member, and an initially graceful owner converted by a
join. They inspect the actual marker and preserve publication files. Existing
successful saves, graceful retries, guarded failures and marker eligibility
cases remain in the affected-family run.

Six compiled controls independently restore the old flags propagation, restore
the old root-plan propagation, replace the owner's Forced marker with Closed,
use the owner's initiating mode, replace the joiner's Forced marker with Closed,
and remove the author uid guard. Each fails its intended assertion. Every
control restores all 815 Rust/configuration hashes before the next build. The
author control is repeated after narrowing the new error assertion to
plan.event: the full harness gate found that the broader assertion incorrectly
expected UnlicensedSource for the separate SystemPrompt refusal. No runtime
rule changes in that correction. The evidence retains both exact source
cohorts; only author_tests.rs differs among their Rust/configuration files.

The first parallel full-suite run failed the unchanged admin test
precommit_send_peer_closure_drains_exact_refusal_under_original_leave_windows
at preparing/forced (Stopped versus LeftReceived). Its isolated rerun and ten
further isolated runs pass. The failed log is retained; the observation alone
does not establish a cause or justify weakening the fixture. The admin suite
then passes all 606 ordinary tests serially. The unfinished serial harness
portion was deliberately interrupted and is not counted; harness verification
uses its normal parallel runner. Final counts are recorded in the PR and
evidence manifest. Both full-run logs remain in the evidence.

Final author gates pass 2,231 harness tests/doctests, 606 admin tests and 70
affected process identities. The process census names every case rather than
counting nested child summaries. The ordinary harness run leaves 928 ignored
cases and skips known #331; admin leaves 74 ignored cases. Only the named 70
process cases are claimed, not the complete ignored inventory. Both changed
crates pass strict all-target lint; lock, formatting, diff and three DocsMirror
checks pass. The private target, pinned toolchain, --locked commands, source
hashes and all earlier failed attempts are retained in the deposited evidence.

## Dispositions and limits

The Planner's filed races CX2, XH2 and XH9 and batch rows XH1, XH3, XH5-XH8,
XH10 and XH11 are tracked in #402 with their required reproductions or decisions.
XH4 remains declined: report-less Left has no run field and relies on the
authenticated worker and pinned marker. That run-identity question is separate
from the marker-kind correction here. Inherited stack carries remain open.

The author reviewed the rework against the original bounds, custody and marker
rules. Author checks are not a Planner grade, security disposition or production
qualification. The complete ignored inventory, full workspace and GPU lane are
not claimed. Keep dev_loop::drive selected; only the operator merges #363.

The initial review plan was the two drain sites and the marker correction. The
same-class audit added one existing-behavior instrument, and the author census
answered CodeRabbit's test request. The unmodified refusal-close path passing
its instrument prevented an unnecessary runtime edit. Keep reproductions and
protective controls separate from source-only findings in later review acts.
