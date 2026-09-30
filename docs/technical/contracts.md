---
title: Contracts
summary: every seam this program governs, with its parties, its tag, and where the merged contract lives
version: v0.1
date: 2026-08-22
commit: unreleased
parent: WeaverTools Technical Documentation
---

# Contracts

**Status:** technical documentation. Describes, decides nothing.

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

One page rather than one page per contract, settled on length and overturnable.
**This is the one place a contract is read out here.** Every crate paper links to it
and none of them restates a contract, so two pages cannot drift into two accounts of
one seam.

Each entry names the parties, what the seam carries, and the merged document that
governs it. **The merged document is authoritative.** Where an entry here and its
source disagree, this page is the defect.

**Every contract named on this page is MERGED in the corpus.** Where an entry says
its read-out is not drafted, what is owed is this page's prose. The contract
itself is written and in force.

## The internal seams

Seven, declared in the corpus at the declaring side. Six sockets and one link.

### weaver-admin-harness-contract

`weaver-admin` to `weaver-harness`, **socket**. Lifecycle authority inward:
enter, leave, and stop. Admin dials, one connection per verb, and the harness
reads the peer credential at every accept before any byte. Read-out not drafted.

### weaver-harness-gate-contract

`weaver-gate` to `weaver-harness`, **socket**. The turn inward as a frame, the
deliverable outward, and the execution exchange that carries a tool call.
Read-out not drafted.

### weaver-harness-spu-contract

`weaver-spu` to `weaver-harness`, **socket**. The residency seam: admit a model
binding, release it. The organ channel, two initiators. Read-out not drafted.

### weaver-harness-spu-decode-contract

`weaver-spu` to `weaver-harness`, **socket**. The token seam: a generation and
the measurement that rides with it. Read-out not drafted.

### weaver-harness-spu-classify-contract

`weaver-spu` to `weaver-harness`, **socket**. The label seam: content in, every
label the artifact's head defines with its score out. Read-out not drafted.

### weaver-harness-state-contract

`weaver-harness` to `weaver-state`, **socket**. A member seam rather than an
organ channel: the harness asks and the custodian answers. The election opens the
channel, the distillate feeds it, shape and recall answer. Read-out not drafted.

### weaver-harness-trace-contract

`weaver-harness` to `weaver-trace`, **link**. The one seam in the base set that
crosses no process line, so it authenticates nothing. Read-out not drafted.

## The two external contracts

**These two are the program's public surface**, written by the ruling of
2026-08-01 for an outside consumer and for nothing else. A frontend or any other
consumer builds against these and against no other document on this site.
weaver-web, in its own repository beside this one since 2026-09-26, is the first to
build against them, and being first is what turned the sufficiency claim into a
checkable one. **It found one reach these two do not cover.** Turns and the record are
here. The lifecycle verbs are not: the operator contract governs the record and says
outright that running the admin binary is running the crate rather than a channel it
governs, so an outside consumer has no page for that surface and builds it on deployment
fact. The ask for one is filed.

### weaver-gate-world-contract

The world to the agent. One NDJSON line in, one line out, the peer authenticated
at accept against a predicate that admits front-end principals and excludes the
agent's own uid. Order preserved per connection, one turn at a time in arrival
order, and a close that answers a turn names that turn and its run. Read-out not
drafted.

### weaver-admin-operator-contract

The agent's account to the operator. What crosses inward is nothing. What crosses
outward is the NDJSON stream, one event per line, to a sink the operator declares
and owns. Bounded loss is named twice and never silent. Read-out not drafted.

## The system contract

### weaver-admin-systemd-contract

`weaver-admin` to the init system. Not an external consumer surface and listed
apart from the two above for that reason: what is on the far side is the service
manager rather than anyone building against this program. Read-out not drafted.

## Drawn material

### weaver-organ-channel

Not a contract, which its absent suffix marks. The organ channel's mechanics
stated once, drawn by the contracts that carry it: two initiators, the exchange
and its position, boundaries and ordering guaranteed, and closure that is never an
answer. Read-out not drafted.
