# Sketch: the STE conversion loop

**Status:** sketch. Decides nothing. Feeds PRD authoring. It is step 1 of the workflow
loop template in epic #437. It moves to `docs/crates/weaver-harness/Loops/` in the
workflow PR, after the amendments PR (the epic's step 2, carrying #432, #433 and #434)
merges (ruling H).

**Version:** 0.2
**Date filed:** 2026-11-01
**Document ID:** `sketch-ste-conversion-loop`
**Parent:** `weaver-harness-PRD`, as the intended parent once it moves.
**Editorial:** Per the Working Rules.
**Landing PR:** #431

**Placement.** It sits in `docs/project/` beside the other sketches. Document Format
section 2 says a `Loops/` document "walks a settled set", and that "where it and a
charter disagree the charter yields nothing." The loop as ruled departs from merged
text in CLASS-2, CLASS-3, CLASS-9 and CLASS-10 of `state-management-primitives-PRD`,
and those amendments have not landed. So it stays here until the amendments PR merges.

**Citations.** The loop runs today outside the agent, in the `toddwbucy/WeaverTrain`
repository, as scripts an operator seat fires by command. Every script and function
named below is WeaverTrain's, at commit `ab63a91` on its branch `ste/500-run`. Report
citations are to WeaverTrain `main` at `f90d9d7`, where
`reports/2026-11-01-ste-batch-200.md` and `reports/2026-11-01-ste-batch-200-2.md`
live. The link between the two repositories is this document and nothing else: no
code, fixture or path couples them.

**A term of this document.** "The bootstrap agent" is the agent whose first job is
this loop. The term appears nowhere else in the corpus.

---

## 1. Purpose

The loop converts the agent-authored prose of `nvidia/Nemotron-SWE-v1` trajectories to
STE100 and leaves everything else byte-identical. Agent-authored prose is the narration
in each assistant message and the final report in the closing `finish` call. Code, tool
calls, tool output, the issue text and the `think` fields stay exactly as the dataset
holds them, on the scope ratified 2026-11-02 (ruling O).

The loop has two objectives (ruling D).

- **Training data.** The converted trajectories train a LoRA adapter and then a full
  SFT toward a model that emits STE100 natively (section 6). Native emission is what
  CLASS-1 and CLASS-2 assume when they make STE100 the decoder's emission contract and
  key filing by verb (STORE-2).
- **The first real job run through WeaverAgent.** It puts the gate, the state member,
  the trace and the SPU under load, and it is not a benchmark. It runs locally at the
  CLI with no front end. It runs for the life of the agent, so it is codified as a
  workflow.

An agent has several workflow loops. This is the first, and the first with a
production target: every earlier agent targeted benchmarks, and later loops build on
what this one produces (ruling M). STE100 input at the gate (GATE-1) is a later loop's
question, not a factor here.

## 2. The rulings

Rulings A to G are the operator's of 2026-11-01. H to T are the operator's of
2026-11-02.

- **A.** The agent emits at the gate, not at the model. The decoder owns e, the agent
  owns Y, and a splice is the agent's outbound sort working (rung three of
  `graphable-state-management-primitives`). The trace keeps the raw e beside each
  splice. The gap between e and Y is the training signal, and pairing stays
  WeaverAnalysis's work under CLASS-10. #432.
- **B.** The thesaurus and the verb list live in state management. A word with one
  sense and one replacement is spliced automatically. A word with several senses goes
  back to the model as an offer, and the model re-emits the sentence. One verb artifact
  serves every judge. #433.
- **C.** No classifier for this loop at first. Lookup against the verb list and
  thesaurus, with the checker's position rules, is the detector. The loop converts and
  does not file. #434.
- **D.** Two objectives: training data, and the first real job through WeaverAgent.
- **E.** The destination is `docs/crates/weaver-harness/Loops/`. H governs when.
- **F.** The thinkpad builds the loop against a stand-in model while olympus trains the
  adapter. Both halves meet by the end of the week of 2026-11-02.
- **G.** The thinkpad also runs `nvidia/NVIDIA-Nemotron-3-Nano-4B-GGUF` at Q4_K_M to
  exercise the hybrid path, as the first target of #382.
- **H.** The sketch stays in `docs/project/` until the amendments PR merges.
- **I.** A `weaver-spu` family entry for the 4B may land from the thinkpad. The rest of
  the olympus lane is unchanged. #436.
