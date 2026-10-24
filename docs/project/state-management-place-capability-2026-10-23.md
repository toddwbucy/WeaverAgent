# Placement capability and registration review followups

Date: 2026-10-23. Branch: code/state-place-capability.
Exact parent: #284 e3da515821b2cf2d2993f266d01bba60001ae9a8.
This record advances issue #146 and answers the eight followup rows on #278 at
fd873096. It is not a Spec amendment or production qualification.

## Authority and scope

Implements: merged HARNESS-RECORD-LIFETIME and HARNESS-HALT stopped custody;
HARNESS-PLACE and Spec 06 PLACE-CALLS, PLACEMENT-RECORD, STAGED-APPEND and CUT-WAIT;
Spec 06 native marks and the existing ledger's exact source/receipt ownership.
Author assessment: conforms within these dormant owner and return primitives.

| #278 row | Implementation and evidence boundary |
| --- | --- |
| 1 | take_placement and take_placed_generation require Entered before consuming completion or journal state. Real cancelled completed owners in Releasing retain their owner and bound. Failed overlaps live() and preserves its first latch. |
| 2 | PlaceAccount returns inputs only in Entered. Place/Generation staged extraction also checks complete, non-incomplete accounts. Releasing/Failed keep borrowed evidence. The adjacent unplaced-selection take uses the same phase predicate. |
| 3 | One can_return_inputs predicate serves all three consuming accessors. An account already extracted in Entered does not dynamically observe later context phase changes; subsequent actual admission still governs use. |
| 4 | hold_reload_text and consumed-input validation require exactly the named single slot. A real two-Text checked output returns its multi-slot lease unchanged; existing single-source retry remains valid. |
| 5 | release_batch validates every registration and duplicate before a nonfallible removal pass. The tail is cleared only after success. Real foreign and duplicate handles leave earlier registrations intact; positive removal is checked while caller handles still stand. |
| 6 | Engine and native owner share an immutable Arc<NativeScopes>. The owner retains its exact proposal through completion/extraction; engine updates use copy-on-write. Generation retains its existing owned NativeScopes and original grant/cache/current-summary checks. |
| 7 | A noncancelled done reload with unrecorded Staged faults before owner removal. The actual completed-owner test removes only the Recorded projection through a labelled defensive seam. |
| 8 | Separate incomplete/Recorded guards, actual Taken status, ordered consumed indexes and real foreign origin receive independent controls. Thirteen new tests comprise eleven owner tests and two registration tests. |

The source joins nine exact preimages and two clean three-way changes above the
#281 author claims. The joined placement/generation files were independently
checked to retain WaitingAuthor/AuthorRecord, actual ticket/sequence binding,
trace-only wrappers and settled-cancellation behavior. The remaining nine paths
retain their prior independent source review, followed by formatting.

## Actual release fixtures and initial corrections

A completed ordinary placement still blocks TurnClosed through its owner. It
also has no owed work to authorize begin_place_release. The final phase test
therefore starts real Staged placement, sends its actual measurement answer,
enters specialized release while that account is owed, and settles it through
production account steps without taking it. Only then does it test the retained
completed cancelled owner in Releasing. There is no fabricated phase, done bit,
receipt or successful ordinary close.

A completed between-turn reload uses actual ordinary begin_releasing. A cancelled
unsent generation still owns its outstanding grant and uses the specialized owed
release. Tests observe the production owed-state interface and retained owner;
private owner fields remain private. The unplaced-selection check also refuses
its actual empty selection rather than returning Some(empty) in Releasing.

Initial gates found a missing test-only Arc import, two completed fixtures using
the owed-work release entry, then the placement fixture's independent TurnClosed
hold. An intermediate assertion also tried to read private PlaceTransaction
fields and was replaced with the existing production owed-state observation.
All failed build/test logs and pre-correction source remain preserved. Only the
new test module changed to correct those fixtures; no production admission guard
or protective assertion was weakened. The final fixture sequence received an
independent source check that explicitly corrects the earlier reachability read.

## Validation and limits

Full harness gates pass 1,811 top-level tests with 18 inherited ignored cases;
trace passes 160. Ten nested child-exec summaries remain separate from the
harness count. Focused groups pass eleven capability, two batch-registration
and five prior phase tests. All-target harness Clippy, formatting, lock and diff
checks pass. All dependency-resolving Cargo commands use --locked and one private
serialized target. The full gates follow all fixture corrections.

All seventeen source-pinned controls compile and fail their named tests, and
all seventeen exact restored named runs pass. The separate compiler pin rejects
direct mutation through the shared proposal with E0594; its exact restoration
passes the native-owner test. These are distinct behavioral and type-property
checks, not eighteen runtime mutations. All eleven source hashes match the
pre-control tested snapshot. No control survived or failed to compile.

Two recipe anchors were refreshed for rustfmt's equivalent closure shortening
and chain wrapping before execution; the initial metadata assertions and both
forms remain recorded. This changed no production source or test. The initial
build/fixture failures above are preserved separately from successful controls.

P4 remains redundant with ordinary revocation/cancellation. P6/P7 independently
alter only one projection on an actual completed cancelled reload, keeping the
other guard permissive. P10 changes only the actual Taken registration status;
it fabricates no Ready proof. P11 swaps real index/lease tuples, and P13 uses
another actual context's origin. The native proposal test performs a labelled
later engine copy-on-write to distinguish retained original ownership. These
are defensive consumer controls, not claims of ordinary concurrent races.

Batch removal tests retain real caller handles so Drop cannot mask explicit
physical removal. The production mid-loop failure was unreachable after the
existing complete single-thread checks; the foreign/duplicate batch is an API
invariant test. No independent bite is claimed for removing only the production
call and relying on ordinary Drop/pruning. Phase/completeness guards can overlap
private account constructors; counts do not make each overlap independent.

The known #280 parallel child-fixture observation issue remains tracked in its
followup; this passing full run does not establish its repair. Full healthy and
failed Enter composition, remaining reviewer rows, production language artifacts,
reviewed prompt, measured limits and coupled cutover remain open. #260/#263/#268,
#276/#282 and #285 remain unimplemented proposals. Existing serving stays active.
Author checks/source review are not a Planner grade. Only the operator merges,
deploys and requests CodeRabbit; all new PRs begin as drafts.
