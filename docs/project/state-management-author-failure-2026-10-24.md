# Returned author originals and caller custody

Date: 2026-10-24. Branch: `code/state-author-review-fixes`.
Parent: draft #315 at `f372720a12877ec6a53a231a4b309c33dbce5a76`, above
#314 at `a95e0a6d` and reviewed root #312 at `b24a260e`.
Authority: merged #276, `weaver-harness-Spec-amendment`
HARNESS-RECORD-LIFETIME and HARNESS-HELD-BOUNDARY. This is implementation
evidence for #146, #255 review 5636484478 row 1 and #314 Planner review
5637217403. The same child answers #315 review 5637284596 in the paired
held-boundary record. It is not production qualification or a new protocol.

## Behavior and scope

The shared author and both resume roads return a move-owned AuthorFailure,
containing the existing RunFailure and an optional exact AuthoredEvent.
AuthorFailure is not Clone and has no lossy conversion to RunFailure. An absent
returned original means that the context retains it in an existing slot or
it has already reached Recorded; absence alone is not acknowledgement.

A second submission remains a caller error. A first fatal cause stays immutable.
An admitted marks or internal repair continuation retains its existing exemption
for an ordinary held caller. If that continuation meets another nonfatal owner
hold, it must return its new original instead of overwriting the older one.
All twelve early hold branches and the owed-withdrawal branch now use the same
slot helper: fill an empty slot once; otherwise return the exact new event.
This applies to the template slot as well as the ordinary caller slot. No hold
creates a sequence, renews a wait or permits another submission.

The #314 precedence change remains explicit: template-held input is checked
before the owner holds, so a second author is fatal even for marks/repair
continuations. An ordinary caller hold still admits only those private
continuations. The helper protects the earlier original when an exempt
continuation encounters a later hold. It does not expand the exemption.

A withdrawn CapacityPending author has no trace sequence. Freshness may drive
that previous caller's author, but retains a returned original in the existing
waiting slot with its withdrawal reason. Both resume roads and the shared
waiting-content continuation inspect this marker before they can queue it.
The continuation returns that exact event even when a fatal cause is already
latched; without a fatal cause it still cannot treat withdrawal as permission
to submit again. The existing account remains owed.

StoppedAuthor is one Option per logical caller, not an overflow queue. Its
returned event remains with that operation. A returned original clears the
caller's started flag; AuthorRecord also clears the unrecorded ticket. Those
flags are bookkeeping, not evidence of Recorded. The marks regression exercises
that actual caller retaining its returned fault through another advance.

## Finite hold and continuation census

| Road | Exact-return disposition |
| --- | --- |
| Place/placement-stopped, plan summary/effect, boot Open, boot pairs, marks, freshness, inspection, read, repair, measurement | Shared occupied-slot helper returns the new original with the existing owner reason; it cannot replace the first held allocation. |
| Generic ToolCallStarted/ToolCallCompleted hold | The same helper returns PairOwned and preserves prior custody. |
| Template-blocked content/TurnStarted hold | The same helper retains an empty template slot or returns the new input. The earlier second-submit gate still owns competing template custody. |
| BusyAuthored with owed boot/place account | The same helper preserves earlier held input while the existing withdrawal clears only this attempt's ticket/turn/release preparation. |
| Generic and claimed resume | Existing withdrawn-marker return and first-cause rules. Private claims match their exact identity. Claim-less caller identity is still a review carry. |
| Shared continue_waiting_content/continue_claimed_content | Now takes a matching withdrawn original before phase/latch/queue continuation. It cannot allocate another ticket or sequence for that input. |
| Filing, OUTPUT, outbound-sort fault, pair-rejection author | All consume that shared waiting-content continuation and retain its returned AuthorFailure in their existing owner. |
| Marks fault author | Existing StoppedAuthor retains the returned original, clears started, and refuses another attempt. The new actual caller test pins this behavior. |

The twelve early holds are eleven ordinary caller slots plus the template slot;
the thirteenth site is the already-guarded owed-withdrawal return. Grepping the
production flow source leaves their writes in the shared helper. First-fatal
retention remains separate because it owns a different disposition.

## Other current author callers

#314 migrated the existing internal call sites. Their complete census is retained
here, with evidence limits made explicit rather than claiming full legacy
serving migration.

| Caller | Current implementation and evidence boundary |
| --- | --- |
| InputOffered, Refusal, TurnStarted, MessageUser | Input owners retain their exact return and stage. Existing input custody tests remain in the full suite. |
| Repair instruction, association and refusal | RepairAuthor retains the return and clears its driving/submitted state. Cancellation of the separate instruction check remains review row 7. |
| Startup Load | Restores the exact input to the existing Load slot and stops. A failing-author caller test that detects removing this restoration is still owed. |
| Worker organ-fault report | FoundationInput retains the report and return before propagation. |
| Worker Stop TurnClosed | StopWork retains its return without inventing a close sequence; a protective caller test remains owed. |
| Worker emission refusal | EmissionWork retains exact input/stage through cancellation; existing actual emission custody test remains in the suite. |
| Claimed measurement, placement and Enter-refusal authors | AuthorRecord retains the original and clears the unrecorded ticket. A protective actual caller test for that clearing remains owed. |
| Freshness | Reuses waiting_content for its caller's withdrawn original; no second queue. Generic owner identity and nonfatal reason replacement remain rows 3/4. |
| Read recall BusyAuthored and failed pair filing | Their existing transaction retains the attempted original before failure; no fabricated receipt or acknowledgement. |

