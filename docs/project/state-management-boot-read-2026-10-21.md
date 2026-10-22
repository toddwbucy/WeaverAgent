# State management Recorded boot-read owner

Executor implementation record for epic #146.
Branch: `code/state-boot-read-owner`, based on material commit `284f73f`.
The serialized harness suite passed 1230 tests with 18 inherited ignored cases.
The focused boot rerun, all-target clippy and formatting checks passed. Compiled
mutation checks and type-pin probes passed. Joined-stack validation and Planner
review remain owed.
These are author checks of this slice, not production qualification.
This record claims no serving activation, production qualification or epic closure.

## Authority and implemented boundary

Implements: the merged [harness amendment][harness] section 6, HARNESS-RECALL's
boot comparison and Recording-to-Recorded transition, and HARNESS-BOOT from the
first mandatory system_prompt read through the cached BootHead. Implements the
harness-owned comparisons in [TRACE-RECALL-BOOT][trace] against the loaded member
and completed read. These are scoped author claims, not a pass for complete
HARNESS-BOOT or the full state management goal.

The private boot entry fixes SystemPrompt and accepts no caller-provided seed,
selector, turn or trace parent. It obtains the exact StartIdentity already matched
by this context's Recorded load. Positive completion of the same load's actual
template transcript is required under its original Enter bound. An absent or
partial transcript is not completion. Taking the certified template capability
for later instructions does not erase the retained completion proof.

The existing idle-owner and barrier checks remain in the shared read admission.
A Busy call allocates no state request and consumes no first-read marker. The
first queued content Read commits its marker before any post-enqueue clock check
can yield. Metadata selectors retain their ordinary reference-only status. A
boot attempt cannot follow an earlier content read or restart after its own
accepted request, including typed refusal and cancellation. Later explicit
system_prompt queries remain ordinary Read recalls and cannot replace BootHead.

## Exact evidence and continuation

The shared read owner holds one purpose, Ordinary or Boot. Paging, original
request identities, complete-result validation and Released use the existing
owner. Boot preparation follows Released and compares the one seed entry with
the loaded seed ID, ordinal zero, span local zero and revision one, start zero,
and end equal to the complete prompt's decoded UTF-8 byte length. It rejects step
context and pair attachments. The normal provenance walker retains the exact
source and original binding ordinals without inventing a seed event.

The owner retains one canonical system Message containing one unchanged Text
block, its stored SpanRef/end, and one immutable RecallRecord::Boot. The recall
uses the existing turnless internal/runtime harness envelope and observation
author purpose. It grants no filing. Recorder pressure retains the same ticket,
payload and complete result. It does not repeat the state query or release.

The original read bound, capped by Enter and applicable Leave, remains armed
through recall Recorded. The actual admitted sequence is retained before any
recoverable trailing clock check. The boot read additionally owns its original
Enter until final seating or disposal, including the completed state awaiting
the caller. A generic take_read cannot transfer a boot transaction.

finish_boot_read checks liveness and the original wait before moving anything.
Its final transfer contains no clock or other fallible hook: the checked result,
Recorded sequence and prepared canonical content move atomically into BootHead.
The cache retains the exact context origin and Enter bound as well as Message,
SpanRef, UTF-8 end, snapshot ID and recall sequence. BootHead is non-Clone and has
no raw-message constructor or deserializer. Its borrowing accessors expose
content inspection, not decoder, journal, mark or placement effects.

Cancellation revokes head consumption while retaining any owed exchange,
snapshot release and recall author under the same bound. A successful cancelled
read retains its Recorded sequence inside the existing private cancellation
disposition and seats no head. Typed refusal stays distinct, emits no successful
recall and retains its cancellation disposition when applicable. Malformed boot
content or a failed seam uses the existing first-failure path.

## Instruments prepared

Ten new tests use the actual Recorded load, classifier/template transcript,
state stream, coordinator measurements and recorder. The scripted content and
template checker exercise ownership and byte preservation, not language quality.

| Property | Instrument and protective change |
| --- | --- |
| Exact seeded head | Multipage UTF-8 fixture; change role, byte end, loaded seed or stored span |
| Recorded before seating | Observe released and recorded continuations separately; remove the final completion guard |
| Exclusive boot owner | Attempt generic take_read before and after Recorded; allow it to steal the boot read |
| Positive certification | Absent, partially accepted and replaced-Enter fixtures; skip or weaken certification admission |
| First content and one attempt | Prior metadata/content reads and duplicate boot; ignore the retained markers |
| No marker loss after enqueue | Leave expires at the post-enqueue check; move the marker update after that check |
| Original Enter retention | Attempt to complete Enter while boot is active or awaiting finish; permit early disarm |
| Distinct refusal | Actual typed refusal with and without cancellation; restart mandatory boot or fabricate empty success |
| Cancellation | Sent, Released and Recorded cases; seat a head despite cancelled consumption |
| Trace pressure and wake | Real blocked writer, exact ticket/payload and resumed drain; drop or duplicate the boot author |
| Recoverable Leave | Expiry before recall authoring and after Recorded; consume a continuation before retaining it |
| Bound expiry | Original Enter expires with a released result; replace or retire the original wait |

