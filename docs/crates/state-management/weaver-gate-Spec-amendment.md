# weaver-gate Spec: input refusal and fault interrupt amendment

**Status:** MERGED.

**Landing PR:** #182

**Version:** 0.7 **Date filed:** 2026-10-20 **Parent:** weaver-gate-PRD

## 0. What this document is

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

Amend the tool interrupt exchange and its enforcement in gate Spec sections 3, 4, 5 and
6, with harness-gate contract 2/3/5/7 and gate-world 2/3/5. The gate still transports
input without classifying it. The harness owns conformance and the turnless refused
response. The live role-system seeding route is retired by this unit. [GATE-1,
PERSIST-5, FLOW-5]

## 1. Interrupt and relay, destinations sections 3, 4 and 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**GATE-FAULT-INTERRUPT.** Match ToolInterrupt {by} in the gate dispatcher and pass only
Unload or FlowFailure to tools.rs's containment path. Reject other causes as a protocol
error. Every former unit-variant sender now supplies Unload explicitly. A flow halt
supplies FlowFailure. The gate emits Killed {by: FlowFailure} only when containment has
completed, using its existing clock and deadline. No new harness execution timeout is
added. Reason: a component halt must not look like rerunnable operator unload. [FLOW-5,
PAIR-2]

| State | Event | Action |
| --- | --- | --- |
| Tool running | Interrupt unload or flow_failure | Contain with that cause, then answer |
| Containing | Repeated same interrupt | Continue existing containment, no new deadline |
| Containing | FlowFailure after unload interrupt, no result sent | FlowFailure becomes final cause, deadline unchanged |
| Result already sent | Later interrupt | First outcome stands, no second result or retroactive relabel |
| Idle | Interrupt | Existing no-live-call refusal, never fabricate a tool result |
| Any | Invalid interrupt cause | Protocol failure under existing seam rules |

KillCause::Fault remains the harness's report of gate death with an unanswered call. The
gate never sends it as proof of successful containment. If containment fails, the gate
exits under its existing contract and the harness follows that gate-death path. No flow
interrupt changes an already reported unload outcome. [FLOW-5, PAIR-2]

**GATE-RELAY.** Relay the harness's refused response intact through the existing Frame
path. It has no turn when no turn opened. STE100 and invalid-input-shape judgments
remain the harness's. Keep kernel-derived peer identity and the operator UID distinction
even though #76's seeding privilege is removed. [GATE-1, TAP-1]

## 2. Enforcement, destination section 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| GATE-FAULT-INTERRUPT | Real tool containment test with each cause and repeated interrupt | Return before containment, reset timeout, or report unload after fault |
| GATE-RELAY | Existing frame relay fixture with refused response and no turn | Invent a turn, alter response, or replace peer UID from payload |

## 3. Open elections and dependencies

No new measured value or gate classifier is elected. Production input classification
still depends on the separate STE100 duty.

The flow_failure interrupt also amends weaver-admin-Spec section 3.0, states S4 and S10.
The gate's containment still completes before a terminal answer, and admin treats the
ending as failed, never successful unload publication. The cause is the same typed value
at each endpoint. [FLOW-4, FLOW-5, PAIR-1]

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover
