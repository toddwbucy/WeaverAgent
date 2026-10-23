# weaver-admin Spec: flow, seed, publication, and boot amendment

**Status:** MERGED.

**Version:** 0.16 **Date filed:** 2026-10-23 **Parent:** weaver-admin-PRD

## 0. What this document is

Amend Spec 3.0, 4, 6, 9 and 10 in the combined flow/store/seed unit. Admin still runs
per verb, links no interior crate, creates inherited channels, and owns root
publication. It never implements SQLite queries. Foundation Spec section 5 supplies the
three-party state tables and exact artifact format. The persistence contract states the
parties' obligations. [PERSIST-1 through PERSIST-7]

## 1. Inventory and load, destinations sections 4 and 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-VALIDATION.** Inventory parses the new state-management table. Serving requires
sqlite, a flow declaration, and an indexed verified starting artifact. Reject old
serving state-election, engine none, preload socket and empty first load. Diagnostic
retains its separate election and preload, without a serving coordinator.
StateManagementInvalid uses existing failed-enter rollback. [TAP-2, TAP-3, FLOW-4,
PERSIST-5, SCOPE-1, R8]

**ADMIN-STE100.** Add ste100-binary and ste100-scheme.json to the serving config root's
closed inventory. Apply Spec 9's existing executable custody judgment to the former and
regular-file root/group/mode judgment to the latter. Keep both outside the bound
declaration. Read the scheme once and verify its exact digest, scheme_id and ordered
verb keys against the verified declaration, using Spec 02's SCHEME grammar. Pass those
bytes as EnterBinding::Serving.ste100_scheme and the declared instruction as
state_management.ste100. Record the executable digest separately from the model-manifest
and scheme digests. No worker opens these root paths again. Reason: the checker must
receive the same instance that inventory checked, while binary custody stays with admin.
[CLASS-2, FLOW-3, PERSIST-3]

The instruction's model binding names Spec 02's exact manifest file by an absolute
pathname under MANIFEST-LOCATOR. A relative value is ConfigInvalid naming the binding
field. Inventory preserves that declared value and its expected digest. Admin does not
discover or load model files on behalf of the agent uid. The SPU owns their
acquisition and readability judgment during bootstrap. This locator is not another
admin root key and does not change scheme or executable custody. [CLASS-2, CLASS-7,
FLOW-3]

Keep channel.rs::send_with_sink's complete-envelope check against MAX_ENVELOPE_BYTES
before sendmsg, including escaping and every sibling field. Its ChannelFault::Truncated
produces pre-load diagnostic enter_frame_too_large, returns the existing
DescriptorsUnusable send failure and uses the same start rollback. It does not set
standing.entered, grant another load budget, transmit a prefix or ask the worker to
reopen the scheme path. This remains the existing lifecycle carrier, independent of the
STE100 port's larger logical-message allowance. The exact-bound, escaped-string and
one-byte-over cases are admission fixtures. Removing the complete-envelope guard must
fail them. [CLASS-8, FLOW-3, PERSIST-6, SEAM-2]

Inventory also validates the declared sh policy against the elected tool set and the
shared shell maximum. Diagnostic binding requires neither new root key nor a serving
STE100 instruction. An invalid serving artifact is ConfigInvalid with field attribution
at inventory. Checker admission or template certification failures after load use the
existing after-load refusal and rollback path. [CLASS-8, PAIR-2, SCOPE-1]

**ADMIN-SELECTION.** Add one optional regular file, restore-artifact, to the judged
admin config root's closed key list in Spec 9. Its bytes are one artifact ID, with at
most one final LF. The ID is 20 decimal digits, a hyphen and 64 lowercase hex digits.
Whitespace, a path, a digest alone, an empty file or extra lines refuse ConfigInvalid
naming restore-artifact. An absent key selects the latest index entry. The same key is
used by load, restore and bind-state. It is not serialized into agent.toml. Reason: the
declaration cannot select the artifact that contains it. [PERSIST-2, PERSIST-3, October
18 R2]

The argument-free restore verb verifies the named, already indexed artifact and appends
a selection entry that makes it latest. A selection entry carries a new index ordinal
and arrival selected, while preserving the artifact ID, A/B, position and original
confirmation. It does not rewrite the artifact. A publication entry uses arrival
published. The directory's ordinal is its creation ordinal, not every later selection
ordinal. load and bind-state retain the merged N6 rule: a present restore-artifact must
name the latest entry's artifact, or the invocation refuses and names both. After a
later save, the operator removes the key or runs restore again explicitly. The verb
requires the key, accepts no caller argument and no unindexed or hand-placed file.
[PERSIST-3, PERSIST-7, R2, R3]

**ADMIN-LOAD.** Keep the territory locator, binaries, library-path, roles, operator UID,
coordination root, headroom and load-bound-seconds in admin's configuration root under
the judgments of Spec 9. They never move into the bound declaration. Select through
ADMIN-SELECTION, verify A/B and parse that artifact's declaration copy. Refuse identity
or custody paths that disagree with the judged territory. Operational settings declared
in agent.toml, including partition, tools, model elections and state-management bounds,
come from the verified copy. The trace load tuple also records the root-selected
executable identities so replay sees both inputs. Editing the territory draft alone does
not change the declaration loaded next time. bind-state publishes that change. No
admin-con caller supplies a path. [PERSIST-2, PERSIST-3, FLOW-3, R2]

Diagnostic inventory keeps its existing draft and preload selection. It does not require
a serving artifact or apply the serving restore-artifact key. A root naming that key for
a diagnostic binding refuses ConfigInvalid. This separation preserves R8 without
offering a serving fallback. [SCOPE-1, October 18 R8]

Inventory opens state.sqlite read-only under the judged artifact directory. The member
receives it as fd4, its first channel as fd3 and the existing run lock as fd9. It
receives --mode=serving in the admin-owned launch vector. The harness receives its
existing inherited state endpoint and EnterBinding::Serving.state_management plus
StartIdentity and ste100_scheme. Add those facts and the judged checker executable
digest to show. No extra state socket is created. The member verifies SQLite before
opened. Admin verifies configuration before spawn. The worker confirms enter only after
template checks, boot read, placement and any required pair-load repair and committed
plan-summary read. [PERSIST-3, PERSIST-6, SEAM-1]

Keep the existing load-bound-seconds budget and its start anchor. Opening, template
checking, reading the prompt and opening decoder/gate share its remaining duration. No
stage grants another full budget. Failure rolls back the same process set and never
reports loaded. [CLASS-8, PERSIST-6, GATE-1]

## 2. Seed and rebind, destinations sections 4, 6 and 9

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-SEED.** Add root-shell-only `seed <agent>` to surface.rs::Request, the main.rs
dispatcher, verb_table.rs::Verb and verb_table.rs::HolderVerb. It takes no other
argument, stdin content or destination. Do not add it to connector sudo rules. Hold the
exclusive invocation lock and the per-agent run lock, refuse active or previously seeded
chains, and read the judged territory's system-prompt.md and
system-prompt-certificate.json. These are authoring inputs, not fields in configuration.
Group and mode remain root:admin-access-group 0640. Changing the held prompt requires
recreation as a new agent. Reason: only the pre-agent seed may write outside the tap.
[PERSIST-5, PERSIST-7, PROMPT-2]

Apply ADMIN-STE100's root-file, scheme and model-identity judgments against the parsed
authoring declaration. This seed inventory permits the absent start hash under
TYPES-PERSIST and does not require an already indexed serving artifact. All other
custody and declaration checks still apply. Serving load continues to require the
verified bound copy under ADMIN-VALIDATION. [PERSIST-5, PERSIST-3, CLASS-2]

