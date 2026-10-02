---
title: The loop
summary: the operator surface - the seat's calls, the one crossing, and everything the framework refuses to decide
version: v0.1
date: 2026-09-15
commit: unreleased
parent: WeaverTools Technical Documentation
---

# The loop

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.** This page is drafted where the crate papers are not,
because the surface it describes exists in code and has no paper anywhere else on
this site.

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What this page is about

Nine crate papers on this site describe what the machine does. **Not one of them
describes where an operator tells it what to do**, and that place is a single
file.

The nearest thing to it in ordinary experience is a springs-and-terminals
electronics kit. The parts are fixed and someone else made them, the springs are
where wires land, a handful of worked projects come in the book, and past those
the board is yours. **This program supplies parts, springs, and a few worked
examples. The loop is the wiring, and the wiring is the operator's.**

## The loop is one surface of several

This page is about the surface you write against every turn, and it is not the only
place an operator's judgment enters. The declaration sets the model binding and the
elections at load. The composition root decides which knobs are frozen and which are
yours. And where the shape you need does not exist yet, the floor and its contracts
are where it gets made - which is [Extending the program](extending.md), and which
costs more than this page does.

**What is particular about the loop is the loop's turnaround.** The other surfaces
take effect at the next load or the next build. This one takes effect on the next
turn.

## Loop 0 is the framework and loop 1 is yours

**One caution before the numbering.** `loop 0` is doing more than one job in this
corpus and the readings have not been reconciled. Here and in the harness's charter
it is the running agent service. Read narrowly elsewhere it is the load-and-unload
envelope, which is properly the service's lifecycle interior rather than the service
itself. And the formalization numbers the primary reasoning loop `L_0`, which under
this page's numbering is loop 1. **That collision is an apex question with no ruling
on it**, and this page states it rather than choosing.

**Loop 0 is the running agent service.** It is not a document set, not a milestone,
and not a loop anyone supplies. It is the object itself: the thing that boots under
its unit, comes up as the provisioned agent identity, binds the coordination socket
inside its own sandbox, creates the unnamed pairs its organs are reached over, and
sits there being one sealed agent.

**Loop 1 is the builder's**, and loops above it are further builder loops. The one
builder-facing surface reaches loop 1 and above and excludes loop 0, so loop 0 is the
service that runs your loop and is never itself supplied through that surface.

## The seat drives a serving binding

An agent stands up under a binding, and the binding declares its kind at the load -
**serving or diagnostic**, and nothing moves between them afterward. Everything on
this page is the serving kind's: a serving binding stands up the whole interior,
work enters from outside, and the seat is where your loop shapes what happens to
it.

**A diagnostic binding is the other kind, and it is not written against the seat.**
It stands up the interior without the work ingress, replays a finished record, and
its driver sits outside the agent, reading a trace the operator holds. Nobody puts
a running agent into a diagnostic mode - an agent to be served and an agent to be
replayed are two loads of the same weights, so the wrong arrangement is not
guarded against but unrepresentable. The diagnostic loop gets its own page when
its consumer exists, and this page will point at it.

## The seat is nine calls

The seat is what a loop is written against. It is a set of ports the harness
grants, not a library the loop links, and it is small enough to list.

| call | answers |
|---|---|
| `assembled_empty()` | the first-turn test |
| `session_shape()` | the session's runs, each with its held event counts by kind |
| `fullness()` | resident and capacity, as plain counts |
| `flush(keep)` | the resident counts either side, the decode context returned to a prefix |
| `elide(from, to)` | the resident counts either side, a half-open interior span removed |
| `classify(text)` | every label of the classify artifact's head, scored |
| `recall(n)` | the message events of the newest n turns, envelopes whole |
| `score(predicate, passed, measured, denominator)` | whether the task's verdict on the run was recorded |
| `turn(delta)` | runs one turn |

