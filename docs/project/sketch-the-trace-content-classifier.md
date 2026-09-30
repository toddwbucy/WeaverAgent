# Sketch: the trace-content classifier

**Status:** LIVING, outside the set. A sketch in `docs/project/`, per the Document
Format: it maps into no graph and nothing is written against it. What it settles
moves into a charter or a Spec by an act of its own.

**Date filed:** 2026-09-29

**Landing PR:** #738

---

## 0. What this document is

The plan for typing the trace's payload. The envelope is typed and positional already,
per `weaver-trace-PRD` section 3, and the payloads are what the classifier reads. Each
event carries its own: the conversation's text in the `message.*` kinds, the rendered
model input in `model.request`'s `rendered`, the verbatim generation in `model.output`'s
`emission`, the token identifiers and readings in `model.measurement`, an action's
request, its tool name and arguments, in `tool.call.started`, and the environment's
answer in `message.tool_result`. None of it is typed by what it says. Typing that
payload is what the state organ needs, per the operator's word of 2026-09-28, and it is
the prerequisite for the prefetch organ's text head.

The instrument is a trace-content classifier run in the loop as a routing stage
and never a judging one: it types content so the loops can decide, and decides
nothing itself. That is the shape the classify seam already has, per
`weaver-harness-spu-classify-contract` and `weaver-spu-PRD` section 15, and this
sketch is written against that seam rather than beside it.

## 1. The method

HeroBench, the operator's fork at the workshop level, supplies the label ontology
from the game: items, recipes, level gates, locations and actions, each finite and
external to the traces. A label set derived from the traces would be circular. A
label set taken from the game is a middle-range mapping in which the known
process is external and finite, and a classifier trained on it can be measured
against the game's own record of what happened.

The agent plays HeroBench through the gate's shell, the environment's actions being a
CLI in the agent's home. The task arrives as `message.user`, the rules and the game dump
as `message.system`, and the model's reasoning and program as `message.assistant`.
**Where the agent plays action by action, each action its own shell call**, each
action's request, the tool name and its arguments, is recorded on `tool.call.started`
and the environment's answer on `message.tool_result`, each at a position of its own,
and the join from a position to game state is by position and free. The gate's shell
tool records the name `bash` and the command as one opaque `arguments.command` string,
so the requested action, item and tile are read by decoding the per-action CLI's own
argument shape inside that string, which is the tool's schema and the HeroBench tool's
to define. The computable labels that read the requested item, tile or action read the
call event, and those that read the outcome read the result. A program run as one
invocation collapses every action into one call and one `message.tool_result`, which is
how HeroBench's published runs ran, so the state work plays the per-action shape.

## 2. The split that makes the case for building

Nothing is built before this is measured. The label space is split in two and
both halves are counted, over HeroBench's own results and one replayed pair of
programs:

- **Computable.** The label joins to game state by position: an item code exists
  in the ontology or does not, a location matches the resource's, a level gate is
  met or not, the environment accepted or refused the action, the inventory moved
  or did not, the task scored or did not.
- **Judgmental.** The label needs a model: this span recites the dump, this span
  states a plan, this plan contradicts what was just recited. The game does not
  enumerate these. The Pumpkin Spice specimen is that failure and the template
  case for deriving them.

Positions no recipe reaches are a third count, never folded into either.

**A labeled position is an event together with a span of its payload**: a prose line or
a program statement of `message.assistant`, an action's request on `tool.call.started`,
or its answer on `message.tool_result`. The harness writes one `message.assistant` and
one `model.output` per generation, and the trace has no event per line, so the span is
what makes a line a position. The task's outcome is a position too, the run's turnless
`score` event, which belongs to the computable share, joins game state with no model and
is read by no candidate, so it needs no input.

**The input at a position is what the model was shown**, the full effective context as
`weaver-trace-PRD` section 3.2 reads it: "the accumulation of the recorded contributions
under their recorded template identities, from the identity prefix the run's opening
records", through this turn's contribution, which `model.request` carries as `rendered`,
with every recorded flush and elision replayed as section 3.1 requires. To that the
generation is added cut at the span's end, and for a span on `message.tool_result` the
result's content cut at the span. Nothing any organ authored about the generation is in
it, not the measurement, not the per-token fields, not the classify seam's request and
output, not the score, because none of it was presented to the model, and a kind added
to the record later is excluded by the same rule. The span cut on the presented
generation is the causal cut, so nothing the model had not yet written at the span is in
its input.

