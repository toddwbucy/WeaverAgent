# Failed gate execution owner

Date: 2026-10-22.
Branch: code/state-halt-gate-owner.
PR: #251.
Parent PR: #250.
Epic: #146.

## Scope and authority

This slice advances issue #146 E after the first-fault reporting owner. The
original parent was the first-fault reporting owner. Validation joins the fresh
task-material stack through #250 and the #245/#247 review follow-ups.
It retains #190's operator hold for the foundation
cutover. No legacy serving path or deployed agent is activated or replaced.

Implements: weaver-harness-Spec-amendment HARNESS-HALT and HARNESS-HALT-BOUND,
scoped to a frozen external execution's interrupt and terminal custody. It also
consumes weaver-gate-Spec-amendment GATE-FAULT-INTERRUPT and
contract-amendments section 4's existing FlowFailure exchange semantics.
HARNESS-PUMP supplies bounded units and the common clock. These are author scope
claims, not a Planner grade or complete failed-teardown qualification.

The private owner requires the same failed RecordingContext, an attempted
first-fault report, the frozen pair's exact execution and its retained OrganSend,
and the same OrganPort identity. It accepts no caller-supplied CallId, exchange,
contained flag or copied halt account. An account containing a capture refuses
this entry because this slice cannot complete its prior publication obligation.
A report aborted on clock failure or expiry cannot authorize another attempt.

The whole-send ticket is authoritative even when the normal reducer did not run
before failure. Its narrow frozen reconciliation records that fact without
retiring a serving deadline. A wholly unsent packet is withdrawn by exact
exchange and retained. No new NotDispatched cause is invented for flow failure.
The frozen account remains unchanged, and the withdrawn intent remains separate
cleanup evidence without a terminal filing or execution grant.

For a sent call, each advance makes at most one receive attempt and one send
attempt. ToolInterrupt uses Continue on that call's exchange with FlowFailure.
The original envelope and sealed send proof survive pressure and interruption.
A proven send is not repeated. Another owner's outgoing packet remains owned by
that caller rather than being sent or replaced by this owner.

A matching result already held, or received before the interrupt, keeps its
original outcome. If it wins while the interrupt is still wholly unsent, that
interrupt is withdrawn. PairLedger retains the exact raw answer beside its
terminal account. A later socket failure cannot erase an already held result.
Wrong-exchange tool replies retain their existing rejection account. Unrelated
world/control traffic stays with its port owner. Wrong-position, malformed or
forged gate-death replies retain available raw evidence without a valid terminal.

Every advance checks the original halt deadline and common monotonic clock
before and after bounded work. The earlier Leave cap is unchanged. Received
answers, withdrawals and successful send tickets remain retained before a later
clock failure. Missing, reversed or expired time produces sticky AbortRequired.
Transport or protocol failure produces sticky Unavailable with secondary
failure evidence. Neither result changes the first failure or permits healthy
work. The owner never calls ordinary flow reduction, files a pair, produces a
ToolResult or invokes the recorder's capacity wait.

## Tests and current validation

Fourteen new tests use the existing real generation, trace, classifier, member
and gate-socket fixtures. The failure trigger is observed state EOF, including
its priority before the healthy send reducer. They cover:

- Failed/report admission and exact gate identity, including another socket
  with the same exchange number.
- One FlowFailure interrupt, exact terminal/raw retention, unchanged first
  account and no ordinary filing or healthy continuation.
- A whole send before the normal reducer, and a separate wholly unsent
  withdrawal with no tool packet crossing.
- An existing terminal before the halt, a terminal before interrupt, and a
  terminal racing a backpressured interrupt.
- Wrong ordinal, opener and position, a forged Killed/Fault, malformed bytes and
  real child exit. None becomes containment evidence.
- Full trace pressure, no repeated interrupt, and another pending control's
  unchanged outgoing custody.
- The earlier Leave deadline, repeated abort, and consumed terminal/send proof
  retained before a post-work clock failure.

A real child inherits the peer socket and exits in the death fixture. This
proves the owner's response to channel death, not cgroup cleanup. Scripted
Killed/FlowFailure answers test the gate contract's consumer. Host descendant
containment, actual gate behavior and device residency remain separate gates.

Compile pins require HaltGate to have no Clone, Default or conversion from a
bare ExchangeId. They do not claim absence of every possible Rust trait.

Olympus validation used /tmp/weaver-146-halt-gate-target and the pinned toolchain.
All fourteen focused tests passed. The joined harness passed 1,429 tests with
18 inherited ignored cases. Strict all-target harness lint, formatting and the
lock gate passed. All fifteen protective mutations compiled, failed their
intended tests and passed after exact-byte restoration. All three type probes
produced the intended E0283 ambiguity in the ownership pin, and baseline and
restored cargo check passed.

The initial compile found a private module import path error, corrected to the
sibling pairs module. An initial fixture expected another parsed packet after
a post-send clock failure had shut down the socket. The test now verifies the
actual retained interrupt first, then the resulting EOF and sticky abort. No
production failure policy changed for that correction. These initial logs remain
in the evidence, beside the final joined passes.

