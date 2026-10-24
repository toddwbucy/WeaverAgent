# State management boot composition

Date: 2026-10-23. Version: 0.1. Author implementation record, not a Spec.

## Scope and pass statement

The operator redirected Issue #146 to composition on 2026-10-23. This act
starts from exact #286 8ffc3dd0b8acc9084e27fa98ad710a2c76830ba9. It connects
existing primitives under one caller and one original Enter wait:

1. Certify the exact recorded compiled template inventory.
2. Read and release the seeded system prompt, then retain its Recorded BootHead.
3. Send the one reserved decoder Open and await Recorded placement and fit.
4. Repair prior-run open pairs through actual terminal filing receipts, then
   require the second empty open_pairs query.
5. Read, release and record plan_current, committing the exact Boot summary.
6. Install the placement engine from that same summary and checked Open chain.

The same context and decode port stay exclusively borrowed for the sequence.
There is no second recorder, author, journal, cursor or deadline. Returned
prerequisites and refusal evidence remain owned by the composition or context.
An interruption, refusal or failure never skips to the next success stage.
Final extraction rechecks the original Enter budget and observed lifecycle.

A passing integration test certifies these connections over actual socket and
recorder owners with explicitly scripted peer contracts. It does not certify
SQL behavior, classifier correctness, model rendering/tokenization or production
language artifacts. It does not send lifecycle Ready, create processes, start
a user gate, select a workflow or activate replacement serving.

The constructor's prepared context already has actual state Open, untapped
Load, semantic activation and classifier admission. Those producer connections
remain owed. The existing primitive admission checks verify this prerequisite;
this act does not reconstruct or assume a successful proof. Orchestration of
serving content remains in worker/dev_loop. Healthy rollback and the complete
process cleanup caller remain owed, including adopted resources and accepted
Leave correlation. Composed boot completion alone grants neither Ready nor Left.

## Authority

Implements: HARNESS-BOOT, HARNESS-OPEN, HARNESS-READ, HARNESS-RECALL,
TEMPLATE-CHECK and HARNESS-PLAN boot sequencing; Spec 05 BOOT-REPAIR;
HARNESS-RECORD-LIFETIME and HARNESS-CLOCK for retained context and waits.
The detailed section grading will accompany the PR. Existing merged authority
is sufficient for this sequence. Approved but unmerged #260/#263/#268/#276/
#282/#285 remain unimplemented here.

## Findings from composition

The first connected run stalled at Prompt. Individual read-backed owners
reduce their state but depend on a caller to service the common pump. The
composition now pumps when those owners report Pending and continues immediate
work when they report Progress. Repair and summary now preserve the actual
underlying local-progress result instead of reporting every incomplete step as
Pending. This prevents a caller from sleeping while it still has local work.

The first nonempty repair fixture incorrectly replaced JSON property names
along with run values. The correction changes only string values. Initial
failure logs and the pre-correction fixture are retained.

## Validation

Seven connected tests pass: complete empty repair; two prior calls with actual
Filed receipts; template rejection; prompt refusal; real failure after summary
before installation; expiry before final extraction; and delayed actual prompt Release. The tests assert one
decoder Open, exact query order, all four cursor Releases, retained original
Enter and run origin, and no input admission. The fixture clock and readiness
are scripted together; actual peers carry the protocol bytes.

The full harness suite passes 1,818 tests with 18 inherited ignored cases; the
trace suite passes 160 tests. Ten nested harness child summaries are excluded
from the top-level count. All-target harness Clippy, formatting, lock and diff
checks pass with private target /tmp/weaver-146-boot-composition-target.
Five compiled protective controls fail their named tests: omit prompt pumping,
continue after template rejection, omit final original-window validation, lose
the stopped summary, and omit placement installation. Each exact restored test
passes, and all harness source hashes match the pre-control snapshot.

The first broad run found two existing tests with three Pending assertions for
steps that now truthfully return Progress after transferring a receipt. Only
those expected progress values changed; the owner and receipt assertions remain.
Initial failure logs are retained. These are author checks, not a Planner grade
or production qualification. The prescribed code-review and verification skills
were unavailable in this environment; a direct source/sequence self-check and
the recorded executable gates were performed instead. No independent review is
claimed.

## Sequence review and next composition

The new progress enum has no external seam or dependency change. Existing
callers either compare Complete or are tests; the new caller services transport
only after actual Pending. The Open primitive already pumps its own transport.
Every extracted prerequisite is stored before the next live check. Rejected
begins retain their returned owner in the stopped cause; an interruption retains
all outstanding context work for the lifecycle caller. The stopped handoff is
custody, not cleanup. The integration tests do not cover accepted Leave, failed
process cleanup, or recorder pressure across the whole sequence.

Next connect actual state Open, load, activation and classifier admission to
this prepared-context entry, then connect its stopped result to lifecycle
rollback and its completed result to the remaining Enter obligations. Keep
worker/dev_loop responsible for content orchestration. The full Issue #146
remains open; no existing serving selection changes in this act.

The composition-first approach found a transport-progress gap that isolated
primitive tests had serviced externally. Sustain one caller driving the complete
sequence, and test the next lifecycle boundary through real transport ownership
before claiming its integration.
