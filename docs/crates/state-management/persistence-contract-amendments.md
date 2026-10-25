# State persistence - contract amendment text

**Status:** MERGED.

**Landing PR:** #348

**Version:** 0.11 **Date filed:** 2026-10-20

## 0. Scope and parties

Replace harness-state contract 2, 3, 5 and 6 for serving adoption and capture,
admin-harness contract 3, 4, 5, 6 and 8 for enter/save/leave, and admin-operator
contract 2, 3, 4.1 and 5 for seed, bind, save and restore. The contracts below state
obligations and failure meanings. [Foundation section 5][representation] owns their
representation, transport, custody layout, actor state tables and clocks. Admin is the
publisher, the harness settles selected-source obligations through its frozen trace
boundary and checks the flow remains healthy, and the member owns the retained content
and its captured SourcePosition. [PERSIST-1 through PERSIST-7, FLOW-5, October 18 R1
through R3]

## 1. Admin-operator - starting state and selection

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-IDENTITY.** A loadable start binds one retained state image and the
operational declaration for that image. Its identity records both digests and the parent
start where one exists. The declaration names the image digest. The binding names both,
without either artifact naming its own digest. Admin must verify all three agreements
before a serving load. A mismatch refuses load, never selects a replacement silently.
Reason: the agent must run the same starting inputs that were bound and recorded.
[PERSIST-1, PERSIST-2, PERSIST-3]

The operator selects through admin's declared selection surface. Selection is outside
the operational declaration and cannot make that declaration select itself. The binaries
and custody authority remain administrative inputs, also outside the bound declaration.
A normal load runs the verified declaration copy, not a mutable authoring draft. The
load record names those actual inputs. [PERSIST-2, PERSIST-3, FLOW-3, October 18 R2]

**PERSIST-OPERATOR-ACTS.** Seed creates a new agent's starting state before an agent
exists. It requires the operator's reviewed prompt, conformance certificate and complete
prompt-duty attestation. It refuses an existing chain or a loaded agent. The seed
contains the prompt and its end mark, and uses the same start identity and load contract
as every later save. Changing the system prompt creates a new seed and agent chain.
[PERSIST-5, PERSIST-7, PROMPT-1 through PROMPT-3]

Rebind combines an existing unchanged image with a new operational declaration. It may
change permitted deployment elections but not the image's prompt, seed or language
scheme. It requires the agent to be unloaded. Restore selects a previously published
start, without rewriting its content or original identity. No operation promotes an
unconfirmed capture or a merely discovered state image. The exact operator surface is
the admin Spec's, not caller-supplied paths or content. [TAP-2, PERSIST-2, PERSIST-3,
PERSIST-7, October 18 R2, R3]

## 2. Admin-harness - enter, capture and publication

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-ENTER.** Admin provides the verified start, operational declaration and
judged component identities. The harness may assume custody and binding checks
passed, but must verify the member's adoption result and exact echoed identity. It
records the untapped trace head, validates its partition, checks its templates, reads
and seats the system prompt, and repairs loaded open pairs through the normal feed,
then reads the committed plan summary before opening input. The plan_current snapshot
carries the exact PLAN-SUMMARY-EVIDENCE fields. The summary is cached at
HARNESS-RECALL's Recording/Recorded transition, after HARNESS-READ has completed
snapshot release. The member owns stored provenance validation. The harness checks
strict shape and already-held trusted expectations, without a new lookup or
reconstructed digest or turn. A failure of any of these duties, including a refused
or malformed mandatory read, refuses enter and invokes existing failed-enter
rollback. A successful enter certifies these duties, not semantic truth or future
model fidelity. [PERSIST-3, PERSIST-6, PROMPT-1, FLOW-3, PAIR-1]

**PERSIST-CAPTURE.** A save uses the loaded declaration and occurs at a between-turn
boundary. The harness freezes its current-run Recorded trace boundary under
[HARNESS-CAPTURE][capture] and accounts for every selected source owed through it before
capture. Correctly rejected material owes no store write. A socket send is not
acceptance, and an uncertain commit cannot pass the barrier. The member supplies a
complete immutable image of the retained content after every selected source through
that frozen boundary is settled. No selected source from the current run beyond that
boundary may enter the image. Admin may prepare a publication only from the matching
capture result. [PERSIST-1, PERSIST-4, FLOW-3, SEAM-2, MARK-4]

The harness-local watermark is not sent as a member control field or substituted for
the image's SourcePosition. That position names the member's last committed content
source. The member's operation metadata keeps STATE-OBSERVE's cutoff before capture,
including no recursive insertion of the snapshot's own observation into its image.
Admin preserves the returned position and verifies the existing image/declaration
binding. It does not infer a stored source from the harness's trace watermark.
[PERSIST-3, PERSIST-4, STORE-5, COST-2]

