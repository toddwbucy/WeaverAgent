# Root plan-update composition

Implements: state-management-spec-07-plan-forward-flags sections 1 and 2
(PLAN-ADMIN, PLAN-TYPES, CATEGORY, PLAN-REVIEW-IDENTITY, PLAN-EXCHANGE,
PLAN-PREFLIGHT-READ, PLAN-FEED and the root PLAN-EFFECT path);
weaver-admin-Spec-amendment section 6 (ADMIN-PLAN);
weaver-types-Spec-amendment TYPES-PLAN-CONTROL and the plan document carriers;
weaver-harness-Spec-amendment HARNESS-READ and HARNESS-PLAN's root coordinator/cache,
HARNESS-CLOCK and HARNESS-PUMP at the root update join.

This is the root operator operation on code/state-root-plan-update-composition,
stacked on #379 dbe96b1e. It advances #146 sections D and E. It does not close
the broad plan, author migration, qualification or cutover items. The existing
worker driver and admin command parser remain selected. The foundation parser
and dispatcher are held for the qualified coupled cutover.

## Operation and custody

The replacement command accepts only `weaver-admin plan-update <agent>`. Both
effective uid and the parsed cause must be zero. It reads no stdin and accepts
no caller-selected document path. Root cause refusal precedes configuration
loading. The fixed protected file, invocation lock, run-lock checks, root file
descriptor judgment, bounded UTF-8 read, strict document parse, exact SHA-256 and
complete escaped-envelope size check precede the update send. The existing
dispatch writes admin.log and returns the one typed answer and exit status.
No connector sudo role gains the verb.

Admin observes the running worker, takes the Decision allowance from its actual
loaded facts, and opens a fresh authenticated update connection pinned to that
same live worker. A serving management answer closes its connection, so reusing
the answered Observe channel would lose the update. Send and receive share one
deadline. Missing delivery is Unanswered, with no replay or rollback claim.

The worker transfers an exact packet only from its authenticated root peer,
with cause zero, the correct opener/exchange, and no ancillary handles. The
private AcceptedPlanUpdate retains the raw document, original packet, digest,
operation and ordered Text projection. It binds once to the actual recording
context. One shared mark allocator holds either a task operation or the root
operation, excluding competing turn, place, capture and mark work.

One Decision bound begins at acceptance. The owner reads plan_current and marks,
then any exact historical Statement selectors needed for preserved closures.
The existing reader now retains finalized refusal evidence: selector, request,
reply, original Recorded measurement sequence, acquired cursor, partial bytes,
release completion and original bound. Only the Spec's selector/leg/code table
can become read_refused. Invalid codes, failed cleanup, expired bounds and dead
seams still latch component failure.

The ordinary recording/classification/filing path retains the exact turnless
operator plan.event, its complete source partition and PlanInputContext. All
admitted dispositions finish, including after cancellation. Correct rejection
returns ordered original block indices without a plan effect. Accepted blocks
remain proposals; neither the trace nor the member is rolled back.

The checked root grant prepares an effect only from the actual catalog reads,
complete input filing, loaded category/review names and original Decision.
Plan, step and span counters stay separate. Revision, seed, retirement, group,
category, closure text and review evidence checks precede the effect. A valid
preserved closure keeps its original StatementRef. The literal certified
plan.authorize, plan.lock and plan.replace families produce the exact internal
plan.event parented to the input filing. This operation opens no model turn.

The effect's actual complete receipt establishes the required PlanSummary
refresh duty. Only its checked summary supplies PlanUpdated. Failed post-commit
refresh retains the committed receipt and latches; it cannot answer with the
prior cache or undo the effect. A cancelled committed update still drains and
refreshes its effect, but sends no PlanUpdated.

Operation settlement releases the allocator only after all reads, filings and
required trace records finish. A refusal has a private author claim and ticket;
its actual Recorded sequence is retained before fallible continuation. The
original first fatal cause precedes foreign-claim checks in both continuation
entry points. The completed account moves with Updated, Refused or Cancelled.

The worker retains this account independently of a secondary management peer.
Observe reports InTransition while it is pending; another update is refused.
Stop and Leave cancel and drain the accepted work before their own continuation.
EOF retires delivery, not the operation. A blocked answer uses the original
accepted Decision deadline under a separate delivery key, never a new budget.
Expiry after settlement preserves the result and does not halt a healthy run.
Failed cleanup and entered rollback retain the root account with the original
run and resource owners.

## Store correction

The real SQLite replacement case exposed a task-renewal edge: the replaced
plan's completed task could coexist with a newer unrelated open task. Renewal
now retires that current open path before creating the replacement task from
the old held prompt prefix. The regression checks the renewed scope, retained
closure evidence, prompt source, unrelated task retirement and both image
validators. It does not change the Spec's allocator endpoint policy (#246).

## Author evidence and limits

