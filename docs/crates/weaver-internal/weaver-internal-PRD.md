# weaver-internal - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-08-18
**Document ID:** `weaver-internal-PRD`
**Parent:** `weaver-agents-PRD`
**Editorial:** Per the Working Rules.

---

## 1. What this crate is

`weaver-internal` is **the operator's promotion space**: the place where an operator
takes a capability their agent needs at loop speed and mounts it as a callable the
reasoning loop reaches directly, dispatched inward and never through the gate. It is
chartered by the tool boundary ruling of 2026-08-18, which settled three things at
once: the agent's working tools are external to the agent and emergent - scripts the
agent writes and keeps in its own home directory, reached through the shell and
governed by ordinary Unix permission under the agent's uid - the shell itself is the
gate's own outbound verb and lives there, and what remains is a third class that is
neither. A member of this crate is a capability that could stand as an external
script but is wanted inside, because a control loop needs its answer at a latency
and a determinism the shell round trip cannot give.

The framework ships the space and its first member. What else occupies it is the
operator's business, which is the boundary this charter exists to draw: **membership
is an operator decision, made against latency and risk the operator has accepted,
and never a contributor convenience.** The path the ruling anticipates runs from the
emergent roster inward - an agent grows a tool in its home, the operator judges it
worth promoting, rewrites it against this charter, and mounts it plugin-style for
the control loop they own.

```graph
node: weaver-internal
kind: crate

edge: parent
from: weaver-internal
to: WeaverTools
```

## 2. What a member is

A member is a callable: it holds no process of its own, binds no socket of its own,
initiates nothing, and answers exactly what it was asked. Who fires it is the
caller's fact, not the member's. Under the two-axis taxonomy of
`weaver-tools-vision` section 6 a member serves either corner of the initiation
axis - elected, when the model emits the call and the harness answers
deterministically, or autonomic, when a control loop fires it on a condition the
loop set - and both corners dispatch inward. Autonomicity is a property of the
control loop the operator writes, never of the member, which is why this crate is
not named for it.

**Framework-shipped members meet the pure bar**: a function of its arguments alone,
reaching no filesystem, no network, no clock, and no randomness. Purity is what
lets a recorded call reproduce its answer, which is the property the splice
mechanism of the harness-SPU amendment prices, and it is the bar the framework
holds itself to because the framework cannot accept risk on an operator's behalf.
The calculator is the first framework member and the cap: no second framework
member joins without an act that argues its corner the way the apex argues the
calculator's at `weaver-agents-PRD` section 4.

**Operator-promoted members answer to the operator.** Admission, latency, and any
reach beyond the pure bar are the operator's to accept, the same way the agent's
sudoers file is. What the framework owes a promoted member is the calling surface,
the containment of section 4, and the clerked trace events that keep the record
authoritative whatever the member touched. What the framework never does is
adjudicate a promotion.

## 3. The calling surface

**PENDING.** Members declare their surface, and two are named. A pure member is
reachable by direct call, because the elected inward corner carries a latency
requirement - `weaver-tools-vision` section 6 rules that an answer that never
leaves the machine must not be bounded by anything slower than the machine. A
promoted member is reachable over a Unix socket handoff, the program's standing
transport for internal traffic, which is where a member that holds state or risk
belongs so its faults land on a seam rather than inside the caller. Which surface
the calculator takes, and how the control loop names a member at its surface, is
settled when the harness-SPU splice amendment lands, because the splice is the
first caller and a surface elected before its caller exists would be a reserved
slot. Until that act, this cell stands open and nothing builds against it.

## 4. What this crate must not hold

**No shell.** The shell is the gate's verb, per the tool boundary ruling, and a
shell mounted here would put the world back inside the reasoning loop, which is the
one move the ruling exists to refuse.

**No roster and no registry.** The agent's tool inventory is emergent and lives in
its home directory as files the uid owns. This crate holds promoted callables and
does not index, discover, or enumerate the emergent space. The trace of the agent
writing a tool is the record that the tool exists, and a registry here would be a
second account of that fact.

**No safety classification.** The bar of `weaver-traits-PRD` section 3.1 reaches
this crate whole: a member does not carry a judgment of its own danger, because the
enforceable constraint is the uid boundary and a heuristic beside it is hope
wearing a uniform.

**No initiation.** A member that can act unasked is a control loop, and control
loops are the operator's code in the harness's seat, never members here. The
member's whole surface is call in, answer out.

## 5. The first member

The calculator: a scientific expression evaluator, a pure function from an
expression string to a value or a refusal in its own words. The apex places it at
`weaver-agents-PRD` section 4 as a model-elected call whose result the harness
supplies deterministically, and the vision seats it in the elected inward corner.
An operator control loop may also fire it autonomically for the eviction-and-splice
pattern of the harness-SPU amendment, and the three-gate ladder of that mechanism -
signal exists, signal is actionable, beats the deliberate loop head to head -
governs the framework's autonomic wiring of it. The ladder gates the wiring, not
the placement: the member lands with this charter and waits.