Use Spec 6's start step and Spec 6.1's containment, including the root-owned delegated
invoker, ancestor checks, cgroup record before fork and namespace/credential setup. Mint
the seed operation's run reference for that containment record. It is not a serving
session and creates no load or unload event. Stand only the member under its restricted
identity inside that cgroup, with --mode=seed, fd3 and fd9, no fd4, no gate, no SPU
decoder, no serving worker, and no preload door. Admin speaks state-seed/1 on this
short-lived inherited channel. It creates seed_id as 16 OS-random bytes rendered in 32
lowercase hex characters. It sends SeedRequest from TYPES-PERSIST. A terminal
SeedCreated answer contains schema_id, seed_id, room_name and size_bytes:DecimalU64.
This is a byte count, never inline dump content. Admin hashes/copies the room image into
an unlisted root part directory. A component crash, absent answer or explicit refusal
before a valid created answer produces no index entry. The seed invocation shares the
existing load-bound-seconds deadline from acceptance and the start step's bounded
rollback and cgroup cleanup. After copying the completed image, close the channel, reap
the member, empty and remove the operation's cgroup. Only after successful cleanup does
admin finish the ordinary three-file publication and index it, then release the locks. A
cleanup failure discards the unlisted root part, is reported and creates no loadable
seed. No worker, gate or uncontained helper is started. [PERSIST-5, PERSIST-3, R1]

Use TYPES-SEED's exact state-seed/1 frames, including its string request ID and
top-level refusal fields. Supply the scheme bytes already read and checked by
ADMIN-STE100 and the authoring declaration's model_artifact_sha256. Verify the
certificate against those inputs before starting the member. No hash is inferred from
the certificate it is supposed to check. The member retains the finished room image
until admin closes this channel, then cleans up. Admin copies it before close. No
subsequent seed command is accepted on that connection. This mode's image is a completed
pre-agent seed, not a serving halt artifact. [PERSIST-5, CLASS-2, PROMPT-2, SEAM-1]

Admin checks certificate byte identity, scheme and complete duty attestation. It does
not spawn or emulate a classifier. Spec 02's SEED-CERTIFICATE defines the pure verifier
port and the external certificate producer. Their implementation and qualified artifacts
remain cutover dependencies. Protocol fixtures alone cannot certify a production seed.
The pass statement is exact insertion of certified/reviewed bytes, not a claim that a
checksum proves meaning. [CLASS-2, PROMPT-2, PROMPT-3, PERSIST-5]

**ADMIN-REBIND.** Add root-shell-only `bind-state <agent>` to the same four verb
surfaces as seed, with no further arguments. Hold the exclusive invocation lock and run
lock and refuse a loaded agent. Select through ADMIN-SELECTION, verify the triple, and
combine the unchanged dump with the root-owned operational draft. Parse the draft in
authoring mode, replace only the starting hash when serializing the published copy, and
recompute B. Preserve seed_id and set the old artifact as parent. Require the new
declaration's scheme_id and artifact digest to equal the verified old declaration's
scheme. Rebinding cannot relabel the store. Refuse scheme_mismatch before publication.
Use PERSIST-FORMAT and the same index. Do not admit a prompt field or arbitrary file
path. [TAP-2, PERSIST-2, PERSIST-3, R2]

Admin's canonical TOML serializer writes the parsed operational declaration in parser
field order, kebab-case keys, with no comments. It emits exactly one start hash. Both
seed and bind use it. A normal save uses the loaded parsed declaration and the same
serializer with the new A, never the mutable root draft. B hashes the final bytes passed
at next load. [PERSIST-2, PERSIST-3]

## 3. Save and recovery, destinations sections 3.0 and 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-SAVE.** Implement PERSIST-SAVE in save_points.rs and lifecycle invocation
handling. Admin opens only the judged room part named by the worker, validates that it
is a regular member-owned file inside the member room, copies it to the root part
directory, computes A, then writes bound configuration, B, and binding.json. The member
neither reads configuration nor computes B. This replaces the current
publish_from_room_noting rule that publication reads no declaration. [PERSIST-1 through
PERSIST-4, R1, R2]

