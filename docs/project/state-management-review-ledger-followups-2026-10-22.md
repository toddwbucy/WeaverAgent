# Repair ledger and closed-codec review followups

Date: 2026-10-22. Issue #146. Authoring seat; draft implementation.
Branch: code/state-review-ledger-followups. This slice answers eight of the nine
batch rows in Planner #261 review 5623117895. It does not change the attempt-counting
policy in row 5. Source was independently staged against exact #262
7cad4da7c4611d214d2fcbdc28442da88ef3cbb2. Root verified every source overlay and
joined it above exact #265 a17eba6343e6386f4d3addd22f35260f1bb0915d; runtime
ordinary gates and all protective mutations have passed.

## Authority and scope

| Implements | Author source assessment |
| --- | --- |
| Spec 02 REPAIR, RECALL-REFUSAL and EMPTY-CORRECTION custody | Conforms within this dormant owner: retain actual refusal evidence through cancellation, settle owed correction pairs before refused transfer, and derive association records from the owned checked requests. Existing attempt-counting semantics remain unchanged. |
| Spec 05 PAIR-STATES and COMPLETED-PAIR | Conforms for the actual NotDispatched owner and terminal filing receipts. Cancellation preserves already admitted terminal causes and owns only the unclaimed suffix. No dispatch is authorized. |
| TYPES-EFFECT, TYPES-READ, TYPES-PLAN-CONTROL and TYPES-CODEC plan carriers | Conforms for the existing required object shapes and nullable-field presence. A single private field list drives each public carrier and closed decoder without changing its wire or public fields. |
| HARNESS-OPEN owner custody | Conforms: shared boot-event access moves the same single retained author ticket at the same call boundaries. |
| HARNESS-STATE image descriptor custody | Existing production behavior is unchanged. The isolated descriptor-test child proves execution with an explicit sentinel after its actual close check. |

## Review dispositions

1. A refused repair settles every owed forbidden correction call through the actual
   NotDispatched/PassPurpose owner before transferring the complete correction as
   evidence. Original generation requests remain the caller's explicit disposition
   duty. This closes a defensive ordering that no current production caller selects;
   the tests and claims retain that reachability boundary.
2. The actual Recorded hook retains structural or runtime-template refusal evidence
   before fallible continuation. A cancelled account carries that exact source and,
   when applicable, the original RecordedInstructionCheck. It grants no restatement.
3. The ledger derives its next association refusal and completion frontier from its
   original/repaired associations and actual recorded count. The author carries only
   the retained index; it supplies no refusal payload or final-entry flag.
4. One refused-ready boundary retires the original Decision wait after all owed
   refusal and correction pair accounting. Extraction only transfers custody once.
5. Carried to draft #263, a separate Spec clarification. Merged #213 requires an empty correction
   to count after its Recorded refusal. The cancelled nonempty/call completion
   boundary is not changed by this slice. No new cancellation-counting policy is
   inferred from a local comment or from the other review corrections.
6. RecallRefusal and Association author variants carry their required typed identity.
   Side Options and their missing-identity expect calls are removed. Compile-time
   constructor pins prevent identity-free variants from silently returning.
7. One boot-author accessor replaces four copies of the retained-event lookup. Its
   takes and restorations keep the same owner order and exact ticket.
8. Four plan carriers use a private closed_object field-list macro with the existing
   map-only visitor, deny_unknown_fields and required-nullable deserializers. The
   public Deserialize trait has no permissive inherent alternate entry.
9. The descriptor test still reexecutes only its exact child assertion in an isolated
   process. The parent requires success plus exactly one explicit child sentinel,
   emitted only after the actual EBADF close proof. It no longer parses libtest's
   human summary; a zero-test success cannot satisfy the parent.

## Validation boundary

Five new repair fixtures drive actual pair receipt pressure, structural/template
refusal cancellation before and after Recorded, an immutable PassPurpose prefix
with a Stop/Unload suffix, and the ordered association frontier with wrong-index
controls. The existing defensive fixture now checks wait retirement before transfer.
Existing plan codec and boot suites cover the behavior-preserving refactors.

Root ran lock first and all resolving Cargo commands with --locked in the dedicated
/tmp/weaver-146-review-ledger-followups-target. Gates and mutations are serialized.

| Gate | Result |
| --- | --- |
| Lock | 0 |
| Types default / config | 158 / 214 passed |
| Five focused repair fixtures | 5 passed |
| Full harness | 1,572 passed, 18 inherited ignores |
| Full state | 225 passed, eight inherited ignores |
| Full trace | 160 passed, zero ignored |
| Types, harness and state all-target clippy | Pass with warnings denied |
| Formatting and diff whitespace | Pass |
| Protective mutations | 15 compiled failures and 15 exact restored passes |

The first ordinary suites passed. Clippy requested one equivalent let-chain
instead of nested guards in advance_refused_repair. Root preserved short-circuit
order and the body, formatted, and reran full harness and lint on the final source.
The initial lint log remains in evidence. Independent final source review verified
both this change and the inherited #264/#265 code; no concrete blocker was found.
All final mutation source hashes match. The probes pin actual pair/refusal custody,
wait retirement, frontier/index/payload derivation, boot-event allocation identity,
descriptor close/child execution and strict closed-map/nullable decoding.
These author checks are not a Planner grade or production qualification.

Evidence is /tmp/state-review-ledger-followups-gates/ and
/tmp/state-review-ledger-followups-mutation-evidence/. Published source, hashes,
logs, recipes and review notes are deposited at
/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/review-ledger-followups-a17eba63/.

The source is dormant infrastructure. Full lifecycle and worker caller migration,
ready-gate and placement composition, qualified artifacts and limits, and the coupled
serving cutover remain open under #146. This record grants no activation or merge.
