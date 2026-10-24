# Bounded Failed startup cleanup

Date: 2026-10-23. Branch: code/state-failed-enter-cleanup. Exact parent #279
5f7a337256a4693d97900efffa1d8a38d0aa337e. This slice advances issue #146;
it closes no issue and activates no worker caller.

Implements: HARNESS-RECORD-LIFETIME, HARNESS-HALT, HARNESS-HALT-BOUND,
HARNESS-HALT-ACCOUNT, stopped HARNESS-OPEN and HARNESS-PLAN accounting, and
existing fork/child custody. Author assessment: conforms for the explicit
Failed pre-input domain below. Full lifecycle composition remains owed.

## Accepted ownership and terminal result

The consuming cleanup entry requires an actually Failed context, its first
latch and its unique original resource bag. It refuses prior TurnStarted,
input/task/capture/repair/place/measurement authority, ordinary reads, held or
template-blocked caller input, waiting content and accepted Leave. Every
refusal returns both originals unchanged. Only genuine private Boot-derived
read/pair/filing/summary owners are admitted. Extracted Boot proof must first
be restored through its existing checked adapter.

The resource bag is issued once from its context and populated only by actual
fork producers. It retains each child before later fallible bootstrap work.
Original decode/residency identities prevent replacing a producer endpoint.
There is no public scalar PID constructor, duplicated bag or reconstructed
transport. The serving-thread lifetime contract stays explicit on fork entry.

| Boundary | Retained evidence and next act |
| --- | --- |
| Admission | Exact Failed context, original first cause and one same-origin producer bag; refusal returns both |
| Fault | Attempt the existing first fatal account before owed Boot work |
| Boot account | Settle or retain the original incomplete Open/summary under its inherited minimum |
| Retirement | Close decode before Release; move state/classifier/packet buffers and identities into descriptorless evidence |
| Release | Confirm only the actual fully sent exchange, matching Close position and Released answer; preserve wrong or premature bytes |
| Child custody | Poll original children with WNOHANG; at cap attempt kill once and retain any unreaped original |
| Unload and drain | Attempt the existing typed failure Unload once where reportable; drain or abandon under the original bound |
| Closed | Return only failed evidence and nonblocking child custody, never a usable run or successful lifecycle result |

The driver preserves the existing halt allowance and any earlier Leave cap.
Known clock failure or expiry stops further sends/authors, even if a later
clock read recovers. Waiting uses the same PumpIo and the minimum of that
original bound and a local 10 ms wake. It adds no timeout or retry grant.
A reap is an actual wait status; kill success is not a reap.

State already retired into a stopped summary read is consumed as well. The
retained descriptorless channel keeps its exact partial bytes, requests,
answers, timings, operation, completion and record facts. No filing receipt
or successful state operation is invented. An unreportable pre-open failure
does not manufacture Load, flow facts, Fault or Unload.

## Intentional limits and later callers

A valid UnsupportedArtifact admission refusal remains healthy and cannot enter
this owner. Healthy refusal and explicit rollback need their own actual
lifecycle cause/exchange, grants observations, pending-author custody and
stopped-inspection handoff. This owner excludes those unsupported states;
none is silently relabelled Failed to gain cleanup access.

The actual fork producer retains the required stderr pipe/tee and LastWord
owner. The driver closes its owned transport endpoints and does not call the
legacy extra LastWord wait or blocking reap. The existing tee reader may live
until stderr EOF/process exit; typed last-word death-account enrichment still
belongs to the later lifecycle composition. This is not a complete section
2.2 stderr/death-account implementation.

The final caller must retain the resource bag across all early returns,
restore any extracted Boot proof, finish and exit failed, and keep external
admin/listener/gate containment under its own original bounds. Active-turn,
accepted-Leave and healthy rollback cleanup remain separate. The existing
serving implementation remains active until complete qualified cutover.

## Validation

Eighteen source paths were independently reviewed against exact #274 and
joined onto #279. Twelve match their incoming preimage; six preserve the later
context, Boot Open, typed summary, read, receipt and coordinator changes.
The source includes 21 actual owner fixtures and six child entry fixtures.
The child entries are inert in an ordinary suite invocation; the actual fork
producer selects them by exact test name where required. Scripted peers are
test doubles, not qualified production language or state artifacts.

Harness passes 1,779 tests with 18 inherited ignored cases; trace passes 160.
All 21 cleanup-owner fixtures pass. They invoke the six child entries ten times
through actual fork/exec; those ten nested summaries are retained separately
and do not inflate top-level counts. Required lint, formatting, lock and diff
checks pass. Full suites precede only the two-file mechanical lint correction;
all 21 owner fixtures and affected lint/format checks pass afterward. The
original files and lint failure remain in the evidence.

Twenty unique behavioral controls compile and fail their named tests, with
exact restored passes. The initial run retained one deadline-control survivor:
the Reporting fixture's separate halt guard enforced the same cap. The same
mutation fails the existing actual wait fixture, which isolates the driver's
post-wait check. One invalid bare-let match-arm mutation was corrected to a
block expression and then compiled and failed. No source or fixture changed
for either control correction; all original results remain preserved.

Three separate compile pins produce E0308 for cloning resources, E0599 for a
live-fd method on retired state, and E0616 for child PID access. Each restores
and passes its named test. All eighteen restored source hashes match the
pre-control snapshot. These author checks do not constitute a Planner grade
or production qualification.

The controls preserve real socket/fork ownership, trace pressure, raw answers,
first cause and original cap. Reaping performed by fixtures after the driver
cap is evidence collection outside that cap. Removing two overlapping Boot
proof guards together is explicitly a grouped control; neither alone is
claimed indispensable. Existing transport tests retain partial-send scope.

Issue #146 still owes author migration, the separate review followups, healthy
lifecycle and worker composition, qualified artifacts and measured coupled
cutover. Unmerged #260/#263/#268/#276 are not implemented. All new PRs begin
as drafts; only the operator merges and deploys.
