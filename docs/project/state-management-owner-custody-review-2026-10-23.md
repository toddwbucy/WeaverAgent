# Owner custody and completed Boot cleanup

This slice advances #146 on code/state-owner-custody-review, above exact #273
6bd5ddf86ae28e5fef9f5b95f37a833533429c30. It answers #270 Planner review
5625064169 and #271 review 5625350211. It remains dormant.

## Authority and scope

Implements: HARNESS-RECORD-LIFETIME, HARNESS-HALT-ACCOUNT and HARNESS-CAPTURE;
Spec 02 repair/refusal custody and Spec 05 completed-pair accounting;
PERSIST-ENTER, HARNESS-OPEN-LIVE, HARNESS-PLAN, HARNESS-SAVE-TIME and
PLAN-SUMMARY-EVIDENCE. Author assessment: conforms within these retained
owner and cleanup boundaries, subject to Planner verification.

Nonfatal held originals now have separate custody from immutable first-fatal
evidence. A caller can take the latest held original and retry through the
existing admission checks. A later hold replaces only that held slot. Callers
needing an earlier original must take it before another attempt. Early holds
preserve an existing author's turn metadata. A returned unadmitted attempt
retires only its own metadata, and a later intake failure retains that same
original as fatal evidence.

Capacity-pending release validates its exact scope, mark and original ticket
before handling newly owed BootOpen or Placement debt. A defensive debt rise
withdraws the unadmitted original and prepared event together, with no sequence,
new ticket or renewed bound. Public admission currently prevents this debt rise
in Releasing. Tests label that injection as defensive and separately prove real
new Open/Place/Cut admission remains refused. Shared debt classification keeps
Placement precedence and names the actual owed owner in terminal diagnostics.

One borrowed pair-proof scan distinguishes pending pre-intake absence from
genuine errors. It continues past a pending prefix to retain later errors.
After accounting, missing completion is WrongPhase and UnknownCall remains an
error. Shared refused-proof validation retains the correction and account on
failure and keeps successful settlement once. Correction counting is unchanged.

A successful Boot carrier now statically owns RepairedBootPairs and the actual
committed-read Arc. Exhaustive safe moves retain every original transaction
field through extraction and return. The nested repair allocation and read Arc
remain the same evidence. No unsafe layout conversion, scalar reconstruction
or cloned unique capability replaces that custody.

Completed Boot return in Failed or Releasing is cleanup-only. It accepts state
retirement and orphan waits, while excluding every actual competing owner,
author, read, required summary and installed engine. It retains the first
failure, phase, exact context/cache/Open/read identities and original account
minimum. Entered keeps strict admission. Closed refuses and returns the same
proof without reopening the run.

An absent Enter wait is accepted only with an exact private receipt from its
actual successful disarm. A no-op disarm cannot create or erase the receipt.
Successful rearm invalidates it. A currently registered Enter must match the
original and cannot fall back to an older receipt. No new clock is created.
Cleanup still clamps to the existing Enter/read/Leave/halt minimum.

## Complete reviewer dispositions

| Review row | Disposition |
| --- | --- |
| #270 1, 3 | Separate held and first-fatal originals, exact consuming take and returned-attempt metadata. All existing nonfatal guards and fatal storage callers audited. |
| #270 2, 5 | Exact unadmitted release withdrawal and shared debt classification. Debt-rise probe is explicitly defensive; real admission-refusal controls remain. |
| #270 4 | Already corrected by #271. Boot and Read retain dedicated evidence writers. |
| #270 6 | Shared phase-aware proof scan and refused validation, with actual frozen-owner and foreign-proof custody tests. |
| #270 7 | Deliberate ordering retained: release retains Recorded completion before observers, while ordinary completion binds its owner proofs before exposing caller completion. |
| #271 1 | Cleanup restore works after actual release/state retirement and orphan waits. Actual owner conflicts and Closed still refuse without losing the original. Full failed-Enter composition remains separate. |
| #271 2 | Typed successful Boot evidence replaces impossible Effect/missing-read runtime assumptions, preserving every rollback field. |
| #271 3, 4, 5 | Shared immutable identity checks, take-once prerequisite match and named test dispositions. No admission guard or assertion removed. |

The original #271 T1 and T7 remain overlapping/redundant, not newly independent
protections. Actual cancel_boot_open refuses after the grant was taken, so a
fabricated flag-only success path is not claimed. T12's blanket Releasing
refusal is superseded by explicit safe cleanup acceptance and actual Closed
and owner-conflict refusal. Full caller and probe ledgers accompany the evidence.

## Author validation

The joined source preserves exact #273 Stop-return behavior and #272 runtime.
Three overlapping files were merged against their actual ancestors and read
independently. Only the nineteen changed source paths were transferred.

Eighteen new tests supplement existing actual Boot, placement, release and
repair controls. Serialized private-target gates used lock.sh first and
--locked. Full harness passed 1,721 tests with 18 inherited ignored cases.
Trace passed 160. Focused cleanup passed eight and release custody passed eight.
The five reload-return and four native-return regressions passed.

Compilation first caught an incorrect test-only PairError module import.
After correction, the full suites passed. Clippy then required collapsing one
nested refusal-accounting if into the same short-circuit conjunction. All 101
repair tests and affected all-target Clippy, fmt, lock and diff passed afterward.
The full harness result predates only that equivalent condition collapse.
Initial failure logs and the pre-lint source remain in the evidence.

All 32 behavior controls compiled and failed their intended assertions. Each
exact restoration passed its named test. All nineteen source hashes match the
final tested snapshot. The separate typed-carrier check rejected a generic
optional-read owner with E0308, then all eight restored cleanup tests passed.
The compiler rejection is not counted as a behavioral control. Independent
source review and author gates are not Planner grades or production qualification.

## Completion boundary

This closes no issue and activates no serving path. The complete failed-Enter
composer must retain returned proofs, settle real owners, dispose transports,
record terminal facts and drain before Closed. Worker/dev_loop orchestration,
SPU classification, actual production artifacts and measured qualification
remain required. The separate #269 ticket/assembly followup and its filed/batch
rows remain tracked. Approved but unmerged #260/#263/#268 are unimplemented.