For a loaded save or leave, acquire the existing invocation lock, obtain the worker's
current LoadFacts through LifecycleDirective::Observe and its State reply, as see_run
already does. Do not call public show while holding the exclusive lock. The existing
five-second observation bound applies before directing save or leave. Pin the reply's
run, StartIdentity and state-management declaration. Open and verify that named artifact
and require its B to equal LoadFacts.declaration. Use its parsed copy and save_wait, not
a fresh territory draft or restore-artifact selection. The lifecycle exchange retains
this run identity throughout. A changed run, missing facts or B mismatch refuses before
requesting capture. The source artifact becomes the new artifact's parent. A later save
in the same run still binds the same loaded settings. [PERSIST-2, PERSIST-3, FLOW-3, R2]

**ADMIN-SAVE-TIME.** In main.rs::save_point, after Observe and loaded-copy verification,
sample raw CLOCK_MONOTONIC immediately before send and include SaveWindow from
TYPES-SAVE-TIME. For unload retain main.rs::leave_window and admin Spec 3.0's existing
observation, drain, wind-down, force and escalation rules. No save_wait timer covers S4
or S5. Await SavePointBeginning before accepting an image and validate its windows under
foundation PERSIST-BEGIN. [PERSIST-1, PERSIST-4, CLASS-8, admin Spec 3.0]

Once Beginning announces the bounded save stage, watch the earlier capture, shared-tail
and existing outer leave deadlines. No frame restarts them. On leave,
SavePointCaptureComplete ends the capture window only. Await the real Left under the
remaining shared/outer tail before applying the normal conclusion. On demand,
CaptureComplete is out of order. A queued-save timeout is CaptureNotReady only when the
worker's typed refusal says so. No answer is Unanswered, never an inferred phase or
successful outcome. R20-6 governs missing-result retry, and R20-7 forbids completing
publication after worker failure. [FLOW-5, PERSIST-4]

These clauses amend admin Spec 3.0 and section 3's leave/save wait routines. The
protocol fixture checks both graceful bound omissions, finite and forced windows, stale
Beginning, reordered Image and early or duplicate CaptureComplete. It must fail when a
later frame resets an earlier deadline or when a save bound forces an otherwise
unbounded S4/S5. This is a protocol pass, not a wall-time guarantee for an
uninterruptible filesystem call. [PERSIST-4, COST-4, FLOW-5]

Do not publish before the matching SavePointCommit. A prepared artifact is not loadable.
Before Commit, a worker failure cancels publication. After Commit, failure before the
final matching outcome breaks the save. Apply PERSIST-PUBLICATION-GUARD's pending/broken
record, keep artifacts and return FailedRun with their identities and locations. Never
remove the guard or resume publication in recovery. No halt requests a new image.
[FLOW-5, PERSIST-4, R20-7]

Replace S3 save-point ActivityNotAtRest with accepted/pending to the next turn boundary.
Explicit refusal and missing image/finished timeout retain the merged retry path under
R20-6, including S6/S9. For a timeout, SavePointNotTaken is sent only after the
harness's correlated abort receives aborted under PERSIST-CLEANUP. Admin never treats a
late image/finished result as success or initiates another ask on the member channel. A
cleanup failure is FlowFailed and ends the run, not a retryable save refusal. Silence at
admin's unchanged outer deadline remains Unanswered, never inferred cleanup success.
Member death remains a separate terminal failure. [PERSIST-1, PERSIST-4, FLOW-4, R20-6]

**ADMIN-FLOW-END.** Replace the existing serving MemberDies column, rather than adding
contradictory cells. FlowFailed follows it. Diagnostic cells remain separate. The worker
owns the latch and teardown. Only a current or subsequent admin invocation writes
root-owned markers. [FLOW-4, FLOW-5]

| Position | New serving member-death or flow-failure cell |
| --- | --- |
| Enter not confirmed | Refuse enter, ordinary failed-enter rollback |
| S2, S3 | Worker halts and exits unsuccessfully, S8b recovery, no new capture |
| S4, S5 | Cancel remaining wind-down and unconfirmed capture, no successful Left |
| S6 graceful, S9 | End run on member death or flow failure, including failed cleanup. Missing snapshot retains retry only after matching aborted under R20-6 and PERSIST-CLEANUP |
| S6 forced, S10 | End run without successful Left or save claim |
| S7 | Complete bounded release, no successful Left, preserve indexed artifacts. Any incomplete capture is broken under R20-7 |