- **J.** The bootstrap plan of section 6.
- **K.** The first adapter trains on both converted batches, 387 trajectories.
- **L.** Restate-then-solve is not in the first adapter's training data. R sets when it
  enters.
- **M.** Gate input is not a factor for this loop.
- **N.** J refines WeaverTrain issue #2 rulings 1 and 2.
- **O.** The think scope is ratified: think fields and issue text stay unconverted.
- **P.** The first local runs are single translations, one text per request.
- **Q.** Round 1 is calibration, the kill line is 20 to 25 percent, malformed output
  gets three strikes, and drops are counted and kept on the trace.
- **R.** Restate-then-solve enters this loop's rewrite turns at round 2 or later.
- **S.** The kill line counts drops.
- **T.** A workflow document authorizes loop code. Each piece of loop code traces to a
  clause of the workflow document this sketch becomes, and code that changes an organ
  still traces to that organ's Spec. It departs from Working Process H1 and the
  Document Format's reading of a workflow document. #438.

## 3. Who starts each turn

The harness PRD section 2 says a turn begins at the gate and nowhere else. Vision
section 6 calls autonomic outward action "the empty corner": the loop reaching the
world unprompted is out of scope. So the loop splits in two.

**The driver** runs outside the agent, at the CLI. It samples, masks, builds each
request, assembles, scores and runs the reviewer pass. It sends each text to the agent
as one turn through the gate (`deploy/turn.py`). It seeds the rules prompt with
`deploy/turn.py --system`, which enters it as state: the state member holds it and
every later load seats it.

**The agent's loop** is the model turn, then the outbound sort: lookup against the
verb list and thesaurus, splice for one sense, the offer and the model's re-emission
for several. The trace records all of it.

The marks are those of the original sketch. **Detect** reports a condition and changes
nothing. **Compute** is a deterministic transform with no model in it. **Refit**, or
**splice**, writes binding material into the content at a position k. **Offer**, or
**append**, places material after the content for the model to use or not. **Clerk**
records what happened and decides nothing.

| Stage | Side | Mark | WeaverAgent under load |
|---|---|---|---|
| Sampling | driver | compute | none |
| Masking | driver | compute | none |
| Rules prompt | driver seeds it | offer | gate, state member (the prompt is state) |
| Request build | driver | compute | none |
| Model turn | agent | the model's turn | gate, SPU, trace |
| Collection | agent | clerk | trace |
| Lookup | agent | detect | state member (the held artifacts) |
| Splice, one sense | agent | refit | trace, and the SPU when the text feeds a kept session (section 5) |
| Offer, several senses | agent | offer | SPU (the re-emission), trace |
| Shape check and three-strike retry | open cell | detect | trace |
| Literal check | open cell | detect | trace |
| Score | driver | clerk | none (reads the trace and the output) |
| Reviewer pass | driver, by hand | detect | none |
| Assembly | driver | compute | none |

**Open cell: the literal check and the three-strike retry.** Where each sits is open.
The literal check has two candidates: the outbound sort beside lookup, since it checks
Y, or the driver at assembly. The three-strike retry (ruling Q) has two: the driver
sends a new turn through the gate, or the agent's outbound sort re-asks, as the offer
for multi-sense words does. A strike is a shape fault only, a parse failure or a lost
placeholder. Splices and offers are not strikes.

## 4. The stages as they run today

Every stage below runs today as a WeaverTrain script an operator seat fires, with a
remote model (Gemini 4 Argon) at stage 5.

**1. Sampling** (`scripts/ste_batch.py`, `cmd_sample`, `agent_segments`,
`TokenCounter`). The script reads dataset rows in a seeded order and keeps a row only
if it has a final report, every narration and final segment masks to at most 6,000
characters, and the whole row fits 32,768 tokens under Nemotron's own chat template.
That limit is the trainer's verified sequence length, and about one row in ten meets
it, so the sample favors shorter tasks. A row is named by its line number, because
4,000 uuids repeat across different rows, and one row per uuid is kept. Rows already
used in the rewriter-selection kits are excluded. The first batch took 200
trajectories holding 6,931 segments.

**2. Masking** (`scripts/ste_convert.py`, `mask`, `mask_listings`, `mask_brackets`,
`split_ws`, called from `ste_batch.cmd_prepare`). Code-like spans become `[[Cn]]`
placeholders before any model or checker sees the text: fenced and inline code, URLs,
paths, file names, calls and bracketed literals, dotted and snake_case names, quoted
strings, HTML tags, exception messages to the end of the line, padded "name : value"
program listings and Markdown blockquotes. A blockquote is someone else's text, a
quotation of the issue or of output, and stays as written. The spans go to a local
codes file for assembly.

