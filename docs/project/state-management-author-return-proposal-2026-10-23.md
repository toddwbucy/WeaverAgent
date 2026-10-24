# Proposed author rejection custody

Date: 2026-10-23. Status: operator approved, subject to review and merge.
This clarifies draft #276 at exact
11e96da3dcfac5fb7a3b2de407632836d9d4ddb0 after the #279 Planner review at
5f7a337256a4693d97900efffa1d8a38d0aa337e. It addresses #274 rows 1-2 and
#279's carried row 1 election for issue #146. It changes no code and is not
implementation authority. The operator approved both the author-result and
recoverable held-boundary choices together on 2026-10-23; the [ruling][approval]
retains the review and merge requirements.

The current owner retains one nonfatal held original and one immutable
first-fatal original. A third call consumes its AuthoredEvent but returns only
RunFailure. That signature cannot return the new original when both slots are
occupied. Replacing either slot loses evidence; another finite overflow slot
only postpones the same defect.

The proposed election changes author_and_wait, resume_author_and_wait and
resume_claimed_author_and_wait to return Result<Sequence, AuthorFailure>.
AuthorFailure moves the RunFailure plus an optional exact returned original
and is not Clone. RunFailure keeps its existing nonfatal owner-hold and wait
reasons as well as fatal failures; only an already latched fatal cause is
immutable and takes precedence over later reasons. No returned original means
the context retained it in an existing slot or it was already Recorded; it
never means discarded input. A returned original remains the caller's owner.
Its return grants no retry, serving continuation, new wait or trace identity.

The first held and first fatal originals cannot be overwritten. A second
submit latches protocol_failed only when no fatal cause is already latched.
After a fatal failure is already latched, later rejected calls return that
same failure with their own originals. Returned custody joins the caller's
bounded teardown under that first cause. A pending record retains its existing
ticket and bound. If an unadmitted pending author must be withdrawn for owed
account work, it can return its original even when the held slot is occupied.
Its preparation and attempt bookkeeping retire together without a new sequence
or renewed deadline.

## Explicit held-boundary election

The #279 review found that its held-original guards in release, Enter rollback,
Closed and member-capture preparation currently latch fatal ProtocolFailed.
That follows the merged caller-error convention, retains the original, and is
not a defect against the merged Spec. The earlier #276 phrase "before retrying
these boundaries" implied recovery which that behavior does not provide. This
revision surfaces the choice instead of deciding it by default.

Recommend a recoverable Busy refusal for the held-input condition at new owner
admission and these four boundaries. Use one private held-input predicate and
reason that identifies the occupied caller-held or template-blocked slot. Both
slots retain their exact originals if both are occupied. Methods may keep their
existing result families, but a held original must not be reported as a wrong
proof, missing head or failed template certification. Genuine template
certification remains TemplateBlocked; a held-template original is a separate
custody reason. This adds no wire field, trace refusal or new event kind.

| Boundary | Proposed change for held input alone | What recovery still requires |
| --- | --- | --- |
| begin_releasing | Return Busy before release/phase/wait mutation, instead of latching ProtocolFailed | Take all held originals; satisfy the same settled-owner and original Leave checks. |
| begin_enter_rollback | Return Busy before rollback/phase/wait mutation, instead of latching ProtocolFailed | Take all held originals; retain the actual recorded Enter refusal and same explicit Leave authority. |
| close_after_drain | Return Busy without entering Failed or Closed, instead of latching ProtocolFailed | Take originals held even after release began; prove actual recorded Unload, resource retirement and drain under the same cap. |
| prepare_member_capture | Return Busy before capture/barrier admission, instead of latching ProtocolFailed | Take all held originals; recheck the real between-turn boundary, declared window and all other capture prerequisites. |

The common reason also corrects ordinary caller-held input misreported by
completed_template_enter as TemplateBlocked and by install_place as WrongProof.
The latter must return the same offered Boot proof with Busy. This proposal
does not select a new clock, permit fresh work from held input, or turn a
held original into a ticket. Previously admitted owners retain only their
exact existing private continuations. Other invalid phases, bounds, proofs
and protocol preconditions retain their current rules.

No fatal state is recoverable through this election. An already latched cause
keeps precedence over Busy and remains immutable. Required clock/deadline and
independent failure observations still apply. Taking the original in Failed
restores custody to its caller; it does not revive the context, enable healthy
release/Closed, or restart Enter/Leave/halt time. Independently bounded fatal
teardown must retain the exact caller/context originals and actual pending
work, account only what is real, and close/exit failed at the original cap.
It cannot wait beyond that cap for a caller to fix the hold, fabricate a
successful Closed/Left, or drop originals to make closure pass. Full lifecycle
composition must carry those retained facts; this election alone does not
supply that integration.

A caller with no fatal failure may take the exact original and offer the
boundary again while its original bounds and other preconditions permit.
The Busy return changes no phase, ticket, original, resource or deadline.
It gives no guarantee of later success and no new permission to resubmit an
author event. In particular the separate second-submit misuse above still
latches protocol_failed if no earlier fatal cause exists, and otherwise
returns that earlier cause with the new original.

