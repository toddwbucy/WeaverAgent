# World request refusal after SPU loss

Date filed: 2026-10-27. Author record for issue #146.
Branch: code/state-world-request-composition. Base: main at 0043f1de.

This act implements merged #358's INPUT-NO-RESIDENCY and TYPES-WORLD-REQUEST
through the existing input, trace and worker owners. The foundation dispatcher
remains unselected. Do not deploy the incomplete replacement.

## Authority and behavior

Implements: Spec 02 section 5 INPUT/INPUT-NO-RESIDENCY; types amendment section 4
TYPES-WORLD-REQUEST; harness amendment HARNESS-INPUT/HARNESS-CLOCK;
weaver-gate-world-contract section 5; admin Spec section 3.0 S2(Dead) and
section 3(B), including the later graceful Leave.

Author assessment: conforms for the roads below. These verdicts do not grade the
remaining sections of the foundation, or replace the Planner's review.

After observed SPU loss, an eligible world request still allocates its own input
identity and records its exact original frame. Before parsing or submitting a
check, it takes the typed WorldRequest/NoResidency refusal. Its same-run envelope
has no turn, internal/runtime attribution, subsystem harness and causal_parent
pointing to that request's input.offered sequence. A loss between Offered and
check submission can still select that refusal; a submitted check or cancellation
keeps its existing account.

The original input gate bound covers the offer and refusal authors. Only a
Recorded refusal permits the unnamed close with reason "no model is resident".
Recording a refusal starts one Decision interval for the required send, covering
the remaining input measurements and actual gate delivery. Partial sends or trace
progress cannot renew it. A failed clock after a whole send remains TraceFailed;
a completed response requires the original port, exchange and send witness.
This same send owner covers shape/language refusals.

After delivery, the original serving seat can refuse the next request. No check,
AcceptedInput, turn, user message, filing, placement or generation is granted.
Requests behind an accepted Leave retain the existing Unloading account. The
later Leave still saves, lowers, reaps and records the dead SPU's Unconfirmed
release; another request cannot invent another organ death or Released receipt.

## Owner and caller census

| Owner or caller | Obligation and disposition |
| --- | --- |
| LifecycleAsk / RefusalRecord | Add only WorldRequest to trace's existing ask vocabulary; no lifecycle directive, refusal code, trace event kind or world message is added. Existing ask encodings remain. |
| ObservedOrgans | Retain the original SPU-loss observation. Input reads it without changing or repeating the organ account. |
| InputSlot | Keeps the frame, input identity, offered sequence and bound; selects a private refusal without parsing when SPU loss is already known. An unsubmitted input can be refused after loss; cancelled/submitted inputs cannot be recast. |
| RecordingContext::advance_input | Owns the original offer/refusal author through recorder pressure and lifecycle interruption. It renders the shared typed record and supplies the offered causal parent. |
| RecordingContext::finish_input_author_stage / finish_input | Retire gate_wait after the required records. Bound the required close, including intervening measurements, and retire that interval only after the input's complete disposition. A consumed turn retains its own Decision owner. |
| FoundationInput::drive_with_io / step | Serialize later gate exchanges, retain exact received packets and tracked close sends, then return the original seat. A post-send clock failure cannot become a successful refusal. |
| Graceful Quiesce / input rollback | Existing cancellation and Unloading owners retain priority. They resume an owed input author without minting acceptance, and finish the original input when delivery or checked Lower has settled. |
| Trace recorder | Carries the typed payload as its existing opaque refusal JSON. Harness trace fixtures verify its envelope and link; no trace schema or dependency change is needed. |
| worker/dev_loop | Existing drive remains selected. Foundation orchestration is still dormant; this act changes no deployment choice. |

## Evidence

The full admin/harness/types invocation passed 2,965 tests and doctests (552
admin, 2,165 harness, 248 types, including config-feature tests), with 855 declared
ignores. Nested child summaries are not counted again. All 717 selected worker
process identities passed: 711 non-joint cases and six actual admin/SQLite joint
parents, each group at default parallelism under an owned delegated scope.

After that full run, one additional unit test covers SPU loss between Offered and
classifier submission, and the send-clock test asserts the reported failure
without depending on its helper getter. No runtime code changed. The final
restored-source build, 60-test input selection, typed wire case and five focused
process parents passed. All-target clippy for types/harness/admin, formatting,
SQLite member build, before/after lock and diff checks passed. Cargo used the
pinned toolchain, --locked, a private target and serial mutations.

Seven independent protective changes compiled and failed the intended assertions:
substitute Enter for WorldRequest; parse before the no-residency outcome; drop the
offer's causal parent; omit the close deadline; skip the pre-submission SPU-loss
check; send the language-restatement sentence; misclassify the send-clock failure.
Every mutation was restored before the final build; source manifests and complete
logs are retained with the operator's evidence deposit.

The process fixtures run actual worker, gate, SPU and classifier fixture children,
a SQLite member, and a later graceful Leave. Their language/model replies are
synthetic mechanics inputs and are not production qualification. This record is
an author check, not a Planner grade, security disposition or cutover approval.

The former excluded worker_session_demand_save_sqlite_organ_loss_spu case now
belongs in the selected worker suite. The added pressure case retains the first
refusal behind a full recorder, sends two distinct world closes, then queues a
third frame behind Leave to pin Unloading precedence. Separate close tests use a
non-ready transport through the original deadline and fail the actual send's
clock observation.

Issue #146 remains open. Pinned normative language artifacts, model/classifier
qualification, reviewed prompt/template evidence, measured operating limits,
remaining lifecycle/emission composition and the coupled cutover are still owed.
The next stack act addresses #332's carried Z2 outbound/refusal-author Decision
budget. Only the operator merges or deploys.