Every row forbids a new capture after failure. A valid Commit does not permit admin to
finish a publication after worker failure. Preserve its guard and artifacts, report the
broken save and leave repair to the operator. [FLOW-5, PERSIST-4, R20-7]

Remove ServeOnSeamRetired, LeaveGoesOnSeamRetired, StaysS9MemberDead and
MemberUnconfirmedAtRelease meanings for serving. Distinguish explicit snapshot refusal
from a missing result in LegMissed and SavePointNotTaken. A failed worker writes no
admin marker. With no Left, the invocation makes no conclusion: the marker remains Open,
S8b applies until the run lock frees, then S0d. The next load records NoCleanUnload.
conclusion.rs selects Forced for its existing forced outcomes, not for worker death.
Worker unload evidence carries end_reason component_failure and never substitutes for
Left or asserts an operator force. [FLOW-4, FLOW-5, R3]

**ADMIN-RECOVERY.** Replace the publication actions in Spec 3.0's S0d load, unload and
force-unload cells and S8b's unload/force-unload cells. Keep their cgroup cleanup, lock
handling and marker/reset behavior, but delete recovery publication from room files.
Also replace the conclusion rows that publish an unreported room image or promise its
later recovery: only an artifact whose save operation completed and whose publication
guard is absent remains eligible. No Left still means no conclusion. Use only indexed
verified artifacts, never a room file as evidence of a healthy capture. Refuse malformed
latest rather than skipping it. New reset evidence names A/B and the selected artifact.
Retain an already completed save after a later crash. An incomplete or broken
publication remains in place with its guard for manual repair. Unconfirmed parts without
a publication guard follow pre-Commit cleanup, never recovery as latest. Admin supplies
the existing Reset {prior_run, reason} on enter. The harness records it in load.reset,
beside the selected state_start carrying A/B. Neither actor invents a separate reset
event or duplicates A/B inside Reset. [PERSIST-3, FLOW-3, FLOW-5, R3]

## 4. Deployment retirements, destinations section 9 and runbooks

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-RETIRE.** The combined unit removes #76's live seeding turn and the empty first
load at FOUNDATION-TRANSITION's qualified code cutover. Harness parsing refuses role
system through the ordinary input-refusal path. Remove deploy/turn.py --system and all
claims it replaces a running prompt. create-agent.sh prepares root draft files, then
seed must complete before load. No storeless engine can pass create, inventory or
serving enter. [PERSIST-5, PERSIST-7, PROMPT-1]

Replace HowToDeployANewAgent sections 0, 2, 3 and 4 and REDEPLOY's seed/load sequence:
create territory and draft, obtain prompt review and STE100 certificate, run seed,
verify index and A/B, then load. Reconfiguration uses bind-state before load. A prompt
change creates a new agent. verify-lifecycle.sh and test_plans.py must exercise this
sequence, old-layout refusal, and no --system route. update-stack.sh must refuse old
agents pending recreation. No deployment script is changed in this documentation pass.
The replacement runbook also states N6: after every new save, a still-present
restore-artifact key names an older entry. Remove it before the next load or explicitly
run restore to select that older entry. Never silently ignore it. The retirement
register names all contract counterparts. [PERSIST-3 through PERSIST-7, TAP-2]

**ADMIN-OPERATOR-ROWS.** Amend admin-operator contract section 4.1 with these
argument-free command rows. The invocation cause must be root-authenticated before
reading a fixed key or changing selection. No caller chooses a path. [PERSIST-2,
PERSIST-3, PERSIST-5]

| Verb | Required state and fixed input | Success and refusal |
| --- | --- | --- |
| restore | Unloaded, restore-artifact key naming a verified indexed complete artifact | Append selection, existing RestoreNamed. Missing key, pending/broken guard or invalid artifact refuses before change |
| seed | Unloaded, no seed exists, judged seed prompt/certificate and declaration inputs under ADMIN-SEED | Seeded with StartIdentity only after complete publication. Refuse an existing seed or bad input |
| bind-state | Unloaded, selected complete artifact and judged territory declaration draft | StateBound after verified same-state publication bound to the new declaration. N6 and scheme identity checks still apply |

