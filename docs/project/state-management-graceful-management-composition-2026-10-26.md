# Secondary management during graceful Leave

Date: 2026-10-26. Author working record for issue #146.
Branch: code/state-graceful-management-composition.
Stack base: #352 at 1bd6453adf6d2942e9cb1c009ed1fcb31959dd79.
Joined #350 at cb741ce554dd7f800742ae53a8d3e4269804e004 and
#332 at 42436df71d1247caef3abc9d296093ad2ecfe01e, preserving both parents.
Local tested implementation parent: 41725cceb2e2085fac7be94e30b56f84ebbfca63.
Publication recreates the two joins as 7882e9ee592694a9f41b96ea2ae7dcf1ee3f3698
and c3bc9db7ab827e7fb873f2c31df1a182bd09509a, with identical trees and the
same reviewed parents, after the local Git credential rejected the push.

When a graceful Leave becomes forced while a secondary admin connection has
sent no directive, management now continues reading that same connection under
its original two-second bound. Observe answers InTransition, a real JoinLeave
joins the original Leave, silence is dropped at that bound, and other valid
directives receive a correlated OutOfOrder. The original input and Leave then
continue through the existing forced tail.

This answers #350 review 5657773404 L5, B3, B5 and the secondary-dial part of B6.
Its separate component-death case L6 remains. The new code implements merged
rules; it needs no election from the pending #348/#349 proposals or #350 Q1.
It advances #146 section E's lifecycle/owner handoffs and section G's review
corrections. No broad completion checkbox closes.

Implements: weaver-harness-Spec section 3 serial directive ordering and section
6 pending Leave, secondary dial bound, force conversion and joining; harness
amendment HARNESS-CLOCK and HARNESS-RECORD-LIFETIME.

| Section | Author disposition, pending Planner verification |
| --- | --- |
| Harness serial directive ordering | Conforms on the exercised Leave paths: eight out-of-order requests receive their own OutOfOrder, before and after force. Observe and JoinLeave retain their separate outcomes. |
| Harness section 6 | Conforms on the four exercised conversions and secondary outcomes. Original cause, no late forced_by, normal SQLite capture, actual reap and final Left are checked. |
| HARNESS-CLOCK | Conforms at this read boundary: acceptance starts the original two-second AdminRead; conversion does not restart or retire it. Declared phase expiry and actual gate EOF drive conversion. The separate post-send clock-priority carry B4 remains. |
| HARNESS-RECORD-LIFETIME | Conforms at this handoff: the exact input, original Leave and any joining dial remain owned through the complete tail. The closed gate stays owned for cleanup; no Quiesced or Lower acknowledgement is invented. |

## Implementation and boundary review

- FoundationManagement keeps Pending when an actual phase expiry or retained
  Quiesce gate EOF converts the Leave. DirectivePending requires a real held
  directive; it is no longer used as a substitute for the missing continuation.
- A helper derives retained gate EOF from Quiesce's existing failure and the
  exact current gate port identity. Management's pump and input's observation
  pump omit only that already failed gate. The listener, admin dial, residency
  and decoder remain observed. No descriptor is retired and no grant is added.
- The input pump is necessary after a secondary answer: the next accepted
  connection must reach management rather than encounter the same EOF again.
  Restoring either pump's old gate entry independently fails a case.
- During a pending graceful Leave, only a real JoinLeave is delegated. Observe
  still uses its typed InTransition response. Every other valid directive is
  consumed and refused through the existing tracked lifecycle send. Stop does
  not settle a competing turn or create StopWork in this position.
- Existing validation for opener, position and descriptors remains before
  disposition. Outside a graceful Leave the existing Observe/Stop and lifecycle
  delegation paths remain. The combined gates exercise those consumers too.

The new helper has exactly two callers. After a retained EOF, input progress
is LeavePending, so input's ordinary step, report and Quiesce work do not resume;
management or the forced handoff owns the next action. The forced tail retains
the failed gate and its accounts until actual reap. A gate without this retained
EOF, a foreign port, an SPU failure and existing fatal handling keep their prior
behavior. Identity exclusion is also reviewed at the helper; the four controls
below protect continuation and the two reachable pump sites, not every possible
foreign-port construction.