**Alternative: fatal caller error.** Keep ProtocolFailed at all four named
boundaries, preserving any earlier cause. Under this election taking the
original permits evidence recovery and bounded fatal teardown only; healthy
release, rollback, capture and Closed cannot be retried on that context.
The earlier retry wording must then be removed. This is consistent with the
current merged convention, but makes a correctable held-input ordering mistake
terminate the run. The operator elected the recoverable alternative, subject
to review and merge. This proposal does not implement either choice.

The #279 admission work and this draft remain distinct: its current fatal
boundary behavior is accurately described above, while the last-wins author
result remains deferred to #276. This clarification does not claim the prior
implementation batch has or has not merged, and does not resolve its remaining
review/test rows by changing this prose.

## Landing form and sequence

The owning Spec is written in the prospective landing form for this draft's
approved Busy election: Landing PR #276, normative clauses, and no proposal
banner or losing alternative inside the Spec. This project proposal records
the operator's ruling and retains the alternative for comparison. Approval
alone does not authorize implementation; review and merge remain required.

Main now contains #285 and #282, with harness amendment version 0.31. This
draft proposes 0.32 and retains both merged rules. Later proposal landings
must retain preceding merged clauses and advance shared versions again.
The author-custody election still requires its own review and merge.
No proposal merge or implementation is claimed here.

The operator ruled that Date filed retains the first filing date. The owning
Spec header therefore remains 2026-10-21; Version and Git identify revisions.
This header correction changes no proposed clause or implementation authority.

## Author-result alternative and caller migration

An alternative to the proposed consuming author-result interface is a borrowing
admission interface that refuses before taking the caller's original, with ownership
transfer represented in its type. An advisory preflight bool is insufficient because
the old consuming call remains able to discard a third original. Retaining an
unbounded queue, cloning events into errors, inventing tickets, or expanding the
halt-account wire format are not part of this proposal.

Implementation after merge must migrate all six direct production author
sites: four input paths, mark-refusal authoring and repair authoring. It also
must cover retained-freshness retries and the pair/place/measurement resume
paths, including both named resume methods. A caller cannot map the new error
into RunFailure while dropping the original. From<AuthorFailure> for
RunFailure is forbidden, so ? cannot silently erase returned custody. Its
existing event/stopped-evidence owner must accept the exact return, and
started flags must reflect actual context custody. There is no new worker
orchestration or serving activation in this election.

Required evidence includes two and three caller attempts, an already-latched failure,
a pending release returning under debt while another original is held through either
resume entry, exact original identity and stage retention, unchanged ticket/sequence
and deadline facts, and propagation into bounded caller cleanup. New admission and all
four boundaries above must refuse untaken held or template-blocked input while
existing private work can settle. Under the recommended election, actual
otherwise-valid boundary fixtures must show nonfatal Busy, unchanged phase/bounds/exact
originals, then successful or truthfully expired retry after the real take. Repeat
with both held slots, an already-latched cause and an independent failure observed
during the call: no case may clear the latch, delay fatal teardown, replace a cause,
falsely report a template/proof failure or fabricate Closed. Tests must separately pin
the unchanged second-submit fatal rule. Protective changes that relatch a held-only
boundary, bypass held custody, grant a fresh retry wait or ignore the first failure
must fail the matching fixture. Compile-time negative pins must reject cloning
AuthorFailure and any From<AuthorFailure> for RunFailure implementation. Existing
owned pending authors and first-fatal evidence remain distinct from an unaccepted
return.

This does not enlarge HARNESS-HALT-ACCOUNT. Only a real allocated pending
ticket belongs in AuthorAtHalt; no unaccepted original is encoded there by
fabricating an identity. This proposal closes no issue, changes no deployment
and leaves full failed-run composition and qualification open.

[approval]: https://github.com/toddwbucy/WeaverAgent/pull/276#issuecomment-6365802749

## Integration with merged release-absence wording

On 2026-10-23, main at f4482c60936efb6b7b38dd888fb61f25f8a953ed
was joined into this proposal without rewriting its history. The header
conflict keeps this proposal as Landing PR and advances the harness
amendment from merged version 0.30 to proposed version 0.31. Date filed
retains each owning document's date from main under the operator's
first-filing convention. Both the proposed clauses and the merged
HARNESS-RELEASE-ABSENT / TRACE-RELEASE-ABSENT wording are retained.

Exact body comparisons preserve the previously reviewed Spec clause text;
the harness body additionally contains the unchanged merged absence
clause. Documentation checks accompany this conflict resolution. At this
checkpoint the five proposals were separate main-based PRs; later landings
must incorporate preceding merged clauses and advance shared versions again.
The proposal remains unmerged and grants no implementation authority.

## Integration after the Enter-window merge

On 2026-10-23, #282 merged as main commit
8010e16e64e137a28c239800c1e9724ae80c27b5. This proposal now joins that
exact base, retaining the merged Enter-window and release-absence clauses.
The harness amendment advances from 0.31 to 0.32. Where this proposal
also changes another shared amendment, its version advances from main
likewise. Landing PR still names this proposal; filing dates stay fixed.

The manual resolution changes only conflicting headers. The clause bodies
match the clean three-way combined text byte-for-byte. Lock, document,
ASCII and diff checks pass. No runtime source changes or proposal merge
is claimed. Other proposals remain separate and need the same version
sequencing as each preceding proposal actually lands.
