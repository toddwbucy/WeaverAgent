# Leave and save composition

Date: 2026-10-26. Author working record for issue #146.

## Plan and authority

This act is stacked on #347 at
`534eca304ad4d28d86a423093e7351b7002867d1`, on
`code/state-leave-save-composition`. It composes forced and graceful Leave
through the worker, capture parent, normal SQLite state member and admin
publisher. It also carries the paired capture representation used by those
actors. This record describes source snapshot `85e0fbd5` below.

Implements: harness Spec section 6 Leave, Quiesce and wind-down; admin Spec
section 3 unload and force-unload; foundation PERSIST-SAVE, PERSIST-BEGIN,
PERSIST-TIME, PERSIST-EXPIRY and PERSIST-PUBLICATION-GUARD; persistence contract
amendment sections 2-3 PERSIST-CAPTURE, PERSIST-CONFIRM, PERSIST-END,
PERSIST-IMAGE and PERSIST-CLEANUP; HARNESS-CAPTURE, HARNESS-CLOCK,
HARNESS-HELD-BOUNDARY, HARNESS-HALT and HARNESS-HALT-BOUND; TYPES-PERSIST;
ADMIN-SAVE and ADMIN-RECOVERY; STATE-CIRCUIT-INSTRUMENT; Spec 05 pair
accounting and Spec 06 placement/generation cancellation.

The author assessment is conforms at the exercised boundaries in the table.
It is not a claim of complete conformance for these sections. The outstanding
composition and authority rows below remain part of #146.

**Do not deploy or select foundation.** Public worker serving remains
`dev_loop::drive`; the replacement admin invocation is dormant. The existing
serving implementation remains active until the qualified coupled cutover.

| Authority and boundary | Implemented behavior and evidence |
| --- | --- |
| TYPES-PERSIST; persistence contract 2 | Strict capture envelope checks direction, shape and identity. The single lifecycle reader retains raw packets, descriptors and actual send receipts. Actors judge phase, original exchange and absolute windows. Codec, transport and publication tests cover malformed and mismatched packets. |
| HARNESS-CAPTURE; PERSIST-CAPTURE | Capture retains the greatest actual Recorded trace watermark independently of content position. It freezes once and waits for owed content filing. Later observations cannot move the frozen boundary. Recorder and parent tests pin the distinction. |
| PERSIST-BEGIN / TIME / EXPIRY; HARNESS-CLOCK | Whole Beginning precedes the barrier. The exact prearmed member window is capped by the existing Leave tail. Graceful drain and wind-down precede that tail; force preserves their original owners. No join renews the accepted Leave or capture deadline. |
| PERSIST-CONFIRM / IMAGE / CLEANUP | The parent retains requested, confirmed, published and refused authors, member terminals and A/B identity. Recorded confirmation precedes whole Commit; actual cleanup and timely CaptureComplete precede the final report. Pre-Commit admin EOF cancels; post-Commit failure retains failed-run evidence. |
| ADMIN-SAVE / RECOVERY; PERSIST-PUBLICATION-GUARD | The dormant invocation holds the real invocation lock, checks the paired observation and indexed loaded copy, publishes through descriptors, waits for the exact final outcome, ends the actual run and concludes under existing marker rules. Pending or broken guards and original packets survive failure. Workerless recovery contains the run without promoting room files. |
| Harness 6 Leave and Quiesce | Forced and graceful owners retain actual Lower/Quiesce control identity, original input, accepted capability, author tickets, engine work and joins. Forced drain and Quiesce conversion settle through the same Lower/capture tail. Malformed acknowledgements retain the typed refusal. |
| Harness 6 wind-down; Spec 05; Spec 06 | Wind-down owns its internal turn without a world request or reply. Force retains the exact instruction, filing, placement, generation, repair/correction and pair accounts; it starts no new model work or dispatch. Already admitted or Recorded closes retain their original outcome. The prior input engine waits until the internal successor settles. |
| HARNESS-HELD-BOUNDARY / HALT / HALT-BOUND | Unadmitted caller/template holds still refuse a consuming handoff unchanged. Failed cleanup retains actual placement grants, first failure and original caps. This snapshot does not complete the production taker for held originals. |
| STATE-CIRCUIT-INSTRUMENT | The harness instrument spawns the normal SQLite state binary across its normal descriptors and typed requests, without linking the state crate. Save image readback checks source content and pair identities, statement bindings, original raw payloads and terminal fates. Model and classifier peers remain declared doubles. |

## Implementation map

| Owner | Main files |
| --- | --- |
| Paired representation | `crates/weaver-types/src/state_flow/capture.rs`, `persistence.rs` |
| Actual Recorded boundary | `crates/weaver-harness/src/flow/recording.rs`, `record_owner.rs`, `context_capture.rs` |
| Lifecycle transport and capture parent | `pump.rs`, `pump_transport.rs`, `context_leave_capture.rs`, `context_rollback_listener.rs` |
| Forced Leave and terminal cleanup | `context_worker_forced_leave.rs`, `context_worker_entered_rollback.rs` and its handoff/terminal modules |
| Graceful drain, Quiesce and wind-down | `context_worker_graceful_leave.rs`, `context_worker_graceful_quiesce.rs`, `context_worker_wind_down.rs`, `context_worker_wind_down_cancel.rs` |
| Worker orchestration | `crates/weaver-harness/src/bin/worker/dev_loop/foundation_session.rs`, `foundation_turn.rs`, `foundation_wind_down.rs` |
| Admin capture and publication | `crates/weaver-admin/src/channel_capture.rs`, `serving_capture_window.rs`, `serving_leave.rs`, `serving_leave_invocation.rs`, `serving_leave_join.rs`, `serving_leave_response.rs`, `save_points/serving/repository.rs` |
| Normal member circuit | `crates/weaver-harness/tests/state_flow.rs` and its `state_flow/` fixtures |

