# Cancelled Text-only and empty correction counts

Date: 2026-10-24. Branch: `code/state-cancelled-correction-count`.
Parent: #318 at `1d82867eb2ee4616a83dbee80f94405c3f28e18b`.
Stack: main, #312, #314, #315, #317, #318, then this draft.
Authority: merged #263, Spec 02 CANCELLED-CORRECTION and EMPTY-CORRECTION,
OUTPUT-UNIT and the harness amendment HARNESS-RECALL-REFUSAL. Refs #146 and #213.
This record describes a partial implementation of #263, not its completion.

## Behavior and authority

A complete, nonempty correction with a recorded language defect and no attempted
call now charges one failed attempt at its last Recorded checker result. The
count is retained even while the classifier's final release probe yields, an
output loan remains with the caller, or a later clock fails. Cancellation changes
neither that boundary nor the exact rejected-source/correction identity. At the
cap, accounting alone grants no retry, EffectiveEmission or RepairExhausted and
authors no new repair_exhausted event.

OutputChecks retains one actual Recorded reply while classifier release is
pending. The original recorder proof validates that copy before a fallible
clock or release probe. This evidence does not make OutputChecks complete and
does not mint a CheckedGeneration. Ordinary release still owns that grant.
ComponentFailed follows the existing fatal classifier path and cannot count.
The retained copy is bounded by the one outstanding classifier exchange; its
memory cost remains part of production limit qualification.

The repair owner loads RequestScheme once from the exact validated artifact.
Association and early accounting share its attempted-call predicate. ToolCall,
UnrecoveredCall and accepted recall bindings, including malformed recall, all
exclude the Text-only boundary. Every original Text must have a Recorded result;
a partial checked prefix, an accepted pass or an absent correction counts zero.
Ordinary accepted Text filing adds no extra wait to this count.

RepairAccount retains each failed pass's rejected source, correction source and
per-source cumulative count. Identity-based retention prevents recharging when
the checked loan returns. The already issued instruction ordinal stays stable
when the count arrives before that return. Existing live call accounting uses
the same retained identity record; its late boundary is not claimed fixed here.

An empty checked pass returned after cancellation still owes its exact attributed
EMPTY-CORRECTION refusal. It counts only when that refusal is Recorded, with the
existing detail and causal parent, and requires no invented checker or pair.
Cancellation cannot take the account while that refusal is still owed. Existing
Decision and lifecycle windows are retained; no observation or extraction renews
them. Normal completed-owner retirement still ends its existing wait.

| Spec scope | Author assessment |
| --- | --- |
| CANCELLED-CORRECTION, nonempty Text-only failure | Conforms for the changed owner boundary: complete Recorded checks, exact issued identity, one retained count before release/take/return, no cancelled continuation. |
| EMPTY-CORRECTION | Conforms for the late-loan cancellation road and preserves the existing Recorded-refusal boundary. |
| OUTPUT-UNIT / HARNESS-RECALL-REFUSAL | Complete check grants remain separately guarded. Actual loaded recall meaning excludes attempted calls. Original waits and first fatal evidence remain retained. |
| CANCELLED-CORRECTION, attempted-call accounting | Still drifted/incomplete: all required refusals and actual CompletedPair receipts must establish the count before account extraction. This is the next stacked implementation, including the two-call/held-terminal instruments. |
| Full worker/lifecycle and qualification | Incomplete under #146. These owner instruments do not prove real decoder correction composition, full Stop/Leave cleanup, production artifacts, limits or cutover. |

## Instruments and validation

Seven new tests use the actual recording, checker and state socket owners with
scripted replies. Stop and Unload are supplied through the repair cancellation
port; they are not claimed as full worker lifecycle circuits.

The cases cover both cancellation causes below and at the cap; a checked prefix;
the last Recorded check before interrupted classifier release and output loan
return; cancellation after completed checks; success, external, unrecovered,
valid-recall and malformed-recall exclusions; an actual unanswered ordinary
accepted-Text filing; empty refusal after a late loan; cancellation before any
correction and with an unanswered checker request; and an actual post-Recorded
clock failure. They inspect identity, count, retained bounds and absence of new
retry/exhaustion/success. A compile-time check keeps failed-count evidence move
owned and non-Clone.

The initial fixture runs exposed missing pre-correction Decision setup, a recall
reply rejected by wire validation rather than recall shape, and an earlier
instruction filing blocking the intended held correction receipt. Those logs
and fixture sources are retained. The corrected instruments arm before the
correction/cancellation, use a valid binding with a forbidden recall condition,
and settle the earlier instruction before withholding the correction's receipt.
The first full harness run found one older private-ledger test expecting an
empty correction to transfer before its refusal. That expectation contradicted
the merged rule. It now requires the refusal and retained count before transfer,
while keeping its original stale protocol-flag assertion. The failed run and
its source snapshot are retained separately; final gates and mutations use the
corrected test.

Final restored-source checks passed with the pinned toolchain,
CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=4, --locked and private target
`/var/tmp/weaver-state-146-cancelled-correction-count-target`.

| Gate or instrument | Result |
| --- | --- |
| lock.sh before/after Cargo, formatting, strict harness clippy and diff check | Passed |
| Full harness suite with four test threads | 2,006 passed, including 22 doctests; 183 ignored in the ordinary run |
| Seven focused cancelled_correction_count_tests | Passed across both cancellation causes, cap boundaries, exact loans, actual records, held checks/filing and the post-Recorded clock failure |
| Delegated gate/startup/input/Observe/Stop/generation/emission | 46/17/19/14/16/20/33 passed; 165 total, serially in isolated scopes |
| Protective mutations | All ten compiled and failed the intended tests: delayed accounting/release, checked prefixes, external/unrecovered/recall exclusions, duplicate return count, missing empty refusal, wrong rejected source, accepted-pass charge and cancelled-count omission |
| Final source manifest | All 661 tracked crate/configuration files restored and verified |

Source manifest SHA-256: `07f241cf54d60b07b94c6d408f5bc27792a728fa1466ac72133fa4889756360e`.
Observe and Stop used the actual admin test executable rebuilt in this private
target. Eighteen inherited harness ignores and eight privileged admin-startup
instruments remain outside this gate. Serial passes do not close #313. No Cargo
or Git mutations overlapped, deployed agent changed or CodeRabbit review was
requested. These are author checks, not an independent Planner grade, security
disposition or production qualification.

## Remaining work and review carries

The child on `code/state-cancelled-call-count` implements attempted-call accounting
at the actual terminal filing boundary, including every original request, admitted
non-dispatch cause, two-call prefixes, held terminal receipts and delayed account
return. Its [record](state-management-cancelled-call-count-2026-10-24.md) gives the
separate source, validation and limits. This Text-only slice remains partial; no
full #146 implementation or qualification checkbox closes here.

The inherited #317 and #318 Planner reviews passed with no fix rows and were
marked ready by the Planner. Their batch/file rows are now mapped in the current
author, held-boundary and positive-boot records. #316 includes the exempt marks
findings. The notice-producing-refusal precedence election, generic caller
identity/takers, release-ownership test, zero-refusal detail and composed boot
test gaps remain open. This child changes none of those reviewed runtime heads.

The code-review and verification-before-completion skills remain unavailable;
no invocation or independent grade is claimed for this slice. Production
artifacts, measured limits, security disposition, full failed-run exit and the
qualified coupled cutover remain open. Existing serving stays active. New PRs
are published as drafts; only the operator merges, deploys or requests CodeRabbit.
