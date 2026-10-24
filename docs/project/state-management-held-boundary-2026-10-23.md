# Held-input lifecycle boundaries

Date: 2026-10-23. Branch: `code/state-author-review-fixes`.
Stack: reviewed draft #312 at `b24a260e`, draft #314 at `a95e0a6d`,
draft #315 at `f372720a`, then this act.
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
PlanSummaryOwned. An earlier unheld attempt may already have cancelled an
unstarted read; that account also returns PlanSummaryOwned because it can settle
without taking the held original. None starts cancellation while input is held.

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
first exclude their operational owners. Their side-effect-free proof, context
and wait refusals precede held-only Busy. Refusals that install a notice owner
must wait until custody is clear: InvalidCut, stale placement prefix and invalid
selection may otherwise create a second owner, arm Cut or close an active turn.
Both operational and custody Busy leave place_notice, place_owed and deadlines
unchanged and grant no trace refusal, reservation, decode send or placement.

Measurement projection, schema and selection checks are read-only; they follow
operational exclusion and precede held-only Busy. They burn no IDs and install
no notice. Placement uses the journal reservation's existing full validation
and single error mapping. The duplicate selection precheck is removed rather
than repeating coverage scans and the InvalidPair/InvalidSpan mapping.

## Caller and boundary census

| Road | Disposition |
| --- | --- |
| begin_releasing, begin_enter_rollback, close_after_drain, prepare_member_capture | Require genuine context and coordinator prerequisites before held-only Busy; preserve original bound before mutation. |
| begin_boot_open_release, begin_boot_plan_summary_release, begin_place_release | Common stopped-disposal liveness and existing owner/bound refusals. Summary/place preserve unheld revocation, while custody excludes cancellation. |
| certified_template_enter, template_enter_live | Real incomplete/rejected certification keeps TemplateBlocked. The combined completed_template_enter helper is test-only; production boot callers check certification and custody separately. |
| begin_boot_read, begin_boot_open, install_place | Typed Busy carries Some(Caller/Template/Both), ordinary contention carries None. MissingHead/WrongProof precedes custody, with exact proof return. |
| prepare_prefix, begin_place_measurement, begin_place_inner, begin_place_effect_marks | Operational Busy first, then side-effect-free genuine checks, then held-only Busy. Notice-producing cut/prefix/selection refusals require clear custody. Offered request and effect evidence remain exact on Busy. |
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
and direct coordinator callers must remain safe. The rollback missing-state
branch remains a bounded fail path, not an expect panic, even though the preceding
requirement excludes it today. Removing the second check or introducing a
new unchecked commit capability is deferred; it would expand this correction
into a different ownership boundary. Capture therefore still scans its frontier
twice on admission.

Placement release snapshots ownership before liveness only to decide whether
an unheld attempt must revoke future use. It rechecks current ownership after
release_live before admitting the transition; a settled account cannot pass
using stale pre-liveness debt.

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
| 2 | Apply the corrected #315 review ordering: operational Busy, pure checks, held-only Busy, then notice-producing validation. Retain exact offered evidence on Busy. |
| 3 | Open composition carry: worker prefix Busy and placement retry ownership. |
| 4 | Add coordinator-only close and rollback refusals after context prerequisites pass. Protective mutations remove each context-side coordinator requirement. |
| 5 | Report Template/Both custody when it alone prevents an uncancelled unstarted summary read; an already cancelled account or admitted continuation keeps PlanSummaryOwned. |
| 6 | Record and test the common-clock reason change explicitly, including held and unheld boot and summary roads. |
| 7 | Open composition carry: RollbackComposition healthy Busy exit. |
| 8 | Restrict the production-unused combined template helper to tests. |
| 9 | Spell both post-measurement-admission Busy reasons as None. |
| 10 | Name the latching prerequisite methods require_*. Retain defensive revalidation and the bounded missing-state failure for direct coordinator callers as explained above. |
| 11 | Retain per-road Leave checks with their different refusal policies; no shared policy helper in this correction. |
| 12 | Replace appended dated history with this single current record. Prior evidence remains in Git and the deposits. |

Planner review 5637284596 of #315 corrects the earlier broad prerequisite-order
request. All eight numbered findings are handled in this child; the reviewed
parent remains unchanged pending independent review of these corrections.

