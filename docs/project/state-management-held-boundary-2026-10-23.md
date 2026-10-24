# Held-input lifecycle boundaries

Date: 2026-10-23. Branch: `code/state-author-custody-composition`. PR: #312.
Base: consolidated main `f8ca92ef5e32e298239f562b3a65b5a8811e712e`.
Authority: merged #276, `weaver-harness-Spec-amendment`
HARNESS-HELD-BOUNDARY. This is implementation evidence for issue #146,
not a new protocol or a production qualification.

## Behavior and scope

An untaken caller original used to make release, Enter rollback, member-capture
preparation or final close latch ProtocolFailed. These boundaries now return
recoverable Busy when held input is the sole unmet condition, after their
ordinary prerequisite checks and before phase, resource, ticket or wait mutation.
The private HeldInput reason identifies Caller, Template or Both. Taking an
original removes only its custody hold. Every retry still passes the same
phase, proof and original-deadline checks.

Required liveness checks precede the hold. A previously latched cause or an
independent failure observed during the call wins over Busy. Taking input in
Failed cannot restore a healthy phase. An expired original Leave or capture
window is not replaced. Both originals remain the same allocations when both
slots are occupied.

Completed template certification remains valid while unrelated caller input is
held; it returns Busy rather than claiming certification failed. Genuine
incomplete or rejected certification keeps TemplateBlocked. Boot read and
boot Open return typed Busy(Some(HeldInput)) for custody and Busy(None) for
other busy causes. Placement installation returns the exact offered
BootPlanSummary with the same held-slot reason. Real template/head/proof
failures keep their original reasons even when input is also held. No template grant, boot proof, read request,
decoder exchange or placement grant is fabricated by these refusals.

One shared held-input predicate now also governs the existing private read,
filing, measurement, output and repair-pair continuation exemptions. It preserves
their prior rule: an ordinary held caller cannot stop an already admitted
owner's exact continuation; a template-held caller still blocks a new content
continuation. The predicate accepts no public continuation election.

## Caller and boundary census

| Road | Disposition in this slice |
| --- | --- |
| `begin_releasing`, `begin_enter_rollback`, `close_after_drain`, `prepare_member_capture` | Shared RunFailure::Busy reason after liveness and ordinary context/coordinator prerequisites, before boundary mutation. |
| `begin_boot_open_release`, `begin_boot_plan_summary_release`, `begin_place_release` | Shared stopped-disposal liveness admits original Enter/Leave expiry, observes other failures, then checks owners/bounds before held-only Busy or release changes. |
| `completed_template_enter`, `template_enter_live` | Distinguish custody from real certification failure; retain first cause and original Enter expiry. |
| `begin_boot_read`, `begin_boot_open`, `install_place` | Typed Busy carries Caller/Template/Both; real invalid prerequisites precede the hold. Request/proof ownership stays exact. |
| Read, filing, measurement, output, repair-pair intake | Shared private predicate retains the existing ownership exemptions. |
| Input, repair, freshness, inspection, mark, template, boot and plan-summary admission; prefix/placement admission; Enter-refusal join | Existing Busy guards use the shared held-input predicate. No new admission permission. |
| `take_template_certificate` | Existing held-input check keeps the actual certificate until custody is returned. |
| `BootComposition::advance_retained` install arm | Existing Install error owns the returned BootPlanSummary and the stopped composition owns its context/resources. No successful boot is fabricated. |
| `RollbackComposition::step` release arm | Existing error path retains the complete rollback owner and its original Leave; it cannot produce Prepared from Busy. |

The four named lifecycle APIs are internal. Capture and final-close composition
are still incomplete under #146. This slice does not add an autonomous retry
loop, a new teardown wait, a model-facing refusal or an active serving caller.

## Instruments

Thirteen new tests exercise the three occupied-slot cases, real release and
refused-Enter prerequisites, Recorded Unload with real writer drain, real
capture requests, completed template checks, retained boot reads and decoder
admission, exact BootPlanSummary return, and placement release with a pending
decoder exchange. Cases include repeated Busy, take and successful retry,
take and expired retry, an already latched cause, and an independent clock
failure observed at the call. Pointer and full-value checks pin exact originals.

The shared slot fixture deliberately installs unaccepted event inputs to
isolate all three boundary states, including Both. It creates no recorder
ticket or successful proof. Existing integration tests separately reach the
ordinary and template slots through actual author calls and complete admitted
mark, repair, summary, read, output and measurement continuations.

Fourteen compiled mutations must fail: bypass each of the four boundary checks;
relatch fatal release; hide an independent failure; renew Leave during Busy;
stall an existing private owner; allow a template-held continuation; replace
the first template failure; misreport installation as WrongProof, boot read as
Templates, boot Open as MissingHead, or placement release as PlaceOwned.
Every mutated source is restored before the final gates.

Initial-head (04e5532a) restored-source author gates passed:

