# Exact waiting author and Recorded custody

Date: 2026-10-23. Branch: code/state-author-claims. Exact parent #280
49e276aa5dfa3c7513eea3bbd4ff4dc832b96742. Advances issue #146 and answers
#275 review rows 1-9 within the dormant private owners. It closes no issue.

Implements: merged HARNESS-RECORD-LIFETIME, HARNESS-HALT,
HARNESS-HALT-ACCOUNT, HARNESS-CLOCK and HARNESS-PLACE; Spec 06 PLACE-CALLS,
PLACEMENT-RECORD, FIT-REFUSAL, STAGED-APPEND and CUT-WAIT; the existing decode
terminal/Cancel contract. Author assessment: conforms within this primitive
scope, subject to the validation and explicit limitations below.

## Original, claim and allocated ticket

WaitingAuthor moves the original event and its optional private claim together.
Ordinary input cannot acquire a placement or measurement claim. Each owner has
an independently allocated Arc identity; matching requires both the owner kind
and that exact identity. Claimed queue admission requires Entered before taking
the original or allocating a ticket. A foreign retained carrier fails without
being consumed. Ordinary cancellation takes its original event; stopped owner
retrieval requires the exact claim.

QueuedAuthor returns the ticket assigned at the actual allocation site, even
when later preparation fails. An earlier Busy/unadmitted result carries no
ticket and cannot borrow the older pending slot. Shared AuthorRecord predicates
retain the separate exact-ticket/sequence completion backstops. There is no
ticket prediction, event-shape lookup, public factory or cloned event custody.

## Recorded facts before witness replacement

Every context trace-only resume uses one wrapper that immediately binds the
actual Recorded witness to its Place/Measure owner. A later trace-only author
can replace the barrier's one witness slot only after those facts are retained.
Boot and summary consumers keep their own distinct account proofs. An ordinary
post-Recorded observer failure also binds actual admitted facts, including the
original TurnClosed, while Failed remains terminal and yields no live result.

The whole production callsite census covers generic halt draining, stopped
Boot Open and summary reads, measurement's prior/refusal/stopped drains and
stopped placement authors. Lower-level barrier tests deliberately retain direct
calls. Existing held-original guards, exact admitted continuations, typed Boot
rollback and the Failed-Enter owner's domain exclusions remain unchanged.

Both measured/render and fit refusals use the retained immutable MeasureRequest
assembly identity. They do not infer it from a request prefix. Generation's
cancel_settled flag records the exact decode reservation's settlement separately
from cancel_sent and done. Settled transport does not satisfy owed model records;
an already queued Cancel still requires its actual AtRest response.

## Tests and limits of their claims

Fourteen new tests cover actual queue allocation and Busy, exact genuine owner
identity, Releasing refusal, admitted-before-failure facts, trace-only witness
replacement and real terminal/Cancel accounting. Named private seams model a
foreign original, declined claim or post-Recorded observer failure. These are
defensive instruments, not asserted ordinary caller paths or forged tickets.

The older-placement/new generic-fatal-measure interleave is unreachable under
current ordinary admission. The shared wrapper has an actual pressured-owner
test and every production callsite is migrated, but reverting only that prior-
drain callsite is an unbitten caller-specific probe. Removing cancel_settled
alone is redundant with the existing exact reservation-settled check. Neither
limitation is represented as an independently biting behavioral control.

The first focused run found one new fixture error: it asked the live
place_close accessor for a frame while Failed. The accessor correctly hid it.
The corrected fixture checks the privately retained frame's stopped kind,
actual run and original turn, then separately checks that the public accessor
still refuses it. Production is unchanged; the original file and failed log
are retained. Earlier source review corrected two fixture counts to include
the actual Boot placement baseline.

## Validation and remaining scope

Twenty-one Rust paths were integrated from the frozen reviewed source: twelve
exact preimages and nine explicit three-way joins against #280. The joins
preserve #279 held admission and #280 cleanup changes. Source reviews are
author evidence, not a Planner grade.

Harness passes 1,793 tests with 18 inherited ignored cases; trace passes 160.
The focused 10 claim, two queue and two cancellation tests pass. Full suites,
affected all-target Clippy, formatting, lock and diff checks pass after the
fixture correction. Ten nested child-exec summaries in the full harness log
are retained separately and not added to the top-level count.

All 23 unique behavioral controls compile and fail their named tests, with
exact restored passes. Across 24 preserved attempts, the original A9 queue-only
mutation survived its older eventual-placement test: the new actual ticket
return lets that final owner recover. The identical mutation fails the existing
direct barrier test, which checks the exact pending original/ticket and no
Recorded admission before explicit resume. This proves the queue-only API
boundary, not independent failure of eventual placement binding. No production
or test change followed that survivor; the original log and filter-only rerun
remain preserved.

Two separate E0308 compiler pins reject cloning WaitingAuthor or AuthorRecord,
with one restored named test passing after each. All 21 restored source hashes
match the pre-control snapshot. These are author checks, not a Planner grade
or production qualification.

No worker caller, wire field, deadline, placement grant or generation permission
is added. Healthy refusal/explicit rollback, complete worker/lifecycle migration,
remaining placement/SPU review followups, qualified language artifacts, reviewed
system prompt, measured limits and coupled cutover remain open. The existing
serving implementation remains active. Unmerged #260/#263/#268/#276 are not
implemented. All new PRs begin as drafts; the operator alone merges and controls
CodeRabbit review requests.