| #315 review row | Disposition |
| --- | --- |
| 1, fix | Restore operational Busy before placement defects; exclude held custody before creating a refusal owner. Tests assert unchanged notice, debt and waits with active generation and held originals. |
| 2 | Restore operational Busy before native-mark and measurement checks. Actual active native work retains its owner and rejects a foreign proof with Busy; active generation excludes measurement of an invalid proposal. |
| 3 | Outstanding notice is caught by operational Busy with the actual held-slot reason, before InvalidCut can lose it. |
| 4 | An already cancelled unstarted summary retains PlanSummaryOwned and settles without a held-input take. |
| 5 | Restore the bounded missing-state failure branch in coordinator rollback. |
| 6 | Re-evaluate placement ownership after release_live. |
| 7 and 8 | Remove the duplicate selection precheck; reservation owns the single validation pass and refusal mapping. |

## Instruments and validation

The eleven boundary_batch_ cases use real recorder, state and decode evidence
with all three held-slot cases. They cover refused unheld cancellation,
coordinator-only close/rollback defects, common-clock taxonomy and exact offered
proofs. The corrected prefix/request cases now inspect place_notice, place_owed
and all deadlines, rather than merely unchanged journal IDs and originals.

Three additional author_review_ boundary tests run an actual pending generation,
an actual native-mark owner, and an already cancelled unstarted boot-summary
read. The generation fixture combines both a foreign prefix and foreign units
with operational Busy; its invalid Cut must install no notice or new wait, and
no Refusal or TurnClosed may appear. The native fixture preserves the exact
active owner and offered read allocation. The summary fixture settles its
cancelled account without taking any held original, then encounters custody
Busy at the release boundary itself. No fixture manufactures a successful
capability or receipt.

The three author-custody tests in state-management-author-failure-2026-10-24.md
share this child and final gate run. Earlier exact sources and failing fixture
runs remain in Git and their evidence deposits.

Final restored-source checks passed with the pinned toolchain,
CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4, --locked and private target
`/var/tmp/weaver-state-146-author-review-fixes-target`.

| Gate or instrument | Result |
| --- | --- |
| lock.sh before/after Cargo, formatting, strict harness clippy and diff check | Passed |
| Full harness suite with four test threads | 1,996 passed, including 22 doctests; 183 ignored in the ordinary run |
| Focused author_review_ and boundary_batch_ cases | All six new and eleven corrected existing cases passed |
| Delegated gate/startup/input/Observe/Stop/generation/emission | 46/17/19/14/16/20/33 passed; 165 total, serially in isolated scopes |
| Protective mutations on the final combined source | All eleven compiled and failed the intended tests; every source restored exactly |
| Final source manifest | All 660 tracked crate/configuration files verified |

The source manifest SHA-256 is `b2b203c2e4d35c9668edc3a98b934d7831e3f02e9d7ef085ccedf960af7773a8`.
Observe and Stop used the actual admin test executable rebuilt in this private
target. Eighteen inherited harness ignores and eight privileged admin-startup
instruments remain outside this gate. Serial passes do not close #313's parallel
startup/generation clock races. No Cargo or Git mutations overlapped, deployed
agent changed, or CodeRabbit review was requested. These author checks are not
an independent Planner grade, security disposition or production qualification.

## Remaining work and after-action

Draft #314 implements move-owned AuthorFailure and migrates its current internal
callers. Its evidence record is state-management-author-failure-2026-10-24.md.
This child corrects its two blocking custody findings as well as #315's notice
admission finding. These are author corrections awaiting an independent grade;
the parent stack is not declared ready to merge from these checks alone.
This boundary correction does not complete worker/dev_loop, lifecycle Busy
exits, failed-run drain, final world close, production artifacts, measured limits,
security disposition or coupled cutover. Those remain under #146. The legacy
serving path stays active; only the operator merges or deploys. All new PRs
remain drafts and CodeRabbit requests remain operator-controlled.

The Executor's code-review and verification-before-completion skills are absent
from this environment; no invocation or independent grade is claimed. The
Planner and operator still review the completed draft at its exact head.

The review exposed why validation ordering must include its effects. A refusal
that installs a notice is owner admission even when no placement is granted.
The corrected instruments inspect that owner, its debt and original deadlines
as well as unchanged journal IDs and offered allocations. Pure prerequisite
checks can precede custody; notice-producing checks require clear admission.