**The loop files' headers now agree with the connector.** They said the seat offered
exactly seven calls while the connector exposed eight, `elide` having arrived with the
elision port of 2026-08-22 without the headers being recounted, and this page reported
that rather than settling it, a page being no place to correct a code comment. The act
that added `score` on 2026-09-26 (#523) touched the headers and recounted them, so both
now list the nine calls the table above draws from the connector.

## The one crossing

```python
def drive(seat, text):
    ...
    return seat.turn(delta)
```

`drive(seat, text)` is the whole of the seam. Run at least one turn, because a
crossing that runs none falls back to a plain unshaped turn.

Everything a loop does happens between being handed the text and calling
`seat.turn`. It may read the shape, read fullness, elect a flush or an elision,
recall what custody holds, classify content, and compose whatever system, user, and
assistant lines it wants into the delta. **The framework has no opinion about any
of it.**

## Every judgment is the loop's, and the organs say so

This is not a permission the loop was granted. It is a refusal every organ makes,
recorded in each of their charters:

- **The elision.** Which span to elide is the loop's election and the harness holds
  no policy about it. The seat forwards the span unjudged, because a port that
  judged one would be the switchboard deciding what a context is worth.
- **The flush.** When a flush is worth its cost is the loop's business. The
  mechanism is the harness's and the threshold is nowhere in the framework.
- **The context.** What a kind's count means to a turn is the asking loop's
  business. Custody answers with organized envelope fact and no judgment.
- **The prompt.** The harness is content-neutral. It dispatches on the payload kind
  of a message and never on anything inside it.

The loop file states the same thing from the inside: **the trigger, the recall
depth, the quote budget, the memory conventions, and what every injected line says
are the loop's alone, and the framework holds no threshold and no convention
anywhere.**

## What the loop cannot do

The springs are where wires land. They are not everywhere.

- **No loop mints a port.** The granted surface is the surface, and a capability
  change enters as a charter and contract edit rather than as an import.
- **No loop authors the trace.** The harness is the sole writer. A loop's actions
  reach the record because the harness authored them, never because the loop wrote.
- **No tool result is fabricated.** A result has exactly one construction site.
- **No state crosses a residency.** What the loop accumulates dies with the session,
  which is what proto-stateful means.
- **No handle to the hot cache.** The loop elects the flush and never touches what
  the flush clears.
- **No loop changes its binding's kind.** The kind was declared at the load this
  loop is running under, and there is no verb to move it - a replay is a different
  load, not a state this loop can reach.

## A miswired loop costs the project and not the board

**Every failure in a loop file is printed and survived, at the stage that reads
one.** A loop that throws, a file that will not parse, a file that cannot be read at
all - each falls back to running a plain unshaped turn, so a broken loop costs the
injection and never the agent's answer. **That is the file stage's property and not
the compiled stage's.** A worker carrying its loop in the binary reads no file and
takes no `--loop-file`, refusing it as an unknown parameter, so a loop named at that
stage fails the load rather than falling back.

The fallback is the property the kit analogy turns on. You can wire it wrong,
watch it do nothing interesting, and try again on the next turn, without the board
breaking and without the session dying.

## Two forms, and which one ships

This distinction is easy to trip over and the corpus states the halves in different
places.

**The compiled form is the deployment form.** The loop is compiled into the worker
binary rather than read from a file at runtime, and what a builder inherits is an
array they did not choose at runtime. That is what holds variance to a range, so
what remains in a measurement is attributable to the thing under study rather than
to the rig.

**The Python form is the iteration surface.** A worker built with the `pyworker`
feature reads its loop file at every crossing, so an edit takes effect on the next
turn. It carries the same behaviour as the compiled loop and exists to iterate at
conversation speed.

Both are real and they are not in competition. **Iterate in Python, ship the
compiled array.**

## What this page does not yet carry

- **The extension seam** is the one builder-facing surface and is named here rather
  than described. What a builder writes against it, at the Python stage and at the
  compiled one, is owed.
- **The worked examples.** The kit's project book is the part this page most
  obviously lacks. The repository records the loops the agents run, and a reader
  would be better served by two of them read line by line than by any amount of
  prose about the seat.
- **The argument for where the loop's boundary falls** sits in the reasoning-loop
  formalism, which is on an open pull request rather than in the tree, so that
  citation resolves to nothing a reader holding this commit can open.
- **The diagnostic loop's page.** The binding kind is chartered and the serving
  side of the split is this page. The replay side - what drives it, what
  certifies it, what an instrument is - is owed its own page when that work
  lands, and is a sketch in the project space until then.
