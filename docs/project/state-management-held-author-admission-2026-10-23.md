# Held author admission and retained rollback

Date: 2026-10-23. Branch: code/state-held-author-admission. Exact parent #278
fd8730967b28092aeeba61558a10cdb4be541079. This slice advances issue #146
and answers #274 review rows 2-9. It closes no issue and activates no serving
caller. Row 1 remains the independent, unmerged author-return proposal #276.

Implements: HARNESS-RECORD-LIFETIME, HARNESS-HALT-ACCOUNT, existing release
and Closed custody, HARNESS-PLAN-SUMMARY, and Spec 07 failed-attempt/pair
accounting. Author assessment: conforms in the dormant owners. The code uses
existing bounds and outcomes; it adds no trace field, wait or admission policy
from an unmerged proposal.

## Held originals and existing owners

An untaken held or template-blocked author now prevents new owner admission,
readiness, ordinary release and Closed. Boot Open, summary and placement
release refuse before cancelling or revoking their owned work. Boot Open's
existing first-fatal result keeps precedence. No held original acquires a
synthetic ticket or enters AuthorAtHalt. The caller can take its actual input.
This does not fix the consuming author interface's third-call loss; #276
retains that separate election and caller migration.

The hold must not stop already-admitted work from settling. Exact private
purpose, driving state, owner stage, turn, and existing bound permit the
summary, boot-pair, marks and inward read continuations; owed pair/mark/boot
filings; repair's internal pair; confirmed generation output; and queued
reload measurement. There is no public bypass flag or general live exemption.
The early source review found and fixed an unconditional guard that would
have stalled a summary's own read. That original snapshot is preserved.

Real-owner tests hold competing input while those existing continuations
progress. Separate release tests retain the original and owner without
cancellation; taking the held input permits the valid next operation. A late
Closed hold and a genuine foreign-engine composition are labelled defensive
fixtures, not claimed ordinary public reachability.

## Pending Unload and typed rollback

Ordinary pending Unload rechecks Boot Open and placement debt after existing
release-scope validation. An unadmitted authored ticket returns its exact
AuthoredEvent and owed type; a raw prepared event keeps its raw Busy return.
Preparation and attempt bookkeeping retire together without a new sequence,
ticket or wait. Placement debt retains precedence. Public admission currently
excludes the tested debt rise; the tests use actual pending raw/authored owners
to exercise the defensive barrier.

Ordinary summary admission and Entered restore require standing state.
Failed/Releasing cleanup still admits its actual retained rollback without
that resource. The drain exception is computed once and passed to the owner
guard; original Decision/Cut/Enter/Leave bounds remain.

PlanSummaryRollback groups the same thirteen non-evidence fields. Its typed
carrier retains the prerequisite and actual Recorded evidence separately;
conversions move all three fields. No Default, clone, reconstructed proof or
Deref hides ownership. The separate type pin rejects constructing successful
Boot from a generic or missing read owner.

## Correction proof accounting

A shared iterator validates the correction's phase and receipts. Completion-
only cancellation scans use a no-op sink and do not collect proof references;
callers needing their order collect it. A pending prefix does not hide a
later true error, while missing post-accounting proof remains an error.
Actual immediately completed pair returns reach production validation, and
a failed validation retains the actual returned correction. The defensive
foreign-proof and foreign-engine tests use genuine capabilities and do not
claim independently constructible successful evidence.

Original P10/P11 remain redundant producer defenses; no fabricated proof is
used to claim independent coverage. The thirteen-field grouping and removal
of unnecessary collection are structural/source observations, with behavior
tests for custody and error ordering.

## Validation and remaining scope

Thirty-one Rust paths were independently reviewed. Twenty-eight matched the
exact incoming preimage; context_place, context_place_account and recording
were joined against #278. Independent comparison found their complete edit
blocks unchanged from the reviewed batch, preserving #278's phase/evidence
and source-receipt changes. Fourteen new tests and three existing migrations are
included. The root migration adds a 32nd Rust path: an existing repair test
started new output while its unrelated caller original was still held. It now
checks the exact pointer/bytes after actual repair progress and takes the
original before starting that new output; production guards are unchanged.
The original failed full-suite log is preserved.

Harness passes 1,752 tests with 18 inherited ignored cases; trace passes 160.
Focused groups, harness all-target Clippy, formatting, lock and diff checks
pass after the test migration. All 26 source-pinned behavioral mutations compile and fail their named tests,
with exact restored passes. The separate typed-Boot pin produces the expected E0308 mismatch and its
eight restored tests pass. The initial log validator expected an unqualified
type name; rustc supplied its qualified path. The exact diagnostic was then
validated from the preserved logs without changing source or rerunning tests.
All 32 restored source hashes match the tested snapshot. These are author
checks, not a Planner grade or production qualification.

The next Failed-Enter cleanup retains its explicit refusal of unticketed held
input and must return the context/resources unchanged. Healthy refusal and
explicit rollback still owe stopped-inspection and waiting-input custody,
process/LastWord enrichment and complete lifecycle composition. #275's witness
and claim followups, remaining #269 rows, worker migration, qualified language
artifacts and measured coupled cutover remain open. Approved but unmerged
#260/#263/#268 and the unelected #276 are not implemented. Only the operator
merges and deploys; all new PRs begin as drafts, and issue #146 remains open.