The contract rows state custody and outcomes. ADMIN-SEED, ADMIN-SELECTION and
PERSIST-FORMAT own representations and publication order. [PERSIST-3, SEAM-1]

## 5. Enforcement, destination section 10

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| ADMIN-VALIDATION | Serving/diagnostic inventory matrix | Accept none, preload, empty store or legacy table |
| ADMIN-STE100 | Judged-root mutation, dictionary/digest and tool-policy fixtures | Reopen scheme after inventory, confuse model/scheme/executable digests or accept an unelected tool policy |
| Publication guard | Every post-Commit crash phase, guard update failure and explicit older restore | Load or recover a guarded image, publish after failure or silently choose an older image |
| ADMIN-SELECTION | Root-key parser and latest/explicit restore index corpus | Put selector in bound copy, accept caller path, or silently reuse a stale selector |
| ADMIN-LOAD | Different mutable draft and verified copy, fd probes, failed-enter fixtures | Run draft after checking copy or report loaded before boot |
| ADMIN-SEED | Temporary-member process fixture, shared TYPES-SEED corpus, root verb parser and cgroup probes | Seed live state, require a prior artifact, omit checked scheme/checker inputs, skip invoker/ancestor checks, accept caller path or index before successful seed cleanup |
| ADMIN-REBIND | Same A/new B artifact and immutable prompt check | Mutate dump or use old B after editing config |
| ADMIN-SAVE | Three-party protocol, timeout cleanup then retry, cleanup failure, and fsync/rename failure hooks | Index before Commit, overwrite a good artifact, infer cleanup from silence or retry after FlowFailed |
| ADMIN-FLOW-END | Every replaced lifecycle cell and tool fault cause | Serve on dead member, return Left, or let worker write marker |
| ADMIN-RECOVERY | Crash before/after Commit, rename and index append, next-load tuple | Promote unconfirmed room data, skip malformed latest or associate load.reset with another selected start |
| ADMIN-RETIRE | Deployment plan tests and old-format corpus | Accept --system, empty load, none or old one-file save |
| Custody | Existing manifest and descriptor tests | Link state implementation or add caller-chosen root path |

## 6. Root plan update, destinations sections 2, 3.0, 4, 7, 8 and 9

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-PLAN.** Add Request::PlanUpdate(AgentName) in surface.rs, its exact plan-update
parser arm, dispatch and HolderVerb/Verb entries. The command has only its agent
argument and never reads stdin. Keep the normal root admission and require cause uid
zero for this verb. A connector cause refuses Unauthorized even if a local sudo rule was
widened. Keep this verb outside both generated connector sudo roles. Extend CLI help,
stdout one-answer rendering, exit status and admin.log through the existing dispatch
path. Reason: R5 places plan authority at the root operator's fixed verb, not at an
agent or connector supplied path. [PLAN-6, R5]

Read only the judged config-root/plan-update.json with O_NOFOLLOW and verify the opened
fd is a regular file, root owned and not group/other writable. Keep the bytes once read.
Strict UTF-8 and PlanUpdateDocument parsing, cause, shape, digest and encoded
coordination-envelope length checks precede sending. An absent or unsafe file is
ConfigInvalid with field plan-update. Bad document syntax is
PlanUpdateRefused/malformed_plan. Use the existing bounded coordination envelope,
including JSON escaping, not a new large-message protocol. [PLAN-6, FLOW-1]

Take the ordinary invocation lock. Observe the running peer using weaver-admin-Spec
section 7's uid, run-lock and pidfd checks. Refuse absent/diagnostic/nonserving or
nonresting state, leave, save or another pending update with OutOfOrder,
ActivityNotAtRest or the existing lock refusal as applicable. Observation is not a grant
to bypass the worker's own check when it receives the directive. Derive the update wait
from LoadFacts.state.Serving.state_management.ste100.decision_wait. Start admin's wait
at sending PlanUpdate, never from a mutable declaration. Send the exact document string,
digest and cause with a new existing-channel exchange ordinal. [PLAN-6, CLASS-8]

