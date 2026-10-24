# Held-input lifecycle boundaries

Date: 2026-10-23. Branch: `code/state-author-custody-composition`.
Base: consolidated main `f8ca92ef5e32e298239f562b3a65b5a8811e712e`.
Authority: merged #276, `weaver-harness-Spec-amendment`
HARNESS-HELD-BOUNDARY. This is implementation evidence for issue #146,
not a new protocol or a production qualification.

## Behavior and scope

An untaken caller original used to make release, Enter rollback, member-capture
preparation or final close latch ProtocolFailed. These boundaries now return
recoverable Busy before their phase, resource, ticket or wait mutation. The
private HeldInput reason identifies Caller, Template or Both. Taking an
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
boot Open return their existing typed Busy cases rather than Templates or
MissingHead. Placement installation returns the exact offered BootPlanSummary
with Busy rather than WrongProof. No template grant, boot proof, read request,
decoder exchange or placement grant is fabricated by these refusals.

One shared held-input predicate now also governs the existing private read,
filing, measurement, output and repair-pair continuation exemptions. It preserves
their prior rule: an ordinary held caller cannot stop an already admitted
owner's exact continuation; a template-held caller still blocks a new content
continuation. The predicate accepts no public continuation election.

## Caller and boundary census

| Road | Disposition in this slice |
| --- | --- |
| `begin_releasing`, `begin_enter_rollback`, `close_after_drain`, `prepare_member_capture` | Shared RunFailure::Busy reason before boundary mutation, after liveness. |
| `begin_boot_open_release`, `begin_boot_plan_summary_release`, `begin_place_release` | Same held-input reason and liveness precedence before cancellation/release changes. Existing owned drain rules remain. |
| `completed_template_enter`, `template_enter_live` | Distinguish custody from real certification failure; retain first cause and original Enter expiry. |
| `begin_boot_read`, `begin_boot_open`, `install_place` | Existing typed Busy family; retain request/proof ownership instead of misreporting a missing or invalid prerequisite. |
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

Final restored-source author gates passed:

- `process/gates/lock.sh`, `cargo fmt --all -- --check`, and `git diff --check`.
- `cargo test -p weaver-harness --locked -- --test-threads=4`: 1,959 passed,
  including 22 doctests; 183 ignored. The 165 delegated instruments among these
  ignored cases were not rerun for this slice; this is not their qualification.
- `cargo clippy -p weaver-harness --all-targets --locked -- -D warnings`.
- All 13 focused tests passed before the mutation run. All 14 mutations
  compiled and failed the matching behavioral test. The final full suite
  passed after exact restoration.

All Cargo runs used the pinned toolchain, a private target and `--locked` where
dependencies resolve. The 268 harness Rust files matched their recorded
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
