# Capture frontier and recall proposal

This draft proposes one meaning for the harness's capture frontier under issue #146.
The operator has not elected it. The owning clauses are proposed against main
c2646e293c8c1bb4376c4b9ea1d41cbebcc080b2, with the ordinary document headers stating
their intended state after merge. They authorize no implementation before that merge.
The complete serving replacement remains held for the qualified coupled cutover.

## Choice requested

Elect a complete current-run Recorded trace watermark, maintained through the shared
recording owner. Freeze it once when capture admission freezes. Ordinary authors,
recall and coordinator observations contribute on the same terms. Untapped records
may advance the live watermark without adding a selected source, changing an existing
frozen boundary or advancing the member's content position.

This interpretation gives the word sequence its trace meaning. It does not add a
second persistence field or pretend that trace observations are filed content. Capture
still waits for the real dispositions of every selected source through its boundary.
The proposal deliberately requires completing the watermark across coordinator
observations. Keeping recall's present assignment alone would not implement it.

## Facts found

[Planner review 5609135461][review], row 6 and mutation O6, found that the recorded-read
owner advances last_author_sequence while coordinator observations do not. Removing
the recall assignment survived the reviewed tests. The review classified this as an
open semantic and a batch test gap, not evidence of lost state or an unsafe capture.

The code uses last_author_sequence in context_capture.rs::prepare_member_capture.
coordinator.rs freezes the supplied number in CaptureSnapshot and refuses selected
ledger/inbox sources above it. It separately waits for all owned obligations and
observations. That check does not file a turnless recall or equate the number with a
member commit. context.rs::wait_recorded updates the field for ordinary author
completions. context_read.rs adds recall's independent update. The coordinator's
Recorded path does not update that context field.

The merged Specs require a frozen boundary and actual source dispositions, but do not
elect which untapped Recorded events contribute to the local number. HARNESS-RECALL
requires recall to be Recorded and excluded from the partition. STATE-OBSERVE already
says its operation metadata does not advance SourcePosition. TYPES-PERSIST sends no
harness frontier in snapshot, and the returned image position names stored content.
Those existing duties permit more than one local watermark choice.

## Alternatives

| Choice | Meaning and consequence | Recommendation |
| --- | --- | --- |
| Complete Recorded trace watermark | Greatest Recorded sequence in the current context/run, including untapped records, frozen once for a capture. Every author path follows one rule. The number may change after a read without claiming stored content changed. | Proposed |
| Selected-source watermark | Greatest current-run sequence with a selected source, plus an explicit empty-current-run representation. Recall and measurements never move it. Source disposition checks still decide readiness. | Coherent alternative, requiring an election and different tests |
| Mixed author-path watermark | Ordinary authors and recall move it, but coordinator observations do not. Meaning depends on the reducer used for an otherwise equivalent untapped event. | Decline as the intended contract |

The selected-source alternative could be implemented consistently. It needs an explicit
choice for a seed-only run, a run whose only selected blocks were correctly rejected,
and events whose partition produces no blocks. This draft does not silently reject
those captures by requiring a nonempty selected-source frontier. Neither coherent
alternative changes the member's content identity or eliminates the filing barrier.

## Proposed owning changes and parties

| Owner | Proposed clause or unchanged duty |
| --- | --- |
| Harness amendment | New HARNESS-CAPTURE defines the live watermark, freeze boundary, late-observation rule and distinction from SourcePosition. HARNESS-RECALL explicitly follows it for Read and Boot. |
| Foundation | The capture actor rows and persistence explanation distinguish trace boundary, content position and operation-metadata cutoff. |
| Persistence contract, this repository's copy | PERSIST-CAPTURE assigns the frozen boundary to the harness, the returned content position to the member, and preservation of that position to admin. |
| State amendment | STATE-PAIR-READ clarifies that a pair terminal can be the image's SourcePosition, not the harness's trace watermark. STATE-OBSERVE's existing metadata cutoff is unchanged. |
| Trace | Existing Recorded identity, one pending event and partition exclusion remain authoritative. No event, payload, record limit, writer durability claim or reservation changes. |
| Types and admin | No wire field, StartIdentity, save result, binding format or publication step changes. Admin continues using the member's returned position. |
| Worker loop | Existing between-turn admission and lifecycle choreography remain in worker/dev_loop. The loop does not choose an arbitrary smaller frontier or infer a source disposition from the watermark. |

The capture watermark is local and run-bound. A loaded position can name another run
and cannot initialize it. Recorded load gives the new context its initial watermark.
A selected event's blocks all fall within its trace sequence. The watermark never
certifies that a selected source was filed, that an image contains an untapped record,
or that the trace sink survived.

The chosen boundary remains fixed while the harness drains the selected debt it
already admitted. Required untapped observations can be Recorded during that drain or
the capture continuations. They update only the live watermark. The image separately
contains operation metadata committed before capture began, so its snapshot,
acknowledgement and release observations are absent from that same image under the
existing STATE-OBSERVE rule.

## Required implementation evidence after election

| Case | Required observation | Protective change |
| --- | --- | --- |
| Ordinary author, successful Read, successful Boot, classifier result, state measurement and flow sample | Each real Recorded identity updates the same live watermark before capture. | Omit any one author class or maintain only the recall assignment. |
| Author held at CapacityPending | No invented identity or watermark advance. A pending content author cannot be omitted by freezing early. | Advance at submit or freeze past pending content. |
| Refused read | No recall event is invented. Its real operation measurement still contributes when Recorded. | Treat refusal as empty success or skip its measurement identity. |
| Late observation during filing or capture | Live watermark advances, frozen frontier and original deadlines do not. | Recompute the frozen frontier or reset a wait after a wake. |
| Source above the frozen boundary | Protocol failure before capture, even when an unrelated observation has advanced the live watermark. | Compare against the live watermark or ignore a later queued source. |
| Source at or below the boundary still awaiting receipt | Snapshot stays blocked until filed or correctly_rejected. | Accept a maximum sequence as a receipt. |
| Seed-only and no-new-content captures | Recorded load supplies a local boundary. Image position remains null or the loaded content position. | Require a new selected source or replace image position with a trace sequence. |
| Same content with and without extra recall/measurements | Watermarks differ by real trace history. Content positions agree. Metadata follows its own capture cutoff. | Claim byte-identical dumps despite different observation metadata. |
| Wrong run or caller-chosen smaller value | No foreign or truncated frontier can be installed. | Substitute the loaded run's sequence or accept an arbitrary lower bound. |
| Recoverable lifecycle wait immediately after Recorded | The identity remains owned and no event is authored twice. | Update only after a fallible check or replay the event on resume. |

O6 alone pins a representation choice. The later-source and pending-receipt cases pin
the safety boundary, and must be tested separately. Real member fixtures should check
the returned SourcePosition and metadata presence, not assume that two snapshots with
different observations have identical bytes. API or type pins are separate from
compiled behavioral mutations. The eventual implementation owes affected tests, lint,
formatting and a lock check with a private target.

No implementation or runtime gate result is claimed by this draft. Static document
checks pass for ASCII, prose width, synchronized versions, local links and diff hygiene.
The coordinating author's lock gate passed. Two independent document readings found no
concrete semantic defect within this proposal's scope. The operator election and
Planner review remain owed. Landing PR numbers must be set after this draft receives
its assigned number.

[review]:
https://github.com/toddwbucy/WeaverAgent/pull/229#pullrequestreview-5609135461
