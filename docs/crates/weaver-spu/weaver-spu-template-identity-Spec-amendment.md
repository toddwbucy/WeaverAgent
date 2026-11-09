# weaver-spu Spec: template identity amendment

**Version:** 0.1
**Date filed:** 2026-11-08
**Document ID:** `weaver-spu-template-identity-Spec-amendment`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.

## 0. Authority and scope

**TID-0.** This amendment fixes how admission decides which registry entry
serves an artifact. The operator ruled its content on 2026-11-08 (#483, option
a2, and the family segregation ruling of the same day). The ground is that a
wrong render is silent: a model given a prompt shape it was never trained on
degrades, and nothing in the record says why.

Amend SPU Spec sections 3, 5 and 10, as mapped below.
The grounds are SPU PRD sections 4.1 and 14.
These clauses authorize code after merge. They do not claim that the mechanic
exists at the source baseline. Until the TID-2 to TID-5 code lands, admission
keeps today's behaviour: an uncontested key selects on the architecture alone,
and a contested key selects by the detector's markers.

| Clause | Destination | Addition |
| --- | --- | --- |
| TID-1 | Spec 5 | One family, one copy: each family module holds its own template facts |
| TID-2 | Spec 5 | Every entry pins the digests of the chat templates it serves, with evidence |
| TID-3 | Spec 5, and Spec 3 for the account line | Selection is by digest for every architecture key |
| TID-4 | Spec 5 | What text is hashed, for each container |
| TID-5 | Spec 10 | Instruments |
| TID-6 | Spec 5 and 10 | Landing: what the code act pins |
| TID-7 | Spec 10 | Open cells |
| TID-8 | Spec 5 | The Spec 5 sentences this amendment replaces |

The baseline is main at `dc5f439d60865c418b951704eefb645839684ce3`.
The dependency pin is `toddwbucy/llama-cpp-rs` at
`ecce255bcb14dd6d88f184cc8776c23a85afafeb`, as recorded by Cargo.lock.

**Terms.** The *chat template* is the program the artifact carries (the GGUF
`tokenizer.chat_template` value, or the safetensors text TID-4 names). The
*turn template* is `Declaration::template`, the family module's placeholder
form (`<|im_start|>{role}\n{message}<|im_end|>\n` for ChatML). A digest is
always of a chat template.

**What this amendment does not do.** It adds no declaration key and does not
change the binding grammar. It changes no renderer: an admitted artifact is
rendered exactly as today. It adds no trace member (TID-7). It does not move
the existing cross-family sharing on main. That is #484, which TID-1 governs.

The source survey is the hub report "#483 survey: what select answers with the
uncontested shortcut removed" of 2026-11-08. Its second-reader review is the
hub report "TID amendment review" of 2026-11-08, which measured the sidecar
facts cited in TID-3 and TID-4. Both are evidence for this document. Neither
is normative.

## 1. One family, one copy

**TID-1.** The architecture key names the tensor layout, as Spec 5 already
says: a builder may train any layout against any chat format. The family
module follows the key. Each family module holds its own copy of its turn
template, its markers, its generation opener, its renderer and its set of
pinned digests (TID-2), and names no other family's module for them.

Duplicated text across family modules is intended. Each family can change its
chat format at any time, independently of every other family, so a change made
for one family's release must not move another family's render.

Measured on 2026-11-08: Granite 4.2 30B is IBM's own model (model card base
`ibm-granite/granite-4.1-30b-base`, vocabulary 100352), and
NVIDIA-Nemotron-3-Nano-30B-A3B is trained from scratch by NVIDIA by its model
card (vocabulary 131072). Both adopted ChatML turn markers, and both Granite and
Qwen3-Coder use XML tool calls (`<function=...>`). A shared ChatML renderer
would tie both to every Qwen change.

The registry may cite one module from more than one entry where those entries
are one family (the two `phi3` entries in `phi`, for example). Whether one
vendor's release lines are one family is ruled per vendor (TID-7).

**Interim home.** Until #484 moves them, an entry that cites another family's
module today (the `llama` ChatML entry and `nemotron_h_moe` cite `qwen2`, the
`llama` `[INST]` entry cites `mistral3`, and the four `qwen3*` entries cite
`qwen2`) keeps its pinned digests in a per-entry constant beside its row, in
the module it cites, named for the entry. #484 moves each set with its entry.

## 2. Pinned digests

**TID-2.** Every registry entry carries the set of chat-template digests it
serves. A digest is the lowercase hexadecimal SHA-256 of the text TID-4 names.
The set lives in the entry's family module (TID-1), and the registry cites it.

