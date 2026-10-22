# weaver-harness / weaver-internal - contract

**Status:** MERGED. Projects the merged recall clauses without changing their behavior.

**Date filed:** 2026-10-22
**Document ID:** `weaver-harness-internal-contract`
**Editorial:** Per the Working Rules.
**Landing PR:** #245

## 0. Scope and authority

This contract applies at the state-management foundation cutover. Until then,
the existing serving contracts remain applicable under
[FOUNDATION-TRANSITION][cutover]. The compiled primitive stays dormant in serving
until the qualified coupled cutover; declaring this library edge does not activate it.

This contract projects the direct pure calling surface already elected by
[INTERNAL-RECALL][charter] and the responsibilities already fixed by
[INTERNAL-QUERY and HARNESS-RECALL][amendment]. Those merged clauses retain authority
over the callable's representation and harness integration. No new operation,
transport, failure or qualification rule is elected here. The calculator and the
operator-promotion boundary remain unchanged.

## Parties

- **`weaver-harness`, the caller.** Retains the subject bytes, calls the pure
  constructor, and owns scheduling, trace authoring, store I/O and result placement.
- **`weaver-internal`, the constructor.** Returns a query borrowing the complete
  nonempty subject or the existing empty-subject failure. It owns none of the
  caller's effects.

This is an in-process library seam tagged `link`. It adds no process, socket or
authentication exchange. The asker declares the sole seam record in
`weaver-harness-PRD` section 4; this contract declares its parties.

```graph
node: weaver-harness-internal-contract
kind: document

edge: party
from: weaver-harness-internal-contract
to: weaver-harness

edge: party
from: weaver-harness-internal-contract
to: weaver-internal
```

## Vocabulary

**From `weaver-traits`.** Nothing crosses this library seam.

**From `weaver-types`.** Nothing crosses this library seam. Call identities, checked
bindings, state selectors and pair outcomes remain on the harness side.

**From `weaver-internal`.** The borrowed noun-history query and empty-subject failure
defined by INTERNAL-QUERY. The subject is the caller's original string, not a
normalized, parsed or allocated replacement.

**From other crates.** Nothing. The constructor has zero normal dependencies and
reaches no state channel, filesystem, clock, model or randomness.

## 1. Call and custody

HARNESS-RECALL admits the call only from the conforming checked recall binding. The
constructor receives only its subject. A nonempty subject returns a query borrowing
that complete original string; the harness retains its storage throughout use. The
constructor neither trims whitespace nor changes case or other bytes. An empty
subject returns the existing failure. Success establishes only that the exact
nonempty subject was retained by reference, not that history exists or retrieval
succeeded. These are INTERNAL-QUERY's existing guarantees.

The harness consumes the query under HARNESS-RECALL: it submits the exact subject
through the sole state owner and owns the bounded read/release and inward pair
accounting. The helper makes no retrieval decision and carries no call identity.
Malformed recall handling, empty-subject refusal, state-refusal mapping and pair
termination remain precisely those of HARNESS-RECALL; none is re-elected here.

## 2. Integration boundary

This call advertises no tool schema and creates no registry entry or self-initiated
activity. The gate's shell entry and confinement remain unchanged. Loop orchestration
remains in `worker/dev_loop`, and classification remains in the SPU. The helper's
empty-dependency library and the harness's exact five-crate normal dependency set are
checked by their existing manifest instruments, as required by the amendment's
section 3. Those checks do not establish retrieval coverage, lifecycle correctness
or production qualification.

[charter]: ../state-management/charter-amendments.md#4-ste100-and-recall-duties-in-the-owning-charters
[amendment]: ../state-management/weaver-internal-Spec-amendment.md
[cutover]: ../state-management/state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