**3. Rules prompt** (`scripts/ste_prompt.py`, `render`, output
`var/ste/prompt-e21370fcd140.json`, rendered for a request by
`ste_experiment.render_rules` and `ste_gemini.group_rules`). The prompt holds the
writing rules, the approved verb list from the operator's local copy of the standard,
the admitted technical verbs, the fixed mappings and four worked examples, about
14,000 tokens. Its hash pins every rewrite to the rules it was written under. It is
offered ahead of the texts and binds nothing.

**4. Request build** (`scripts/ste_gemini.py`, `units`, `group_message`,
`group_request`, `unit_request`). Twenty masked texts go into one request with the
rules as the system instruction, and the answer is constrained to a JSON array of
`{id, output}` by a response schema. Grouping exists because the remote model has no
explicit context cache, so one text per request would resend the rules prefix 6,931
times. Groups never cross a part, so the first batch's 60 parts, at ceil(n/20) requests
each, came to 375 requests for 6,931 texts rather than a flat 347. The first local
runs send one text per request (ruling P).

**5. Model turn and collection** (`ste_gemini.cmd_submit`, `cmd_status`,
`cmd_fetch`, `text_of`, `unit_outputs`). The model's turn is the model's. Only its
collection is clerk. One JSON Lines file is uploaded and one batch job started on
Gemini 4 Argon at high thinking. Collection downloads the results, drops thought
parts, reads each array and records per text what came back, what was missing and the
token usage. It decides nothing about content.

In the first batch, 14 of 375 round-1 answers stopped with finish reason STOP after 1
to 4 texts. Twelve of those groups held 20 texts. Two were a part's last group, of 8
texts (`p021/T069.031`) and 12 texts (`p031/T104.026`), so the report's line "1 to 4
of their 20 texts" is wrong for two of the fourteen. Truncation accounts for 235 of the
236 missing texts. The second batch lost 258 texts the same way
(`ste-batch-200-2.md`), so truncation is a property of grouped requests. The missing
texts went to the repair round.

**6. Checker** (`scripts/ste_check.py`, `check`, `Lexicon`, `repair_note`, with
`ste_convert.Converter.guard`, `ste_experiment.Judge` and `ste_batch.judge_round`).
The checker reports unapproved verbs and words, -ing forms, sentence length against
the instruction and description limits, placeholder counts, capitals that are not
technical names, a copied worked example, a large growth or loss of words, and markup
that the masked input does not hold. A word after an article, a possessive or a
preposition is read as a noun or adjective. The markup guard was added after a
reviewer found one answer group that had ended all 17 of its rewrites with a stray
`<h4>`, which the word checks had passed.

**7. Literal check** (`ste_experiment.glued_literals`, `altered_literals`). A token
that glues a placeholder to other characters, such as `145.[[C8]]` or
`[[C4]]([[C5]]`, is a literal the rewrite must copy unchanged. Placeholders joined only
by slashes may be written out ("[[C8]] and [[C9]]"), since the placeholder count still
holds each one. Assembly refuses a segment whose literal changed.

**8. Repair** (`ste_batch.judge_round` writes `repair-N.jsonl`,
`ste_gemini.group_message` builds the repair group, `cmd_submit` sends round N+1).
Each failing text goes back with its last rewrite and the checker's notes, or a request
for a fresh rewrite if it was missing, and the model re-emits it. CLASS-9 calls this
"restatement" and #432 carries that term. Rounds in the first batch passed 6,181, then
6,873, then 6,920, then 6,926 of 6,931 segments.

**9. Score** (`ste_batch.cmd_status`, `ste_gemini.cmd_fetch` counts,
`ste_experiment.cmd_score`, `substitution_scripts`, `near_copies`, `incomplete`). Pass
counts per part and round, failing rules, altered literals and near-copies are
recorded. In the agentic variant the score also refuses a round made with a pattern
substitution script, which an earlier void run had used. In the batch variant no
script can run.

