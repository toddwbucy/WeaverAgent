# Proposed author rejection custody

This proposal defines the proposed Spec closure of #274 Planner rows 1-2 for
issue #146; it does not claim those changes have landed. It is based on exact
#275 8d86a2545e53ab023bd086756b8852c03d07bd99. It changes no code and is not
implementation authority until the operator's election, review and merge.

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

New owner admission and release refuse while either the held caller slot or
the template-blocked caller slot is occupied. Closed also refuses an untaken
original, including one held after release began. The caller must take the
exact original before retrying these boundaries. Existing admitted owners may
finish their exact private continuations; the held input grants none. Teardown
retains that custody without enlarging the halt account or inventing a ticket.
No successful close may silently drop an untaken original.

A separate source-only implementation batch addresses admission/release and
other #274 findings. It has not landed and does not implement this proposed
author-result election. Its existence does not turn this draft into merged
Spec authority or change the pending operator election.

The alternative is a borrowing admission interface that refuses before taking
the caller's original, with ownership transfer represented in its type. An
advisory preflight bool is insufficient because the old consuming call remains
able to discard a third original. Retaining an unbounded queue, cloning events
into errors, inventing tickets, or expanding the halt-account wire format are
not part of this proposal.

Implementation after merge must migrate all six direct production author
sites: four input paths, mark-refusal authoring and repair authoring. It also
must cover retained-freshness retries and the pair/place/measurement resume
paths, including both named resume methods. A caller cannot map the new error
into RunFailure while dropping the original. From<AuthorFailure> for
RunFailure is forbidden, so ? cannot silently erase returned custody. Its
existing event/stopped-evidence owner must accept the exact return, and
started flags must reflect actual context custody. There is no new worker
orchestration or serving activation in this election.

Required evidence includes two and three caller attempts, an already-latched
failure, a pending release returning under debt while another original is held
through either resume entry, exact original identity and stage retention,
unchanged ticket/sequence and deadline facts, and propagation into bounded
caller cleanup. New admission, release and Closed must refuse untaken held or
template-blocked input while existing private work can settle. Compile-time
negative pins must reject cloning AuthorFailure and any From<AuthorFailure>
for RunFailure implementation. Existing owned pending authors and first-fatal
evidence remain distinct from an unaccepted return.

This does not enlarge HARNESS-HALT-ACCOUNT. Only a real allocated pending
ticket belongs in AuthorAtHalt; no unaccepted original is encoded there by
fabricating an identity. This proposal closes no issue, changes no deployment
and leaves full failed-run composition and qualification open.
