# Capture frontier and recall proposal

This draft proposes one meaning for the harness's capture frontier under issue #146.
The [operator elected the complete Recorded watermark on 2026-10-22][ruling].
The owning clauses are proposed against main
c2646e293c8c1bb4376c4b9ea1d41cbebcc080b2, with the ordinary document headers stating
their intended state after merge. They authorize no implementation before that merge.
The complete serving replacement remains held for the qualified coupled cutover.

## Choice requested

Elect a complete current-run Recorded trace watermark, maintained through the shared
recording owner across enter, serving and leave. Freeze it once when capture admission
freezes. Load, Boot and Read recall, ordinary authors and coordinator observations
contribute on the same terms. Untapped records may advance the live watermark without
adding a selected source, changing an existing frozen boundary or advancing the
member's content position.

This interpretation gives the word sequence its trace meaning. It does not add a
second persistence field or pretend that trace observations are filed content. Capture
still waits for the real dispositions of every selected source through its boundary.
The proposal deliberately requires completing the watermark across coordinator
observations. Keeping unmerged #229's recall assignment alone would not implement it.

## Facts found

[Planner review 5609135461][review], row 6 and mutation O6, reviewed unmerged PR #229.
Its recorded-read owner advances last_author_sequence while coordinator observations
do not. Removing that recall assignment survived the reviewed tests. The review
classified this as an open semantic and a batch test gap, not evidence of lost state
or an unsafe capture. That assignment is absent from context_read.rs on this
proposal's main base c2646e29. It is not merged behavior.

On main c2646e29, context_capture.rs::prepare_member_capture uses
last_author_sequence. coordinator.rs freezes the supplied number in CaptureSnapshot
and refuses selected ledger/inbox sources above it. It separately waits for all owned
obligations and observations. That check does not file a turnless recall or equate the
number with a member commit. context.rs::wait_recorded updates the field for ordinary
author completions. Its bind_repair_author, bind_pair_author and drain_intake calls are
fallible and precede that assignment. The coordinator's Recorded path does not update
that context field.

The proposed rule therefore owes implementation in both paths. The shared recording
owner must preserve each admitted sequence before those fallible calls, not merely add
the missing coordinator assignment or retain #229's independent recall update. Tests
must interrupt ordinary wait_recorded after Recorded but before intake completes and
verify the retained watermark and continuation. No code change or passing regression
is claimed here.

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
| Types and Spec 05 | TYPES-PERSIST and PAIR-SCHEMA name the captured content SourcePosition explicitly. Wire fields and pair commit ordering do not change. |
| Admin | No StartIdentity, save result, binding format or publication step changes. Admin continues using the member's returned position. |
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
| Enter load, successful Boot, ordinary author, successful Read, classifier result, state measurement and flow sample | Each real Recorded identity updates the same live watermark before capture, across enter, serving and leave. | Exclude enter/load/Boot, omit an author class or maintain only the recall assignment. |
| Author held at CapacityPending | No invented identity or watermark advance. A pending content author cannot be omitted by freezing early. | Advance at submit or freeze past pending content. |
| Refused read | No recall event is invented. Its real operation measurement still contributes when Recorded. | Treat refusal as empty success or skip its measurement identity. |
| Late observation during filing or capture | Live watermark advances, frozen frontier and original deadlines do not. | Recompute the frozen frontier or reset a wait after a wake. |
| Source above the frozen boundary | Protocol failure before capture, even when an unrelated observation has advanced the live watermark. | Compare against the live watermark or ignore a later queued source. |
| Source at or below the boundary still awaiting receipt | Snapshot stays blocked until filed or correctly_rejected. | Accept a maximum sequence as a receipt. |
| Seed-only and no-new-content captures | Recorded load supplies a local boundary. Image position remains null or the loaded content position. | Require a new selected source or replace image position with a trace sequence. |
| Same content with and without extra recall/measurements | Watermarks differ by real trace history. Content positions agree. Metadata follows its own capture cutoff. | Claim byte-identical dumps despite different observation metadata. |
| Wrong run or caller-chosen smaller value | No foreign or truncated frontier can be installed. | Substitute the loaded run's sequence or accept an arbitrary lower bound. |
| Recoverable lifecycle wait immediately after Recorded, including ordinary wait_recorded before drain_intake completes | The identity and watermark remain owned and no event is authored twice. | Update only after intake, another fallible check, or replay the event on resume. |

O6 alone pins a representation choice. The later-source and pending-receipt cases pin
the safety boundary, and must be tested separately. Real member fixtures should check
the returned SourcePosition and metadata presence, not assume that two snapshots with
different observations have identical bytes. API or type pins are separate from
compiled behavioral mutations. The eventual implementation owes affected tests, lint,
formatting and a lock check with a private target.

[Planner review 5609450946][proposal-review] found contradictions and ambiguous wording
at d08cb8fe. This revision removes the caller-supplied prepare_save frontier, names the
image's content position consistently, preserves the contract's complete image-content
bound and distinguishes the main and #229 implementation evidence. It also includes
enter/load/Boot in the proposed watermark and names ordinary wait_recorded's fallible
ordering as owed implementation. Mark closure and preview frontiers retain their own
committed-source meaning. Operation metadata retains its independent capture cutoff.

No implementation or runtime gate result is claimed by this draft. The prior lock gate
and two document readings preceded that Planner return and are not validation of this
revision. Static checks of this revision cover ASCII, prose width, synchronized
versions, local links and diff hygiene. The fresh lock gate passed, and the
coordinating review found no remaining concrete contradiction. The dated operator
election is recorded above.

[review]:
https://github.com/toddwbucy/WeaverAgent/pull/229#pullrequestreview-5609135461
[proposal-review]:
https://github.com/toddwbucy/WeaverAgent/pull/237#pullrequestreview-5609450946

[ruling]: https://github.com/toddwbucy/WeaverAgent/pull/237#issuecomment-6333209803