**10. Reviewer pass.** Reviewer sessions read a sample of inputs against their final
rewrites and return one verdict each: faithful, minor drift, wrong, or data altered. On
100 random segments of the first batch the verdicts were 90 faithful, 9 minor, 0 wrong
and 1 data altered, the `<h4>` above. The pass finds what the checker cannot see. No fix
in the first batch was made by hand. Every finding became either a new guard or a repair
round.

**11. Assembly** (`ste_batch.cmd_assemble`, `ste_convert.unmask`, `ste_convert.apply`).
This is compute, not refit: it writes each rewrite into a dataset row, which never
re-enters a model's argument. A trajectory converts only if every one of its segments
passes the checker and keeps its literals. Each rewrite, with its code restored, is
written into the row at its own message and field, and nothing else in the row moves.
The first batch converted 196 of 200 trajectories. A check over all 196 found only the
agent's own messages changed, no placeholder left and no tag that the original did not
hold.

## 5. The thesaurus and the verb list

WeaverTrain's `ste/thesaurus.toml` holds 48 verbs that software text uses and STE100
does not approve, in 55 senses, each with the approved words to use and one rewrite
that the checker passes. Today it is a reference and a test fixture: it is not in the
prompt, and nothing replaces a word from it. One table reaches the loop. The fixed
mappings in `ste/fixed-mappings.toml`, which today hold only the ruled "call" to "use",
make the checker's repair note name the replacement instead of the standard's
alternatives. Every correction today is therefore an offer that the model re-emits.

