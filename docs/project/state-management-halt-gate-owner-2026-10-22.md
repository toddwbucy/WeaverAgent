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
Wrong-exchange, wrong-position, malformed or forged gate-death replies retain
available raw evidence and never manufacture a valid terminal.

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