- `process/gates/lock.sh`, `cargo fmt --all -- --check`, and `git diff --check`.
- `cargo test -p weaver-harness --locked -- --test-threads=4`: 1,959 passed,
  including 22 doctests; 183 ignored. The 165 delegated instruments among these
  ignored cases were not rerun for this slice; this is not their qualification.
- `cargo clippy -p weaver-harness --all-targets --locked -- -D warnings`.
- All 13 focused tests passed before the mutation run. All 14 mutations
  compiled and failed the matching behavioral test. The final full suite
  passed after exact restoration.

Those initial-head Cargo runs used the pinned toolchain, `--locked` where
dependencies resolve, and the private target
`/home/todd/Projects/WeaverTools_Project/WeaverAgent/target/state-146-held-boundary`.
That target was inside home, contrary to the outside-home rule for delegated
fixtures; the ignored lanes were not qualified by that run. The review-correction
gates below use `/var/tmp/weaver-state-146-held-boundary-review-target`. The 268 harness Rust files matched their recorded
SHA-256 manifest after the final gates. The manifest SHA-256 is
`3ac5d3b3942b03975b471154156436048dbf831c05610f01fd887327173fe149`.

The first sandboxed run was refused at Unix socket creation before the code
was exercised. The first socket-enabled run found a test using an active-only
recorder inspector on a correctly refused preflight recorder; that instrument
now reads the actual Recorded sequence. The initial full suite also found an
old fatal-boundary expectation, updated to the approved Busy rule, and a
trace-pressure fixture hit EAGAIN during queue filling. That fixture passed
alone and the corrected full suite passed with four test threads before the
last placement-release addition. These earlier logs remain evidence, not the
final gate result.

## Remaining work and review boundary

The move-owned AuthorFailure migration in HARNESS-RECORD-LIFETIME is the next
connected slice. In particular, #255 Planner review 5636484478 row 1's second
blocked-author overwrite, third-attempt return, resume-withdrawal custody and
all production author callers remain open. This boundary work does not claim
that finding closed. The original six-site census has grown: startup Load and
worker fault, Stop closure and emission refusal authoring also require the
new ownership result. Private pair, placement, measurement, Enter-refusal,
filing/output and retained-freshness paths must preserve their exact returns.

The complete failed-run drain, healthy lifecycle composition, remaining worker
workflow, production artifacts, measured limits, security disposition and
coupled cutover remain tracked by #146. Existing serving stays active. Only the
operator merges or deploys, all new PRs remain drafts, and CodeRabbit requests
remain operator-controlled.

Author checks are not a Planner grade. The code-review and
verification-before-completion skills are absent from this environment after
filesystem/tool discovery; no invocation or independent review is claimed.
The draft still requires the repository's remaining review seats.

After-action: the plan separated boundary semantics from the consuming-author
result migration. The consumer walk found two more misleading boot-admission
translations and the separate placement-release entry; all are included here.
Keep that complete consumer walk for the next signature change, and record
environment/fixture failures separately from restored behavioral evidence.


## Planner review corrections, 2026-10-24

Reviewed base: #312 at 04e5532a, tree 5f90d2f7. These corrections implement
the existing HARNESS-HELD-BOUNDARY rule; no new operator election is needed.
All changes remain in the dormant foundation path. The AuthorFailure caller
migration is preserved in its separate, unpublished child worktree.

| Planner row | Disposition |
| --- | --- |
| 1, fix | The four lifecycle boundaries validate genuine context and coordinator prerequisites before the pure held-input check. Release and rollback check the original Leave identity before Busy and arm only after admission. Shared coordinator checks are reused by the mutating entry points, avoiding a second predicate that can drift. New cases combine all three holds with invalid phases, an open turn, missing refusal or Unload, changed bounds, and invalid capture identity/window. Both original allocations remain owed after ProtocolFailed. |
| 2, 3, 8 | All three private release roads use one stopped-disposal liveness policy on held and unheld paths. Original Enter/Leave expiry permits stopped disposal; other observed failures retain priority. Existing ownership and bound refusals precede held-only Busy. An admitted release cancels/revokes only after taking custody has ceased to block it. An unfinished summary still needs cancellation/drain and returns PlanSummaryOwned under its existing account cap. |
| 4 | Genuine template certification is checked separately from custody. Incomplete/rejected certification retains Templates/TemplateBlocked. A completed certificate plus template-held input yields the held reason. |
| 5 | Real missing BootHead and stale BootPlanSummary keep MissingHead/WrongProof, with exact proof return, before custody is considered. |
| 6, carry | RollbackComposition still has no healthy Busy retry/exit disposition. Its preceding Enter-refusal join excludes held input today, and no serving binary constructs it. The complete worker/lifecycle composition in #146 must make this disposition explicit and prove retry or bounded failed exit before wiring that route; this slice does not claim it resolved. |
| 7 | BootReadBeginRefusal, BootOpenBeginRefusal and PlaceError carry Busy(Option<HeldInput>). Some identifies Caller, Template or Both; None keeps ordinary owner/decode pressure distinct. All constructors and comparisons are migrated; the worker generation caller accepts either Busy disposition without reconstructing a proof. |
| 9 | Boot Open and plan-summary release now test latched failure, independently observed plan failure, backward clock, original Enter expiry, original Leave expiry, and owner absence alongside all three held-slot states. Place release adds owner/expired-Leave cases to its existing failure instrument. |
| 10 | check_held_input is a pure custody predicate. Each entry runs its own required liveness first; the three stopped-disposal roads share release_live. No unreachable second latch check is credited as enforcement. |
| 11 | preinput_cleanup_domain now uses the shared held-input predicate. |
| 12 | Initial and current target directories are named above. The absent code-review and verification-before-completion skills remain a disclosed process carry; no unavailable invocation or independent self-review is claimed. |
| 13, filed | The pre-existing parallel worker fixture timestamp race is tracked by #313. Its reported failure is not a clean qualification result; production timestamp validation is unchanged. |

