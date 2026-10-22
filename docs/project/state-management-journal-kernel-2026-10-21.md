# State management private journal kernel

Date: 2026-10-21. Executor implementation record for issue #146, journal work.
Branch: `code/state-journal-kernel`, stacked on #234 at `1c5257b`.
This is a completed private arithmetic slice; production ownership remains owed.

## Implements and boundary

Implements: state-management-spec-06-marks-renderer-engine POSITION, MESSAGE-MAP,
COVERAGE-JOURNAL, PREFIX-PREPARE and N; state-management-spec-08-sort-selection
COVERAGE. These are scoped arithmetic and ownership clauses. Author grade: conforms for
the private kernel operations below, with live ports and grants outside this slice.

The new private journal has no production initializer. Its confirmation inputs have
private fields and no production constructors. Tests alone establish a fixture journal
and supply already-confirmed append, cut, mark and source facts. No integer, wire
record, decoded Message or SelectedUnit can construct a usable production journal.
This act does not claim a boot owner, a PreparedPrefix, a PlacementGrant, a place port,
a mark registration port or a generation completion owner.

The implemented core provides the following operations for those later owners:

- Reserve checked assembly and Message IDs. A cancelled reservation burns its IDs and
  installs no layout, source or physical boundary. A second reservation returns Busy.
- Build canonical occurrences from immutable SelectedUnit values, retaining the actual
  RecordedRead evidence through shared immutable ownership. Equal source sequences from
  a different context fail the origin check. Repeated units get distinct occurrence
  and Message IDs; their source locations are preserved separately.
- Apply a supplied confirmed completion atomically, preserving the exact canonical
  assistant Message, including empty content, empty Text and UnrecoveredCall. Rejection
  and pair classifications have no path that edits that content.
- Validate and order cursors by current layout, block and decoded UTF-8 endpoint.
  Message IDs are identities, not position numbers. The system head is separate.
- Project a canonical prefix and its exact source coverage. Repeated locations remain
  in the retained messages; the coverage list is ordered and distinct. Text coverage
  uses source identity and interval containment, not equal bytes or merged partial
  intervals. Request and returned coverage use separate exact CallIds.
- Derive N from a still-resident completed whole-generation boundary at or before the
  cursor, falling back to the admitted prefix count. No byte offset, rendered text,
  measured proposal or intermediate stream event creates a token position.
- Apply supplied confirmed suffix-cut facts, retain the canonical rebuild with its
  original IDs and proofs, remove later physical boundaries and invalidate later marks.
  Successive cuts cannot reuse a removed boundary. A later append installs the logical
  rebuild once, then the new units and canonical answer.
- Advance the revision on completion, cut, annotation and mark registration. Every old
  projection becomes stale, including one from another journal with equal counters.

The core checks arithmetic, shape, origin and identity before changing state. Invalid
confirmation, source mapping, cursor and counter exhaustion leave the active structure
unchanged. IDs already exposed by a reservation stay consumed on cancellation.

## Cursor, coverage and physical boundaries

A byte cursor addresses only Text and must be a UTF-8 boundary. A cursor at Text length
orders identically to keeping that whole block. block_end zero addresses only a real
empty Message and requires null byte_end. An empty assistant answer still has its own
Message ID and completed physical boundary. Whole calls, returns and unrecovered calls
have no byte cursor.

A partial retained Text projects its local source interval back to the matching
absolute ContentRef interval. Prefix preparation does not mutate the original source
map or the immutable read. An instruction's StepQuery occurrence remains attached to
its whole unchanged Message; a shortened Message cannot claim the complete instruction.

Each whole pair placement retains the request and returned locations for that exact
occurrence. A cut cannot keep part of a request interval, split a whole block, or keep
only one side of that placed pair. Earlier coverage with an equal CallId cannot excuse
splitting a later reminder of that pair. Result-only placement is not implemented.

The only physical boundaries in this kernel come from the private confirmation input
at the end of a complete assistant Message. The core does not itself establish that
input's decode authority. Rebuild remains logical content in RAM after a cut, even
though its former tokens have been removed. Its original Message IDs are reused when
recomputed. Prefix and resident comparisons reject inconsistent counts instead of
inventing a token position. Flush and suffix Elide share this arithmetic; neither wire
operation is issued here.

## Evidence prepared

The core tests exercise UTF-8 interior endpoints, invalid cursors, reversed numerical
Message IDs, distinct empty answers, whole-block cursor refusal, cancellation and ID
exhaustion, invalid confirmation rollback, successive cuts and recomputation, source
identity and repeated coverage, revision invalidation, exact repeated mark identities,
and indivisible request Text.

A real state-stream fixture completes paging and release, records recall, constructs
HeldMaterial, and passes its SelectedUnits into the journal tests. It checks exact read
bytes and recall sequence remain retained, foreign-context proof rejection, distinct
reminder occurrences and per-occurrence pair-cut safety. It makes no claim of an actual
decode or a live boot journal.

Compile-time ambiguity pins reject Default, Clone and From<u64> for Journal,
PrefixProjection and PendingAppend. Private construction and the absence of a wire
conversion are also source-reviewed; they are not a claim that every possible trait
conversion has been mechanically proved absent.

Serial Olympus checks used /tmp/weaver-146-journal-target and --locked after the
lock gate. The complete harness suite passed 1231 tests with 18 inherited ignored.
All-target strict clippy, formatting and diff checks passed. Initial formatting and
one test-only clone lint were corrected; the 13 focused journal tests passed on the
final source after that cleanup.

All 17 compiled protective mutations failed their intended focused tests. Each
exact source restoration passed. Nine added trait implementations (Default, Clone
and From<u64> on Journal, PrefixProjection and PendingAppend) each failed with
E0283 at the ambiguity pins; the restored normal test compilation passed.
Two independent source readings found no concrete defect within this private scope.
StepQuery preservation/drop remains source-reviewed, not separately mutation-pinned.

Logs, recipes, restoration hashes and final source hashes are deposited under
weaver-testing/state-management-146-olympus-2026-10-21/journal/ on the shared bulk
store. These are author checks, not a Planner grade or production qualification.

## Remaining authority and integration

Production initialization still requires the exact boot-held system content, the
admitted Open exchange and its prefix facts. The OPEN-FACTS measurement carrier remains
a separate owning-Spec proposal. This kernel does not infer boot authority from counts.

The real append owner must retain the proposal, schemas, original reservation and
bound; verify matching Generated identity, complete canonical answer, measured token
array shape and input/output/terminator arithmetic; and only then construct the private
confirmation. Raw Generation is not a shortcut into this kernel.

The source/mark owner still owes recorder and filing proofs for emission source maps,
PairStarted request annotations, native scopes and mark.position. Boot and native mark
registration are not inferred from the private arithmetic tests. Fresh result units
still need kept request and statement coverage plus their private terminal/tool-result
proofs. The full cut coordinator, staged carry ownership, trace-before-use sequencing,
placement grants, selection ports and serving/lifecycle composition remain under #146.

No serving path, classifier ownership, worker loop or deployed agent changes in this
slice. Runtime activation remains held for the qualified coupled foundation cutover.
