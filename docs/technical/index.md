---
title: WeaverTools Technical Documentation
summary: per-crate technical papers for a local-first agent framework whose primary artifact is the trace
version: v0.1
date: 2026-08-22
commit: unreleased
parent: WeaverTools
---

# WeaverTools Technical Documentation

**Status:** technical documentation. Describes, decides nothing.

The code described here is unreleased and is scheduled for release in the first
quarter of 2027. These pages are published as a dated snapshot of a system under
construction, and each says on its face what it does not yet have.

**The directories carry the boundary, and one level down they carry
membership.** `weaver-agent/` holds the agent domain: nine crate papers, the two
surfaces an operator writes against, and `weaver-internal/` beneath it for the
callables that crate mounts. The frontend domain, `weaver-web`, left this repository on
2026-09-26 for `WeaverTools_Project/weaver-web/` beside it, reaching an agent only
across a contract as before. `weaver-diagnostic/` holds the instruments that read a
finished record, and is the newest and least built of the two. The contracts page stays
at the top because it is the seam itself, and the two external contracts on it are where
that boundary is actually drawn.

A paper reads out a merged document and never decides anything: where a paper and
its source disagree, the paper is the defect.

This page is the roster. [The introduction](README.md) says what the set is, what
governs it, and where to start depending on what you came for.

## The floor

Two crates every domain draws from and no domain contains.

- [weaver-types](weaver-agent/weaver-types.md) - the declaration, peer identity, and
  the wire vocabulary loop 0 speaks
- [weaver-traits](weaver-agent/weaver-traits.md) - messages, roles, permission modes,
  and the tool surface

## The organs

A crate that governs a domain and holds a two-initiator channel with the harness.

- [weaver-harness](weaver-agent/weaver-harness.md) - the switchboard, the loops, and
  sole authorship of the trace
- [weaver-spu](weaver-agent/weaver-spu.md) - model residency, two decode engines, and
  the measurement that rides a generation
- [weaver-gate](weaver-agent/weaver-gate.md) - the agent's boundary, and the shell as
  its own outbound verb
- [weaver-admin](weaver-agent/weaver-admin.md) - lifecycle authorization, boundary
  verification, and custody of the sink
- [Reproducibility, confirmed in the lab](weaver-agent/reproducibility.md) - a
  recorded turn reissued from the trace alone reproduced bit-exact across full
  reloads, with the method and the scope stated

## Under the harness's domain

- [weaver-trace](weaver-agent/weaver-trace.md) - the recorder, and the working
  structure the loop reasons over
- [weaver-state](weaver-agent/weaver-state.md) - the session custodian, sqlite behind
  the member's own channel

## Neither

- [weaver-internal](weaver-agent/weaver-internal.md) - callables the loop dispatches
  inward, and never through the gate
  - [The calculator](weaver-agent/weaver-internal/calculator.md) - its first and
    so far only member, and the reason the perturbation mechanic a diagnostic
    instrument would need already exists

## The surfaces written against

Where judgment enters the program rather than travels through it. **The loop is
written per turn and takes effect on the next crossing. Extending the floor is a
build-time change** and is grouped here because both are places a person decides
something rather than places the framework does.

- [The loop](weaver-agent/loop.md) - loop 0 and loop 1, the seat's nine calls, the one
  crossing, and everything the framework refuses to decide
- [Extending the program](weaver-agent/extending.md) - the floor, the seams, a new
  organ, and where the framework's requirements stop

## The frontend

Outside the agent boundary, reaching an agent only across the external contracts.
The first thing to do so, and therefore the first real test of whether those
contracts are enough to build against.

- weaver-web - the channel, the lifecycle view, and the live trace view, and the
  one reach the contracts turned out not to cover. Its page moved with the crate on
  2026-09-26 to `docs/technical/weaver-web/weaver-web.md` in
  `WeaverTools_Project/weaver-web/`, the operator's ruling of that date.

## The diagnostic domain

The instruments that read a finished record rather than driving an agent. The
domain is chartered and its crate papers are not written yet, so this directory
holds one instrument and will gain the rest as that work lands.

- [The Jacobian lens](weaver-diagnostic/jacobian-lens.md) - a per-layer readout of
  the interior, why it captures during the run and reads at analysis, what it would
  cost, and the two questions that gate it

## Across all of them

- [Contracts](contracts.md) - every seam, its parties, and its governing document,
  including the two external contracts an outside consumer builds against
