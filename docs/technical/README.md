---
title: WeaverTools Technical Documentation - Introduction
summary: what this collection is, why Level A is eliminated rather than measured, what governs these pages, and three ways into them
version: v0.1
date: 2026-08-22
commit: unreleased
parent: WeaverTools Technical Documentation
---

# Introduction

**Status:** technical documentation. Describes, decides nothing.

The code described here is unreleased and is scheduled for release in the first
quarter of 2027. These pages are a dated snapshot of a system under construction,
and each says on its face what it does not yet have.

If the pages in this directory are chapters, this is the introduction. It says what
the book is about, what authority it has, and where to start.

## What WeaverTools is, briefly

**A local-first agent framework whose primary artifact is the trace.**

An agent is four processes on one machine, plus a fifth program that stands outside
every agent and does not run while one is serving. Every seam that crosses a process
line is a Unix domain socket, and there is no listening network socket in the
program at any depth.

**The count follows the socket rule rather than a roster.** A seam is tagged socket
where a process line is crossed and link where none is, so the sockets to
`weaver-gate`, `weaver-spu`, and `weaver-state` each name a process, and the worker
that holds the harness is the fourth. `weaver-admin` is the one outside. An earlier
count of three predates `weaver-state`, chartered 2026-08-18, and is the same
omission the apex's seven-crate roster carries.

The harness is a content-neutral switchboard: it holds
the sockets, routes between organs, and has no opinion about what it routes.

The agent is **proto-stateful**, holding real state within a session and none
across sessions. What it does is recorded as one ordered stream, one event per
line, in the same canonical form the agent itself reasons over - and **the agent
cannot reach its own record**, because the stream lands behind a boundary the
kernel enforces.

That is the shape and not the argument. **The repository's root `README.md` carries
the longer statement** of why the network is refused, why the trace is the artifact
rather than a log, and where the name comes from. This page does not restate it.

## The levels, and which one this program is trying to make boring

Warren Weaver, introducing Shannon in 1949, drew three levels at which
communication can fail. **Level A, the technical problem:** were the symbols
transmitted accurately. **Level B, the semantic problem:** did the transmitted
symbols carry the meaning intended. **Level C, the effectiveness problem:** did the
received meaning change conduct the way the sender needed.

This program reads them as a **dependency hierarchy**, each level's ceiling set by
the one beneath. You cannot convey more meaning than the channel permits, and you
cannot produce more effective action than the conveyed meaning supports. The
consequence is what makes the reading worth having: **a single fault travels the
whole stack while changing appearance at each layer**, so a failure that began at
Level A arrives wearing the face of a Level C effectiveness problem, and no amount
of study at Level C will find it there.

**The ambition at Level A is elimination rather than measurement.** That is the
distinction to carry into everything below. This program is not trying to
instrument transmission carefully. It is trying to drive Level A's influence down
until it can be taken for granted, so that what is left to study is Level B with
Level A's noise out of it.

Every structural decision serves that. One machine. Kernel-enforced identity. No
network path and no listening socket at any depth. Boundaries preserved on every
seam. **None of those exists to measure a source of transmission variance. Each
exists to remove one**, so that transmission stops being a variable rather than
becoming a well-measured one.

That is why the trace is worth what it costs. A complete Level B record is only
diagnostic to the extent Level A is quiet underneath it. If transmission still
varied, a semantic anomaly in the record could not be told from a transmission
artifact, and the record would document the apparatus rather than the agent.

**The claim is partly checkable, and what the check does not reach is worth stating
first.** The measurement regime's A/A test runs two arms and bounds the difference
between them. **A difference is not the quantity the ambition names.** An A/A sees
only what varies between arms, so a constant contribution the rig makes to both is
invisible to it however large, and the test would report the same near-zero
difference whether the machine adds nothing or adds the same thing twice.

**The arms also vary on two things at once**, the device they held and the seeds
they drew, so even the differential the test does bound cannot be attributed to
either one. What survives is a bound on between-arm variation from those two
sources taken together.

So **Level A is argued structurally and bounded differentially, and it is not
measured to be small in absolute terms.** Nothing in the tree measures that today.
The honest form of the ambition is that the mechanisms above remove the sources of
transmission variance a reader can name, and that the A/A rules out one class of
residual rather than the class the claim would need.

## What these documents are