Measured 2026-09-29 over HeroBench's own results, 3,882 single-shot completions
whose text is on disk, segmented into positions (prose lines, draft lines,
program steps, executed actions, refusals, the scored outcome):

    Weighting               Positions    Computable   Judgmental   Unreached
    every position          1,477,957    51.4%        44.6%        4.0%
    what the model wrote      834,725    13.9%        78.9%        7.1%

The first weighting is mostly the environment's log, a looping program
contributing thousands of accepted actions per task. The second is the text the
model authored, and four of every five of its positions are judgmental.
Judgmental of authored text runs 87.0 per cent at level 1 and 79.4 to 83.7 from
level 2 to 9. The scripts are under `experiments/trace-content/label-split/`,
and the deposit is `herobench-positions-2026-09-29` on the share. **The judgmental
count is the count of the model's prose lines, which no game-join labels, a
definition, and not yet a measurement of any model's difficulty on them**, which the
evaluation of section 4 supplies.

## 3. The candidates

Four are weighed and one is ruled out. The table records what each is, the
memory it takes to serve, its licence, and how it meets the seam.

    Candidate        What it is                                   Serve      Licence     Seam
    ModernBERT       encoder, 149M, classify head, id2label       under 1G   Apache-2.0  one head
    Laya             Convai, 421M, runs on a T4                   (owed)     Apache-2.0  (owed)
    open-jev         DeBERTa-v3-large encoder                     (owed)     (owed)      one head
    Kev-0.8B / 4B    Qwen3.5-Base, LoRA r=16, pointer head        4G / 9-17G Apache-2.0  contract change

**The classify seam serves one head as built.** The SPU's classify submodule admits one
artifact and applies one softmax over its one `id2label` head, per `weaver-spu-Spec`
section 11, so ModernBERT and open-jev meet the seam as built with one head only.
Serving a decision per judgmental predicate takes three admitted classifiers or a
multi-label head, and either is a Spec act in the SPU and in the harness alike, since
the harness's lifecycle module holds one optional classify arm and its engine one
classify port today, per `weaver-harness-Spec` section 6.

**ModernBERT is the first classify family the SPU carries**, per
`weaver-spu-Spec` section 11, and `ModernBERT-base-zeroshot-v2.0` is on olympus
under the models store, so the encoder default is one admission away.

**Kev is a decision model rather than a label head.** One state document and a
set of typed questions, `noul`, `choice` and `score`, answered as a probability
distribution per question in one forward pass, states to 65,536 tokens, served
over its own HTTP endpoint on PyTorch, transformers and peft, fine-tuned from the
released adapter on JSONL of state, questions and labels. The judgmental labels
above are `noul` and `choice` questions over the trace window at a position,
which is Kev's shape exactly. Its serving memory is stated at about 9 GB on the
model card and 17 GB in the repository, and which is true on this hardware is a
measurement.

**The constraint that decides between them.** A LoRA over the decode model makes
the observer part of the observed and puts the adapter into the deployment
tuple. A separate model keeps them apart on weights, and can keep them apart on
device. Kev is not a LoRA over the decode model, it is its own base, so it meets
the constraint on weights. What it adds is a second model with a tuple of its
own, weights, precision, device and kernel stack, PyTorch where the decoder is
candle, and that tuple is recorded in every deposit beside the decoder's or the
classifier's answers are not reproducible. **The encoder is the default unless
measurement says otherwise**, on the operator's position of 2026-09-28.

**Where Kev disagrees with the corpus.** The classify contract's section 2 says
the label set is the artifact's and never the ask's, an ask carrying content
alone. Kev's questions ride the ask. Serving it behind the label seam is a
contract change reaching both parties, on the operator's ruling, and the member
honouring the seam would be a Python process, which the contracts permit since
they name no language.