The complete diff is against #347. The wind-down checkpoint is the last
verified increment of this composition, not its entire implementation.

## Verification

The final snapshot's ordinary harness suite passes 2,105 tests and doctests;
696 process/inherited cases are ignored by that invocation. A separate,
delegated run passes these 229 cases. These counts overlap earlier checkpoint
runs and must not be added to them as unique coverage.

| Delegated family | Passed |
| --- | ---: |
| Graceful tail | 45 |
| Wind-down | 19 |
| Quiesce | 12 |
| Drain | 10 |
| External pairs | 64 |
| Sessions | 59 |
| Stop | 17 |
| Forced admin/worker/SQLite joint | 3 |

The last increment adds 18 full SQLite force/expiry circuits: unstarted,
start author, instruction, held filing, measurement, placed grant, generation,
generated output, repair, restatement, correction placement, correction
generation, pair filing, closing, close author, generation expiry, retained
prior generation and expiry after an actually Recorded clean close.

Strict harness clippy for all targets, formatting, diff and lock gates pass.
All 770 crate/configuration source hashes were checked after final gates and
again in the isolated publication checkout. No runtime source changed during
publication.

The 538-pass / 65-ignored admin suite is reused from the preceding checkpoint
because all admin and floor source hashes are unchanged. Its strict admin
clippy and root invocation/join checks remain evidence for that source, not a
new admin run. Earlier types checks passed 186 default and 244 config-feature
tests/doctests, with strict types clippy. Their source lineage is retained in
the evidence. No new workspace-wide, SPU or GPU gate is claimed here.

Nine semantic wind-down controls compiled and failed at inspected assertions.
One narrower mutation survived: removing only the explicit cancellation call
leaves cancellation through `revoke_place` and `advance_generation`. It is
retained and not counted as detected. Removing both triggers is detected by
the fixture watchdog, which proves loss of progress, not a measured production
deadline. Skipping pair accounting fails the actual owned-content transaction
guard. Exact sources and the library executable were restored; generation,
pair filing, prior-turn ownership and post-Recorded closure passed again.

Earlier capture-parent, admin, drain and Quiesce controls retain their own
source snapshots and logs. They are not represented as rerun on the final
snapshot. Development failures and instrument corrections remain preserved.

All runs use pinned `nightly-2026-02-13`, locked dependencies and private
target `/tmp/weaver-state-146-worker-session-target`, with
`CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=4`. Cargo and Git mutations are
serialized. Delegated process families use one test thread in isolated
root-owned scopes. No deployed agent was changed.

These are author checks. Planner review, security disposition, production
artifacts, measured limits and qualification remain outstanding. The required
`code-review` and `verification-before-completion` skills are unavailable in
this session; manual checks do not claim those invocations.

## Evidence

The immutable deposit is
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-graceful-winddown-convert-20261026-85e0fbd5/`.
Its 619 regular files were copied with zero skips and independently reread.

- Complete source archive SHA-256:
  `85e0fbd5e07c76097e61d2a150d1fb9215c8cb10bbd396c638139236549b816a`.
- Source manifest SHA-256:
  `878aeeae13c292f29b74b88607dc365936acfb3f4594d70ac04a7de76f2b56fd`.
- Deposit manifest SHA-256:
  `aff93dcbb564368db38f293926918b6f7ada925c5c14fdd5cbcb40152fdba303`.

The source archive contains 1,023 files. This review record was added during
publication; it is not in that earlier source archive. The publication
checkout is `/tmp/weaveragent-leave-save-publication-20261026`.

## Remaining work and after-action

| Work | Disposition |
| --- | --- |
| Exact caller/template held-original handoff | Continue as the next composition. The newer unverified experiment is preserved separately and excluded from this snapshot. |
| Healthy capture-refusal final Leave outcome | Proposed in #348; dependent implementation waits for its existing operator/Planner/merge gates. |
| Forced admin admission without verified loaded facts | Proposed in #349; dependent implementation waits for its existing gates. |
| On-demand save and complete serving selection | Still open under #146. This act implements unload capture and leaves the selected serving path unchanged. |
| Remaining review findings, language/prompt artifacts, limits and cutover | Still open under #146; no broad checklist item closes. |

The planned end-to-end Leave/save work exposed ownership joins that primitive
tests did not establish. The real SQLite image reader initially assumed no
pairs, and the completed-session fixture could not drive its retained
management owner; both instruments now exercise the intended boundary.
Preserve exact owner and image checks when extending this composition.

Keeping verified increments unpublished delayed independent review. Publish
the tested composition while naming the unfinished paths, and keep later
experiments separate until their own checks pass. Only the operator merges,
deploys or requests CodeRabbit.
