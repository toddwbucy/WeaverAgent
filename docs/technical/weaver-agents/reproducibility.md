---
title: reproducibility, confirmed in the lab
summary: a recorded turn reissued from the trace alone reproduced bit-exact across reloads - method, result, scope
version: v0.3
date: 2026-09-13
commit: unreleased
parent: WeaverTools Technical Documentation
---

# Reproducibility, confirmed in the lab

**Status:** technical documentation. Describes, decides nothing. This page
records one empirical result with its method and its scope, and claims
nothing beyond them.

**The claim under test.** The program holds the trace to be its primary
artifact, and a record is only as strong as what can be done with it alone.
The strongest thing a record can support is reissue: take a finished turn's
bytes from the trace, run them again, and get the same turn back. The seed
mechanism exists to make that possible - each generation's stream is fixed
by the declared seed, the turn's reference, and the generation's ordinal,
and by nothing else. Whether it holds on real hardware, through a real
teardown, was an open question until 2026-08-25. It holds.

## Method

One agent, `karl`, on the laptop deployment: qwen2.5-0.5b-instruct at
q6_k, CUDA, one RTX PRO 5000 Blackwell, declared seed 451234785645,
context capacity 8192. The agent served a turn through the channel surface
in ordinary use. That turn's record then supplied everything the reissue
used - nothing was kept outside the trace:

1. The turn's request text was read back from the record's own
   `message.user` event.
2. The agent was fully unloaded and loaded again: transient unit
   destroyed, model re-admitted to the device, a fresh run reference
   minted. Twenty-seven minutes separated the runs.
3. The recorded text was reissued byte-exact as the fresh run's first
   turn, dialed directly at the gate socket so no surface rebuilt or
   windowed the context.
4. The two runs' records were compared field by field.

The turn reference is an ordinal (`t-1`), not a run-qualified name, which
is what makes the cross-load reissue meet the same derivation inputs: same
declared seed, same reference, same generation ordinal.

## Result

Every compared field matched:

- the rendered prompt, byte for byte
- the derived generation seed, 14458752852352082704 in both runs
- every effective sampling knob
- the emission's bytes
- the finish kind and the resident count
- the input token ids
- all eighteen per-token entropies, float-exact

The last line is the strong one. Matching emissions could survive small
numeric drift, since sampling quantizes the distribution to a choice.
Float-exact entropies mean the distributions themselves matched to the
bits that reach the measurement: the forward pass reproduced, not merely
the sampled path through it.

## Scope, stated rather than rounded off

Confirmed in the lab means exactly this configuration: one turn shape, one
binary, one GPU, one driver, a serial load with the device to itself.
Untested and not claimed: concurrent turns, batch-shape variation across
context lengths, other devices, other builds. Entropy equality is a strong
proxy for logit equality and is still a projection of it. The reissue was
manual - the recorded bytes were carried back to the gate by hand. The
automated form of this experiment is the null replay, which is chartered
in the diagnostic domain and was not built here. What this result gives
that work is its precondition, proven: a null diff is achievable on real
hardware, so a divergence under an instrumented replay can be read as
caused rather than as noise.

## What it opens

A record that reissues bit-exact can stand behind more than debugging. A
distribution over a task can be gathered by varying only the declared
seed, with every other input pinned and known to pin. A fault reproduces
from its record. An evaluation result carries run-it-yourself force. And
the diagnostic domain's instruments inherit a floor: zero is reachable,
so every nonzero reading means something.

Two smaller facts the experiment surfaced are filed rather than resolved
here. The worker's default loop rendered its own identity message rather
than the declaration's, and the record's `model.request` and
`model.output` events carry one timestamp, so the record cannot yet
separate prefill from decode - a first-token stamp is the missing fact.

## The cross-precision entries

Run later the same day, scripted rather than hand-driven, because the
manual reissue was the one weakness the first result named. The harness
at `cross-precision-repro/` drives the whole protocol: serve a short and
a longer turn, unload fully, reload, read the request texts back from the
record's own events, reissue byte-exact at the gate socket, compare,
deposit. **That harness left this repository on 2026-09-13** for the
`weaver-experiments` tree, per `WeaverTools-Working-Process` section 5.
That tree has no remote, so the name above resolves to nothing a reader
holding this repository can open. The content is recovered from this
repository's own history, with `git archive` because the target is a
directory and `git show` on one lists its entries without retrieving
them, and to a file rather than an extraction because extracting from
the repository root would put `experiments/` back:

    git archive -o /tmp/repro.tar 0bca344 experiments/cross-precision-repro

Two entries on this box, the same
declaration apart from the artifact path, the same declared seed and
turn texts, single device, no sharding:

| entry | artifact | long turn | verdict |
|---|---|---|---|
| q8_0 | official GGUF, sha `ca59ca7f...` | 404 tokens | REPRODUCED, all checks, both turns |
| bf16 | local conversion, sha `cba13b11...` | 795 tokens | REPRODUCED, all checks, both turns |

The bf16 artifact is a conversion from the official safetensors by the
runtime's own vendored converter at the pinned rev, no public bf16
existing. Seven hundred ninety-five tokens bit-exact at bf16 is the
long-generation entry the first result lacked: drift had room and did
not come.

One observation across the entries, outside reproducibility's own
question and recorded as the first brick under the quantization
transfer question the Jacobian lens page holds open: with prompt and
derived seed identical across precisions, the short turn's emissions
were identical while the long turn's opened identically and then
diverged into different trajectories, each internally reproducible.
Precision changed where the path went, not whether the path repeats.

The cross-architecture entries stand open: the same script, the same
artifacts by hash, on the Ada and on a single Ampere device, are the
olympus seat's arm, tracked on the repository issue that carries the
pre-registered expectations.