## Test-first evidence

Sixteen cases use actual accepted Unix connections and original monotonic read
bounds: drain expiry with an owed filing, wind-down expiry before its turn is
started, Quiesce expiry, and actual Quiesce gate closure, each followed by
Observe, JoinLeave, silence or eight out-of-order commands. The gate case also
checks all eight commands while the Leave is still graceful, before the fixture
closes the gate. The refused requests are graceful, forced and rollback Leave,
SavePoint, Stop, Release, Quiesce and Lower.

The shared helper checks the exact input pointer, accepted read origin and
absolute deadline, original Leave hearing, unchanged cause and absent forced_by.
It exercises sequential fresh connections after refusal, not just the first
answer. Every case then drives the existing normal-member capture, confirmation,
release and final Left. Gate-closure cases require actual child reap and the
absence of invented Quiesced/Lower replies. The gate cannot have written a Lower
receipt after it has exited; that fixture assertion now checks absence there.

Before runtime edits, twelve cases reproduced the empty-dial DirectivePending
dead end. Four initial Quiesce cases had a missing Enter drain bound; after
fixing that fixture, those four reproduced the same intended assertion. These
are two explicit baselines, not a claim that the original sixteen-case run was
cleanly attributed. A Pending-only trial exposed the remaining gate and refusal
paths. One later fixture mode ended in the older capture-refusal suffix; its
name was corrected without changing the expected lifecycle result.

| Compiled control, restored afterward | Case and detected failure |
| --- | --- |
| Set DirectivePending on conversion again | Drain Observe loses its readable owner before any directive exists. |
| Poll the retained gate from management again | Gate Observe is dropped at AdminRead instead of receiving its correlated answer. |
| Poll the retained gate from input again | The next secondary dial cannot be accepted after an answer. |
| Delegate every non-Observe directive during Leave again | A valid out-of-order command is stranded instead of receiving OutOfOrder. |

All four controls compiled and failed at their intended assertions. Every source
byte was restored before the final checks. These are behavior protections, not
compiler failures or timeouts of the test runner.

## Combined verification at 2c645e62

Pinned nightly-2026-02-13, --locked, private target
/tmp/weaver-state-146-graceful-management-target, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4 and RUST_TEST_THREADS=4. The source stayed frozen during
checks. Cargo and Git mutations did not overlap.

- Full harness: 2,109 top-level tests/doctests passed;
  734 ignored. This is 2,028 library, 50 binary/integration
  and 31 doctests, excluding nested child summaries.
- All 670 inventoried combined worker composition cases passed,
  including the actual admin/SQLite joint participants. The normal state member
  and admin test executable were built in this private target before that run.
- The sixteen new cases also passed three further serial repetitions, 48 case
  executions. The combined run already includes each once.
- Four compiled controls failed at their intended assertions. The final restored
  source passed strict harness clippy for all targets, formatting, diff and
  first/last lock checks. All 1,030 tested file hashes match.
- Gate runs used the operator-authorized real-root delegated scopes. Exact
  filters, inventories, commands and executable hashes are in the evidence.

The combined source retains #350's accepted F2/J1 correction, #352's accepted
F1/F2 corrections and #332's merged rollback join. #352 itself still lacks the
parent J1 join; this new branch does not change that parent branch or claim its
review hold is lifted. The merge joins here retain the original reviewed parents.

Source archive SHA256: 07f41cd5abac144cdee83a76fee7c48337af2cabd3ea86c04aef0f7976e6a6c4.
Source manifest SHA256: 0bc8e9e0042108b9ed458cf5e07e30249a532e56354a289f6b90e79c2178514d.
Evidence: /bulk-store/weaver-testing/state-management-146-olympus-2026-10-22/state-graceful-management-20261026-07f41cd5/.

The source archive includes all tested tracked and new source files. This record
is written after the gates and retained separately. The deposit includes all
baselines, the failed fixture iterations, passing cases, four controls, restored
source hashes, exact commands, inventories, binary hashes and the full gate logs.
The publication receipt identifies the commit separately. Publication metadata
and this updated record are also retained in the adjacent
state-graceful-management-20261026-07f41cd5-publication deposit; the tested
source files are unchanged.