The on-demand save and a leave's capture have distinct timing scopes. The former may
wait for the current turn under its supplied absolute window. The latter starts only
after drain, wind-down and lower, preserving the operator's existing unbounded graceful
phases. The harness announces the actual capture and shared leave windows before
capture. Admin validates them and never grants more time by receiving a later phase. The
foundation's PERSIST-TIME through PERSIST-EXPIRY own the representation, anchors and
expiry rules. [PERSIST-1, PERSIST-4, CLASS-8]

On leave, capture completion is announced after image release and recording but before
the remaining unwind. Admin may then stop watching the capture window, while the shared
tail and its existing outer leave limit continue. That announcement is not Left and
cannot close the run marker or license public unload success. On demand, the save's
final answer itself completes its window. A missing answer gives no authority to invent
a phase-specific refusal. [PERSIST-4, FLOW-5]

**PERSIST-CONFIRM.** The harness confirms one capture only after checking its trace
boundary remains frozen, selected-source obligations through it are settled and its flow
remains healthy. Commit authorizes that captured image with the member's returned
content SourcePosition. It does not equate the position with the trace boundary. The
harness records the confirmation before sending it to admin. Confirmation authorizes
that capture, not a future state or a claim of completed publication. Admin may not
infer confirmation from an unfinished image, an earlier preparation or loss of the
worker. [PERSIST-2, PERSIST-4, FLOW-4, FLOW-5]

Admin alone publishes and returns publication identity. A public save success requires
completed publication and the matching worker outcome. A clean leave also requires the
existing successful leave obligations. Neither actor may invent the other's success.
Worker failure before the final matching outcome breaks the save, including after
confirmation. Admin stops publication, preserves what exists and reports it for manual
repair. No recovery promotes that capture. [PERSIST-2, PERSIST-4, FLOW-5]

This table projects the foundation section 5 actor tables onto the parties' duties. It
elects no independent states, transition order or deadlines. [PERSIST-2, PERSIST-4]

| Standing | Admin obligation | Harness obligation | Meaning of completion |
| --- | --- | --- | --- |
| Entering | Supply verified start and enforce load budget | Complete required duties before input | Required serving flow stands |
| Saving requested | Pin loaded facts and one operation | Freeze trace boundary and settle its selected-source obligations | Capture may begin |
| Capture prepared | Hold matching immutable result | Check frozen trace boundary, settled source obligations and healthy flow, then record confirmation | One healthy capture authorized |
| Confirmed | Report actual publication outcome | Stay paused until matching outcome | Publication and authorization remain distinct |
| Publication reported | Return success only after final matching outcome | Record publication, release capture, return outcome | Completion makes the named start loadable |
| Worker fails before final outcome | Stop publication, retain broken artifacts and report them | No successful outcome or new capture | Broken save, manual repair only |
| Refused operation | Preserve previous selection | Report typed refusal and release healthy resources | No successful new save |
| Image or finish timeout | Publish nothing and await the worker's cleanup outcome | Keep barrier, send correlated abort, discard late result and await aborted | Refusal disposition only after image holdings are released |
| Broken flow before confirmation | Publish nothing new | Halt and report without another capture | Failed run, never successful leave |

**PERSIST-END.** A flow halt takes no dump and supplies no successful leave. It retains
the account of owed sources and uncertain commits in the trace where possible. Admin
uses unclean-run recovery and cannot publish a leftover image as recovery. A complete
typed operation refusal differs from a missing component result. Missing image/finished
answers keep the timeout/retry rule under R20-6 after PERSIST-CLEANUP. The timeout alone
is not a component-failure halt. A failed cleanup is component failure. [FLOW-4, FLOW-5,
PERSIST-4, October 18 R3]

A refused save within an accepted leave is not itself the leave's final outcome.
The worker confirms cleanup and refusal accounting before the lifecycle continues.
Admin retains that refusal and waits for the final outcome. Under the existing policy,
a forced leave completes without a save and a leave still graceful remains available
for retry. Neither permits a save report for the refused capture. Failed cleanup or
failed flow follows failed-run recovery. Foundation
PERSIST-LEAVE-REFUSAL owns the actor order and windows. [PERSIST-4, FLOW-4, FLOW-5]

## 3. Harness-state - adoption, reads and image custody

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**PERSIST-ADOPT.** The member verifies the exact supported catalog, binding scheme, seed
identity, content SourcePosition and complete prompt span before adopting a start. It
may assume admin verified the declaration binding. It must not claim to have verified a
declaration it does not receive. Its success echoes the identity that it adopted. Failed
adoption leaves no partial serving store. [PERSIST-3, PERSIST-6, STORE-3]

The first content read returns the seeded system prompt through its end mark. The
harness caches that head and records what it seated. Reading does not file it again.
Later explicit retrieval of the same span is permitted and does not become another boot.
Diagnostic preload is a separate duty and cannot create or repair serving state.
[PERSIST-6, PROMPT-1, STORE-8, SCOPE-1]