Independent source reading covered the owner and the corrected EOF fixture.
Evidence, recipes, type diagnostics and source hashes are deposited under
weaver-testing/state-management-146-olympus-2026-10-21/halt-gate. These are author
checks, not a Planner grade or complete production teardown qualification.
The capture-admission defense has no fabricated capture fixture. Its prerequisite
is part of the still-owed publication owner.

## Planner follow-up at 872b545c

Review 5611305834 found two blocking custody defects despite passing gates. Its
independent perturbation pass killed eight of fourteen probes. Admission probes
G2/G3 lacked fixtures, G4 still lacks a real capture prerequisite, and G10/G12/G13
were equivalent under existing callers. The earlier author mutation report above
is historical evidence, not a claim that the Planner reproduced every probe.

The correction gives OrganSend its immutable staged ExchangeId. Healthy send
reconciliation, halt admission and frozen reconciliation each require that exact
exchange. A completed first call's ticket remains true evidence of that call
when the second stage fails. It cannot mark the second call sent, authorize its
interrupt or manufacture a withdrawal. The second call's exact failed input
remains in PairTransaction.refused_send.

Non-ToolAnswer traffic on another exchange stays held for its owner while the
owed interrupt can cross. A late answer for a closed exchange enters the existing
rejection account. That pending rejection cannot block the frozen, already-sent
execution's matching terminal. Ordinary healthy rejection exclusivity remains.
A second foreign tool return stays held rather than overwriting the rejection.
Its later disposition remains owed to the teardown composition. No ordinary
post-latch rejection author or state filing was introduced.

A ledger entry already ended by harness gate-death closure, with no received gate
terminal, is excluded from interruption admission. A received terminal remains
eligible for retained accounting. Failed or busy interrupt staging preserves its
original input in unstaged_interrupt. interrupt and its sealed send ticket exist
only after successful staging. No kernel containment conclusion follows from
these checks.

Six added context tests use real generation, receipt, socket and report owners.
They cover the oversized second stage, a held world frame, a closed-call late
answer followed by the current terminal, a second foreign return, unattempted
and aborted reports, and defensive staging-input retention. The staging helper
test deliberately supplies actual busy/dead ports below the outer precheck. It
pins that defensive transition without claiming an intervening reachable race.
The additional private ledger test assumes the separate verified-death boundary
and checks only that an existing GateFault closure is not reopened. It does not
manufacture a public containment grant. The malformed fixture still checks wrong
position and a forged Killed/Fault. Wrong ordinal/opener now check rejection.

G4 remains unpinned by a truthful capture fixture because this slice does not own
capture publication. G10 remains redundant behind the caller's sent check, and
G13 behind Failed admission. G12's call check remains defensive behind exact
execution matching and the ledger's sequential execution invariant. The original
halt bound and post-work AbortRequired behavior are unchanged. Common-clock
helper extraction and a new secondary-reason taxonomy are outside this fix.
Context-port and ticket-port checks name separate retained owners. Neither
substitutes for the new exchange association.

The correction passed 20 focused halt-gate tests and the gate-death ledger
regression, then the full harness suite: 1458 passed, with 18 inherited ignored
cases. Strict all-target harness clippy, formatting and lock checks passed. All
13 protective mutations compiled and failed their intended assertions. Every exact
restored baseline passed and all 490 tested source hashes matched after restoration.
The recipe is /tmp/weaver-pr251-followup-mutations.json. New logs and source hashes
are retained at /tmp/weaver-146-evidence/review-round2-251/ and have not been
deposited in shared storage. Independent author source review found no remaining
concrete defect in this scoped correction. No production qualification, Planner
re-grade, capture containment or completed teardown is claimed.

## Remaining executable work

Gate death has no successful cleanup branch here. The owner retains socket
failure/raw evidence and returns Unavailable. It does not call close_gate_fault,
claim tools containment or close an execution with fabricated Killed/Fault.
The next owner needs the real process and ToolsCgroup custody currently held by
lifecycle::Run. The same limitation applies at the halt deadline. AbortRequired
requires that cleanup owner to stop waits and close/reap, rather than making
this bounded primitive a complete failed exit.

The trace-only mapped PairEnded attempt under Spec 05 PAIR-END and
TRACE-CAPACITY remains owed when the received terminal requires it. That attempt
will use the reserved tool.call.completed entry. It cannot file state or grant
a new model-facing ToolResult. Capture cancellation and refused save_point,
stopped turn and caller response, Lower, SPU residency Release, failed
state/classifier shutdown, component_failure unload and final trace drain remain
separate work. No status from this owner is a proof that
those phases happened. The Run-to-context resource association and lifecycle
dispatch remain held for the coupled cutover.

The existing measurement refusal path may progress after latch but before its
external halt-account snapshot. Requiring an installed account here does not
resolve that older latch-time snapshot composition. No new wire or policy
choice was needed for this scoped primitive.