**Jev is ruled out.** TypeSafe's Master Customer Agreement, section 2.3(b), forbids
training a model to imitate the Services' output, which is what a fine-tune on our
labels would be. Recorded here so the exclusion is not rediscovered.

## 4. The evaluation, a protocol draft

Every candidate is evaluated on the same held-out judgmental positions, side by side.
**The labeled positions are split by task, never by line or by result set**: every
result set's completion of one task falls on the same side, so neither a completion's
lines nor a task's text straddle the split. Each candidate is fitted on the training
split and scored on the held-out split, the split is recorded in the deposit beside the
labels, and a score on positions a candidate was fitted on is not reported. **The labels
come from outside every candidate**: no candidate, and no model of a candidate's family,
labels a position it is scored on, and the labels' source is recorded in the deposit
beside them. **Every candidate reads the same input at a position**, the input section 2
defines for it, its extent fixed before any candidate is scored, within every
candidate's context, and recorded, so a difference in score is the model's and not its
input's. The computable share needs no model, its labels joining game state by position,
so no candidate is scored on it.

**Each judgmental predicate is scored as its own binary decision**: whether a position
recites the dump, whether it states a plan, and whether it contradicts what was recited,
a position able to hold more than one. The candidates answer in different shapes, Kev a
distribution per typed question and the SPU's ModernBERT path one softmax over a fixed
id2label head, so each candidate's output is normalised to that decision before it is
scored: for Kev a `noul` question per predicate with its probability thresholded, and
for ModernBERT one encoder head per predicate or a multi-label head thresholded per
predicate. **Every threshold is chosen on the training split and never on the held-out
one**, by one rule for every candidate and every predicate: the threshold that maximises
F1 on the training split, chosen once per candidate and predicate and recorded in the
deposit. The candidates are compared per predicate on precision and recall over the
held-out positions, and a single figure across the three predicates is not reported.

**The label split's size is the case for building a classifier and not evidence for
choosing one**: four of every five positions the model wrote are judgmental, which says
the instrument is worth building, and which candidate serves is what the evaluation
measures. A contract change to carry questions on the ask follows that measurement
rather than the label split. Either way the classifier's own tuple is recorded and the
observer stays off the decoder's weights and device.

## 5. Open cells

- Each candidate's precision and recall per judgmental predicate on the held-out
  positions, a measurement.
- Who labels the judgmental positions. Section 4 requires only that no candidate
  and no model of a candidate's family does.
- The evaluation's protocol. Section 4 is a draft, and the protocol is settled in its
  own experiment plan under `experiments/trace-content/`, a plan document reviewed as a
  plan before any candidate is fitted. A further finding against the protocol is
  answered there rather than here. The plan fixes the input window's extent as a bound
  every candidate and the seam can hold and records it: the seam carries the serialised
  `LabelDirective`, its JSON wrapper and escaping included, at most `MAX_ENVELOPE_BYTES`
  per frame by `weaver-spu-Spec` section 11, so the bound is measured on the serialised
  form rather than inferred from the raw window's size, and the window also stays within
  the artifact's own position limit. The plan measures each candidate's latency at that
  extent on the device it would serve from, and a candidate that cannot answer inside
  the harness's classify bound, `CLASSIFY_ANSWER_BOUND_MS` in `weaver-harness`, is not
  selected however it scores, since the harness serialises classify asks and retires the
  classify arm after an answer past it. The plan budgets latency per turn, over every
  position the routing stage asks about in that turn, and not per answer alone, since
  the harness serialises the asks and blocks on each. The plan predeclares its selection
  rule before any candidate is fitted, a rule stated over the per-predicate figures and
  the serving bound, so the held-out results decide the winner and are not read before
  the rule is fixed.
- The Pumpkin Spice specimen is not an artifact on disk: it names a decision
  point, the moment after a retrieved fact lands, per the operator's word of
  2026-08-31. The template case for the judgmental labels is still to be chosen
  or captured.
- Kev's serving memory on the Ada, 9 or 17 GB, a measurement.
- Whether the classify contract changes to carry questions on the ask, the
  operator's ruling after the evaluation.
- The prefetch organ's text head, which this sketch is prerequisite to and does
  not design.