Thirteen root worker process instruments cover two consecutive malformed
updates, a disconnected requester, typed preflight refusal and delayed-read
blocking, Stop during a read, actual Leave rollback cleanup, actual failed-run
cleanup after member EOF, normal SQLite commit and lost-answer recovery, a
separate admin process, answer backpressure and original-deadline expiry, and
Stop while actual SQLite input/effect receipts are held. The last two cases
step a real-readiness proxy to hold an exact receipt; they are not CPU or
blocking-wait measurements. The delayed-read and answer-pressure cases use the
real blocking pump and explicit CPU assertions.

The admin-process instrument uses the foundation parser and shared dispatch,
including one actual admin.log entry. It substitutes only fixture account/path
lookup. Root file checks, invocation/run locks, UID/pidfd authentication,
fresh connection pinning, update and subsequent committed Observe are real.
Separate child-command cases retain an empty open stdin pipe while checking
nonroot sudo cause and malformed command refusal before configuration access.

Private owner instruments cover source identities, read refusal provenance,
closure/counter checks, admitted filing and template refusal, committed summary
validation, trace pressure, exact refusal claims and fatal precedence. Earlier
source snapshots and their controls remain separate evidence cohorts; their
counts are not added to the final suite as distinct cases.

Eight additional compiled controls test the CLI hold, root cause, dispatch log,
failed-run custody, unchanged answer deadline, nonfatal answer expiry, admitted
input drain and committed refresh. Every mutation is restored by byte hash.
The two cancellation controls fail the completion watchdog, with fixture child
watchdog failures also retained; they are not presented as typed runtime
refusals. Earlier controls cover the reader, input/effect and settlement owners.

The final non-SPU source passes these serialized package gates:

| Package | Passed | Ignored in the package invocation |
| --- | ---: | ---: |
| weaver-types, default | 198 | 0 |
| weaver-types, config | 256 | 0 |
| weaver-admin | 603 | 73 |
| weaver-state, sqlite | 226 | 8 |
| weaver-gate | 86 | 57 |
| weaver-harness | 2,197 | 906 |

The harness count is 2,115 library, eight worker, 43 integration and 31
doctests. It excludes nested child summaries and explicitly filters known
#331. The full serial harness package takes 1,070 seconds on this host and
includes the existing pressure/timeout waits.

The separate root runs pass 77 distinct harness identities: 30 private root
owner cases, 13 root worker process cases, 14 Observe, 17 Stop and three
management cases. One authentication identity overlaps the ordinary library
run. Two additional root admin cases cover fixed-file judgment and command
admission. Those runs do not establish a pass for every remaining ignored test.

All five non-SPU affected-crate all-target lint gates, formatting, lock, diff
and three document-mirror tests pass. The pinned nightly and --locked use
CARGO_INCREMENTAL=0 and private target
/tmp/weaver-state-146-serving-organ-loss-target. All 803 Rust/configuration
hashes remain unchanged through these gates. Reverse-transform SHA-256 checks
isolate the test-only additions and lint rewrite between the control cohorts.

The SPU cuda,gguf package passes 407 tests with 0 ignored; its all-target
strict lint passes. The package takes 744.65 seconds on Olympus,
using private target /var/tmp/weaver-state-root-plan-cuda-20261029 with debug
symbols disabled. The native model suite passes all ten cases, including the
two-device generation route. These gates retain the same 803 source hashes.
No production classifier, grammar, corpus, prompt or template qualification
follows from these tests.

The source archive, binary tracked patch, untracked source archive, gate logs,
controls, source-cohort proofs and failed development runs are deposited at:

`/bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/root-plan-composition-20261029T022316Z-70c7b1d3`

The deposit verifies every file by SHA-256. It identifies the #379 base and the
uncommitted tested diff; the PR names the resulting commit. The final census
excludes nested child summaries. Earlier control cohorts retain their own
source manifests and are not counted as new final-source suite identities.

Development failures remain in the evidence: fixture compile corrections,
wrong fault-record spelling, a missing admin test-binary environment variable,
and two incorrect proxy-readiness implementations. The final proxy polls real
readiness and never reports expiry before an armed deadline. The initial lint
also rejected repeated sequence-string allocation in a test comparison; the
comparison retains the wire's string shape after correction.

The requested code-review and verification-before-completion skills were not
available in this session. Manual author review and executed gates remain
subject to Planner verification. This record claims neither a Planner grade nor
a security disposition.

## Remaining composition

The full foundation CLI election, Diagnostic route, remaining plan/forward-flag
and selection consumers, author/retirement map, inherited review carries,
production language implementation, qualified artifacts, measured limits and
coupled cutover remain open in #146. The merged #377 dictionary and rule walk
are candidate inputs, not qualification or owning-Spec elections.

The process tests here do not establish every combination of root update with
graceful/forced save, every late summary cancellation, or a complete qualified
admin/worker/model run. The actual Leave and failure cases begin during the
first root read; the committed cancellation cases use Stop and real SQLite.
Those boundaries are kept distinct from the shared owner proofs.