## Planner disposition

Planner review 5637217403 of #314 at a95e0a6d reported two fix, thirteen batch
and three file rows. This act addresses the two fix rows; it does not claim the
remaining caller/lifecycle work complete.

| Review row | Disposition |
| --- | --- |
| 1, fix | The shared continuation takes the withdrawn marker before it can queue. A real freshness withdrawal returns its exact original through this road; an isolated healthy withdrawn slot cannot acquire a sequence. |
| 2, fix | Every nonfatal hold uses the occupied-slot helper. A real marks fault continuation encountering an independent read hold returns its new fault and preserves the prior caller allocation. |
| 3, batch | Open: claim-less withdrawn returns lack a per-caller identity. Private AuthorClaim matching is retained. Full caller composition must settle the generic owner. |
| 4, batch | Open: a later nonfatal live Wait can replace the withdrawal reason. This act preserves the existing reason policy. |
| 5, batch carry | Open: no production taker yet drains ordinary/template held slots. The existing started flag is not a successful admission. |
| 6 and 16, file | Filed together as #316: a nonfatal returned author can keep a cancelled filing/OUTPUT owner replaying the same reason without completing its take. Full bounded lifecycle exit is owed. |
| 7, batch | Open: repair cancellation must retire the instruction check independently of returned author custody. |
| 8, batch | The template-held precedence change is recorded explicitly above. |
| 9, test gaps | This act adds real freshness/marks continuation coverage and a no-resubmission control. The reported take_withdrawn live-precedence, early-latch, AuthorRecord ticket, Startup Load and StopWork caller controls remain open; they are not credited as enforced by unrelated tests. |
| 10, batch | Open: owed withdrawal can classify an original as first-fatal evidence when failure was already latched. The first failure itself remains immutable. |
| 11, cleanup | Hold storage is now shared and the twelve touched redundant owner-error wrappers are removed. Separate caller retention helpers keep their different invariants; narrow lint allows remain rather than suppressing the module. |
| 12, record | One current record and gate result replace the earlier counts and appended history. Git and evidence deposits retain prior versions. |
| 13, record | #315 maps all twelve #312 regrade rows, including its carries and reasoned dispositions. |
| 14 and 15, carries | Open: worker prefix Busy and healthy rollback Busy exit. They belong with the actual caller/lifecycle composition. |
| 17, process | Self-check skills remain unavailable. No invocation or independent author-review pass is claimed. |
| 18, file | #313 now includes the Planner-reported parallel startup race and the existing generation race. Serial passes do not qualify either parallel run. |

## Instruments and validation

Three new tests cover the changed behavior. One drives the real freshness owner
and invokes the same waiting-content continuation used by filing/output/sort.
A second isolates a nonfatal withdrawn-slot state with an exact unadmitted event
and no ticket or fabricated success; the continuation must return it, not record
it. The third drives a real rejected mark to its actual fault author, then moves
an actual read owner from another task solely to isolate the defensive competing
owner case. That read is neither driven nor used as evidence in the other run,
and is returned to its original fixture afterward. Ordinary public admission
excludes that overlap; the test does not claim it is a production sequence.

The first two focused tests passed. An attempted nonfatal freshness fixture
raised place debt under an ordinary pending observation; that debt applies only
to release/Unload admission, so the ordinary observation remained pending and
its Cut expired. Its failing source and log are retained. The replacement makes
the nonfatal slot precondition explicit and leaves the real freshness test beside
it. All three focused cases pass on the corrected source.

The same child adds three boundary regression tests and corrects the eleven
existing boundary_batch_ cases as described in the held-boundary record.

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

Full author-site migration in worker/dev_loop, generic held-caller identity and
taking, Stop/Leave cancelled-owner exits, failed-run teardown, final world close,
worker correction and pair composition, production artifacts, measured limits,
security disposition and qualified coupled cutover remain under #146. The legacy
serving implementation stays active; only the operator merges or deploys.
All new PRs remain drafts for independent review at their exact heads.

The review exposed two paths omitted by the original caller walk: an internal
continuation could pass the exemption and then reach a competing hold, and a
caller could use the waiting-content continuation instead of either resume API.
Protecting a slot at only one admission site did not enforce its lifetime.
The correction puts preservation at the shared storage boundary and withdrawal
at every shared return road, then tests exact custody separately from Recorded.
