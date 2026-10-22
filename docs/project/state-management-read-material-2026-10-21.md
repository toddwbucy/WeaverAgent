# State management read-backed material

Date: 2026-10-21.

This slice answers the read-backed part of issue #146's material and placement
prerequisites. It extends the RecordedRead owner from draft PR #229 and stacks on draft
PR #231. Existing serving remains active; these private replacement ports do not
activate a new loop.

## Implements

- Spec 08 UNIT: block, statement, complete pair and successful closed/retired span
  units from released, validated and recall-Recorded reads. Author grade: conforms for this scoped portion.
- HARNESS-MATERIAL: private constructors, original binding ordinals, source-kind roles,
  exact byte intervals, whole pair attachments, repeated locations, immutable units and
  retained read provenance. Author grade: conforms for this scoped portion.
- Spec 06 MESSAGE-MAP, unit-local portion only: exact Text and whole call/result maps,
  ordered distinct coverage and retained StepQueryOccurrence. Author grade: conforms for this scoped portion.

Fresh result and instruction units are not implemented here. A fresh result still needs
its live terminal grant and private kept-request/statement coverage; a fresh instruction
needs its certified template and filing receipt. BootHead, journal initialization,
physical boundaries, conservative N, marks, placement, generation confirmation, actual
sort selection and serving integration remain separate work. OPEN-FACTS remains subject
to its owning-Spec proposal. This slice creates no replacement implementation authority.

## Ownership and behavior

Each RecordingContext creates one private Arc identity. RecordedRead receives that
identity only when the read owner transfers its completely released and Recorded result.
Neither equal run names, snapshot IDs, raw bytes nor trace sequences can substitute for
the originating context. HeldMaterial consumes the proof; immutable SelectedUnit values
retain shared ownership of that exact evidence after the holder is consumed. Their
inspection accessors and floor Message records do not construct live input, tool,
filing, dispatch or placement grants. Future consumers must check the originating
context when accepting a unit, as the current material entry points do for reads.

Subjects/conversation create complete block units. History/condition/statement preserve
one original binding and its exact statement interval. A span is one atomic unit with
all returned occurrences; repeated Text locations remain separate while a CallId expands
once within that unit. Repeated Text and pair occurrences must keep their exact original
bytes, event attribution and binding ordinal/value association. The same checks cover
CallIds across units and every referenced opening/terminal event. Pair construction
takes an actual read attachment by entry/pair index and a second Recorded statement
query for its original StatementRef. It rejects changed source bytes even when the
source identity matches. Both proofs remain owned.

Every held request and terminal is retained in the original read evidence. External
requests preserve name/arguments; review requests use sh and the exact command; inward
recall maps its complete request statement without fabricating a ToolCall. Terminals
with outcomes use the same pure formatter as live ToolResult completion. Null outcomes
use complete canonical PairEnded JSON. Only floor ToolResultBlock records are made.

A valid open attachment refuses IncompleteCandidates without omitting the pair or
latching a component failure. System material refuses InvalidSpan. Preview and metadata
selectors return the private NotMaterial disposition. Malformed attribution and foreign
proofs latch protocol failure, with the original read inputs retained in the error.
Lifecycle admission checks precede consumption; an unadmitted caller keeps the proof for
its own retry. Conversion itself has no I/O continuation or new timing interval.

## Evidence

The new tests drive the actual state stream, complete paging, Released response and
recall recorder before constructing material. They cover filtered UTF-8 intervals,
complete block attachments, external/review/inward pairs, null and typed terminal
content, repeated span positions, source roles, step-query provenance, foreign contexts
with equal trace metadata, lifecycle refusal and exact input retention. Compile-time
pins exclude Clone and conversion from raw records for the private material types. Their
lack of Deserialize is source-reviewed, not compile-pinned. Existing tool-grant
constructor manifests remain applicable. Serial Olympus validation used
/tmp/weaver-146-recall-target, --locked and the lock gate before Cargo. The harness
suite passed 1218 tests with 18 inherited ignored cases. Strict all-target clippy,
formatting and diff checks passed. Nineteen compiled protective mutations failed, each
with byte-exact restoration and a passing focused rerun. Six compile checks deliberately
added Clone or From<Message/QueryResult> to the two private material types and failed
E0283 at their negative pins. The restored material tests passed. No Planner or
third-party grade is claimed.

Independent review found inconsistent repeated binding and pair identities across one
immutable read. A read-wide source/pair/binding check and three real-read regressions
now reject those contradictions. Subsequent review found no concrete defect. The first
compile exposed an unavailable serde trait in a test pin; the pin was removed rather
than adding a runtime dependency. The first behavioral run exposed noncanonical fixture
seed IDs. Strict production validation was retained and the fixtures corrected. A
test-only lint correction passed its focused rerun and strict lint gate. Failed logs are
retained alongside the successful checks.

Logs, source hashes and mutation/restoration evidence are deposited on the shared bulk
store at
`weaver-testing/state-management-146-olympus-2026-10-21/material/`.
Issue #146 remains open; only the operator merges.
