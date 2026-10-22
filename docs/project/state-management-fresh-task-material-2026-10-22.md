# State management fresh task-input material

Date: 2026-10-22. Executor record for issue #146, fresh material ownership. Branch:
`code/state-fresh-task-material`, stacked on draft #249 at 5512789. This ancestry
retains exact #190 and its qualified coupled-cutover hold.

## Implements and scope

Draft PR: #250.

Implements: state-management-spec-08-sort-selection UNIT and SORT-PORT's source
provenance boundary; weaver-harness-Spec HARNESS-MATERIAL and HARNESS-MARKS's
completed-task handoff. The author grades these scoped clauses conforms on the checks
below. This is not a Planner grade or production qualification.

The new `hold_task_material` port consumes an actual successful `BegunTask`. It
constructs one complete canonical user Text unit from that task's filed input and
retains the entire private task owner. Raw Message values, source IDs, SeatedInput,
AcceptedInput and refused or cancelled TaskAccount values have no conversion into this
material. The existing read-backed material constructors remain unchanged in meaning.

The unit contains the exact full Text bytes, its original source block,
one full UTF-8 MaterialRef interval and one exact source-map entry. It has no pair,
span or previous-step query attribution. An existing open task still supplies fresh
input material, without inventing a new mark receipt or claiming that input is the
existing task's original prompt. Its canonical user role is derived from the proved
message.user source kind, rather than copied from an inspection Message.

An explicit evidence variant retains the full BegunTask, including filed input,
receipt, marks catalog, scope and optional mark filing. The container and each unit
share that immutable evidence. Consuming the container or dropping the originating
context does not discard a retained unit's licensing evidence. A borrowed task view
allows later owners to inspect that evidence without recovering another task grant.
The completed task's real catalog supplies context origin, not content-read rank.

## Provenance and custody

The fresh unit's content-read slice and Recall-sequence iterator are empty. Its actual
marks Recall remains retained inside task evidence. It is not presented as a candidate
read, a new Recall or a content result. SORT-PORT permits such material as mandatory
input only. Optional candidate selection/rank enforcement is a later selection owner
and is not implemented by this adapter.

The admission check runs before consuming the task. A recoverable lifecycle refusal
returns the exact task owner for retry. A foreign-context task also returns intact and
latches protocol failure in the receiving context. Equal numerical source IDs cannot
substitute for the original context identity. The source evidence dispatches origin
and ownership by variant, so an empty read slice cannot vacuously authorize a fresh
unit in any context. Task shape and context-association defects belong to the harness
barrier, not a state-member reply. Task and read error handling share one fault mapper
while preserving that attribution difference.

All shape validation precedes the custody transfer. Successful conversion has no
subsequent clock read or recoverable yield. The adapter issues no read, source filing,
recording, placement, decoder request or mark-position operation.

The private successful producer already establishes the filed input shape. The adapter
also checks that it is nonempty and one-Text and validates its constructed source map.
Spec 02 CLASSIFY requires empty Text to be non_ste100. A non-template accepted reply
passes through Ste100Reply::validate_for and validate_bindings, which require nonempty
text and bindings, before AcceptedReuse and AcceptedInput can exist. Wire parsing alone
does not prove acceptance. Consequently an empty BegunTask is not reachable through
these real owners. The additional empty check is defensive and redundant with the
source-map validator's nonempty Span rule. No test manufactures a successful task to
claim otherwise.

## Evidence prepared

Twelve focused tests use the existing actual socket/recorder begin-task fixture. They
cover:

- Exact UTF-8 bytes, spacing, role, source identity and full source-map interval, with
  unchanged input/mark receipts and released/Recorded marks catalog.
- Fresh material without candidate read rank or a fabricated Recall sequence.
- An existing task's fresh input without a new mark or span occurrence.
- Retained evidence after the container and originating context are dropped.
- Equal source IDs in different contexts, returned custody and the receiving latch.
- Expired Leave before admission, exact owner return and retry without refiling.
- Successful ownership transfer before a subsequent Leave expiry, with no late yield.
- Actual writer pressure, held input receipt, held Release and held mark receipt before
  a successful task can become material.
- Real cancelled and refused task results that supply no successful conversion owner.
- Empty wire input receiving a valid non_ste100 reply, with its actual Recorded result
  and refusal but no AcceptedInput, turn, task or material owner.
- An actual malformed accepted reply for empty wire input, refused at the classifier
  boundary while retaining the exact reply and granting no accepted input or task.
- The internal Invalid fault mapper, using an explicit defensive error wrapper around
  an unchanged real successful task. The probe checks the first failure, harness
  attribution and returned receipts. It does not claim an invalid task was produced.

Compile-time ambiguity pins reject From conversions from raw Messages, SeatedInput,
AcceptedInput and TaskAccount into HeldMaterial or SelectedUnit. Existing no-Clone and
raw-read conversion pins remain in force. Private construction and the absence of a
production deserialization path are source-reviewed, not claimed as universal trait
proofs. Test-only snapshots inspect genuine retained evidence and construct no grants.

The published baseline at 4ddfd534 was validated on 2026-10-22 with private target
`/tmp/weaver-146-fresh-task-material-target` and locked dependencies:

- All nine focused tests passed.
- Full harness suite: 1400 passed, with 18 inherited ignored cases.
- Strict all-target harness clippy, formatting and lock checks passed.
- All ten compiled protective mutations failed their intended tests, with each
  exact restored baseline passing.
