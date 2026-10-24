# Held-input lifecycle boundaries

Date: 2026-10-23. Branch: `code/state-boundary-review-batch`.
Stack: reviewed draft #312 at `b24a260e`, draft #314 at `a95e0a6d`, then this act.
Authority: merged #276, `weaver-harness-Spec-amendment`
HARNESS-HELD-BOUNDARY. This is implementation evidence for issue #146,
not a new protocol or production qualification. Git and the evidence deposits
retain the superseded source, measurements and review records.

## Behavior and scope

An untaken caller original makes new owner admission, release, Enter rollback,
member-capture preparation and final close return recoverable Busy when custody
is the only unmet condition. The private HeldInput identifies Caller, Template
or Both. Genuine phase, proof, ownership, bound and coordinator defects retain
their own refusals. Liveness observes independent failures before Busy, and a
previously latched fatal cause remains first. Taking an exact original removes
only its custody hold; it grants no admission, retry, phase recovery or new time.

A held-only refusal leaves original allocations, pending ticket, resources and
waits unchanged. In particular, it cannot cancel an admitted summary read or
placement. An unheld summary or placement release attempt still revokes future
use before its later transition checks, even if foreign work or a mismatched
Leave refuses the release. This restores the behavior predating #312's review
corrections. An already completed summary keeps its exact past disposition;
revocation prevents future cache use without rewriting that evidence.

An unfinished summary whose required read has not started reports Busy for a
Template/Both hold: that hold prevents the read from starting. An ordinary
caller hold permits the admitted summary's private read continuation, so its
unsettled account still returns PlanSummaryOwned. A started read also retains
PlanSummaryOwned. Neither case starts cancellation while input remains held.

The three stopped-disposal roads share release_live: original Enter/Leave expiry
permits disposal, and other liveness failures retain priority. Clock reversal
now deliberately uses the common TraceFailed/Author reason and detail
"common clock reversed" on both held and unheld roads. This differs from the
older boot-specific ProtocolFailed/Barrier mapping and unavailable flag.
The common failure latch revokes boot/place/plan use and keeps the run Failed.
Existing Failed liveness gates prevent subsequent use; equivalent private flags
or complete teardown are not claimed. The tests pin this implementation choice;
the Spec has no distinct clock-reversal taxonomy row.

Placement prefix, read-only measurement, request and effect-mark admission
validate genuine defects before the held-only Busy check. Prefix validation
is read-only. Selection validation checks the retained origin, source coverage,
message form and required prior pair coverage before moving selection or burning
IDs; the journal reservation still validates its exact occurrence construction.
Invalid requests retain their existing bounded refusal-notice behavior. Busy
alone creates no trace refusal, reservation, decode send or placement grant.

## Caller and boundary census

| Road | Disposition |
| --- | --- |
| begin_releasing, begin_enter_rollback, close_after_drain, prepare_member_capture | Require genuine context and coordinator prerequisites before held-only Busy; preserve original bound before mutation. |
| begin_boot_open_release, begin_boot_plan_summary_release, begin_place_release | Common stopped-disposal liveness and existing owner/bound refusals. Summary/place preserve unheld revocation, while custody excludes cancellation. |
| certified_template_enter, template_enter_live | Real incomplete/rejected certification keeps TemplateBlocked. The combined completed_template_enter helper is test-only; production boot callers check certification and custody separately. |
| begin_boot_read, begin_boot_open, install_place | Typed Busy carries Some(Caller/Template/Both), ordinary contention carries None. MissingHead/WrongProof precedes custody, with exact proof return. |
| prepare_prefix, begin_place_measurement, begin_place_inner, begin_place_effect_marks | Genuine cut/projection/material/context/proof/wait defects precede custody. Caller-owned request and effect evidence remain exact on refusal. |
| begin_measurement refusal translations inside context_place | Busy(None): earlier custody validation has excluded held input before reaching the decode/measurement admission. |
| Read, filing, measurement, output, repair-pair continuation | Shared private predicate permits an already admitted owner's continuation with an ordinary caller hold; template custody still excludes a new content continuation. |
| Input, repair, freshness, inspection, marks, templates, boot, plan-summary admission, Enter-refusal join | Existing shared held-input guard; no new continuation permission. |
| take_template_certificate | Retains the actual certificate until custody is returned. |
| BootComposition install | Existing Install error retains the complete offered BootPlanSummary. |
| Worker advance_engine prefix and placement retry sites | Prefix Busy still becomes a fault, and retries cannot take held input; carried into the actual caller composition. |
| RollbackComposition::step | Healthy Busy still enters Stopped without its required exit; carried into full lifecycle composition. |

The coordinator's side-effecting prerequisite methods are named require_*,
because a refusal may latch failure. Both the context and the mutating
coordinator entry call them: context must establish precedence before custody,
and direct coordinator callers must remain safe. The duplicate rollback
missing-state branch is removed. Removing the second check or introducing a
new unchecked commit capability is deferred; it would expand this correction
into a different ownership boundary. Capture therefore still scans its frontier
twice on admission.

