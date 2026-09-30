---
title: The calculator
summary: weaver-internal's first and only member, the inward-dispatched reference tool, and the mechanic it charters but has not yet earned
version: v0.1
date: 2026-08-23
commit: unreleased
parent: WeaverTools Technical Documentation
---

# The calculator

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## Where it sits, and why that is worth a sentence

**The calculator is inside the agent boundary**, and it files here for that
reason: it is [weaver-internal](../weaver-internal.md)'s first member and so far
its only one. It is dispatched inward, reached by the loop calling into the
worker's own binary, with no seam and no contract between - and **it never crosses
the gate**.

That is worth stating because the thing it proves points outward. **The
perturbation mechanic a diagnostic instrument would need already exists, and it
exists because the calculator needed it first.** A reader arriving at
[the Jacobian lens](../../weaver-diagnostic/jacobian-lens.md) and wondering what
it would take to perturb a replay should know the mechanic is chartered and
specified here, is not theirs to invent, and is not finished either. **The
calculator is built. The cut-and-recompute wiring it needs is not**, which the
last section states rather than leaves to be discovered.

## What it is

**A scientific expression evaluator: a pure function from an expression string to a
value or to a refusal in its own words.** That is the whole of it.

The apex names it in the definition of done as the reference case for a
protoautonomic tool call, **where the harness injects a deterministic result into the
stream in place of a stochastic one.** That is the point of it. The model is bad at
arithmetic and reliably confident about it, so the interesting property is not that
the answer is computed but that the answer the model would have produced is
**replaced**.

## The pure bar, and why it is the framework's own

**A framework-shipped member is a function of its arguments alone** - no filesystem,
no network, no clock, no randomness.

**Purity is what lets a recorded call reproduce its answer**, which is exactly the
property replay needs. A member that read a clock would make a recorded call
irreproducible, and the record would carry a result nothing could check.

**The framework holds itself to that bar because it cannot accept risk on an
operator's behalf.** An operator-promoted member answers to the operator instead:
admission, latency, and any reach past the pure bar are theirs to accept, the same way
their agent's sudoers file is. What the framework owes such a member is the calling
surface, the containment, and the clerked trace events that keep the record
authoritative whatever the member touched. **What the framework never does is
adjudicate a promotion.**

## It is the first member and it is the cap

**No second framework member joins without an act that argues its corner** the way the
apex argues the calculator's, per `weaver-internal-PRD` section 2, which is the
authority for the ceiling rather than this page. That is a deliberate ceiling rather
than a roadmap
waiting to be filled, and it is what keeps the inward corner from becoming the place
tools accumulate once the gate has been made inconvenient.

## The mechanic it charters, which is the reason for this note

The calculator's autonomic firing uses **cut-and-recompute**: the loop cuts the
context at a position and recomputes forward from there, so a deterministic result can
stand where a stochastic one was. The harness knows how to do this because the
calculator needed it.

**That is the same mechanic an intervention over a replay would use.** The difference
is motive rather than machinery. Under the calculator a detector chooses the position
and something is being corrected. Under an operator intervention the position and the
substitution come from the operator and nothing is being corrected. **Same mechanic,
motive removed**, and who chose the cut belongs to the consumer rather than to the
harness.

So the ordering is worth stating plainly: **the mechanic is meant to earn its
correctness on the production side before any diagnostic consumer inherits it.** A
perturbation over a replay is not a new capability the framework has to grow. It is a
chartered one pointed somewhere else. **That ordering is a plan and not yet a record**:
the splice amendment has not landed, so nothing has earned anything yet.

## What is not settled

- **The splice amendment is chartered and not closed.** `weaver-internal`'s own cell
  waits on the harness-SPU splice amendment landing, so the mechanic is specified and
  the wiring is not finished.
- **The autonomic wiring is gated, not the placement.** A three-gate ladder governs
  whether the framework fires the calculator autonomically at all: the signal exists,
  the signal is actionable, and it beats the deliberate loop head to head. The member
  lands and waits.
- **Where the licence boundary falls is the operator's question and is open.** The
  intervention mechanic is shared between the calculator and any consumer that
  perturbs a replay, which makes it the place a give-away boundary would actually run.
  Nothing here decides that.