Each digest is pinned by a code act that records, beside the digest:

| Field | Content |
| --- | --- |
| Source | The artifact the chat template came from: repository and revision for a published artifact, or the workshop artifact's name |
| Evidence | One of the two forms below |

| Evidence form | What it shows | Closes substitution within a detector class |
| --- | --- | --- |
| Golden render | The chat template, rendered by its own reference engine over the family's golden conversations, matches the family's renderer byte for byte (the `granite` goldens and `regenerate.py` are the model) | Yes |
| Detector reading | The detector's class for the chat template at pinning time, and a probe render through the family's renderer | No. The detector collapses every `<\|im_start\|>` template to one ChatML class, and every `[INST]` template to one class, so a thinking variant, another opener or a Llama-2 `[INST]` template reads the same as the entry's own |

A golden render matches on the served path: the conversations that carry a
system message (a system and user turn, and a history), rendered in the mode
the entry's ruling or opener implies. A case off that path (a lone user turn,
a final assistant turn, a tool result) is not evidence and not a failure: it
is recorded beside the pin as the entry's named limit, with the reference's
rendering and the renderer's, so the census can read what the family does not
reproduce. The `granite` goldens are the model here too: their served-path
rows pin the digest, and their `user_only` and `tool_result` rows are named
limits.

A chat template the detector refuses (Gemma4's, at `ecce255`) can only be
pinned by a golden render.

So a digest makes every admission a reviewed act, and it closes the
substitution surface outright only where the evidence is a golden render.
Whether golden renders are mandatory for every pin, or for the ChatML and
`[INST]` classes, is the operator's ruling (TID-7).

A digest appears in at most one entry of an architecture key. This is a
compile-time property of the registry, for the reason the marker uniqueness
is, and the existing `const fn same_text` is the model. The same digest under
two different keys is allowed, because the key is judged first.

An entry with an empty set admits no artifact. At the baseline this is the
first `llama` entry (Llama 3), for which the survey found no instruct artifact
carrying a chat template. It stays registered and refuses until an act pins a
digest.

## 3. Selection by digest

**TID-3.** Admission resolves an entry in this order, for every architecture
key, contested or not:

1. The header's architecture names the candidate entries. No candidate refuses
   `UnknownFamily`, as today.
2. The header carried two different chat templates (TID-4): refuse
   `TemplateConflict`, naming the architecture.
3. The artifact carries no chat template (TID-4): refuse `TemplateAbsent`.
4. The digest is in exactly one candidate's set: that entry is admitted and
   retained for the session, as today.
5. The digest is in more than one candidate's set: refuse `TemplateAmbiguous`.
   The compile-time pin makes this unreachable, and the refusal is what
   happens if it is ever reached, as `MarkersAmbiguous` is today.
6. No candidate's set holds the digest: refuse `TemplateUnpinned`, naming the
   architecture and the digest.

The header read carries both safetensors texts where both exist, and the
disagreement is judged at step 2, after the architecture. So an artifact of an
uncarried architecture refuses `UnknownFamily` whatever its template files
hold.

**One read serves selection and identity.** The digest is computed from the
bytes the header read. The artifact hash (`weights_hash`, SPU Spec 3) is taken
later in admission and reads the same files again, so a sidecar that changed
between the two reads would select one template and record another. The hash
walk therefore compares its bytes for the template file against the header's
and refuses `ArtifactUnreadable`, naming the file, where they differ. The
pinned-file rule of LORA-2 (hash and load through one pinned file) is the
model.

**The account line.** Every family refusal crosses the seam as
`ArtifactUnreadable`, and today the admit account line (Spec 3) names only
`BackendDoesNotServe`. Under this amendment the account line also names
`TemplateUnpinned` with its architecture and digest, `TemplateConflict` and
`TemplateAbsent` with the architecture, and `TemplateAmbiguous`. An operator
needs the digest to bring the artifact to a pinning act.

The uncontested shortcut (`[only] => Ok(only)` in `family::select`) is removed.
The detector render (`render_probe`) leaves the production path, and with it
the production use of `TemplateUnrecognised` and `MarkersMatchNoEntry`. A
build without the `gguf` feature, which today refuses every contested key
because it has no detector, now selects contested keys by digest.

Effects measured against the baseline:

| Case | Today | Under TID-3 |
| --- | --- | --- |
| A Granite 3 chat template under the `granite` key | Admitted and rendered as ChatML (a silent substitution) | `TemplateUnpinned` |
| A ChatML variant under a ChatML key (another opener, a thinking variant) | Admitted by the detector's one ChatML class | Admitted only if its own digest is pinned |
| An `[INST]` variant under `llama` (Llama-2, Mistral v1 or v3) | Admitted by row 2, the v7 renderer | Admitted only if its own digest is pinned |
| Gemma4, which the detector cannot render | Admitted only because `gemma4` is uncontested | Admitted only if a golden render pins its digest (TID-2) |
| A Granite 4.2 or Qwen3 safetensors export with only a `chat_template.jinja` | Admitted by the shortcut | Admitted by digest, through the sidecar (TID-4) |
| KAT-Coder-V2.5-Dev safetensors (`qwen3_5_moe`), whose two template files differ at line 85 | Admitted on the config's text | `TemplateConflict`. The config raises on a non-leading system message and the sidecar renders it, so the artifact declares two behaviours |

**The `nemotron_h` substitution, measured.** The Nemotron-3-Nano-4B tokenizer
sidecar and the 30B-A3B sidecar are byte-identical (SHA-256
`ab7813c3abdd9cb655905a410728b26c7884eca45ddfab8d9f931553485a7862`, 10504
bytes). Safetensors spells both architectures `nemotron_h`, where GGUF splits
them. Once a `nemotron_h` entry pins that digest, a 30B safetensors directory
would select the 4B's entry, with the 4B's flush and widths. A digest cannot
separate them, so no safetensors-key `nemotron_h` digest is pinned until a
further key is ruled (TID-7).

## 4. What text is hashed

**TID-4.** The hashed text is the chat template exactly as the artifact
carries it, with no normalization of whitespace, line endings or encoding:

| Container | Chat template text |
| --- | --- |
| GGUF | The `tokenizer.chat_template` string value, as today |
| Safetensors | The `chat_template` string in `tokenizer_config.json`, and the full contents of `chat_template.jinja` beside it, by the rules below |

| Safetensors case | Result |
| --- | --- |
| Only a string `chat_template` key | That text |
| Only `chat_template.jinja` | That text |
| Both, the same text | That text |
| Both, different text | `TemplateConflict` (TID-3 step 2) |
| `chat_template` is `null` | As if the key were absent |
| `chat_template` is a list (named templates) or another non-string value, with no sidecar | `TemplateAbsent` |
| A list or other non-string key beside a sidecar | `TemplateConflict` |
| A sidecar that is not UTF-8 | `ArtifactUnreadable`, as for any unreadable sidecar |
| `chat_template.json` | Not read |
| A safetensors binding that names a file, not its directory | No sidecar is read. The binding refuses `TemplateAbsent`, with an account naming the directory binding, because `weights_hash` covers only the named file and a template read from a sibling would not be identified by the record |

The refusal of two different texts is the only answer that agrees with both
upstream readers. Transformers takes the sidecar over the config (and on save
removes the key), while the pinned converter (`gguf-py` `vocab.py` at
`ecce255`) takes the config first, then the sidecar, then
`chat_template.json`. Measured: Qwen3-Coder-Next carries both files, differing
by one trailing newline.

Both upstream readers convert CRLF line endings to LF. A safetensors sidecar
with CRLF endings and a GGUF converted from it therefore carry two different
digests. That is expected, and a pinning act pins each text it admits.

## 5. Instruments

**TID-5.** The code act carries these tests. Each is perturbation-verified.

| Behaviour | Test | Perturbation that fails it |
| --- | --- | --- |
| Every key judges the chat template | A Granite 3 fixture under `granite` refuses `TemplateUnpinned` | Restore the uncontested shortcut |
| The refusal names the digest | The admit account line carries the architecture and the digest of the presented text | Name only the architecture, or print no line |
| A pinned digest admits | Each entry's pinned fixture admits that entry | Hash a different text than TID-4 names |
| A near miss refuses | A pinned fixture with one trailing newline added refuses `TemplateUnpinned` | Compare a prefix, or normalize whitespace before hashing |
| An empty set admits nothing | The Llama 3 entry refuses a fixture under `llama` that no entry pins | Treat an empty set as admitting |
| Digests are unique per key | A duplicate digest within one key stops the build | Pin one digest to both `llama` ChatML and `llama` `[INST]` |
| The sidecar is read | A safetensors fixture with only `chat_template.jinja` admits | Read only `tokenizer_config.json` |
| Two texts refuse after the key | A fixture with both files and different text refuses `TemplateConflict`, and the same files under an uncarried architecture refuse `UnknownFamily` | Prefer either file, or judge the conflict in the header read |
| A non-string key | A list `chat_template` with no sidecar refuses `TemplateAbsent`, and beside a sidecar refuses `TemplateConflict` | Read the list's first template |
| No family names another | Each family module under `family/` names no other family's module outside comments. The registry in `mod.rs`, `family/tests.rs` and `generation_tests.rs` are out of scope, because they name several families by design. The `granite` pin is the model | Cite `qwen2::TEMPLATE` from another family module |

Tests that reach an entry through `select(name, None)` move to `lookup` or to a
pinned fixture. The named limit
`limit_an_uncontested_granite_key_admits_any_template` flips to the first row
above.

The marker probe (`tests/markers.rs`) stays as an instrument, not a selector:
for each pinned digest whose chat template the detector recognises, the
detector's rendering yields the entry's selecting markers. Gemma4 is named as
outside the detector.

## 6. Landing

**TID-6.** TID-3 refuses every artifact the code act does not pin. The survey
recorded no digests, and re-uploaders patch chat templates. So the code act
first takes a read-only census: the digest of every artifact the survey admits
today, per entry. It then pins each one with evidence (TID-2), or names in its
pull request each artifact it drops and why. An agent whose deployed artifact
is dropped refuses at its next load, so a drop is listed, never silent.

## 7. Open cells

**TID-7.** These are outside this amendment:

- **Mandatory golden renders.** Whether every pin needs a golden render, or
  only pins in the ChatML and `[INST]` classes, where detector evidence is
  blind. Until ruled, both evidence forms admit, and the code act reports which
  form each pin used.
- **Trace member.** The admitted digest is not recorded in the trace as its
  own member. The record's `weights_hash` covers the chat template's bytes,
  inside a GGUF or as a sidecar in the safetensors directory walk, and TID-4
  admits no template the hash does not cover (a file-named safetensors binding
  reads no sidecar). So the template an artifact carried is identified by the
  record and a replay can recompute its digest from the same bytes. A member
  would repeat that fact. The tuple act that records the deployment tuple in
  the trace may still elect one, for the reader's sake.
- **Named-template lists.** A `chat_template` list refuses (TID-4) until a
  ruling says which named template serves.
- **Qwen release lines.** Whether `qwen2`, `qwen3`, `qwen3moe`, `qwen35` and
  `qwen35moe` are one family module or one module per release line is the
  operator's ruling, on #484.
- **One template, two entries of one key.** The `nemotron_h` safetensors case
  (TID-3). A further key (the safetensors `architectures` list, for example)
  would be a ruling.
- **Operator pinning without a code act.** A declaration key that admits an
  unpinned digest is not built. Pinning is a code act with evidence (TID-2).
- **Probe defaults.** Six marker-probe default paths name files absent from
  the workshop (survey, ambiguity 6). The code act repoints or names them.

## 8. Sentences replaced

**TID-8.** These Spec 5 statements are replaced or answered when this
amendment folds into the Spec. Line numbers are at the baseline.

| Spec 5 line | Statement | Disposition |
| --- | --- | --- |
| 1480-1485 | `qwen2::renderer` serves seven entries across vendors | Replaced by TID-1. Each family module holds its own copy |
| 1498, 1503, 1508, 1509, 1515 | Ownership rows: template identity, markers that select where contested, the chat template as the selector where contested | Template identity becomes the pinned digest set. The markers stay as an instrument (TID-5). A new row names the pinned digests and their evidence |
| 1582-1585 | `lookup` for an uncontested architecture, `select` where contested | Replaced by TID-3: `select` by digest for every key |
| 1636-1646 | The header's one template field, and "where one architecture carries one template the key is unchanged" | Replaced by TID-3 and TID-4: every key judges the chat template, read from the GGUF key or the safetensors files |
| 1649-1660 | Architecture and marker set unique, ambiguity refuses | Kept for markers as an instrument, and joined by digest uniqueness and `TemplateAmbiguous` (TID-2, TID-3) |
| 1662-1737 | The detector paragraphs, including "the render is reached only where an architecture is contested" and the residual-risk paragraph | The detector leaves production (TID-3). The residual risk closes where a golden render pins (TID-2), and stays bounded by review where detector evidence pins |
| 1739-1750 | "Marker sets are compared and templates are not", and a later act comparing more than markers "is proposing to refuse" Phi-3.5-mini and Phi-4-mini | This is that act. It refuses neither by design: each artifact is admitted by its own digest once a pinning act pins it with evidence. The workshop holds Phi-4-mini, which the landing act pins, and Phi-3.5-mini waits for an act that pins its template. Whitespace variants are each pinned by evidence rather than admitted by a marker match |
| 1752, 1811 | Several-templates branch, and entries that "compete on their markers" | Entries are chosen by digest, and markers no longer select |
