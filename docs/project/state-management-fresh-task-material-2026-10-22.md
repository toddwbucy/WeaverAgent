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

The unit contains the exact full Text bytes and user role, its original source block,
one full UTF-8 MaterialRef interval and one exact source-map entry. It has no pair,
span or previous-step query attribution. An existing open task still supplies fresh
input material, without inventing a new mark receipt or claiming that input is the
existing task's original prompt.

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
unit in any context.

All shape validation precedes the custody transfer. Successful conversion has no
subsequent clock read or recoverable yield. The adapter issues no read, source filing,
recording, placement, decoder request or mark-position operation.

The private successful producer already establishes the filed input shape. The new
adapter also checks that it is nonempty, one-Text and user-role and validates its
constructed source map. These defensive checks are not advertised as demonstrated
runtime refusal paths for corrupt private successful owners. No test manufactures a
BegunTask to reach such impossible producer states.

## Evidence prepared

Nine new tests use the existing actual socket/recorder begin-task fixture. They cover:

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

Compile-time ambiguity pins reject From conversions from raw Messages, SeatedInput,
AcceptedInput and TaskAccount into HeldMaterial or SelectedUnit. Existing no-Clone and
raw-read conversion pins remain in force. Private construction and the absence of a
production deserialization path are source-reviewed, not claimed as universal trait
proofs. Test-only snapshots inspect genuine retained evidence and construct no grants.

Validation on 2026-10-22 used private target
`/tmp/weaver-146-fresh-task-material-target` and locked dependencies:

- All nine focused tests passed.
- Full harness suite: 1400 passed, with 18 inherited ignored cases.
- Strict all-target harness clippy, formatting and lock checks passed.
- All ten compiled protective mutations failed their intended tests, with each
  exact restored baseline passing.
- Eight E0283 probes rejected all named raw-source conversions. The exact
  restored source and final baseline check passed.

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
cutover. Checked Open authority remains dependent on draft #232. Fresh result units
still need actual kept request and statement coverage. Fresh certified instruction
material still needs its own combined template and filing evidence. Selection policy,
optional candidate ranking, legacy author migration and runtime activation remain
owed.
