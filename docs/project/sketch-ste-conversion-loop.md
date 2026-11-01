# Sketch: the STE conversion loop

**Status:** sketch. Decides nothing. Feeds PRD authoring.

**Version:** 0.1
**Date filed:** 2026-11-01
**Document ID:** `sketch-ste-conversion-loop`
**Editorial:** Per the Working Rules. ASCII, no semicolons.

**Placement.** It sits in `docs/project/` beside the other sketches because no crate
owns it yet. It describes a loop that runs today outside the agent, in the
`toddwbucy/WeaverTrain` repository, as scripts an operator seat fires by command. It is
filed here because that loop is the first loop the agent is meant to run, and its
first agent is a bootstrap agent whose one job is converting training data to STE100.
Every script and function named below is WeaverTrain's, at commit `ab63a91` on its
branch `ste/500-run`. The link between the two repositories is this document and
nothing else: no code, fixture or path couples them.

---

## 1. Purpose

The loop converts the agent-authored prose of `nvidia/Nemotron-SWE-v1` trajectories to
STE100 and leaves everything else byte-identical. Agent-authored prose is the
narration in each assistant message and the final report in the closing `finish` call.
Code, tool calls, tool output, the issue text and the `think` fields stay exactly as
the dataset holds them. The issue and `think` exclusion is WeaverTrain's scope
candidate of 2026-11-01, pending ratification.

The converted trajectories have two uses in order. The first is a LoRA adapter for
Nemotron 3 Nano that lets the agent run this same loop locally in place of the remote
model. The second is a larger converted corpus for supervised fine-tuning, aimed at a
local model that emits STE100 natively. Native emission is what
`state-management-primitives-PRD` CLASS-1 and CLASS-2 assume when they make STE100 the
decoder's emission contract and key filing by verb (STORE-2). A closed verb set is
also what lets a verb or noun act as the key for autonomic functions the harness fires
on a condition it set, in the sense `weaver-tools-vision` section 6 gives autonomic.

## 2. The marks

Each stage carries one mark and a firing.

**Detect** reports a condition and changes nothing. **Compute** is a deterministic
transform with no model in it. **Refit**, or **splice**, writes binding material into
the content at a position k. In the terms of `graphable-state-management-primitives`
rung three it sets N at the splice, so the prefix after k is rebuilt. **Offer**, or
**append**, places material after the content for the model to use or not, leaving N
at full length. **Clerk** records what happened and decides nothing, as the harness
PRD uses the word for a report written to the record with no turn opened.

A stage is **harness-fired** when the loop would fire it on a condition the loop set,
with no model turn. That is autonomic in the corpus's sense, since initiation is the
axis and the trace shows who moved first. Today no stage is harness-fired: an operator
seat runs each one by command. The table gives the firing the stage would have inside
the agent.

| Stage | Mark | Inside the agent |
|---|---|---|
| 1 Sampling | compute | harness-fired |
| 2 Masking | compute | harness-fired |
| 3 Rules prompt | offer | harness-fired |
| 4 Batch request build | compute | harness-fired |
| 5 Model submit and collect | clerk | the model's turn, collected by the harness |
| 6 Checker | detect | harness-fired |
| 7 Literal check | detect | harness-fired |
| 8 Repair | offer | harness-fired |
| 9 Score | clerk | harness-fired |
| 10 Hand or reviewer pass | detect | hand, outside the loop |
| 11 Assembly | refit | harness-fired |

## 3. The stages as they run

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

**4. Batch request build** (`scripts/ste_gemini.py`, `units`, `group_message`,
`group_request`, `unit_request`). Twenty masked texts go into one request with the
rules as the system instruction, and the answer is constrained to a JSON array of
`{id, output}` by a response schema. Grouping exists because the remote model has no
explicit context cache, so one text per request would resend the rules prefix 6,931
times.