- Eight E0283 probes rejected all named raw-source conversions. The exact
  restored source and final baseline check passed.

The current follow-up adds the three tests above, derives the canonical role and
corrects internal fault attribution. Receipt comparisons now use typed identities,
canonical filing/receipt bytes and actual terminal facts instead of Debug strings.
The follow-up passed all 12 focused tests and the full harness suite: 1437 passed,
with 18 inherited ignored cases. Strict all-target harness clippy, formatting and
lock checks passed. Four protective mutations compiled and failed their intended
assertions for internal fault attribution, the defensive Invalid latch, the actual
upstream empty-acceptance boundary and canonical user role. Every exact restored
baseline passed. New logs and 487 source hashes are retained at
`/tmp/weaver-146-evidence/review-round2-250/`; these follow-up results have not
been deposited in the shared evidence store.

Planner review 5610852745's T1 role-check removal is equivalent because the private
input producer always supplies user role. That redundant check is now removed. T2's
empty check cannot be reached by a real successful task and the map validator also
refuses an empty interval. The new empty-input tests pin the actual upstream boundary.
T6's task-conversion Invalid arm is likewise defensive. Its common fault mapper now
has an explicit fault-classification probe, which does not establish runtime
reachability of that arm. Removing only the impossible conversion call site's mapping
can still be an equivalent mutation. No fake proof is used to make it fail.

The initial pressure fixture let its fake clock jump to the Decision deadline while
the actual writer was full. The corrected fixture uses an earlier Leave yield, checks
retained author/task custody and the unchanged Decision bound, then drains the writer
and resumes through the withheld source receipts. The initial failure log remains in
the deposit. Production timing was not changed.

Independent author readings of the material owner and corrected pressure fixture found
no concrete defect in this scope. The code-review skill was unavailable, so no
invocation is claimed. Logs, hashes, runners and recipes are deposited under
`fresh-task-material/` in
`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-21/`.

## Remaining work

This supplies genuine fresh user-source material. It is not a production placement
owner, journal initializer, generation confirmation, mark-position registration or
cutover. The checked Open owner is still unimplemented; its evidence authority was
merged in #232. Fresh result units
still need actual kept request and statement coverage. Fresh certified instruction
material still needs its own combined template and filing evidence. Selection policy,
optional candidate ranking, legacy author migration and runtime activation remain
owed.

## Review dispositions

Planner review 5610852745, carried to 902c088f by review 5611201842, has the
following dispositions:

| Finding | Disposition |
| --- | --- |
| T1 and row 5, role | Remove the redundant inspection-role check and derive canonical Role::User from the private message.user source. The existing exact-Message assertion pins that result. |
| T2, empty text | Demonstrate the real upstream boundary with both non_ste100 and malformed accepted replies for actual empty wire input. A successful empty BegunTask is not constructible through that boundary, and the source-map Span check independently rejects an empty interval. No manufactured task is described as a reachable producer. |
| T6, Invalid attribution | Share material fault mapping and attribute a task-owned shape defect to protocol_failed/barrier. A defensive error wrapper around an unchanged real task pins the mapper and retained receipts without pretending the invalid task was produced. |
| 1, UNIT table wording | The merged UNIT prose already admits fresh receipt-backed sources; a filed message.user is conversation material. Keep the proposed table clarification with the operator's Spec wording review, without adding a new source class or weakening any receipt check. |
| 2, recovering task ownership | Keep the complete BegunTask under the unit's evidence, available through the borrowed task accessor. BegunTask has no Drop action. An owned recovery port is deferred until a real placement consumer needs it and can specify consumption across the shared evidence; no current consumer requires a move. |
| 3, read indexing | Document explicitly that reads() can be empty. The only production consumer iterates; all direct indexing is in fixtures that obtain units through real completed reads. Fresh input has no fabricated read rank. |
| 4, duplicate fault mapping | Factor the shared failure classification while preserving the state-reply attribution for read-derived defects and harness-barrier attribution for task-derived defects. |
| 6, catalog retention | Retain the original Recorded marks catalog: it is the genuine operation provenance and carries context ownership. Dropping it or replacing it with copied IDs would weaken this slice's custody guarantee. Its allocation cost belongs in the complete placement owner's retained-memory measurements. |
| 7, boxes | Keep boxed task evidence to avoid expanding every read-backed Evidence allocation to the inline size of BegunTask. The error Box and successful evidence Box are alternative outcomes, not two task copies retained together. Moving either without a measured consumer requirement would trade heap allocation against larger values and errors, not remove retained evidence. |
| 8, refused/cancelled conversion | A refusal or cancellation supplies TaskAccount, not BegunTask. The existing compile-time pins bar its conversion to HeldMaterial and SelectedUnit, and the actual transaction test checks that no successful task can be taken. Calling hold_task_material with that account is a type error, so no fabricated success owner is introduced for a runtime test. Production placement remains outside this slice. |
| 9, Debug comparisons | Replace task evidence snapshots with typed input identities/Message/scope, exact catalog bytes and snapshot, canonical filing/receipt bytes, terminal facts and filing sequence/time. These are inspection copies only and cannot reconstruct a receipt-bearing capability. |

The source-class sweep covered Evidence::origin, reads, task and belongs_to;
SelectedUnit and HeldMaterial accessors; all from_task outcomes; both context
fault mappers; and direct reads() indexing. No runtime read consumer assumes a
nonempty slice. The task conversion still performs no I/O, filing, placement,
mark positioning or post-consumption clock read.
