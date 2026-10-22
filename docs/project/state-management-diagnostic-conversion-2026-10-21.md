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
A mismatched typed payload is OutsideDiagnosticVocabulary, not an opaque splice
accepted merely because the kind has a diagnostic counterpart.

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

The new entry point refuses on the legacy serving arm. It cannot send new
serving events around the replacement recorder's barriers, switch a run's record
mechanism, or activate the replacement serving loop. Existing callers still use
their existing input types. Their eventual migration must select this explicit
conversion boundary where a shared author belongs to a diagnostic binding.

Every conversion failure reaches Record's existing sticky failed latch. A
conversion refusal does not damage the diagnostic writer or consume its next
sequence, but a sound writer does not repair a failed run. Tests retain that
latch while probing the writer and never clear it or replace the Record's arm.
The lifecycle caller remains responsible for ending service on the failed latch.

## Instruments and validation

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