**5. Model submit and collect** (`ste_gemini.cmd_submit`, `cmd_status`, `cmd_fetch`,
`text_of`, `unit_outputs`). One JSON Lines file is uploaded and one batch job started
on Gemini 4 Argon at high thinking. Collection downloads the results, drops thought
parts, reads each array and records per text what came back, what was missing and the
token usage. The stage decides nothing about content. In the first batch 14 of 375
answers stopped after 1 to 4 of their 20 texts with a normal finish reason, and the
236 missing texts went to repair as missing.

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

**8. Repair** (`ste_batch.judge_round` writes `repair-N.jsonl`, `ste_gemini.group_message`
builds the repair group, `cmd_submit` sends round N+1). Each failing text goes back
with its last rewrite and the checker's notes, or a request for a fresh rewrite if it
was missing. This is restatement as `state-management-primitives-PRD` CLASS-9 frames
it, run as a batch. Rounds in the first batch passed 6,181, then 6,873, then 6,920,
then 6,926 of 6,931 segments.

**9. Score** (`ste_batch.cmd_status`, `ste_gemini.cmd_fetch` counts,
`ste_experiment.cmd_score`, `substitution_scripts`, `near_copies`, `incomplete`). Pass
counts per part and round, failing rules, altered literals and near-copies are
recorded. In the agentic variant the score also refuses a round made with a pattern
substitution script, which an earlier void run had used. In the batch variant no
script can run.

**10. Hand or reviewer pass.** Reviewer sessions read a sample of inputs against their
final rewrites and return one verdict each: faithful, minor drift, wrong, or data
altered. On 100 random segments of the first batch the verdicts were 90 faithful, 9
minor, 0 wrong and 1 data altered, the `<h4>` above. The pass finds what the checker
cannot see. No fix in the first batch was made by hand. Every finding became either a
new guard or a repair round.

**11. Assembly** (`ste_batch.cmd_assemble`, `ste_convert.unmask`, `ste_convert.apply`).
A trajectory converts only if every one of its segments passes the checker and keeps
its literals. Each rewrite, with its code restored, is written into the row at its own
message and field, and nothing else in the row moves. The first batch converted 196
of 200 trajectories. A check over all 196 found only the agent's own messages changed,
no placeholder left and no tag that the original did not hold.

## 4. The thesaurus

WeaverTrain's `ste/thesaurus.toml` holds 48 verbs that software text uses and STE100
does not approve, in 55 senses, each with the approved words to use and one rewrite
that the checker passes. Today it is a reference and a test fixture: it is not in the
prompt, and nothing replaces a word from it. One table reaches the loop. The fixed
mappings in `ste/fixed-mappings.toml`, which today hold only the ruled "call" to "use",
make the checker's repair note name the replacement instead of the standard's
alternatives. Every correction today is therefore an offer that the model restates.

The proposed autonomic path tags each entry with the path it takes. An entry that is
unambiguous, one sense and one replacement that fits every context the checker can
see, is tagged **splice**: the harness replaces the word in place, with no model turn.
An entry that is ambiguous, several senses or a replacement that needs the sentence
rebuilt, is tagged **offer**: the harness returns the sentence to the model with the
entry's senses as a suggestion, and the model restates it. A splice costs more than it
looks on this model family, because the SPU's `nemotron_h_moe` family declares
`ReestablishAndReprefill` as its flush. A splice on a hybrid recurrent model cannot
truncate to k, it rebuilds the state and re-prefills, while an offer appends and keeps
the prefix. The tag is per entry, kept with the entry, and changed only by a ruling.

## 5. What the trace records

For every correction the trace holds the model's emission as it came, the correction
as its own event, the position (message, field and character span), the rule the
emission broke, and whether the fix was a splice, an offer or a hand fix. Under
CLASS-10 the rejected original stays on the trace and the corrected emission is an
ordinary one, and there is no training-pair duty inside the agent. Pairing an
original with its correction is WeaverAnalysis's work, read from these events. That
pairing is what turns the loop's own trace into the next round's training data.

