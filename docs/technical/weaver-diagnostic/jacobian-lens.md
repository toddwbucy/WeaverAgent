---
title: The Jacobian lens
summary: an unbuilt instrument, why it is a replay tool rather than a live one, what it would cost, and the two questions that gate it
version: v0.1
date: 2026-08-23
commit: unreleased
parent: WeaverTools Technical Documentation
---

# The Jacobian lens

**Status:** technical documentation. Describes, decides nothing.

**Nothing on this page is built.** Every other page here describes a system that
exists and names what it lacks. This one describes an instrument that does not exist
at all, filed as a feature request carrying no decisions, and it is written down
because its preconditions are the interesting part and they are easy to discover
late. The code described elsewhere on this site is unreleased and scheduled for the
first quarter of 2027. This instrument is not in that scope.

## What it would be

A per-layer matrix, fitted offline against a fixed corpus, that turns a
residual-stream vector into a ranked vocabulary readout. The record shape is the
probability field's with a layer index added.

**Where the field says what the model was about to emit, the lens says what it was
working with while emitting it.** That is the whole of why it is wanted. The field
reads the output distribution at one position. The lens reads the interior at the
same position, one layer at a time.

The result that makes it worth the cost is **selectivity**, reported in the
transformer-circuits workspace's published work rather than by any measurement of
this program's. **This page carries no citation for it**, which is a gap rather
than a style: a reader who wants to check the claim has nothing here to check it
against, and the reference is owed. Information a model uses correctly can be **entirely
absent** from the lens readout, and where it is absent the model cannot report on
that information or reason flexibly with it. That converts a class of failure from a
behavioural description into something with a mechanism attached.

## Why it is a replay tool rather than a live one

This is the design choice the request names, and it is the reason the instrument
belongs on this site at all rather than in a crate paper.

**Capture the residual stream at the elected layers during the run. Defer the lens
multiply and the softmax to analysis.** Two things follow and both are the reason.

**The in-path cost becomes a copy off the device rather than arithmetic.** That is
the intervention least likely to disturb the run being reproduced. It matters more
here than for any instrument before it, because the behaviour-neutral obligation has
to pass for this election too, and **a diagnostic that perturbs the run it observes
is worthless in a way no consumer can detect.**

**The trace holds the activation rather than a readout.** A lens refitted later
still applies to a run recorded months earlier. A stored readout would freeze one
fitting into every record made under it, **which is the same provenance failure the
no-profile-names rule refuses one level down**: a named set drifts, and every earlier
record carrying the name silently becomes a record of something else.

So the instrument splits across the boundary the program already draws. **The run
captures. The analysis reads.** Nothing about the lens has to be decided while a turn
is in flight, and a lens that did not exist when the run happened can still be
pointed at it.

**Custody does not bend for it.** The capture is authored by the harness like every
other event and recorded by `weaver-trace`, which is the sole writer. **A diagnostic
consumer reads the operator-held stream and writes nothing to it**, the same terms
every consumer meets, so an instrument gains no route to the record its subject could
not already be trusted with.

## What it would cost

Against the deployed artifact at `n_embd` 2048 and `n_layer` 40, and the 2026-08-20
series at 3,011,135 generated tokens, at bf16 and 4 KiB per layer per position:

| captured | per 200-round series |
|---|---|
| 1 layer | 12.3 GB |
| 4 layers | 49.3 GB |
| 40 layers | 493 GB |

For comparison, the probability field at full depth 50 is 3.1 GB and **the whole
record today is 0.137 GB.** Both figures are the feature request's, taken against the
2026-08-20 series. **Whether the 0.137 GB carries any conditionally emitted
`model.field` is not stated there**, so the two should be read as an order-of-magnitude
comparison rather than a like-for-like one until the election state behind them is.

**So conditional emission is a precondition here rather than a relief.** For the
field, conditional pricing was an optimisation. Here it is the difference between
possible and not. Measured against the same traces, the field's conditional shape
gives an eleven-fold reduction at a one-bit threshold, with 5.3 percent of positions
carrying full depth. **The four-layer estimate follows the second figure rather than
the first**: roughly 49 GB against 5.3 percent of positions is roughly 3 GB, where
the eleven-fold overall reduction would give closer to 4.5. Which of the two a
capture actually pays depends on whether partial-depth positions carry layers at
all, and that is not settled here.

## What has to be true first, and what changed on 2026-08-22

The request was filed 2026-08-21 naming one blocking prerequisite: **the GGUF
residual tap did not exist**, and both live agents run GGUF, so the instrument could
not run on the deployment.

**That prerequisite has partly discharged and the blocker has moved rather than
cleared.** The GGUF tap landed 2026-08-22 and carries `spu-two-taps-one-shape`, so
both engines now tap and the container is no longer a ground for refusing. What
stands in the way now is narrower and is stated on the readout's own surface:

- **The deployed family declares no tap**, so nothing in service elects the path.
- **The device-pair neutrality measurement is owed.** The tap is watched clearing the
  no-token-change bar on the host backend, which reaches everything except the hazard
  the Spec names: installing the callback turns one graph compute into a walk of
  windows, and a fusion candidate straddling a window boundary goes unapplied. That
  is a device concern no host run can reach, and **the tap is not shipped against a
  family until the measurement is taken.**

The two sequencing dependencies the request named have both closed. The probability
field is chartered and `model.field` is in the record. The sampler question behind it
was answered by the per-generation seed derivation of 2026-08-21, which is what makes
a single turn re-enterable, and **a lens readout of a run that cannot be re-entered is
an observation with nothing to check it against.**

## The two questions that gate any design work

Both are empirical and **neither is answered by building anything.** They are stated
here in the order they would be run.

**Does a corpus-averaged Jacobian resolve anything on this architecture?** The
deployed artifact routes 8 of 256 experts per token and is hybrid, which the family
module carries as a fact about its flush behaviour. A Jacobian is a linearisation of
the effective function. Where that function changes discretely per token because a
different expert set fires, **a single averaged linearisation may describe no
computation that actually happened.** The paper's result was not obtained on this
architecture.

**Does a lens fitted against bf16 survive quantisation?** The deployment serves Q8_0.
Activations from a quantised model are not the activations the lens was fitted
against, and **whether the readout transfers is measurable rather than arguable.**

## Where it would sit

Under the election pattern the probability field established: elected per feature and
per load, frozen for the residency, named individually in the load event, and shown
behaviour-neutral before it ships. The residual capture is already anticipated as
arriving that way rather than as a widening of the field's election.

On the replay side it is a consumer of finished traces rather than a peer of the
loop, which is the same door every later capability comes through.

## What is not built

Everything. Named individually so the list is a work list rather than a mood:

- **The capture.** No election exists for residual capture at a layer set, and no
  path writes activations to the record.
- **The record shape.** The field's shape with a layer index added is a sketch rather
  than a charter clause.
- **Conditional emission for it.** Priced above, chartered nowhere.
- **The lens itself.** No matrix is fitted, and no corpus is chosen to fit it against.
- **The analysis side.** Nothing reads an activation out of a trace and multiplies it.
- **Both gating questions**, which precede all of the above.
