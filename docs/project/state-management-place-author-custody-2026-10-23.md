# Placement author identity and terminal cleanup

This slice advances #146 on code/state-place-author-custody, above exact #274
f3b16c7f7504033290fa6f4c56cab55c953b4f14. It answers #269 Planner review
5624951821 rows 4-6 and targeted evidence duties Q6, Q12/Q13 and Q24.

## Authority and behavior

Implements: Spec 06 PLACE-CALLS, PLACEMENT-RECORD, FIT-REFUSAL, CUT-WAIT and
STAGED-APPEND; HARNESS-PLACE, HARNESS-RECORD-LIFETIME and HARNESS-CLOCK;
the decode contract's typed refusal and Cancel ordering. Author assessment:
conforms at the dormant owner boundaries, subject to Planner verification.

Measured fit and decoder refusals now name their actual reserved assembly.
An earlier failed port admission can burn an assembly before the same retained
proposal retries. Premeasurement input refusals still name that proposal.

Placement and measurement author owners queue the exact original, claim its
allocated ticket before first recorder resume, and bind only the matching
actual Recorded ticket/sequence. A private owner identity accompanies the
queue, with no ticket prediction or event-shape lookup. Both ordinary and
trace-only recording retain this witness before fallible observers. A foreign
genuine author cannot satisfy the owner's record or close its turn. Existing
other author consumers retain their atomic submission and completion order.

The unused private cancel_place entry point is removed. A whole-source census
found no callers, and the merged Spec does not require its separate Measuring
or Recording cancellation operation. Required reserved-grant cancellation,
generation cancellation and observed Stop/Leave revocation remain unchanged.

The new fatal append test exposed an additional cleanup defect. After taking a
complete NotOpen or OutOfOrder answer and latching its first failure, cleanup
started a new Cancel and waited for a new AtRest. cancel_generation now checks
whether the exact reservation is already settled before opening that exchange.
An already queued Cancel still requires its own AtRest. Invalid or unconsumed
answers remain unsettled. Original failure, raw answer, journal and owed refusal
record are retained, and no new decode request follows a settled fatal refusal.

Only advance_generation and advance_place_account call that cancellation
primitive in production. Both retain their existing unsent withdrawal and
sent-exchange accounting. No wire shape, workflow, deadline or cancellation
outcome changes.

## Tests and limits

Twelve new tests cover actual failed-admission retry assemblies, exact immediate
and pressured author tickets, trace-only records, typed append refusals, unsent
Staged return and subsequent measured generation, actual sent Cancel/AtRest,
and frozen cut-boundary checks while real native marks drain.

Foreign-ticket substitution and saved-prefix corruption are defensive boundary
tests using real owned proofs, not demonstrated production races. The normal
single-author facade prevents the ticket substitution. Q12/Q13 corrupt only
saved N or one per-message source assertion and require InvalidPrefix before
measurement. The positive case drains actual marks and completes Flush/Staged.
Q24 separately checks the exact unsent-port refusal classification; outgoing
Busy is an overlapping defense. Actual unsent owner cancellation sends no wire.

The Failed plus admitted-before-failure branch retains only matching factual
place/measurement evidence. Current valid author kinds do not reach that inner
selected-intake failure road, so no lawful production reachability or independent
behavioral control is claimed for it. Nineteen parent custody paths, typed Boot
cleanup and the Stop-return count correction are preserved by explicit joins.

## Author validation

Serialized private-target gates used lock.sh first, the pinned toolchain and
--locked. All twelve focused new tests pass. Full harness: 1,733 passed with
18 inherited ignored cases. Trace: 160 passed. Affected harness all-target
Clippy with warnings denied, fmt, lock and diff pass on the final source.

The first compile found private-field access in the new drain tests. Test-only
named evidence and borrowing accessors corrected it without widening production
fields. The first runtime run exposed the fatal-refusal Cancel defect. A direct
wire assertion reproduced it before the implementation fix. Initial compile
and runtime logs remain in evidence. This is a real implementation correction,
not a relaxed test expectation.

All fifteen behavior controls compiled and failed their intended assertions,
including removal of the new settled-terminal guard. Every exact restoration
passed its named test. All thirteen changed and two additional probed source
hashes match. Existing type pins remain separate. Independent joined-source and root-fix
reviews are author checks, not Planner grades or production qualification.

## Review and completion boundary

#269 rows 1-2 are closed by the Planner's #273 grade. Rows 4-6 and the named
test duties are addressed here for review. Row 3's dropped extracted-grant
window remains filed; rows 7-8 settlement/drain cleanup remain separate.
Q2/Q7 remain redundant, Q8/Q9 overlap and Q14 remains redundant or untested.
No independent protection is inferred from those original surviving probes.

#273's later eight batch rows remain tracked. Its record is corrected here to
name the retained, possibly re-prepared prefix and to retract the unsupported
claim that an existing Releasing test independently pins native phase adoption.
Its N1/N2/M3/M6 are redundant or implied; N4/M2/M4/M5 remain test gaps.

Full failed-run teardown, lifecycle/serving composition, #272 runtime followups,
qualified language/model/prompt artifacts and coupled cutover remain required.
Approved but unmerged #260/#263/#268 remain unimplemented. This slice closes no
issue and changes no deployment or active serving selection.