Today the same facts live in files rather than a trace: each round's output, each
repair file with its notes, the batch results as returned, and a run record with the
dataset revision, prompt hash, counts and output hash. The position is the segment id
and its locator. The fix kind is always offer today, since nothing splices.

## 6. The bootstrap plan

Gemini's 196 converted trajectories train a LoRA adapter on Nemotron 3 Nano 30B A3B in
bf16, on attention `q/k/v/o_proj` and Mamba `in_proj`, the configuration WeaverTrain's
smoke test passed at 32K. The loop then runs locally with that model and adapter in
stage 5's place, converting more trajectories. Its output, filtered by the same
checker, guards and reviewer pass, trains the next adapter, and the rounds repeat.
Each adapter round is scored against one fixed held-out set that never enters
training. The proposed set is WeaverTrain's frozen 100-segment kit, whose trajectories
were excluded from the batch by uuid, together with its recorded reviewer verdicts for
Gemini and DeepSeek. A round that scores below the one before it is not used. The
corpus that results feeds the supervised fine-tune toward native emission, which
CLASS-7 selects among decoder candidates for STE100 emission fidelity, acceptance of
STE100 input and use of recall verbs.

## 7. The model swap

The loop is developed against a small local model and moved to Nemotron by changing
the declaration's model binding, with no change to the loop. The SPU has a
`nemotron_h_moe` family entry whose template the marker probe measured against a
Nemotron 3 Nano 30B A3B artifact. Whether that hybrid Mamba MoE architecture completes
a turn end to end on the SPU is unconfirmed, as is whether the SPU loads a LoRA
adapter for it. The SPU's STE100 duty (`crates/weaver-spu/src/ste100/`) states that its
production adapters and artifacts remain held. WeaverTrain trains in bf16 through
transformers and PEFT, and the SPU's default backend is GGUF through llama.cpp, so the
adapter's path from one to the other is part of the same question.

## 8. Open cells

**Grouped against single-text fidelity.** Whether twenty texts per request change
what the model writes. Settled by running one kit both ways and comparing pass counts
and reviewer verdicts. The live check so far: 80 then 100 of 100 grouped, against 81
then 99 of 100 for the agentic session on the same kit.

**The splice tag per thesaurus entry.** Which entries can be replaced with no model
turn. Settled per entry by splicing it on the held-out set and counting the splices a
reviewer judges faithful.

**The restatement retry cap.** CLASS-9 asks for a measured cap. Settled by the pass
curve over rounds, which in the first batch was 89.2, 99.2, 99.8 and 99.9 percent.

**Local rewriter quality.** Whether the adapter matches the remote model. Settled by
the held-out score of each adapter round against Gemini's.

**Drift across adapter rounds.** Whether training on the loop's own output compounds
its minor drift. Settled by the held-out faithful count per round.

**Checker blind spots.** What the checker passes that a reader would not. Settled by
the count of reviewer-found defects per hundred checker-passed segments, round by
round. The first was the invented `<h4>`.

**Splice cost on a hybrid model.** Settled by measuring the re-prefill time per splice
under `ReestablishAndReprefill` on the Nemotron artifact.

**Sequence length.** Whether the 32K limit can rise. Settled by the trainer's peak
memory at 64K on the two cards.

**Input at the gate.** GATE-1 and CLASS-4 make STE100 input the condition of exchange,
while the training data keeps issue text natural under the scope candidate. Settled by
the trained model's task success with the issue converted against unconverted.

**Hedges.** The rewrites turn "seems" and "appears" into "possibly", which reads less
sure. The hedge forms await a ruling. Settled by the reviewer drift count on hedged
sentences under each candidate form.

**The SPU on Nemotron.** Settled by one turn end to end on the Nemotron artifact
through the declaration, then one turn with the adapter loaded.