The five Leave checks retain their explicit local dispositions. A common helper
would need policies for fatal context misuse, boot refusal and summary/place
owner refusals, plus their different arming and clock rules. This act keeps the
short checks at their owning roads instead of adding a policy abstraction.
Existing wrong-bound tests cover them; no deadline renewal is authorized.

## Review disposition

Planner regrade 5637136323 of #312 at b24a260e reported zero fix, twelve batch
and zero new file rows. That reviewed root is unchanged by this child.

| Regrade row | Disposition in this act |
| --- | --- |
| 1 | Restore unheld summary/place cancellation before refused transitions; held cases remain non-cancelling. Actual read settlement and decoder Cancel tests enforce the distinction. |
| 2 | Validate all four placement admission roads before held-only Busy, retaining request and proof ownership. |
| 3 | Open composition carry: worker prefix Busy and placement retry ownership. |
| 4 | Add coordinator-only close and rollback refusals after context prerequisites pass. Protective mutations remove each context-side coordinator requirement. |
| 5 | Report Template/Both custody when it alone prevents an unstarted summary read; ordinary admitted continuation keeps PlanSummaryOwned. |
| 6 | Record and test the common-clock reason change explicitly, including held and unheld boot and summary roads. |
| 7 | Open composition carry: RollbackComposition healthy Busy exit. |
| 8 | Restrict the production-unused combined template helper to tests. |
| 9 | Spell both post-measurement-admission Busy reasons as None. |
| 10 | Name the latching prerequisite methods require_* and remove the duplicate rollback absent-state branch. Retain defensive revalidation for direct coordinator callers as explained above. |
| 11 | Retain per-road Leave checks with their different refusal policies; no shared policy helper in this correction. |
| 12 | Replace appended dated history with this single current record. Prior evidence remains in Git and the deposits. |

## Instruments and validation

Eleven new tests combine real recorder, state and decode evidence with the three
held-slot cases. They exercise refused unheld cancellation, unstarted summary
custody, placement prerequisite reasons and exact returned selections, missing
coordinator port retirement, split rollback state, and common-clock taxonomy.
The rollback fixture deliberately calls the real coordinator transition without
advancing context phase, isolating the defensive split-state check. Invalid
request fixtures use genuine foreign evidence or remove a prerequisite; they
create no successful capability. Held-slot fixtures retain exact unaccepted
originals, not successful author tickets.

Final restored-source author checks passed with the pinned toolchain,
CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4, --locked and private target
`/var/tmp/weaver-state-146-boundary-review-batch-target`. Cargo and Git
mutations ran sequentially.

| Gate or instrument | Result |
| --- | --- |
| lock.sh before and after Cargo, formatting, strict harness clippy, diff check | Passed |
| Full harness suite with four test threads | 1,990 passed, including 22 doctests; 183 ignored in the ordinary run |
| Focused boundary_batch_ cases | All 11 passed |
| Delegated gate/startup/input/Observe/Stop/generation/emission | 46/17/19/14/16/20/33 passed; 165 total, serially in isolated scopes |
| Protective mutations | All 13 compiled and failed the intended behavioral tests; every source restored exactly |
| Final source manifest | All 660 tracked/new crate and configuration files verified |

The manifest SHA-256 is `43f8c308b5e328bb916c8a3a6db6821de54d5b35701d30c4c0b562f596ae6dfe`.
Observe and Stop used the actual admin test executable built in the same private
target. Eighteen inherited harness ignores and the separate eight privileged
admin-startup instruments are outside this gate. Serial fixture passes do not
resolve #313's parallel startup/generation clock race. No deployed agent changed.
These author checks are not a Planner grade or production qualification.
 The first focused run compiled and passed ten tests; its one
failure was a fixture expecting generic Cancelled extraction from boot summary.
Boot cancellation retains a stopped rollback account. The corrected fixture
uses that API and all eleven focused tests pass. Its exact initial source and
logs remain in the deposit.

## Remaining work and after-action

Draft #314 implements move-owned AuthorFailure and migrates its current internal
callers. Its evidence record is state-management-author-failure-2026-10-24.md.
The Planner review of #314 at a95e0a6d found two fix rows: a claimed continuation
can resubmit a withdrawn original, and an exempt internal author can overwrite
an occupied held slot. Both remain owed by the next stack act; this boundary
correction neither changes those roads nor claims the parent ready to merge.
This boundary correction does not complete worker/dev_loop, lifecycle Busy
exits, failed-run drain, final world close, production artifacts, measured limits,
security disposition or coupled cutover. Those remain under #146. The legacy
serving path stays active; only the operator merges or deploys. All new PRs
remain drafts and CodeRabbit requests remain operator-controlled.

The Executor's code-review and verification-before-completion skills are absent
from this environment; no invocation or independent grade is claimed. The
Planner and operator still review the completed draft at its exact head.

The useful correction from this review was to test competing invalid conditions,
not just healthy Busy. Context-only invalid cases could not prove coordinator
ordering, and moving a cancellation below all prerequisites silently changed
unheld behavior. The new instruments separate custody-only refusal, genuine
prerequisite refusal and already-owned work, using actual continuation effects
and exact returned allocations as their witnesses.