Under ruling B both the thesaurus and the verb list are held in the store, and the
loop keys on them (#433). Each entry is tagged with its path. An entry with one sense
and one replacement that fits every context the checker can see is tagged **splice**:
the outbound sort replaces the word in place, with no model turn. An entry with several
senses, or a replacement that needs the sentence rebuilt, is tagged **offer**: the
outbound sort returns the sentence to the model with the entry's senses as a
suggestion, and the model re-emits it. The tag is kept with the entry and changed only
by a ruling. One verb artifact serves every judge, so the checker's local list and the
CLASS-2 set fixed at load are the same list.

**What a splice costs.** Rung three: an edit to the act Y alone does not change the
buffer, and the prefix stays valid. If the edit also changes the next argument, first
at position k, reuse ends at k: everything up to k is kept, and everything after k is
re-decoded. So a splice costs a flush only where the spliced text feeds a next argument
in a kept session. A splice into finished output that no later turn reads costs
nothing. Where a flush does happen, the cost depends on the family. The SPU's
`nemotron_h_moe` family declares `ReestablishAndReprefill`: its flush reestablishes the
backend and then decodes the whole kept prefix from position 0
(`crates/weaver-spu/src/decoder/session.rs:719-723`), because a hybrid recurrent state
cannot be truncated to k. An offer appends and keeps the prefix.

## 6. The bootstrap plan

In this order (ruling J):

1. A small LoRA adapter on Nemotron 3 Nano 30B A3B, trained on the Gemini-converted
   trajectories: both batches, 387 trajectories (ruling K). It is small on purpose:
   there is only enough data for an adapter, not an SFT.
2. The same loop that produced the data with Gemini runs locally with that adapter in
   stage 5's place, one text per request at first (ruling P). Its output is expected to
   be imperfect, and that is acceptable.
3. The loop carries the correction: thesaurus and verb-list lookup in state management,
   splice for one sense, the offer and the model's re-emission for several (ruling B).
   No classifier (ruling C).
4. Every model emission is kept beside its correction (ruling A). Misses and their
   corrections are training data, not waste. Drops are not: they carry no correction,
   so they are trace records for analysis, not training pairs (ruling Q). Whether round
   1's checker-passed output enters the next adapter's corpus is unruled (section 11).
5. That corpus trains a full SFT. The adapter is then removed.
6. Once the model emits STE100 reliably, autonomic functions are built on its
   predictable output.

**Why this path.** In the operator's words, it is the cheapest path in money and the
best in engineering, because it forces the loop to be tight. A remote run of the whole
corpus would buy data, but it would not prove that a loop can improve the agent's
output with only an adapter as the change to the model.

**The training set** (ruling K) is both converted batches:

- batch 1: 196 trajectories, `converted.jsonl` sha256
  `63afa6fe3aa1f44cb98355e6717b073921c2ee1ce1451a93acf0aae20e349ad9`
- batch 2: 191 trajectories, `converted.jsonl` sha256
  `9bd8bac719156498c4f2ac7916b0afae85aa0ac909ad9cece95b5fe48fbe23a5`

Both hashes are from each deposit's `run.json`. The adapter's own sketch owns the
training run, and this document carries no more adapter detail than the loop needs.

**WeaverTrain issue #2** (rulings of 2026-10-31). Ruling J refines its rulings 1 and 2
(ruling N). Ruling 1 dropped untrained Nemotron self-conversion, and J's adapter is a
bootstrap tool, not the untrained model. Ruling 2's endpoint holds: STE100 goes into
the SFT, and the adapter is removed after it.

## 7. The detector without a classifier

Under ruling C the detector is lookup against the verb list and the thesaurus, with the
checker's position rules for verb against noun (stage 6). The classifier comes later,
once a model emits STE100 stably enough to classify against. Filing by verb (STORE-2)
needs the classifier, so this loop converts and does not file. CLASS-2 and CLASS-3 ship
classify-on-write, and #434 carries the amendment for a loop that converts without
it.

## 8. What the trace records

For every change the trace holds the model's emission e as it came, the agent's
emission Y, the position (message, field and character span), the rule e broke, and
whether the change was a splice or an offer. A drop holds its three emissions. Under
ruling A the raw e stays beside each splice, and the gap between e and Y is the
training signal. Under CLASS-10 there is no rejected-emission kind and no training-pair
duty inside the agent: pairing e with Y is WeaverAnalysis's work, read from these
records. That pairing is what turns the loop's own trace into the next round's
training data.

Today the same facts live in files rather than a trace: each round's output, each
repair file with its notes, the batch results as returned, and a run record with the
dataset revision, prompt hash, counts and output hash. The position is the segment id
and its locator. Every change is an offer today, since nothing splices.

## 9. The hardware split and the two cards

From 2026-11-02, for about one to three days, olympus trains the adapter. The loop is
built on the thinkpad in that time (ruling F), against a stand-in model (Qwen 8B, any
quant) and the Nemotron-3-Nano-4B at Q4_K_M for the hybrid path (ruling G). The 4B is
reported as dense, so its GGUF may declare `nemotron_h` rather than `nemotron_h_moe`
and need its own family entry, which may land from the thinkpad (ruling I, #436). Its
`general.architecture` key is read, not assumed. If it is `nemotron_h`, MoE routing is
tested on olympus only.

The thinkpad is a verification target, not only a stand-in. Part of the goal is to
verify the code on more than one NVIDIA card, and the split is part of code review.
When work leaves the thinkpad, the loop runs end to end there, through the gate, the
state member, the trace and the SPU. Olympus is then the second card, not the first
real run. The two machines are rows `crates/weaver-spu/kernels/PROVENANCE.md` already
names: the thinkpad's RTX PRO Blackwell laptop (sm_120, CUDA 13.4) and olympus's A6000
pair (Ampere, sm_86). When training ends, the code moves to olympus and the model is
swapped through the declaration's model binding, with no change to the loop. Each run's
evidence goes to a deposit under `weaver-testing/`, and the repository carries only the
result note.

## 10. The model swap and the adapter

The SPU has a `nemotron_h_moe` family entry whose template the marker probe measured
against a Nemotron 3 Nano 30B A3B artifact. Whether that hybrid Mamba MoE architecture
completes a turn end to end on the SPU is unconfirmed.

The SPU cannot load a LoRA adapter today: no LoRA load path exists in its source. That
is a Spec gap and the operator's ruling, not a measurement. The "model adapter" that
`crates/weaver-spu/src/ste100/mod.rs` names (lines 3 to 6) is something else: the
STE100 duty's production grammar compiler and model adapter, which remain dependencies
of Spec 02 O02-1 and O02-2. It is the classifier duty's binding to a model, not a LoRA
adapter.

WeaverTrain trains in bf16 through transformers and PEFT, and the SPU's default backend
is GGUF through llama.cpp, so the adapter's path from one to the other is part of the
same question (#435).

## 11. Open cells

**One cell per card.** The loop end to end on the thinkpad's Blackwell laptop. Then
the same loop on olympus's A6000 pair, with the swapped model. Settled by one run on
each, with its deposit.

**The literal check's place, and the three-strike retry's.** Section 3.

**The adapter's job format** (ruling P). The adapter trains on whole trajectories. The
first local runs send one text per request, under the rules prompt, and expect the
rewrite with every placeholder intact. Whether an adapter trained on trajectories does
the rewrite request is settled on the first local run, by parse failures and
placeholder losses per single request. Lookup and splice repair vocabulary, not shape.

**Group size** (ruling P). Truncation is a property of grouped requests (stage 5).
Grouping enters only after the single-request baseline is known. Settled by the
missing-text count per group size, measured against that baseline.

**The grouped check.** "80 then 100" is a checker result. On the 100-segment kit, the
grouped setup passed the checker on 80 of 100 in round 1, and 100 of 100 after one
repair round. Gemini in opencode sessions passed 81, then 99. The meaning review of the
same runs was 77 faithful, 21 minor, 1 wrong and 1 data altered for the grouped setup,
against 79, 20, 0 and 1 for opencode (`ste-batch-200.md`). That comparison varied
grouping and the session harness together. A clean test holds the harness fixed and
varies only the group size.

**Drift across adapter rounds** (ruling Q). The loop trains on its own output, so
errors can compound. The metric is the faithful count on the held-out set, not the
checker pass rate. Faithful runs 77 to 81 on the 100-segment kit, so near 80 percent one
standard error is about 4 points (sqrt(0.8 x 0.2 / 100)). The held-out set is the kit
that selected Gemini as the rewriter, so it favors the baseline. Round 1 with the
adapter is calibration and is not scored for keep or drop: the operator reads its
success rate and how bad its failures are, then states the margin before round 2. From
round 2, a round is not used if its faithful count falls below the prior round's by
more than that margin. Malformed output gets three strikes, and each drop stays in the
trace with its three emissions. The drop count per round is a metric beside the
faithful count, because dropping hard items biases the corpus toward easy ones, which
adds to the bias the 32K row-length limit already causes.

**The kill line** (rulings Q and S). It counts drops, items dropped after three shape
strikes. It is a drop rate in the range of 20 to 25 percent, over a sample the operator
judges large enough, and it is not judged on the first few items. The open part is the
sample size.

**Round 1's output.** Whether round 1's checker-passed output enters the next adapter's
corpus is unruled and owed to the operator. Round 1 is calibration, so its output is
not scored for keep or drop either way.

**Restate-then-solve** (rulings L and R). WeaverTrain issue #2 ruling 6, and ruling 9's
second bullet ("the model's first emission each turn is the STE restatement"), put an
STE restatement of the input at the head of every converted turn. Neither converted
batch carries it. The entry point is ruled: it is not in the first adapter's training
data, and it enters this loop's rewrite turns at round 2 or later, so round 1 measures
the base rate without it. WeaverTrain #2 ruled it for agent turns, and ruling R carries
it into rewrite requests. The open part is the mechanism.

**The think scope** (ruling O). The scope is ratified: the model reasons freely and
writes its narration and output in STE100, and think fields and issue text stay
unconverted. The 387 converted trajectories rest on it. What stays open is the PRD
question: which blocks CLASS-10 makes subject to STE100 classification ("a block
subject to STE100 classification"), given that the family entry calls the think pair
outbound (`crates/weaver-spu/src/family/mod.rs`, near line 946).

**The splice tag per thesaurus entry.** Which entries can be replaced with no model
turn. Settled per entry by splicing it on the held-out set and counting the splices a
reviewer judges faithful.

**The repair round cap.** CLASS-9 asks for a measured cap on what it calls
"restatement" (#432 carries the term). Settled by the pass curve over rounds, which in
the first batch was 89.2, 99.2, 99.8 and 99.9 percent.

**Local rewriter quality.** Whether the adapter matches the remote model. Settled by
the held-out faithful count of each adapter round against Gemini's.

**Checker blind spots.** What the checker passes that a reader would not. Settled by
the count of reviewer-found defects per hundred checker-passed segments, round by
round. The first was the invented `<h4>`.

**Splice cost on a hybrid model.** First, whether any splice in this loop reaches a
next argument in a kept session (section 5). If one does, settled by measuring the
re-prefill time per splice under `ReestablishAndReprefill` on the Nemotron artifact.

**Sequence length.** Whether the 32K limit can rise. Settled by the trainer's peak
memory at 64K on the two cards.

**Hedges.** The rewrites turn "seems" and "appears" into "possibly", which reads less
sure. The hedge forms await a ruling. Settled by the reviewer drift count on hedged
sentences under each candidate form.

**The SPU on Nemotron.** Settled by one turn end to end on the Nemotron artifact
through the declaration. The turn with the adapter loaded waits for the LoRA load path
(section 10, #435).