**PERSIST-IMAGE.** The member supplies complete image content, its size and captured
source position in answer to a matching request. It keeps the image stable through the
capture exchange, refuses incompatible mutations and releases it only through the
defined completion, cancellation or run end. It does not read the operational
declaration, compute its binding identity or publish a loadable start. The harness owns
the single exchange and must not send a second operation behind an uncertain one.
PERSIST-CLEANUP's abort is a correlated continuation of that exchange, not another ask.
[PERSIST-1, PERSIST-4, SEAM-1, SEAM-2]

**PERSIST-CLEANUP.** After a snapshot or finished timeout, the harness keeps the filing
barrier and sends abort with the original request and capture IDs before any other
state ask. The member accepts it in CaptureHeld or FinishedHeld, deletes the retained
room file and answers aborted before returning Ready. A capture refusal that crossed
the abort retains just that capture identity until cleanup or the next fresh exchange,
so the same abort can confirm that no file remains. No cleanup changes held content.
Reason: a recoverable save timeout must not leave the member refusing the next filing.
[PERSIST-4, SEAM-2, FLOW-4]

Before aborted, the expected late snapshot, finished or capture refusal for the
timed-out leg is validated for identity, shape and operation observation, then discarded
as a save result. Any other result is protocol failure. The late result is never relayed
to admin, acknowledged or used for publication. Its observation still reaches
the trace. Only the matching aborted result releases the barrier and permits
the final refusal disposition. On demand this permits operator retry. On leave the
accepted lifecycle disposition remains owed under PERSIST-LEAVE-REFUSAL. Partial writes
retain their offsets, so abort cannot splice into an unfinished frame. No later filing
or read is sent during cleanup.
[PERSIST-4, SEAM-2, FLOW-3, COST-2]

Cleanup uses the existing two-second small-control bound, starting when abort is
enqueued and including carry. Late bytes do not restart it. It does not renew the save
or leave windows or authorize late publication, and existing earlier lifecycle
containment still applies. A missing, malformed, wrongly correlated or refused cleanup,
failed delete, dead peer or failed carry is component failure. The harness halts rather
than resuming on an unproved Ready state. The next fresh request follows the original
snapshot request ID, as TYPES-PERSIST defines. [PERSIST-4, FLOW-4, FLOW-5, CLASS-8]

Seed uses a separate pre-agent duty that creates the store and inserts exactly the
certified prompt and end mark. A serving member accepts no seed insertion, arbitrary
schema or alternate content feed. Structural certificate verification proves byte and
scheme agreement with the reviewed artifact. It does not prove prompt quality or
conformance by itself. [PERSIST-5, PROMPT-2, PROMPT-3, FLOW-1]

## 4. Pass statements, failures and enforcement

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

The binding pass certifies custody, the three digest agreements and parseability of the
declaration. The adoption pass certifies catalog and source structure, not semantic
truth. The capture pass certifies settled source obligations through the frozen trace
boundary, healthy flow and the matching image with its content SourcePosition. The
publication pass certifies the actual published identity. They cannot substitute for
one another. No pass proves that a trace sink survived. [PERSIST-3, PERSIST-6, FLOW-6]

Every parseable refusal identifies the operation and its reason. Wrong identity, phase
or reply shape is a protocol failure. Correct conformance or operation refusal remains
distinct from component failure. No retry resends an uncertain filing or pretends the
old state is the newly requested result. [FLOW-4, SEAM-2, CLASS-8]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| PERSIST-IDENTITY | Mismatched image/declaration/binding and changed-draft fixtures | Load a mismatched start or substitute the draft |
| PERSIST-OPERATOR-ACTS | Seed/rebind/restore authorization and chain tests | Seed a loaded agent, change the prompt by rebind or promote an unconfirmed image |
| PERSIST-ENTER | Failed duty and pair-repair boot matrix | Open input before the seated head and empty-pair recheck |
| PERSIST-CAPTURE | Delayed receipt, rejected-statement and untapped-record capture fixtures | Capture an owed or later source, mistake correct rejection for loss, move a frozen boundary or replace image position with a trace watermark |
| PERSIST-CONFIRM | Each preparation/confirmation phase interrupted | Infer confirmation from an image or report publication before it happened |
| PERSIST-END | Broken-flow teardown and later recovery | Attempt a halt dump, invent successful leave or publish a leftover capture |
| PERSIST-ADOPT | Catalog/scheme/content-position/prompt failure fixtures | Partly adopt or claim declaration verification in the member |
| PERSIST-IMAGE | Concurrent mutation, oversize result and cancellation fixtures | Return partial content, mutate held image or allow a second uncertain exchange |
| PERSIST-CLEANUP | Timeout at snapshot and finished, late success/refusal before aborted, then ordinary filing and retry, plus cleanup failure cases | Resume before aborted, leak a room file, publish a late result, reset a deadline or continue after cleanup failure |

The concrete state tables and wire refusal representations are in the owning Specs.
R20-6 and R20-7 settle the missing-result and broken-publication branches. Measured
production values remain qualification work, not invented contract constants.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
[representation]:
state-management-foundation-Spec.md#5-persistence-representation-and-actor-protocols
[capture]:
weaver-harness-Spec-amendment.md
