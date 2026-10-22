# Failed-context first-fault report owner

Date: 2026-10-22.

## Scope and authority

This slice advances issue #146 E through an actual consumer of the stopped
recorder's first-fault reserve. Its parent is #245 at ec3b340. That ancestry
retains #190's operator hold for the foundation cutover. Runtime serving remains
on the complete legacy implementation.

Implements: weaver-harness-Spec-amendment HARNESS-RECORD-LIFETIME,
HARNESS-HALT, HARNESS-HALT-BOUND and HARNESS-HALT-ACCOUNT, scoped to the
first-fault reporting operation and opportunistic progress of already owed
trace-only records. Spec 09 FLOW-COUNTS supplies the existing frozen sample.
This is part of the Reporting phase, not its complete lifecycle composition.
Author grades and validation remain pending below.

The new owner requires the same failed RecordingContext and its already installed
checked halt account. The caller freezes real external owner facts through the
existing freeze_halt_account operation first. This port accepts neither a raw
account nor caller-supplied fault sequence. It does not invent capture windows,
Commit/Published progress or a pre-open flow observation. Spec 09 retains the
existing diagnostic path for Open failure before load.

The first bounded advance constructs an immutable runtime FaultReport using the
exact first-failure account. A trace-first cause uses StreamWriteFailed. Other
causes use StateFlowFailed. It attempts the existing Fault reserve once while
retaining any ordinary pending event and its original ticket. The report owner
keeps the prepared original and actual Recorded sequence before a later clock
sample. Rendering or submission refusal cannot recursively attempt a new fault.
The raw account and first cause remain in their original HaltLatch.

Every advance checks the same common clock and original halt deadline before and
after bounded work. The existing earlier Leave cap remains authoritative. Missing,
reversed or expired time, including an unavailable overflowed deadline, produces
AbortRequired with retained evidence. It never rearms a serving wait, renews the
halt deadline or clears failure. An aborted report stays aborted.

Later advances make at most one opportunistic trace-only progress step. They never
poll or wait for capacity. A future teardown driver can therefore interrupt the
gate without waiting for an ordinary pending record. The measurement-refusal
owner retains its own ticket and exact sequence for work still owed at report
admission. Other already pending events use
the existing stopped-tap trace-only road. Neither road creates a state copy,
receipt, generation permission or successful serving continuation.

The evidence remains in the context for a future unload to name its exact fault
sequence, or null if no fault was recorded. It is neither Clone nor Default and
has no conversion from a bare Sequence. Its status is not containment, release,
drain completion or authority to skip another halt phase.

## Tests and protective changes

Twelve new tests cover:

- Failed-phase and installed-account admission, with no replacement of an owner.
- Real state EOF and wrong control response, preserving the exact request and
  frozen account in one canonical fault without resuming or requerying.
- A full actual writer, a real pending author, reserved fault admission and later
  trace-only drain without a tap copy or ledger transition.
- The earlier Leave cap, exact expiry, initial clock failure, deadline overflow,
  and consumed fault or ordinary trace sequences retained before a later clock
  failure, reversal or expiry.
- A sound writer with its Fault permit already consumed through the real lower
  API. This is an explicit permit-misuse fixture, not a fabricated sink failure.
- A failed sink with StreamWriteFailed and sticky reporting failure.
- Immutable first-fault account and deadline with secondary evidence retained for
  unload, without authoring an unload prematurely.
- A real fatal decoder refusal under recorder pressure. The same measurement
  owner records it after the reserved fault and retains its raw response without
  releasing a measurement grant.

The member and decoder fixtures use real sockets and the actual trace recorder.
They do not establish SQL, device, cgroup, gate containment or deployment evidence.
The overflow case is a checked arithmetic boundary, not an observed host clock.
No test fabricates a capture publication or transport-completion proof.

Fourteen compiled behavioral mutation recipes and three E0283 compile probes are
prepared for root's serial gate lane. Focused tests, the full harness suite,
all-target lint, formatting, lock checks, mutations and restoration checks remain
pending. No pass count or Planner grade is claimed by this source preparation.

## Remaining integration

The older fatal measurement path can queue or progress its newly created refusal
immediately after the latch, before an external caller freezes the halt account.
Its ordinary admission checks exclude the suspected concurrent older pending
writer case. This slice consumes an already installed account and orders only
work still owed at that point. Broader latch-time snapshot and caller composition
remain an integration obligation. The pressure fixture pins the retained refusal
at report admission, not every pre-admission timing in that older path.

The owner does not cancel a real pending publication or author its refused
save_point. That requires the complete capture lifecycle owner and its actual
window, cause and Commit/Published facts. It must never relabel a valid Commit as
refused. Requiring an existing checked account does not qualify external facts
that the supplying caller did not own.

Gate interruption and containment, stopped turn/caller response, gate lower,
SPU release, failed state/classifier retirement, component_failure unload and
bounded final drain remain separate executable work. The legacy Run-to-context
bridge and serving FlowFailed/EndServingWithoutSave dispatch are not implemented.
No grants query, snapshot, successful Left, worker-written admin marker or
serving activation follows from this slice. Production qualification and the
coupled foundation cutover remain open.