Compile-time ambiguity pins reject Clone, From<Message> and Default for BootHead.
The absence of a deserializer is source-reviewed; no unjustified serde dependency is
added to the harness for a test-only claim. The successful fixture checks exact
context origin, retained Enter, no filing obligation, and a later ordinary prompt
read that leaves the original cache unchanged. No fixture invokes a decoder Open.

## Recorded-read review followups

This descendant also answers the authorized read-owner followups from
[Planner review 5609135461][read-review], under the existing HARNESS-READ and
HARNESS-RECALL failure and source-identity rules:

- cancel_read now states that it changes disposition only. The caller still
  drives the owner and takes Cancelled. Cancelling with no owner is a caller
  defect and follows the existing mismatch failure rule.
- A failed read closes its state seam only before its release disposition is
  retained. Released results, including the accepted-complete continuation before
  its trailing clock check, no longer cause an unnecessary shutdown or secondary
  CarryFailed. The first failure and retained evidence remain unchanged.
- Seed grammar, span revision and plan/step identity checks now run in the page
  validator. Metadata identities use the same rules. Recall conversion delegates
  its defensive checks to that owner rather than duplicating the grammar.
  Malformed received identities halt before Release and remain in rejected.

Two additional real-owner tests compare state-seam behavior before and after
Released, and retain the actual malformed answer for isolated entry-seed, span,
plan and step defects. The seed fixture isolates T11: its selector has no seed,
and no selected span can reject it incidentally. The span fixture changes only
a Marks response span revision; it carries no selected-content metadata or span
echo that could mask the revision check. Existing validator fixtures now
use canonical seed identities. The changed conversion test expects rejection at
page acceptance, not only after the cursor has already been released.

The read frontier's last_author_sequence interaction with capture remains an
explicit semantic election. This act does not settle it. Pending-recall state
channel failure attribution, a phase-enum refactor, neutral fixture-corpus
relocation and the remaining mechanical mirror cleanup remain separate batch
work. The trace corpus consumer checks admission/serialization; harness tests
own source traversal. No complete followup-closure claim is made for #229.

## Validation and remaining work

Parent-owned validation used the lock gate and serialized --locked harness
commands with a private Cargo target. The full suite passed 1230 tests with 18
inherited ignored cases; the focused boot rerun, all-target clippy and formatting
checks passed. Root corrected the large-enum lint by boxing the existing
CancelledRead disposition inside BootReadCompletion.

Earlier fixture runs exposed a noncanonical shared load seed, an assertion that
ignored the original read bound after Leave, an early-invalid-response fixture
that expected Release, and private-field access in a test. Those fixture defects
were corrected without weakening the production identity or deadline guards.

The first sixteen behavioral mutations compiled, failed their intended assertions
and passed after exact restoration. The span-revision mutation survived because
its history fixture also carried forbidden selected-span metadata. The fixture
now uses a Marks response with only its revision changed. That corrected probe and the final plan/step probes compiled and failed, with
exact restored reruns passing. All nineteen behavioral mutations are now pinned.
Three BootHead trait additions separately failed their E0283 ambiguity pins, and
the exact restored baseline check passed. The corrected negative fixture, strict
all-target clippy and formatting passed again. Recipes
live outside the repository. Each behavioral probe must compile, fail its
targeted assertion, restore exact source and pass the restored baseline; trait
probes separately require the expected compile-time ambiguity and restoration.

The future Open owner must check this context-owned head and its original Enter,
then use its real recall sequence as the causal source. This slice introduces no
Open carrier and uses no authority from draft #232. Decoder prefix validation,
budget fit, initialized journal, boot mark, boot pair repair, plan metadata,
gate opening, failed-enter teardown integration and old-author/serving migration
remain separate work. The existing serving implementation remains active until
the complete coupled cutover is ready and the operator approves it.

[harness]: ../crates/state-management/weaver-harness-Spec-amendment.md
[trace]: ../crates/state-management/weaver-trace-Spec-amendment.md
[read-review]: https://github.com/toddwbucy/WeaverAgent/pull/229#pullrequestreview-5609135461
