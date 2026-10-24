# Rollback read-back composition

Date: 2026-10-23. Branch: code/state-rollback-readback-composition.
Parent: #293, e35760fd36cc3d44020e044e95bd667e6110ab83.
Advances Issue #146 composition. No full checklist item closes.
Operator alone merges/deploys; existing serving remains active.

## Connected path

The actual startup's retained initial Grants and the accepted rollback's measured
current Grants now travel into a consuming release-preparation handoff. A typed
current read refusal retains its exact code, detail and recorded measurement and
continues toward release. It no longer stops healthy rollback in Entered. This
implements the correction to #291 recorded in #292 Planner review 5631239605.

The handoff derives Unchanged from equal actual ordered surfaces, Varied from
unequal surfaces, and Unreadable if either reading is a typed refusal. A refused
read is never an empty surface. Both original observations remain owned, with their
distinct request identities, recorded times and original payload buffers. The
original accepted Leave, its cause and wire forced bit, forced rollback outcome,
unchanged Leave bound, Recorded Enter refusal and actual resources/peers remain
in the same chain. The caller cannot supply a replacement summary or deadline.

A current reading alone is insufficient: the existing begin_releasing transition
must succeed before into_prepared consumes the owner. Premature or unusable joins
return the complete owner. PreparedRollback exposes only borrowed facts and the
derived GrantSurface; it provides no mutable context or terminal permission.

After receiving the actual Leave, that invocation's connection is an answer path,
not another source of directives. Its EOF therefore cannot cancel Grants read-back
or the accepted rollback. The eventual answer remains owned for terminal cleanup.
Actual state transport loss still stops preparation through the common pump.

## Authority and verification

Implements trace PRD 3.1 and harness Spec 6 for the two boundary readings and their
comparison before release; HARNESS-HALT-BOUND for sole-owner observations and no
stale substitution; admin-harness contract 2 for independence from the accepted
caller's answer path. These subpaths conform within the dormant composition slice.
Listener and terminal obligations in those sections remain incomplete. No unmerged
Spec election is implemented, and no wire type or active serving path changes.

Twenty-two joined response/rollback scenarios pass, including six added cases for
equal/reordered/empty surfaces, two refusals, accepted-peer EOF and actual state EOF.
The formerly stopping current-refusal case now checks both readings and consumes
the preparation handoff. Tests preserve source buffers and encoded observations
across that move, distinct request IDs, the original Leave and its deadline, and
reject preparation between the read's completion and the release transition.

Final gates: 1,895 harness passes, 18 inherited ignored; 160 trace passes; strict
all-target harness Clippy, formatting, lock and diff checks. Private Cargo target:
/tmp/weaver-146-boot-composition-target. Ten compiled protective mutations fail their
named tests and all restored tests pass. A compile pin rejects preparation through
a shared borrow with E0507. The restored 22 joined tests pass. All 242 harness Rust
source hashes are checked after restoration. The initial sandbox run's socket
permission failures are retained in the evidence; the gates use real sockets and
forked Rust protocol doubles outside the sandbox.

These are author checks, not an independent/Planner/security grade or production
qualification. The prescribed review and verification skills were unavailable;
direct source/sequence review and executable checks are recorded instead.

## Completion boundary

This slice ends at actual release preparation. Listener servicing, rollback
JoinLeave rejection, accepted-Leave Failed cleanup, pending initial Grants/refusal
accounts, bounded LastWord, terminal resource release/Unload/drain/Left and the
actual admin/listener/worker root remain composition work. No-SPU Unload awaits
#285 review and merge. Prior review carries remain tracked, including pending Enter
send withdrawal and window transition ordering. The initial Grants timeout election
from #292 remains pending. Artifacts, system prompt, measured limits, worker stack
and qualified coupled cutover remain owed. No successful Left or Issue #146 closure
follows from these primitive and joined test counts.

After-action: self-review found that rejection before any read did not isolate the
stage guard. The joined test now also rejects after the read but before release,
and deleting that guard fails its named test. Continue checking the transitions
between operations, not just their individual results, while joining actual owners.