The caller walk covers the four lifecycle boundaries, their coordinator checks,
the three private release roads, template completion, boot read/Open, placement
installation, and every typed Busy constructor/consumer. Placement prefix,
measurement, placement-begin and effect-mark Busy sites
carry the same optional held identity; actual decode/reservation contention
without a held caller retains None. This signature change adds no serialized
field or public protocol.

The review fixtures add twelve tests to the initial thirteen. They use actual
context/recorder/transport prerequisites and isolate held slots through the same
unaccepted-original fixture. Deliberately invalid phase/turn/window variants
exercise the refusal, not a successful capability. The new tests and exact
original-pointer checks do not qualify full failed-run cleanup or production
artifacts.

The first full run found the older admitted-summary fixture still expecting
Busy where the unfinished account independently requires PlanSummaryOwned.
The correction preserves that reason and returns before cancelling the admitted
read when input is also held. The updated fixture then completes its actual
Boot/effect refresh with the exact competing original still retained. Separate
compiled mutations pin both the owner reason and the absence of cancellation.
The earlier failing run and its exact source are retained with the gate evidence.

The typed-result census is finite:

| File and consumers | Held/ordinary disposition |
| --- | --- |
| context_boot_read::begin_boot_read | Certified custody refusal carries Some(HeldInput); underlying read contention carries None. |
| context_boot_open::begin_boot_open | Held slot carries Some; non-custody owner pressure and decoder reservation contention carry None. |
| context_place::{install_place, prepare_prefix, begin_place_measurement, begin_place_inner} | Preserve exact proof/request and carry the current held identity at every held-aware Busy return. |
| context_place_marks::begin_place_effect_marks | Carry the same held identity through native-mark admission. |
| context_place_refusal::queue_place_refusal | Existing notice/bound contention carries None; this is not held-input admission. |
| context_place_generation::begin_generation | Actual decoder reservation contention carries None. |
| context_place_reload::{begin_cut_and_reload, begin_staged_place} | Existing reload/prefix contention carries None; owned continuations retain their prior rules. |
| context_worker_generation::advance_engine (two retry sites) | Match Busy with either optional reason, retaining the existing operation/request owner. |
| context_boot_composition::BootStop | PromptStart, OpenStart and Install retain their complete existing typed error, including the exact offered installation proof. |

Final review-correction author gates passed on the restored source, using
`CARGO_TARGET_DIR=/var/tmp/weaver-state-146-held-boundary-review-target`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4` and the pinned toolchain:

- `process/gates/lock.sh` before and after Cargo: 0; formatting, strict harness
  clippy and diff checks: 0.
- `cargo test -p weaver-harness --locked -- --test-threads=4`: 1,971 passed,
  including 22 doctests, with 183 ignored in the ordinary run. Counts exclude
  duplicate child re-execution summaries.
- Separately, all 165 delegated harness instruments passed serially in their
  own root-owned test scopes: gate startup 46, worker startup 17, input 19,
  Observe 14, Stop 16, generation 20 and emission 33. The actual admin test
  executable was freshly built with `cargo test -p weaver-admin --locked
  --no-run` in this same private target for the Stop/Observe consumers.
  Eighteen inherited harness ignores and the separate privileged admin startup
  suite are not qualified by these runs. Serial generation success does not
  resolve #313's reported parallel fixture race.
- All 25 focused held-boundary tests passed. The final full run also includes
  the corrected admitted-summary continuation fixture.
- Twenty-four compiled protective mutations failed as intended: 19 boundary,
  prerequisite, reason and liveness controls; three additional placement-caller
  controls; two unfinished-summary refusal/cancellation controls. Each run
  restored its exact source. The intervening source revisions and the initial
  failed full run remain retained; final gates ran after the last restoration.
- All 657 tracked crate/configuration source files matched the final gate
  manifest, SHA-256 `f3572b4168d1f77228c11ebeb56f905e4ee207bf9362ee7db497347583a76e79`.

These are author gate results, not a fresh Planner grade, security disposition
or production qualification. #312 remains draft for regrade and the operator's
merge decision. Full AuthorFailure, lifecycle/failed-run composition and the
qualified serving cutover remain open under #146.