## Planner review and fixture verification

The Planner accepted the production change at 2c645e62 and graded one fix, G1:
the secondary Quiesce fixture pumped the live Leave while waiting for the gate
child to write its request file. Once Quiesce had been sent, another advance
could wait until phase expiry or child departure before the secondary dial
existed. A child scheduling delay made the test miss the boundary it meant to
exercise. The gate-end variants waited until the classifier double's watchdog;
the expiry variants encountered the declared phase bound.

The fixture now advances only until its original Quiesce send ticket is sent,
then waits for the child file without driving the worker. The helper uses the
existing control accessor. The child accepts a test-only scheduling delay from
WEAVER_QUIESCE_REQUEST_DELAY_MS. No runtime behavior or deadline changes.
The secondary helper also asserts that the Leave remains graceful immediately
after acceptance and read arming, answering B4's coverage gap: conversion must
occur while management owns that accepted dial.

The original wait, compiled with a 5 ms child delay, failed all eight affected
gate/Quiesce cases at the expected Stopped assertion. After the correction, all
sixteen secondary cases passed at 0, 5 and 50 ms delays (48 executions). All
eleven earlier Quiesce cases selected by the recorded filter also passed. The
source remained unchanged through strict harness all-target clippy, formatting,
diff checks, and first/last lock gates. The same pinned toolchain, private target
and serial Cargo/Git discipline apply. The broader combined counts above belong
to 2c645e62; they were not rerun for this test-only correction.

Both source snapshots, delay controls, logs and exact commands are retained in
the state-graceful-management-g1-20261026 evidence deposit under the approved
state-management-146-olympus-2026-10-22 root. Its manifest and publication receipt
identify the corrected source separately. This is an author answer to G1/B4,
pending the Planner's regrade.

| Review rows | Disposition |
| --- | --- |
| G1, B4 | Corrected and exercised as above. |
| B1, B3 | Carry: Enter while leaving and Stop during an already-settling refusal still need their named cases. |
| B2 | Carry: exact gate identity is an invariant of current ownership, not pinned by the four runtime controls; lending consolidation remains. |
| B5, B8-B10 | Carry: bounded silent-dial spin, shared directive disposition and sealing Observe inconsistency, dead join guard and fixture duplication remain. |
| B6 | Carry: the pre-existing contract's pending-Leave wording needs its own reviewed reconciliation; no contract copy changes here. |
| B7 | Named review skills remain unavailable; this executor claims manual review and executable gates only, with no waiver inferred. |
| L1, L2 | Filed in WeaverAgent#355 with their separate confirmed-by-reading/plausible qualifications and reproduction criteria. |
| C1 | Cutover carry: the foundation dispatcher must answer JoinLeave with no pending Leave as OutOfOrder. No dispatcher is selected here. |

## Remaining work and after-action

#350 F1/Q1 and L3/Q2, #348/#349 elections, the separate post-send clock priority
B4, component-death L6, #351/#352 permanent holds and other graded carries remain.
#353's rejected-return-after-close carry remains. This branch does not implement
capture-refusal policy, change persistence causes or reorder admin publication.
The Planner accepts this PR's production answer to L5/B3. Its G1/B4 fixture
correction awaits regrade; the remaining review rows above stay carried.

Foundation remains unselected and production still calls dev_loop::drive.
Model/classifier peers in these instruments are declared doubles. No language,
model, GPU, security, whole-workspace or production qualification is claimed.
The named code-review and verification-before-completion skills are unavailable;
the manual second read is not their invocation. #146 still owes complete
lifecycle/author composition, artifacts, limits, selection and coupled cutover.
Only the operator merges or deploys.

After-action: a phase conversion is not an incoming directive. The continuation
must retain the accepted connection and its original clock. Testing through the
next connection and the terminal capture/Left exposed the two pump sites and
fixture suffix collision that a single outcome assertion would miss. Keep
those complete sequences in the review surface.
