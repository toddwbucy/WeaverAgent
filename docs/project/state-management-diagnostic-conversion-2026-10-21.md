# State management diagnostic conversion boundary

Date: 2026-10-21. Executor implementation record for issue #146, item E, and
PR #226's owed runtime diagnostic conversion. Branch:
`code/state-diagnostic-conversion`, based on draft PR #238 at `542b5a7`.

## Implements

DIAGNOSTIC-CONVERT in the merged diagnostic amendment defines this local
boundary. Author grade: conforms for the typed conversion and refusal primitive
below, checked by the author gates. No Planner grade is claimed. DIAGNOSTIC-STATE,
preload/catalog isolation, the complete shared-author migration and the serving
cutover are separate work. This slice changes no deployed agent or selected
serving recorder and provides no production qualification.

## Boundary and ownership

Record::submit retains the existing legacy trace input and serving arm. Its
legacy diagnostic conversion now checks the exact supported kind/payload pair
before serialization. A valid legacy RecallAccount keeps its existing spelling.
A compatible kind with a mismatched typed payload uses the existing diagnostic
PayloadKindMismatch refusal. A missing required payload uses RequiredFieldAbsent.
Unsupported kinds, subsystems and new serving payload variants remain
OutsideDiagnosticVocabulary. A kind with a diagnostic counterpart alone cannot
authorize an opaque splice.

Record::submit_shared_diagnostic is a diagnostic-only entry point for the
replacement serving Event type. It explicitly selects compatible legacy kinds
and payload variants, then uses the same checked legacy converter. New registry
kinds, serving RecallRecord and SavePointRecord variants and extended unloads
refuse before the diagnostic writer. Diagnostic has no unload kind, so all
unloads remain refused, including either or both TRACE-HALT fields. Native
diagnostic constructors keep their existing envelope and 20-kind vocabulary.

The named envelope mapping discards channel and origin, which diagnostic does
not own. It explicitly retains every other envelope member and the exact raw
payload spelling. The input sequence is preserved by conversion. At submission,
the diagnostic recorder still binds its own run/session and allocates its own
sequence, as required by its existing contract. Conversion does not import a
serving record's identity or sequence into the diagnostic run.

Both diagnostic entry points refuse on the legacy serving arm with the harness-local
DiagnosticDoorOnServingRecord. This guard calls neither writer and adds no seam code.
The new entry point cannot send new serving events around the replacement
recorder's barriers, switch a run's record
mechanism, or activate the replacement serving loop. Existing callers still use
their existing input types. Their eventual migration must select this explicit
conversion boundary where a shared author belongs to a diagnostic binding.

Every conversion failure reaches Record's existing sticky failed latch. A
conversion refusal does not damage the diagnostic writer or consume its next
sequence, but a sound writer does not repair a failed run. Tests retain that
latch while probing the writer and never clear it or replace the Record's arm.
The lifecycle caller remains responsible for ending service on the failed latch.

## Instruments and initial validation

The new conversion test module drives actual Record submissions into readable
file descriptors. Its corpus covers all fifteen compatible replacement event
kinds and the separate legacy recall shape. Direct conversion assertions check
all input envelope members. Real writer assertions distinguish its own
run/session/sequence allocation from the converter's preservation duty. A literal
canonical line pins raw JSON whitespace, escaped Unicode and numeric spelling,
plus omission of channel/origin and absent optional envelope fields.

Refusal fixtures cover every unshared registry kind, wrong typed payloads under
legacy/shared kind names, typed read/boot recall, all four save phases, a loop
instruction and unload with neither, either or both halt fields. New payloads
are also tried under an otherwise accepted fault kind to test the payload guard
independently from kind rejection. Unsupported subsystems and use of the new
door on a legacy serving Record refuse. A refusal writes no bytes, consumes no
sequence and permanently latches Record even when a later mechanism probe
confirms the writer itself is sound.