**One paper per crate, plus six that belong to no crate**, and the directories
carry the boundary: `weaver-agents/` for the agent domain, with
`weaver-internal/` beneath it for the callables that crate mounts,
`weaver-web/` for the frontend domain that reaches an agent only across a contract,
moved with its crate to `WeaverTools_Project/weaver-web/docs/technical/` on 2026-09-26,
[`weaver-diagnostic/`](weaver-diagnostic/jacobian-lens.md) for the instruments that read
a finished record, and the seam itself at the top. A crate paper answers what one crate
is, what it owns, which seams it holds, how its primary operation works, what it
refuses, and what it has not built.

**The papers describe and decide nothing.** Every paper is read out of the merged
corpus, which is the authority on every claim it makes. **Where a paper and its
source disagree, the paper is the defect** and the source stands.

**No charter or Spec lives here, and that is the rule rather than the current
state.** PRDs and Specs are build documents and they live under `docs/crates/`. They
are what these pages are read out of, and **they are not part of what is published**:
at release this site is the public face and the corpus behind it is not. A paper may
name a charter as its source. It may not stand beside one.

**The `Sources:` lines are drafting scaffold and do not survive drafting.** An
undrafted section names the charter sections it will be read out of, so whoever
writes it knows where to look. A drafted section carries the substance instead and the
line goes. At release the papers are prose and the corpus they came from is not
shipped, so a published page pointing a reader at a document they cannot open would be
a defect rather than a citation.

**No page restates a contract.** The contracts page is the single site for a
contract read-out here, so a crate paper names its seams and links rather than
repeating what crosses them. That rule is why the same seam is not described in two
voices that can drift apart.

## What these documents are not

- **Not specifications.** A builder implementing against WeaverTools reads the
  contracts, which carry the vocabulary crossing each seam, the errors it can
  return, and the ordering it relies on and provides.
- **Not the ratified document set.** The set is the PRDs, contracts, and Specs
  under `docs/`. These pages read that set back for a reader and are governed by
  it, not members of it.
- **Not a tutorial.** There is no getting-started path here, and the worked
  examples a builder would most want are still missing.
- **Not finished.** See the state below.

## Four ways in

**To understand the architecture**, read in the order the index lists. The floor
first, `weaver-types` and `weaver-traits`, because every domain draws from them and
none contains them. Then the four organs, `weaver-harness` before the others since
it is the hub each of the rest holds its channel with. Then `weaver-trace` and
`weaver-state`, which sit under the harness's domain. Then `weaver-internal`, which
fits neither definition and is described rather than resolved.

**To build something that talks to an agent from outside**, go straight to
[Contracts](contracts.md) and read the two external contracts. They are the
program's entire public surface, written for an outside consumer and for nothing
else. Nothing else on this site is a surface you build against.

**To write a loop**, read [The loop](weaver-agents/loop.md) first. It is where an
operator's judgment enters a turn, and it is the surface with the shortest turnaround:
an edit runs on the next crossing. Then read the contracts for the seams the seat's
calls cross.

**To add something the program does not have yet** - a shape on a seam, a tool the trait
set does not describe, a whole organ - read [Extending the
program](weaver-agents/extending.md). The floor is deliberately thin, a shape
crosses a seam only if both sides name it at compile time, and the page is explicit
about where the framework's requirements stop and your own practice begins.

## The state of this set today

**Sixteen pages beside this one, and all of them are drafted as of
2026-08-24.** The nine crate papers took their prose across that day, the last
of the 54 owed sections landing with `weaver-internal`, and `weaver-web`
joining as the sixteenth, in its own directory beside the agent domain's
rather than a place among the instruments.

**Drafted is not finished, and the difference is worth stating plainly.** Every
page is a rough first pass read out of a merged document. What that buys is that
a reader can now find any part of the system described somewhere on this site.
What it does not buy is polish, worked examples, or a second pass against a
moving tree: several pages already carry notes where a source and this site
disagree, and where they do, the page is the defect.

Each page still says on its face what its subject does not have, which is the
convention this set follows everywhere: a reader looking for the holes should
not have to infer them from careful phrasing.

**These pages are not tied to a commit yet, and the front matter says so.** Every
page carries `commit: unreleased` rather than a hash, because the application is a
moving target and **a hash that looks authoritative and is not is worse than an empty
field** - two months on, no reader could tell which pages were verified against a
tree and which were stamped in passing. The field is held rather than dropped so the
template stays whole, and it is filled for real at release, against the
public release the documentation is tied to.

## A note on this file and the index

They do different jobs and neither is the other's summary. **This page is the
introduction**, and it is what a reader meets first when browsing the directory.
**[The index](index.md) is the table of contents**, carrying the roster grouped by
what each crate is to the others. If you want the list, go there.