| Admin state | Observation or answer | Required action |
| --- | --- | --- |
| Checking | Invalid cause/file/document or busy invocation | Refuse locally, send no plan input |
| Observing | Running serving at rest with loaded facts | Send one PlanUpdate, Waiting |
| Observing | Other state, refusal or observation bound | Existing refusal, no update |
| Waiting | Matching PlanUpdated | Print/log that receipt's PlanRef and source, exit success |
| Waiting | Matching LifecycleRefusal | Print/log it, including typed read_code for read_refused, exit refused, no retry |
| Waiting | Closure or bound without answer | Print/log Unanswered, no rollback or automatic retry |
| Waiting | Wrong exchange/payload | Existing protocol refusal, no guessed update result |

Extend Unanswered's description to include plan-update. It states that the answer was
not obtained, not that the transaction did not commit. The operator uses show's plan
summary and the source/receipt trace to resolve that case. The worker's own bound still
governs an unfinished update, even if admin has disconnected. No timeout here signals or
unloads a healthy worker. A confirmed update has no publication side effect. Save points
still use their own verb and protocol. [PLAN-6, FLOW-4]

For PLAN-PREFLIGHT-READ, print and log the matching PlanUpdateRefused with
case read_refused, its typed read_code, unchanged member detail and empty
rejected_blocks. Admin does not parse detail to recover the code or convert the
refusal to Unanswered, malformed_plan, stale_plan or success. It performs no automatic
retry, narrower read, rollback, save or unload. show may still report the prior
committed PlanSummary once the worker's refused operation has settled. This is a
worker operation outcome, distinct from missing the answer and from a required
post-commit refresh failure. All other plan-refusal cases carry read_code:null.
[PLAN-6, FLOW-4, SEAM-3]

**ADMIN-PLAN-SHOW.** Preserve the existing Observe/State shape and add its optional plan
summary under TYPES-PLAN-CONTROL. show only relays the worker's account. It never reads
the live store file or an old event/field table. An absent plan member means no held
plan in the serving snapshot, not an empty seed. A disconnected worker cannot supply a
current summary. The summary cites the operator document digest and last committed
source, so a lost update answer is investigable without replaying the update. [PLAN-6,
STORE-5, SEAM-3]

The pass certifies root custody of the bytes and a matching worker receipt, not STE100
correctness, adequacy of a plan or an independent durable disk save. Tests must reject a
connector cause, symlink, writable file, trailing argument, stale revision, busy worker
and oversized escaped envelope. Removing the cause/fd/size check must fail its test. A
lost-answer fixture must show the stored update through show without another mutation.
[PLAN-6, FLOW-7]

## 7. Wind-down conformance, destination section 3.0

This addition applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**ADMIN-WIND-DOWN.** In S5 and graceful step 4, the caller chooses pass count and
summary policy. All requested work consumes the same loaded wind-down bound. No pass or
partial result restarts that bound. Admin's existing forced-join and containment rules
still apply. [CLASS-8, PERSIST-4, R20-18]

Correct repair exhaustion adds the S5 completion case with turn.closed stopped for an
operation refusal, then S6. It is not a component failure, changes no forced flag by
itself and answers no world dialer. Healthy capture still waits for every owed accepted
statement and pair disposition. A component failure continues to S8b with no publication
under R3. Reason: the model's emission contract applies to wind-down without changing
who owns the leave or its clock. [CLASS-8, CLASS-9, PERSIST-4, FLOW-4, R3]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| ADMIN-WIND-DOWN | S5 row test with conformance cap, forced join and failed classifier | Treat the cap as a broken seam, restart the wind-down bound or publish after component failure |

## 8. Open elections

Production classifier certificate production/verification is not supplied here. The
separate prompt document is owed. Pair-load closure uses Spec 05's joined boot-repair
contract. The measured memory qualification of capture/read copies and file bounds
remains open. R20-5, R20-6 and R20-7 settle the foundation's copy and persistence
outcomes. The October 18 rulings keep their recorded standing, and the revised packet
still awaits review and approval.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