Protective recipes target kind admission, payload pairing, serving recall/save/
unload refusal, every copied envelope member, raw spelling, subsystem admission,
record-latch propagation and the diagnostic-only arm. All eighteen compiled, failed
their named assertions and passed after byte-exact restoration.

The Olympus gate used private target /tmp/weaver-146-diagnostic-target. Lock,
formatting and strict all-target harness clippy passed. Eight focused conversion
tests passed. The full harness suite passed 1260 tests with 18 inherited ignored
cases. The diagnostic suite passed 16 tests. Two independent source readings
found no remaining concrete defect within this boundary.

Logs, mutation recipes, results and exact source hashes are retained under
weaver-testing/state-management-146-olympus-2026-10-21/diagnostic in the shared
bulk store. These are author checks, not a Planner grade or production
qualification. No deployed agent changed.

## Planner batch correction

The follow-up to review 5609688677 on 2026-10-22 separates supported-payload defects
from vocabulary refusal and wrong-record-door attribution. The legacy kind mapping
runs before the pairing check, so its unsupported-kind branches describe live refusals.
The check reuses trace's pure payload_pairing_licensed predicate, also used by the trace
writer, instead of a second pairing table. The predicate exposes no writer or admission
grant. Existing raw payload owners move directly into the diagnostic splice. Only
structured payloads are rendered, using to_raw_value without a separate explicit parse.
The test modules share one readable scratch-file fixture.

The required-payload fixture checks conversion directly and then real shared submission
for every compatible payload-bearing kind, including Fault with None. It requires the
named missing-field refusal, no bytes, no consumed sequence and the sticky Record latch.
A separate two-submission fixture preserves source sequences 71 and 5 through conversion
while the emitted diagnostic records use their own session/run and sequences 0 and 1.
The wrong-door fixture covers both shared and native diagnostic entries on a serving
Record. Existing pairing, unsupported-shape and raw-spelling instruments remain
in place.

The initial gate and eighteen-mutation results above precede this correction. On
the corrected joined source, the harness suite passed 1276 tests with 18 inherited
ignored cases. The trace suite passed 151 tests and diagnostic passed 16. The
diagnostic-focused harness filter passed 22 tests. Strict all-target harness and
trace clippy, formatting and lock passed with /tmp/weaver-146-diagnostic-target.
All 25 protective mutations compiled, failed their assertions and passed after
exact restoration. Logs, recipes, results and source hashes are retained under
diagnostic/review-batch in the shared evidence deposit. Root and a separate
Executor read the corrected source without a concrete finding. These checks
do not replace the Planner's regrade. No new carrier, limit, serving activation
or history rewrite is part of
this batch. New stack merges use the required descriptive subject prefix.

## Payload-free bracket follow-up

Planner follow-up 6334321041 found no blocking batch findings and retained D3, a
missing protective assertion for turn.started admitting another kind's payload.
The trace Spec's section 3 requires this bracket to carry no payload member. This
test-only correction gives an otherwise valid turn.started envelope a nonempty
Fault payload that is licensed under fault. Direct legacy and shared conversion
must return PayloadKindMismatch before the diagnostic writer can mask a converter
defect. Actual diagnostic submissions retain the failed-run latch, write no bytes
and consume no sequence. A later payload-free bracket is a writer-mechanism probe
with the latch still set, not permission to resume service.

A separate actual trace recorder fixture requires PayloadMalformed, an empty
working structure and no sink bytes after drain. The corrected payload-free
bracket receives sequence zero and emits exactly one line without payload. The
scope is turn.started. SessionClosed remains outside diagnostic conversion and
does not need a new case for this finding. No production behavior changes.

Both new tests passed in the serial host lane with private target
/tmp/weaver-146-diagnostic-target. Strict all-target harness and trace clippy,
formatting and lock checks passed. Each of two protective recipes widened only
TurnStarted's shared pairing predicate, compiled, failed the intended assertion
and passed after exact-byte restoration. The prior full-suite results above are
historical. This test-only follow-up claims its focused gates, not a fresh full
suite. Evidence is deposited under diagnostic/turn-started-payload.
