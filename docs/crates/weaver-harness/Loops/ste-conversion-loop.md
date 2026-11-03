# STE conversion loop

ASD-STE100:9 thesaurus: 022df3d809593696766893bf95f915229628497b89ef571dbc997ac8585224da

**Version:** 0.1, 2026-11-02
**Document ID:** `ste-conversion-loop`
**Parent:** `weaver-harness-PRD`
**Landing PR:** #442
**Standard:** `ASD-STE100`, Issue 9, 2025-01-15
**Lexicon:** `WeaverTrain ste/`, `ab63a918b569e5b8673dc2e78e66575ee9df1bd7`
**Thesaurus digest:** SHA256 of `ste/thesaurus.toml`, in the header above
**Standard copy SHA256:**
`d1f4ea9e7cd6e46b47aa9057209f99e78c0e9cfc4e27a5b07895b05c1a166431`
**Approved words SHA256:**
`c622bc230bc0c29512ef16990f527bf32a5ffaffbe955f73751251a7e5ab4693`
**Source sketch:** [#431, `361b5595`][sketch]
**Amendments:** [#439, `0fffb72a`][pr439], [#441, `5f74db87`][pr441]
**Epic:** [#437][epic], under [#423][parent]

## 1. Authority and terms

**1.01. Authority.** This document gives the requirements for this loop's code.
Cite the applicable numbered clause in each part of that code.
For an organ change, cite its Spec.
First merge any necessary amendments to that Spec.
`Ruling T`, [#438][i438], gives this authority.
The process document change stays with the Planner.

**1.02. Source.** The [sketch][sketch] supplies rulings A to T.
Rulings A to G have the date 2026-11-01.
Rulings H to T have the date 2026-11-02.
The amendments in the header supply the requirements for the organs.
Use the later rulings where they change the sketch.
The [2026-11-02 operator rulings on #442][round-rulings] set the four choices for
this workflow.

**1.03. Record.** Keep the sketch at its original path.
Mark it with this document's path, PR and date.
This new file has the location from rulings E and H, [#437][epic].
The [2026-11-02 correction][retirement] applies Sketch Convention section 7 to it.

**1.04. Terms.** Use "restatement" only for repair after the checker's notes.
Use `preface, then solve` for a statement of the input before the solution.
Use "the offer and the model's re-emission" for the case with several senses.
These are Q4, [#437][q14], with its [correction][q4rename].
There is no Q5 in that series, as the [operator correction][retirement] states.

**1.05. Rounds.** An adapter round uses one model version and one measured result set.
A driver `repair pass` requests a corrected text within that round under clause 6.04.
An `offer pass` requests the use of a thesaurus entry by the model.
Keep an independent count for each.

**1.06. Writing.** Use at most 20 words per instruction and 25 per description.
Use placeholders for clause names, identifiers, paths and backticked technical names
during the check.
Use the pinned lexicon and the local standard copy in the header.
Record the sentence totals, accepted count and each failure rule in the PR.
This applies the [STE100 decision on #437][ste-ruling].
Do not add lexicon entries for this document.

## 2. Purpose and limits

**2.01. Purpose.** Change agent prose from `nvidia/Nemotron-SWE-v1` trajectories to
STE100.
The output supplies training data and the agent's first production job.
The loop operates locally through the CLI for the agent's life.
It has no front end and is not a benchmark.
These are rulings D and M, [#437][epic].

**2.02. Text scope.** Write the narration in each assistant message in STE100.
Write the final report in the `finish` call's `message` argument in STE100.
Keep all other arguments and tool-call data unchanged.
Keep code, tool output, issue text and `think` fields byte-identical.
`Ruling O`, [#437][epic], sets this scope.

**2.03. Input.** Accept only input the operator controls.
An external driver starts each turn through the gate.
The agent starts no external turn itself.
STE100 input classification is absent for this loop.
These are `ruling M` and Q6, [#437][c-q67].
The safety seam and public input are outside this loop.

**2.04. Declaration.** Select the replacement path for the service.
Do not make this loop part of `dev_loop`.
Do not include `state-management.ste100` in this loop's declaration.
Keep its tool set empty and send no tool request.
Select the thesaurus in `state-management.flow.thesaurus`.
`Ruling C`, [#434][i434], and Q3, [#437][q14], remove classification and the `STORE-2`
function.
Q7, [#437][c-q67], removes the tool-dispatch function.
An attempted tool-call record stays on the trace and grants no action.
Without `state-management.ste100`, admin refuses tools with `ConfigInvalid` and the
field name.

**2.05. Bootstrap.** This loop is a simple bootstrap with one job the operator can
grade.
The full agent's classifier requirements apply when a later loop selects that function.
One election covers both input and output classification in the merged amendments.
Independent elections for each direction stay a later loop's question.
This is the reason for Q6 and Q7, [#437][c-q67].

## 3. State, prompt and load

**3.01. Held tables.** Keep the system prompt, verb list and thesaurus in state.
The agent can read these tables and cannot change them.
Keep them across shutdowns.
Only the operator changes a version through admin under a decision.
This is `ruling B`, [#433][i433], and Q2, [#437][q14].

**3.02. Artifacts.** Use [Spec 02][spec02] `SCHEME` and `THESAURUS`.
Use one verb artifact for every judge.
Make `ste100-thesaurus.json` deterministically from the source in the header.
Record its source commit, path and SHA256 with its own digest.
Keep its `scheme_id`, entry sequence, senses, uses and tags.
Use format `ste100-thesaurus/1`.
This is C1, [#437][c-q67], with the [source correction][c1fix].
Work on these tables stays in this repository.
WeaverTools work stays paused.

**3.03. Tags.** Each entry has a `splice` or `offer` tag.
A `splice` entry has exactly one sense and one use.
An `offer` entry can have one or more senses.
Start with an empty splice table and tag every entry `offer`.
Send each lookup result to the offer path until the operator rules a splice tag.
Only an operator decision changes a tag in a new version.
The first splice tags stay open until measurement after round 1, under clause 10.11.
These are `ruling B`, [#433][i433], and `THESAURUS`.
The initial empty table follows [decision 3 on #442][round-rulings].

**3.04. Versions.** Keep each installed version and a current pointer in
`state-content/2`.
Keep the history and prevent writes to these tables during service.
Use [STATE-GOVERNING][state-spec] for the table representation.
This is C2, [#437][c-q67].

**3.05. Load.** Read the scheme and thesaurus from the bound save point.
Compare their digests with the declaration before load.
Send those exact bytes at enter.
Do the comparison again against the image's current versions in `STATE-ADOPT`.
Refuse a mismatch.
Do not use root files to seed the state again during load.
These are C3, [#437][c-q67], and Q8, [#437][q8].

**3.06. Save point.** Keep `state.sqlite`, `agent.toml`, `binding.json` and
`ste100-scheme.json`.
Keep `ste100-thesaurus.json` where selected, as necessary for this loop.
Keep their digests in `binding.json` under `PERSIST-FORMAT`.
Copy the active files to each later publication.
Admin reads these files and does not use SQL.
Q8, [#437][q8], keeps the files and table versions equal.

**3.07. Install.** Install a new table version as root while the agent is not loaded.
Supply the required `--ruling` to the admin install verb.
The install publishes a new save point.
Then edit the declaration with `sudoedit` and use `bind-state`.
The next load refuses until the declaration and image digests agree.
Use [ADMIN-INSTALL][admin-spec] for this sequence.
These are C4, [#437][c-q67], and its [accepted sequence][q8].

**3.08. Prompt.** Use the merged seed path through `deploy/turn.py --system`.
The state member holds the prompt and seats it on each later load.
Supply its digest and the operator's review reference.
Use `SEED-CERTIFICATE` with no bindings and no checker artifact digest.
The scheme, duties and prompt digest checks still apply.
Q7, [#437][c-q67], accepts this seed without a classifier.
C5, [#437][c-q67], keeps the prompt path unchanged.
A new prompt version through admin stays owed under [#440][i440].

**3.09. Templates.** Use the compiled template inventory and record its digest at load.
Apply the structural checks in `INSTRUCTION-METADATA` to every expansion.
No checker starts, and no `TemplateCertificate` is required or issued.
For this declaration, `ste100-binary` is not necessary in the admin inventory.
The scheme stays necessary.
These are Q7, [#437][c-q67], and [Spec 02][spec02] `TEMPLATES`.

## 4. Driver procedure

**4.01. Driver.** Keep sample selection, placeholder substitution, request construction
and assembly outside the agent.
The driver also scores results and starts the manual review of the semantics.
The driver checks literals at assembly and starts shape retries through new gate turns.
Send one text per request through `deploy/turn.py`.
`Ruling P`, [#437][epic], sets this single-text baseline.
Do not start grouped requests before the measurement in clause 10.04.

**4.02. Sample.** Read dataset rows in a recorded seeded sequence.
Select only rows with a final report.
Use at most 6,000 characters per narration or report after placeholder substitution.
Limit the complete row to 32,768 tokens under Nemotron's chat template.
Identify each row by its line number.
Keep one row per UUID and do not use the rewriter-selection kits.
Record the dataset revision, sample seed, row IDs and exclusions.

**4.03. Bias.** The length limit selected about one row in ten in the source experiment.
The held-out kit also selected Gemini as the rewriter.
Record both biases with each comparison.
Do not report the number of accepted checker results as a semantic score.

**4.04. Mask.** Use the source placeholder rules from `ste_convert.mask` in the [pinned
scripts][train].
Replace the protected spans with `[[Cn]]` placeholders before the model or checker reads
them.
Keep the exact spans in the driver's codes file.
Use placeholders for fenced code, inline code, URLs, paths, filenames and `Call` text.
Use placeholders for bracketed literals, dotted names, snake_case names and string
quotations.
Use placeholders for HTML tags and exception messages through the line end.
Use placeholders for padded program listings and Markdown blockquotes.
Blockquotes are quotations and stay unchanged.

**4.05. Request.** Use the system prompt seeded by clause 3.08.
The state member seats this prompt on load.
Send only the text after placeholder substitution in each initial request.
Do not copy the system prompt into a request.
Use clause 6.04 for repair requests with checker notes.
The prompt source includes approved verbs, technical verbs, the fixed mappings and four
examples.
Record the prompt hash with each rewrite.
Request one rewrite with all placeholders intact.
For the single-text baseline, collect one text response for that segment ID.
The source `unit_outputs` uses raw text for this case.
Its grouped JSON format is not the single-text response format.

**4.06. Collection.** Record each request, response identity, finish reason and token
use.
Keep missing output and parser faults in the result record.
Do not include the model thought-block data in the candidate rewrite.
Keep that data unchanged on the trace.
Collection makes no semantic judgment.

## 5. Agent procedure

**5.01. Start.** Use `AcceptedInput` and `HARNESS-LOOPS` for the gate input.
Open one external turn from that input.
Record the model emission `e` without changes.
Set the original answer's `Y` to its decoded text before any recorded change.
The gate sends `Y`, the `EffectiveEmission`.
This is `ruling A`, [#432][i432], and Q1, [#437][q14].

**5.02. Detect.** Read the held verb list and thesaurus.
Apply the checker's word-form and position rules to the current text.
Distinguish a verb use from a noun or adjective use.
A noun entry must not disable the verb or `-ing` check for a verb use.
This applies `ruling C`, [#434][i434], and the [#433 noun condition][noun].
Detection reports a condition and changes no text.
Use Rust for the agent's checks.
Use the pinned Python checker only as the reference and in the external driver.

**5.03. Match.** Use the private lookup constructor in `LOOKUP-OFFER`.
Find the word in the current recorded version of the original model `Text`.
Keep the source, `previous_change`, scheme identity and thesaurus digest.
Use nonempty half-open UTF-8 byte spans in that version.
Keep the word span inside the sentence span.
Public `ThesaurusMatch` data is a record and grants no operation.

**5.04. Word forms.** Use exact held forms and the pinned checker's lemma candidates for
lookup.
If the form of the word is ambiguous, do not make a splice.
Use only the deterministic replacement for the form of the word under the entry
decision.
Keep case and inflection requirements in the per-entry measurement of clause 10.11.
Without a decision for the form of the word, do not make a splice.

**5.05. Splice.** Use the current `splice` lookup result with exactly one sense and one
use.
Make this path with the initial empty table in clause 3.03.
Replace only its word span with the deterministic text for that use.
Use `OUTBOUND-CHANGE` to record the proposed change before its use.
This operation has no model execution.
`Ruling B`, [#433][i433], sets this path for approved splice entries.

**5.06. Offer.** Use the named `offer` variant for the current `offer` lookup result.
Use the exact sentence and every held sense and use in the artifact sequence.
Expand only `thesaurus-offer/1` with its typed `thesaurus_offer` material.
Do not supply new prose, senses or caller-selected roles.
Before record creation, check the source version, `RunId`, turn and loaded artifacts
again.
Follow [Spec 02][spec02] `LOOKUP-OFFER` and [HARNESS-LOOKUP-OFFER][harness-spec].
The [2026-11-02 decision on #437][retirement] has no requirement for a classifier for
this path.

**5.07. Offer record.** Record `loop.instruction` before argument placement or model
execution.
Set its causal parent to the original model source sequence.
Wait for `Recorded` before the `offer pass` starts.
Connect that instruction to its exact `AppendAndGenerate` transaction.
Record the response with its instruction sequence as causal parent.
Use only that transaction's private result for a proposed change.
Source IDs alone grant no change, also IDs from another offer in the same turn.

**5.08. Offer result.** Apply the language and placeholder checks to the candidate
sentence.
Use only a result with no failure from these checks.
The driver checks literals at assembly under clause 6.02.
Select an exact span from the recorded `Text` in that result.
Propose its replacement of the matched sentence through `OUTBOUND-CHANGE`.
Keep an unused or malformed result on the trace without a change to `Y`.
An offer or splice is not a shape fault.

**5.09. Answer.** Keep the offer generation subordinate to the original answer.
It has no independent `EffectiveEmission` or client answer.
Keep the original answer's source and finish.
Use that answer's changed `Y` for the gate and turn closure.
An unused offer response is not a fallback answer.

**5.10. Offer limit.** Read `offer_pass_cap` from the operator's round parameters.
The operator sets this cap at each round start.
Use this parameter as the workflow's elected value.
Supply that exact value to the private turn owner.
Use no default value.
Use `offer-yield-cost` in clause 10.20 to select later values.
Keep the positive `u32` value unchanged for the complete external turn.
Count across all original emissions, Text-block data, matches, retries and offers.
Use the private turn owner from `LOOKUP-OFFER` for every reservation.
Do not use `repair_cap` or `MAX_TOOL_ROUNDS` as this limit.
This is [decision 4 on #442][round-rulings].

**5.11. Offer accounting.** Reserve one allowance before instruction creation.
Refuse if consumed plus reserved allowances are equal to or larger than the cap.
Also refuse if no cap is supplied.
Use `invalid_context` with the cap, consumed count and reservations in the detail.
A recorded instruction consumes its reservation once before model execution.
Keep reservations during writer pressure.
Release a refused reservation only when the recorder accepted no instruction.
Release a cancelled reservation when the recorder accepted no instruction.
Keep an admitted reservation until its outcome is accounted for.
Never refund a consumed allowance, also for malformed or unused responses.
Do not start the count again for a new lookup result or answer generation.
A late `Recorded` instruction consumes its allowance and grants no continuation.
With no cap and an empty splice table, record lookups without a change to `Y`.
Send language failures to driver repair under clause 6.04.

**5.12. Change.** Use `EmissionChangeRecord` for each proposed change to `Y`.
Keep the original source and the previous change identity.
Keep the exact replaced and replacement text with the match and method records.
Use offsets in the previous recorded version, also after a length change.
Keep the other Text-block data and the `OUTPUT-UNIT` LF separators unchanged.
Wait for `Recorded`, then apply the change once.
Refuse a stale head or a second incomplete change to that version.
Do not branch the chain.

**5.13. Failure.** Keep the old `Y` while a change is incomplete or refused.
Keep the same incomplete record during writer pressure.
Use the merged operation refusals, deadlines and author limits.
`Stop`, `leave` or flow failure grants no new decode or changed output.
A late recorded result during teardown grants no continuation.
Follow `HARNESS-OUTBOUND-CHANGE` for the exact refusal cases.

**5.14. Context cost.** A change to the completed `Y` alone keeps the model buffer
unchanged.
If a splice changes a later argument at position `k`, the reusable prefix ends at `k`.
Keep only the prefix before that position under the family's session rules.
For `ReestablishAndReprefill`, decode the complete kept prefix from position zero.
An offer appends to the argument and keeps the prefix.
Measure the reached path under clause 10.16.

## 6. Checks, retries and assembly

**6.01. Language.** Apply `ste_check.check` with the pinned lexicon and limits in clause
1.06.
Check verbs, words, `-ing` forms, contractions, capitals and placeholders.
Check for a copied example, excessive word growth or loss, and new markup.
Use the source `Converter.guard` and `Judge` rules for those extra checks.
Keep the failed rule names with each result.
These checks do not supply a classifier election or a `STORE-2` grant.

**6.02. Literals.** Keep the glued literals unchanged, such as `145.[[C8]]` and
`[[C4]]([[C5]]`.
Do this check in the driver at assembly.
Slash-joined placeholders can become independent placeholders if each count stays
correct.
Refuse assembly if a literal is different from its source after placeholder
substitution.
This site is [decision 2 on #442][round-rulings].
Clause 10.02 keeps a later comparison of sites.

**6.03. Shape strikes.** Count a parse failure or lost placeholder as one shape fault.
The driver checks the shape and sends each retry through a new gate turn.
Missing output from a completed answer is a parse failure.
Use at most three such emissions for one text, then drop that text.
For a completed answer with missing output, request a fresh rewrite only before the
third shape fault.
This clause grants no retry after a transport or lifecycle failure.
Keep the shape count for that text across new gate turns.
Keep all three emissions and their identities on the trace.
Count drops separately for every round.
Do not count vocabulary splices or offers as shape faults.
These are rulings Q and S, [#437][epic].
The driver site is [decision 2 on #442][round-rulings].
Clause 10.03 keeps a later comparison of sites.

**6.04. Repair.** The driver owns each `repair pass` for a language failure after the
shape checks accept the text.
Keep the failure text and the checker's notes.
Send the original text with placeholders, its last rewrite and checker notes through a
new gate turn.
Keep one original text per request.
The agent does not start `REPAIR` under `OUTPUT-UNIT` without a classifier.
Use "restatement" only for this repair, under Q4, [#437][q14].
The named `offer` path accepts only the thesaurus case in clause 5.06.
Use clause 6.03 for shape retries in the driver.
Apply the limit in clause 6.07.
The measured repair cap stays open in clause 10.12.

**6.05. Review.** Compare a recorded random sample of source texts with their final
rewrites.
Use the verdicts `faithful`, `minor drift`, `wrong` and `data altered`.
Record the sample seed, size and each verdict.
Turn each defect into a guard or a repair request.
Do not correct dataset text by hand.
Count reviewer defects per hundred checker-passed segments.

**6.06. Assembly.** Accept a trajectory only when every eligible prose segment has no
check failure.
Use the driver's literal check in clause 6.02 before assembly.
Replace each placeholder with its exact span from the codes file.
Put each rewrite at its original message and field.
Keep all fields outside the scope byte-identical.
Check for remaining placeholders, new tags and changes outside eligible prose.
Record the output hash and the accepted and refused trajectory counts.
Assembly changes a dataset row and does not change a model argument.

**6.07. Driver repair limit.** Read `driver_repair_pass_cap` from the operator's round
parameters.
The operator sets this cap at each round start.
Do not start conversion without an explicit count.
Use no default value.
Apply this cap to each original text.
The initial conversion request does not consume this cap.
Count each driver language-repair request before its gate turn starts.
Keep the cap and count across shape retries and new gate turns.
Do not refund a request for malformed or unused output.
Apply clause 6.03 to malformed responses.
At the cap, send no further language-repair request.
After applicable shape retries, record `language_unresolved` if the last text still has
a language failure.
Use `language_unresolved` only for a text with no shape-drop result under clause 6.03.
Keep that text out of assembly.
Keep its source, all model emissions and checker results.
Report `language_unresolved_count` beside the shape-drop count.
Use clause 10.12 to select the measured cap from yield and cost.
This is [decision 4 on #442][round-rulings].

## 7. Trace and quality decisions

**7.01. Evidence.** Keep each raw `e` and all recorded changes that make `Y`.
Keep source identities, rule failures and the method for each correction.
Keep message, field and segment locators in the driver's result record.
Connect these locators to the trace's source identities and UTF-8 spans.
Keep the dropped emissions without a correction.
`Ruling A`, [#432][i432], and `ruling Q`, [#437][epic], set these record requirements.

**7.02. Ownership.** WeaverAnalysis makes training pairs from the trace.
The agent makes no training pairs and adds no rejected-emission kind.
Drops have no correction and are not training pairs.
If a later loop selects a classifier, it reads `e`.
If that loop files by verb, its verb keys come from `Y`.
The store files only `Y` under that later election.
This loop files nothing by verb.
These are Q1, [#437][q14], and `CLASS-10`.

**7.03. Calibration.** Round 1 is calibration and has no keep-or-drop score.
Measure its success rate and the severity of its failures for the operator.
Before round 2, record the operator's maximum permitted decrease in faithful count.
From round 2, reject a round whose decrease from the prior round is larger than that
margin.
`Ruling Q`, [#437][epic], sets this sequence.

**7.04. Kill line.** Count shape drops and `language_unresolved` texts together in the
kill-line result.
The kill line is a combined loss rate of 20 to 25 percent.
Use a sample size the operator judges sufficient.
Do not judge it on the first few texts.
Report the shape-drop count and `language_unresolved_count` separately beside the
combined rate.
Record their combined lost-text count and the attempted-text count.
Count each lost text once in the numerator.
Count each original text once in the denominator.
Include both lost-text classes in that denominator.
The threshold within that range and sample size stay open in clause 10.06.
Rulings Q and S, [#437][epic], set this range and denominator.
The [operator's decision 1 on #442][round-rulings] extends `ruling S` to both lost-text
classes.

**7.05. Bias.** Report the faithful count beside both lost-text counts for every round.
Shape drops and `language_unresolved` texts can remove hard texts and favor easy ones.
Keep the held-out item set unchanged for comparisons.
Report missing items, shape drops and `language_unresolved` items.
Keep them in the record.
This applies [decision 1 on #442][round-rulings] to both lost-text classes.
The source kit has 100 segments and favors its selected remote baseline.
Near 80 percent faithful, its standard error is about 4 percentage points.

**7.06. Corpus.** Keep round 1's accepted output in its own set until the operator rules
on its use.
Calibration alone grants neither its inclusion nor its exclusion from the next corpus.
Clause 10.07 holds that open decision.

## 8. Training sequence

**8.01. First adapter.** Train the small LoRA adapter on the two rewritten batches, 387
complete trajectories.
Use Nemotron 3 Nano 30B A3B for this adapter.
It is an emitter of narration and final reports in STE100.
Do not make a rewriter adapter in this step.
These are rulings J and K, [#437][epic], and the [emitter decision][emitter].

**8.02. Data identity.** The first batch has 196 trajectories.
Its `converted.jsonl` SHA256 is
`63afa6fe3aa1f44cb98355e6717b073921c2ee1ce1451a93acf0aae20e349ad9`.
The second batch has 191 trajectories.
Its `converted.jsonl` SHA256 is
`9bd8bac719156498c4f2ac7916b0afae85aa0ac909ad9cece95b5fe48fbe23a5`.
Each deposit's `run.json` supplies its hash under `ruling K`, [#437][epic].

**8.03. Local rewrite.** Put that adapter at the model-turn stage of the local loop.
Use one text per request under `ruling P`, [#437][epic].
Measure its rewrite ability under clause 10.01.
The emitter decision keeps that question open.
Use lookup, splices and offers for vocabulary corrections under `ruling B`,
[#433][i433].

**8.04. Full training.** Use the accepted correction corpus for a full SFT.
Then remove the adapter.
Make autonomic functions after reliable native STE100 output is measured.
The reliability measure stays open in clause 10.21.
Rulings J and N, [#437][epic], refine [WeaverTrain #2][train2] rulings 1 and 2.
The adapter is the bootstrap instrument, not untrained self-conversion.

**8.05. Preface.** Keep `preface, then solve` out of the first adapter's data and round
1.
Introduce it only in round 2 or later.
Its mechanism stays open in clause 10.08.
These are rulings L and R, [#437][epic], with Q4, [#437][q14].
The rename also applies to [WeaverTrain #2][train2] rulings 6 and 9.

## 9. Machines and qualification

**9.01. Split.** Make and qualify the loop on the thinkpad while olympus trains the
adapter.
Use Qwen 8B at any quantization as the stand-in.
`Ruling F`, [#437][epic], set completion by the end of the week of 2026-11-02.
The initial training estimate was one to three days from 2026-11-02.
Record measured completion dates with the results.

**9.02. Hybrid path.** Also use `nvidia/NVIDIA-Nemotron-3-Nano-4B-GGUF` at `Q4_K_M` on
the thinkpad.
Read its `general.architecture` key from the downloaded artifact.
Do not infer that key from the model name.
A 4B family entry can come from the thinkpad under `ruling I`, [#436][i436].
`Ruling G`, [#437][epic], makes this the first target of [#382][i382].
If the key is `nemotron_h`, test MoE routing on olympus.
The remainder of the olympus lane stays unchanged.

**9.03. Cards.** Complete an end-to-end test on the thinkpad before the transfer to
olympus.
Use the gate, the state member, the trace and the SPU.
The thinkpad uses the RTX PRO Blackwell laptop, `sm_120`, with CUDA 13.4.
Olympus uses the A6000 pair, Ampere, `sm_86`.
Use the same loop on olympus with the declaration's new `model-binding` value.
Put the result records under `weaver-testing/` and commit only the result note.
These are `ruling F`, [#437][epic], and the [hardware record][hardware].

**9.04. Adapter load.** Qualify the path from bf16 PEFT training to a GGUF LoRA under
[#435][i435].
The classifier's model adapter is not a LoRA load path.
The sketch records the missing LoRA load path as a Spec gap.
Its amendment and implementation are outside this workflow PR.
Measure complete turns on the target model under clause 10.19.

**9.05. Render.** Compare trainer tokens with SPU tokens for one held-out prompt.
Record the template, `enable_thinking`, tools choice and `max_seq_length`.
Keep `enable_thinking` off for the initial comparison.
Compare the trajectory template output with tool schemas and the rewrite template output
with no tools list.
Record each downloaded 4B file's SHA256 and machine.
The [emitter decision on #437][emitter] sets these checks.

## 10. Ruled sites, open cells and measurements

**10.01. Adapter job format.** Measure parse failures and placeholder losses per
single-text request on the first local test.
This gives the result for the emitter adapter's rewrite ability.
Keep independent records for vocabulary correction and shape failure.

**10.02. Literal site.** Driver assembly is the ruled literal-check site under clause
6.02.
The [operator's decision 2 on #442][round-rulings] closes this site choice.
Keep a later site comparison under the measurement `literal-site-cost`.
Compare the outbound sort with driver assembly as the literal-check site.
Count the detected literal changes and released invalid texts at each site.
Measure the time per text.
For a later site change, first record the operator's decision and merge any required
Spec amendment.

**10.03. Retry site.** The driver sends shape retries through new gate turns under
clause 6.03.
The [operator's decision 2 on #442][round-rulings] closes this site choice.
Keep a later site comparison under the measurement `shape-retry-site-cost`.
Compare driver retries through new gate turns with retries inside the same agent turn.
Measure shape recovery, time, token cost and completeness of the three-emission trace
per text.
Keep the three-strike bound unchanged in both comparisons.
For a later site change, first record the operator's decision and merge any necessary
instruction-path amendment.
No open cell grants a constructor that the merged Specs forbid.

**10.04. Group size.** Measure missing-text counts by group size against the single-text
baseline.
Keep the harness unchanged while only group size changes.
The source comparison changed both the group size and the harness.
It gives no answer for this cell.
Also compare held-out semantic verdicts for each group size.
The source grouped checker accepted 80, then 100 of 100 texts.
Its verdicts were 77 `faithful`, 21 `minor drift`, 1 `wrong` and 1 `data altered`.

**10.05. Drift margin.** Measure faithful count and failure severity on the fixed
held-out set for round 1.
The operator sets the allowed decrease before round 2 from those results.
Compare each later adapter round with the prior round and Gemini.
Keep independent records for the checker success rate and faithful count.

**10.06. Kill sample.** Measure the combined loss rate and its uncertainty as the
attempted-text sample increases.
The operator sets the sufficient sample size and threshold within 20 to 25 percent.
Keep shape drops and `language_unresolved` texts in that rate under clause 7.04.
Report both counts separately beside the combined rate.
These are rulings Q and S, [#437][epic], with [decision 1 on #442][round-rulings].

**10.07. Calibration corpus.** Measure faithful count, shape losses and semantic defects
in round 1's accepted output.
The operator rules on its use in the next corpus from those records.
Do not infer permission from an accepted checker result.

**10.08. Preface mechanism.** Compare candidate preface mechanisms from round 2 or
later.
Measure faithful count, shape failures and token cost against the round 1 base rate.
Keep the preface outside the target rewrite for placeholder and assembly checks.
The operator rules on the mechanism from these results.

**10.09. Think classification.** Count the family content by block kind and examine
their declared outbound status.
Record the blocks received by a proposed classifier.
The operator sets the scope of `CLASS-10` through its own PRD amendment.
`Ruling O` still keeps dataset `think` fields unchanged.

**10.10. Cards.** Complete one end-to-end deposit per card with the declared model.
Record the thinkpad result first and the olympus result second.
The deposits give the results for the two card cells.

**10.11. Splice tags.** For each candidate entry, test all forms of the word as splices
on the held-out set.
Count replacements the reviewer judges faithful and record each failure context.
Include case and inflection variants in that measurement.
The operator rules on the first tags and their exact deterministic forms from
measurement after round 1.
Keep this cell open and start with the empty splice table in clause 3.03.
Keep all unruled entries and forms out of the splice path.
This is [decision 3 on #442][round-rulings].

**10.12. Repair cap.** Measure the cumulative success count, faithful count, tokens and
time after each `repair pass`.
The operator selects the measured repair cap from that curve.
Start with the operator's round parameter under clause 6.07.
Name this measurement `repair-yield-cost`.
The source success rates were 89.2, 99.2, 99.8 and 99.9 percent over four cycles.
This cell does not change the three shape faults or `offer_pass_cap`.

**10.13. Local quality.** Measure held-out faithful count for each adapter round against
Gemini's count.
Use the same texts and review method for each comparison.
This gives local rewrite quality independently of the checker success rate.

**10.14. Checker defects.** Count reviewer defects per hundred checker-passed segments
in each round.
Keep a defect's source, output and missed condition.
The source run's invented `<h4>` is the first recorded example.
For each new code check, measure its detection of that defect class.

**10.15. Sequence length.** Measure trainer peak memory at 64K on the two cards.
Compare successful complete rows with the 32K baseline.
Keep the 32,768-token limit until the operator accepts a new bound.

**10.16. Splice cost.** First count splices that change a later argument in a kept
session.
If any occur, measure re-prefill time per splice on the Nemotron artifact.
Record the family's flush policy and kept-prefix length.
The measurement gives the cost under `ReestablishAndReprefill`.

**10.17. Hedges.** For each candidate, count reviewer reports of changes to the
semantics of hedged sentences.
The operator rules on the accepted forms.
Do not change uncertainty to certainty or advice to a requirement.

**10.18. Render choices.** Measure token differences for the two template outputs in
clause 9.05.
Record differences in template, tools, `enable_thinking` and sequence length.
Test `enable_thinking` enabled only under a later decision.
The comparison gives compatibility with the training record.

**10.19. Nemotron and LoRA.** Complete a turn through the declaration on the Nemotron
artifact.
Do the test again with the adapter after the [#435][i435] load path is available.
Record model and adapter hashes, finish, output and trace completion.
These are independent measurements for the base model and adapter path.

**10.20. Offer yield and cost.** Name the measurement `offer-yield-cost`.
For each additional `offer pass`, count new checker-accepted sentences and
reviewer-faithful corrections.
Also record unused responses, tokens, decode time and total turn time.
Keep the input set, model, artifacts and other limits unchanged.
Start with the operator's round parameter under clause 5.10.
Use controlled later tests with each candidate positive `offer_pass_cap` set before the
turn.
The operator selects a later cap from marginal yield and cost.
No cap is a default supplied by the primitive.

**10.21. Later functions.** Measure native faithful output and correction frequency
after full SFT.
The operator sets the reliability criterion before autonomic work starts.
Input classification, independent direction elections and later detectors belong to
their own loop measurements.
This cell grants no autonomic action to this loop.

[sketch]: ../../../project/sketch-ste-conversion-loop.md
[spec02]: ../../state-management/state-management-spec-02-protocol-classifier-gate.md
[state-spec]: ../../state-management/weaver-state-Spec-amendment.md
[admin-spec]: ../../state-management/weaver-admin-Spec-amendment.md
[harness-spec]: ../../state-management/weaver-harness-Spec-amendment.md
[hardware]: ../../../../crates/weaver-spu/kernels/PROVENANCE.md
[epic]: https://github.com/toddwbucy/WeaverAgent/issues/437
[parent]: https://github.com/toddwbucy/WeaverAgent/issues/423
[i432]: https://github.com/toddwbucy/WeaverAgent/issues/432
[i433]: https://github.com/toddwbucy/WeaverAgent/issues/433
[i434]: https://github.com/toddwbucy/WeaverAgent/issues/434
[i435]: https://github.com/toddwbucy/WeaverAgent/issues/435
[i436]: https://github.com/toddwbucy/WeaverAgent/issues/436
[i438]: https://github.com/toddwbucy/WeaverAgent/issues/438
[i440]: https://github.com/toddwbucy/WeaverAgent/issues/440
[i382]: https://github.com/toddwbucy/WeaverAgent/issues/382
[pr439]: https://github.com/toddwbucy/WeaverAgent/pull/439
[pr441]: https://github.com/toddwbucy/WeaverAgent/pull/441
[round-rulings]: https://github.com/toddwbucy/WeaverAgent/pull/442#issuecomment-6590982515
[train]: https://github.com/toddwbucy/WeaverTrain/tree/ab63a918
[train2]: https://github.com/toddwbucy/WeaverTrain/issues/2
[q14]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6580579071
[q4rename]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6580617912
[c-q67]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6580990535
[c1fix]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6581175319
[q8]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6581478927
[retirement]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6586494090
[noun]: https://github.com/toddwbucy/WeaverAgent/issues/433#issuecomment-6581195193
[ste-ruling]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6579488631
[emitter]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6579865632
