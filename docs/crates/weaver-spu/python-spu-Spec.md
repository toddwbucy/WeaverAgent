# python-spu - Spec

**Status:** MERGED. In `main` and the source of truth.

**Date filed:** 2026-09-28
**Document ID:** `python-spu-Spec`
**Parent:** `weaver-spu-PRD`
**Editorial:** Per the Working Rules.
**Landing PR:** toddwbucy/WeaverTools-old2#756

---

## 0. What this document is

The Spec of `python-spu`, a second implementation of the SPU organ, written in Python
and held at the top of the tree as `python-spu/`. It is filed beside `weaver-spu-Spec`
because it implements the organ that document builds, and it names `weaver-spu-PRD` as
its parent because that charter is where the organ is defined. It is not a crate and
does not enter the mirror of Document Format section 2, which pairs a crate's documents
with a crate's sources. It is code that requires a Spec, which Document Format section 3
makes sufficient: a Spec belongs to any code that requires one.

Level discipline. The four contracts govern what crosses the seams and how it fails.
`weaver-spu-Spec` governs how the Rust SPU represents that. This document says which of
the Rust SPU's represented behaviours `python-spu` carries identically, by which
instrument each is proven the same, which are its own, and what a comparison of the two
SPUs certifies. It restates no contract clause and no clause of `weaver-spu-Spec`, and
cites each by section.

It is written before the code it governs lands, per Working Process section 6's rule
that no code lands without a merged Spec, and it states what a pass of the conformance
suite certifies before that suite is hardened, per the same section's rule that a pass
is stated before it is hardened. Epic #25 carries the work in stages, and this document
governs all of them.

## 1. What python-spu is

**A second implementation of one organ's decode role, honoring the same contracts.**
The organ is the one `weaver-spu-PRD` section 1 charters: it holds a model on the device
for as long as the worker that forked it lives, and it answers the harness across a
two-initiator channel. `python-spu` is party to the two contracts the Rust decode
process is party to, `weaver-harness-spu-contract` for residency and
`weaver-harness-spu-decode-contract` for the token seam, and it honors each as written.
The contracts name no language, no type and no module, so the seams admit a second
implementation as they stand and neither changes for it.

**The classify role is not this implementation's.** The label seam is served by a
process of its own, `weaver-spu-classify`, which the harness launches from its own
binary path, per `weaver-spu-Spec` section 11, so no SPU choice in admin's configuration
reaches it. A declaration electing classify under `python-spu` is served by that process
as it is today. `python-spu` accepts the instruction's `classify` member as the wire
admits it and acts on none of it, as the Rust decode process acts on none of it.
`weaver-harness-spu-classify-contract` binds that process and not this one, and a Python
classifier with its own binding is a later version, per section 9.

**It writes no trace, as the Rust SPU writes none.** The harness is the sole writer, per
`weaver-harness-trace-contract` and `weaver-spu-PRD` section 2. What `python-spu`
reports crosses the token seam in the shapes the decode contract fixes, and the harness
authors the records from it. The trace contract binds this implementation through that
route alone: a record the harness writes from a `python-spu` report verifies under the
trace contract exactly as a record written from a Rust report does.

**Nothing else in the agent changes, and nothing else learns which SPU served.** Admin
launches whatever file its configuration names for the agent, `spu-binary` or the
agent's own key in `spu-implementations`, per `weaver-admin-Spec` section 9, so one
agent may be served by the Rust SPU and another by `python-spu` on one box at once.
Admin digests that file into the load record's `stack` under the file's name, and the
determinism matrix reads it into `weaver_binaries` at both ends of a run.

**What served is named by two digests, and neither waits on the SPU reporting its own
identity**, which section 10 defers. The first is the SPU binary's digest in the
record's `stack`. Section 2 ships `python-spu` as one file whose first line names the
pinned interpreter by absolute path, so that digest covers every line of `python-spu`'s
own code and fixes which interpreter runs it, the interpreter's version being the
toolchain reader's to record. The second is the digest of the installed tree section 8
defines, which covers the engine as installed rather than as the lock says it should be.
The box facts carry it beside the lock's hash, and the matrix's record carries it once
section 5's second precondition reaches it.

**It does not replace the Rust SPU.** Both serve, and the record says which. Under the
deployment tuple a different implementation is a different kernel stack, and so a
different deployment, which is why the two are compared rather than required to agree
bit for bit.

## 2. Where it lives

    python-spu/
      src/python_spu/    the SPU: the channel ends, the wire, the session, the engine,
                         the family and the sampler
      tests/             the conformance suite and the behaviour tests
      oracle/            a Rust program linking the workspace crates, which answers
                         the suite with the Rust code's own results
      scripts/           the model smoke test and the build of the one file below

**It ships as one file.** `scripts/build_zipapp.py` packs `src/python_spu/`, the
classifier left out, into one zip archive behind a first line naming the pinned
interpreter by absolute path, and admin's configuration names that file for each agent
it serves. Its entries are sorted, dated 1980-01-01 with fixed modes and stored
uncompressed, so one tree always builds one file and the file's digest names the code
rather than the moment of the build or the zlib of whichever interpreter built it. A
launcher importing a package from elsewhere would leave the package out of the hash the
harness takes, so two runs of different code could report one provenance. The zipapp
holds no third-party package: the engine is imported from the environment, whose
installed tree section 8 digests. The file is built, not committed.

**The oracle depends on the workspace crates at a pinned commit and on no copy of
them.** It is a Rust program outside the workspace, so it is not a member and its
manifest names `weaver-spu`, `weaver-types` and `weaver-traits` by the repository and a
commit. The suite therefore reads what the Rust code computes rather than a
transcription of it, and a copied source tree would be an archive directory, which
Working Process section 1 keeps out of the tree.

**Nothing under `crates/` changes for it.** The pyworker inside `weaver-harness` is the
precedent for Python in this tree and not the model for this one: that is Python behind
a feature of a crate, and this is a separate program behind the organ's seams.


### 2.1 What the tree holds: a prototype, and where it falls short

**`python-spu/` holds the prototype built on thinkpad on 2026-09-28, carried as it
stood**, per Working Process section 6's rule that a carry is not a hardening. It is a
lab instrument until the acts that close the gaps below have landed, and no verdict
rests on it. The carry changed what landing needed and nothing more: the oracle's
manifest names the workspace crates by the repository and a commit instead of a copied
tree, the smoke report reads that commit from the manifest, and the README lost a home
path and a device identifier. Its suite passes on the CPU against the crates at the
pinned commit.

**Where it falls short of this document**, each gap named with the section it falls
short of:

- Section 2: the tree still carries an experimental ModernBERT classifier and its entry
  point, which section 1 scopes out of this implementation. The zipapp leaves it out.
- Section 3.1: its oracle answers eighteen operations, and runs a transport test over the
  oracle's descriptor:
  - `session`, `seed`, `measure`, `render` and `round`.
  - The sampler's `rng`, `select`, `generation`, `probs` and `weighted`.
  - `artifact`, `weights_hash`, `header` and `registry_entries`.
  - The model-free walk's `knobs`, `stops`, `registry` and `partition`.

  Every other operation the walk names is unbuilt.
- Section 5: the sampler is the native engine's, ported and proven as section 5 says,
  and it samples in pure Python at about 120 ms a token over a Qwen2.5 vocabulary on
  thinkpad, the softmax's glibc `expf` over every logit and the partition over every
  index being most of it.
- Section 8: `pydantic` is still in the core, the wire's models going through it until
  the oracle proves the wire, which is its own act.
- Section 3.1, the `tokenize` row: the operation is unbuilt, so the tokenizer's
  equivalence with the Rust side's is unproven until it lands.

**The list names the gaps known at the carry, and it is not a walk.** The hardening act
walks every exchange and every refusal of `weaver-harness-spu-contract` and
`weaver-harness-spu-decode-contract` against the prototype, per H3's rule that a seam is
exercised against its contract's failure cases, and epic #25 carries the walk as one
item.

## 3. What it carries identically

Every row names the `weaver-spu-Spec` section that states the behaviour, and the
instrument that proves `python-spu` carries it the same. An oracle row runs the Rust
code at the pinned commit and compares its answer with Python's. A contract row is a
check the named contract's Conformance section lists.

| Behaviour | `weaver-spu-Spec` | Proven by |
| --- | --- | --- |
| The two channel ends at descriptors 3 and 4, close-on-exec set on both, the dumpable flag cleared before the engine's runtime is imported | 2 | a process test |
| The `SOCK_SEQPACKET` envelope and the segmented frame, per `weaver-organ-channel` | 2 | oracle, over a socket pair |
| Every payload of the two contracts it is party to, accepted and refused, and the instruction's `classify` member accepted and left unread | 2, 9 | oracle, serde round trips both ways |
| Admission judges each device by the one room inequality before the weights load: free memory at least the shard, the pinned containers' size over the device count, plus the headroom the worker's `--headroom-bytes` states or the compiled 512 MiB. It refuses `device_cannot_admit` and never evicts, so a second agent's SPU of either implementation is admitted where the card has room, and the weights hash is blake3 at admit | 3 | a process test on a device, `artifact` oracle for the shard, the inequality's mirror per section 3.1, oracle for the hash |
| The session appends and never rewinds, the identity prefix is permanent, an overflow refuses before any append | 4.2 | contract, decode section 8 |
| The turn terminator made resident on every path, the stop within a token boundary of the cancel | 4.2, 4.3 | contract, decode section 8 |
| The flush per resolved entry and the elision's exact removal | 4.4, 4.5 | contract, decode section 8 |
| The re-feed drive, its registry and its recorded path | 4.6 | contract, decode section 8 |
| The family's rendering, prefix and markers, for each family served | 5 | oracle |
| The signals computed before the sampler, the surprisal election, positional pairing, absent and not zeroed | 6 | oracle for the arithmetic, contract for the pairing |
| The residual readout, elected at admit, a tap failure while elected a fault | 7 | contract, decode section 8 |
| The probability field, its depth refused below the sampling cutoff at admit | 7.5 | oracle |
| The dispositions, the knob set, the tunables arriving in the declaration, the effective values in the record | 8 | oracle, contract |
| The derived seed, to the bit | 8.5 | the section's own test vectors, oracle |
| The failure vocabulary: refusals typed at the floor, faults below the exchange | 9 | oracle, contract |

**A behaviour this implementation does not yet carry is refused at admission, never
dropped silently.** An election the declaration makes and `python-spu` cannot honor
refuses the load naming it, per `weaver-spu-PRD` section 13.7's rule that a load
granting an election it cannot honor fails at its cheapest moment. The table is the set
this document binds, and a row whose instrument is not yet built is a row the
implementation does not advertise.

### 3.1 The model-free surface, walked

The table above names behaviours. This one walks the code: every public item of
`weaver-spu` outside its engines, taken from the crate at the commit the oracle pins,
with the oracle operation that compares `python-spu` against it or the reason it is
excluded. Each module is `pub` in the crate's root, so every operation calls the Rust
code itself from outside the crate, and nothing under `crates/` changes for it. Four
rows are mirrors rather than calls, and each says why. For two of them, the item they
compare against is reachable only through a loaded model. For the other two, the item is
the binary's, or needs a device. The walk is what closes the conformance class. A later
finding that names an item is answered by its row, and an item added to the crate joins
the walk in the act that moves the pin to it.

| `weaver-spu` items | Oracle operation, or why excluded |
| --- | --- |
| `artifact`: `resolve`, `pin`, `PinnedArtifact` and its accessors, `names_a_split` | `artifact`, over a fixture directory: the resolved file, the pinned length and each refusal, run as `engine.containers` then `engine.pin`. The pin holds each container's descriptor for the admit, and the size, the load and the hash's container bytes read through it |
| `artifact`: `read_header`, `ArtifactHeader`, `Container` | `artifact`: the header of a safetensors directory. A GGUF container is refused at admit, a container this implementation does not serve |
| `artifact`: `weights_hash`, and `residency`: `WeightsHash` and its sentinel | `weights_hash`: the canonical walk's blake3 over the fixture, equal to `python-spu`'s wherever nothing is swapped during admission, and the sentinel. `python-spu` reads the pinned containers' bytes through the pins inside that walk, so under a swap its digest names the bytes it served, where the Rust walk of a directory reads the new name. A container reached through a symbolic link is left out of the walk as the Rust leaves it, the pin still serving its size and its load. That parity is #25's symlinked-member item, and whether to hash link targets or refuse linked artifacts waits on the operator's ruling. A pinned name that no longer exists at all refuses `artifact_unreadable`. The sidecars are read by name, as the Rust states for its own, in the directory the kernel names the pinned first container in, through `/proc/self/fd`, as artifact.rs `sidecar_dir_of` and native.rs `sidecar_dir` find it, so a container reached through a link reads its target's sidecars |
| `channel`: `adopt`, `Inherited`, `EntryFault` | excluded from the oracle: they read the process's inherited descriptors, which no call can hand over. A process test, per section 3 |
| `channel`: `LifecycleChannel`, `DecodeSocket`, `lifecycle_from_owned`, `decode_from_owned`, `send`, `recv`, `send_octets`, `recv_octets`, `try_recv_octets`, `as_fd`, `ChannelFault` | `frame`, over a socket pair with the Rust code at one end: envelopes, segmented frames, truncation and closure faults |
| `channel`: `ClassifySocket`, `adopt_classify` | excluded: the label seam is not this implementation's, per section 1 |
| `decoder::backend`: `TokenId`, `DecodeFault`, `FlushMechanism`, `for_container` | `session` for the faults and the flush mechanism, `artifact` for the container judgment |
| `decoder::backend`: `Backend` | excluded as a trait: it is the engine's seam, and the engine is this implementation's own, per section 4. The `session` operation drives the Rust session over a scripted backend so the session's rules are compared without an engine |
| `decoder::session`: `Session` and every method, `Stopped`, `Generated`, `StopCondition`, `CancelPoll`, `NeverCancels`, `SamplerBuild`, `PositionedSinks` | `session`: one script of opens, appends, generations, re-feeds, flushes, elisions and cancels run through both sessions over a scripted backend, the resident sequence and every answer compared |
| `family`: the `Family` trait's rendering methods, `render_each`, `render_template`, `common_role_name`, `text_content`, `fold_system_into_first_user`, `RenderRefusal` | `render`: the identity prefix, each delta, and each refusal |
| `family`: `Family::parse`, `scan`, `Markers`, `Parsed`, `Content`, `Unrecovered`, `has_unrecovered_call`, `text` | `parse`: an emission's canonical text and tool-call blocks over plain text, valid calls, and malformed and unrecovered call fragments. `Generation::content` carries what the parse chose, and a wire round trip proves only its encoding |
| `family`: `Declaration`, `permits_truncation`, `shards_across`, `FamilyName`, `lookup`, `select`, `same_key`, `normalised_key`, `judge_width`, `FamilyRefusal` | `registry`: selection by architecture and marker set, the key's normalisation, the width judgment, each refusal. Where the Rust code selects a family this implementation does not serve, this implementation refuses at admit, and the operation asserts it refuses exactly there |
| `family::qwen2`: `renderer`, `Qwen2` | `render` and `parse`, for the first version's one family |
| `family::gemma4`, `gpt_oss`, `llama`, `mistral3`, `phi`: each renderer and its type | excluded: families the first version does not serve, refused at admit per the `registry` row. Each joins the `render` and `parse` rows when a version serves it |
| `family::modernbert`: every item | excluded: the classify role, per section 1 |
| `measurement`: `log_sum_exp`, `entropy_bits`, `surprisal_bits`, `field` | `measure`, over supplied logits |
| `measurement`: `Accumulator` and its methods, `Signals`, `absent`, `steps`, `NonEmpty` and its methods | `accumulate`: a sequence of records, abandons and a finish, the signals compared, absence included |
| `measurement`: `PromptPartition` and its accessors, `PartitionDefect` | `partition`: offsets and text lengths, accepted and defective |
| `readout`: `Reduction` and its methods | `reduce`, over supplied activations and norms |
| `readout`: `ReadoutElection`, `judge`, `judge_column_ask`, `ReadoutRefusal`, `TapOutcome` | `registry`: each judgment against a declaration |
| `readout`: `Tap` | excluded as a trait: the tap reads the engine's hidden states and is the engine's own |
| `residency`: `promote_stop_conditions`, `StopSet` | `stops`, over supplied stop inputs |
| `residency`: `Headroom`, `AdmitRefusal` | the wire rows of section 3 for the refusals, and the admission's step table below for which kind each of `python-spu`'s refusals crosses as. The headroom judgment's inequality is a mirror and its driver query needs a device, per the `gpu` row |
| `residency`: `Residency` and its methods, `Admission` and its accessors, `Resident`'s `model`, `open_session`, `declared_eos` and `stop_set`, `LoadedModel` | excluded from the oracle: each needs a loaded model or a device. The process tests and the contract rows of section 3 |
| `residency`: `Resident::tokenize`, `Resident::detokenize` | `tokenize`, a mirror. Both reach the native engine's `tokenize` and `detokenize`, which are `pub(crate)` and need a loaded model, so the operation calls `tokenizers` 0.21.4, the version `weaver-spu` links, with the two calls `native.rs` makes, `encode(text, false)` and `decode(ids, false)`, against the served artifact's `tokenizer.json`, and compares both directions on the rendered prefix and every delta. This implementation runs the Python `tokenizers` 0.23.2 the lock pins, which `transformers` 5.17.0 requires, so the two sides run two releases and the operation is the proof of their equivalence on the artifacts served, a divergence being a finding against the engine's version. Section 7's check that each generation's input token identifiers are equal on both sides is the backstop on the prompts a comparison runs |
| `sampling`: `Disposition`, `is_frozen`, `Knobs`, `EffectiveKnobs`, `SessionParameters`, `EffectiveSessionParameters`, `tunable_names`, `resolve`, `KnobRefusal` | `knobs`: resolution against supplied tunable values, and each refusal |
| `sampling`: `derived_seed` | `seed`, and section 8.5's test vectors |
| the native engine's `sample` | `generation`, `probs`, `select`, `weighted` and `rng`, mirrors, per section 5: it needs a loaded model, so the operations run the code it runs at the pinned revisions. `generation` is candle's `LogitsProcessor` with the engine's penalty over a whole generation, `probs` the probability vector's bits, `select` the whole index order `select_nth_unstable_by` leaves, `weighted` rand's `WeightedIndex` over scripted words, and `rng` the generator's words |
| `decoder::native`, `native_pair`, `gguf`, `gguf_tap`, `gpu` | excluded as a whole: the engines, their taps and the device query, which section 4 makes this implementation's own. Two parts are excepted. The native sampler is excepted per the row above. `gpu::room_and_reach`'s room inequality is carried identically per section 3's room row: a mirror, since it needs a device, ported as `engine.judge_room`, with the sum saturating and `NoRoom`'s figures in the refusal's detail, since the wire's `device_cannot_admit` carries none |
| the binary's `main.rs`: `headroom_from`, `HEADROOM_BYTES`, `main`'s `bad_parameter` refusal, and `KNOBS` and `SESSION_PARAMETERS`, the dispositions the binary elects | the process's command line, a mirror: a binary's items are not the library's, so no call reaches them. `server.parameters` ports the rule. The whole vector is read, a missing or malformed value is refused, a parameter stated twice is refused, and an unknown one is refused by name. An absent headroom is the compiled default. The entry refuses in `main`'s form: one JSON line, exit 1, before the channels are adopted. The tests hold it to `the_whole_vector_is_judged`'s cases and to the worker's `OrganParameters::spu_arguments`. python-spu's own `--cpu-experiment` and `--declare-imports`, which the worker never sends, follow the same rules. The dispositions are mirrored as `sampling.KNOBS` and `sampling.SESSION_PARAMETERS`, held to the binary's literal by `tests/test_walk.py`, and resolved through the library by the `knobs` operation |

**Admission runs the Rust SPU's steps, in its order, and each failure crosses as its
step's kind.** This table is the authority `engine.HFEngine` follows: its steps are
named in this order, and a failure inside one, anticipated or not, crosses as that
step's kind (`engine.ADMISSION_STEPS`). The Rust order is `weaver-spu` residency.rs
`admit`, then decoder/native.rs `ResidentModel::load`. Its kinds come from
`From<AdmitRefusal> for LifecycleRefusal` and `From<FamilyRefusal>`. The floor's closed
set carries the kind, and the detail, which names the step, stays on this side of the
seam.

| Step | The Rust | `python-spu` | Kind |
| --- | --- | --- | --- |
| resolve | artifact.rs `resolve`: nothing at the path or a path through a file, a directory with no container or containers that are not one split, anything but a directory or a regular file | `resolve_directory`, `containers` | `artifact_unresolvable` |
| | the same: any other failed look, a lookup the kernel refuses among them | | `artifact_unreadable` |
| pin | artifact.rs `pin`: a reference in the split pattern pins its whole set, first shard first; a shard absent; any other failure to open, or a non-regular file | `split_members`, `containers`, `pin` | `artifact_unresolvable`; `artifact_unreadable` |
| header | artifact.rs `read_header`, step two: GGUF's walk with its caps, or safetensors' length-prefixed JSON, and the sidecars `config.json` and `tokenizer_config.json` where present beside the pinned container (`sidecar_dir_of`), all read by `serde_json` | `read_header`, `sidecar_dir_of`, `strict_json` | `artifact_unreadable` |
| select | family `select`: `UnknownFamily`, `TemplateAbsent` | `select` over `REGISTRY` | `artifact_unreadable` |
| width | family `judge_width`: `WidthNotDeclared` | the selected entries' widths | `device_cannot_admit` |
| readout | readout `judge`: `NotTappable` where readout is elected | the selected entries' tap flag | `device_cannot_admit` |
| distinct | `judge_distinct`: `DuplicateDevice` | a device named twice | `device_cannot_admit` |
| size | the pinned size, `on_artifact` "size" | `pinned_size` | `artifact_unreadable` |
| room | `judge_room_and_reach`, gpu/mod.rs `room_and_reach`: every assigned ordinal present (`Unreachable`), every device's room (`NoRoom`), every ordered pair's peer reach (`NoPeerAccess`) | the CPU experiment's ordinal, `room_and_reach` over torch's device layer | `device_cannot_admit` |
| hash | artifact.rs `weights_hash`, before any device is taken (`weaver-spu-Spec` section 3) | `weights_digest` for a directory, `file_digest` for a file, by the resolution's kind | `artifact_unreadable` |
| load | `load`: `BackendNotBuilt` for a GGUF container or a file reference this build does not serve | the same | `device_cannot_admit` |
| | native `load`: more devices than it serves; `read_declaration` of `config.json` in `sidecar_dir`, the pinned container's directory, every sidecar of the load read there | one device; `strict_json` of `config.json` in `load_dir`, as are `AutoConfig` and `tokenizer.json` | `device_cannot_admit` |
| | native `judge_family` on the declaration's `model_type`: `BackendDoesNotServe` | `same_key` against `qwen2` | `artifact_unreadable` |
| | native `read_config`, `read_eos`, the tokenizer, the weights (`VarBuilder`, `ModelForCausalLM::new`, a tensor missing among them) | `AutoConfig` and a quantized config (the config's parse is python-spu's own, transformers' `AutoConfig`, per the operator's ruling of 2026-10-01), `read_eos`, `Tokenizer.from_file`, the model with its missing keys refused, placement, out of memory | `device_cannot_admit` |

Three judgments are this build's own and sit at the load:
- the family the header selected is the one it renders, qwen2, refused
  `artifact_unreadable` otherwise, per the registry row of section 3.1;
- its renderer's markers each promote to one token, refused `artifact_unreadable`
  (the Rust SPU meets an unpromoted turn close at the open, as a fault);
- it serves one device where the native backend serves two, refused
  `device_cannot_admit` as the native's own width refusal is.

**The registry is ported as a table**, `engine.REGISTRY`, each entry's family, its
declared widths and its tap flag in the Rust order, and the oracle's `registry_entries`
operation holds it to the Rust `REGISTRY`. A contested family's template is matched in
the Rust SPU by llama.cpp's renderer, which this build does not carry. So a contested
family's candidates stand together through the width and readout steps, and a template
no entry matches, `MarkersMatchNoEntry` or `MarkersAmbiguous` there, is selected here,
where the Rust SPU answers `artifact_unreadable`. That is the one divergence the steps
keep, and it is reachable only by a GGUF or file reference this build refuses either
way.

**The reads are the Rust's reads.** A header read is `read_exact`'s: it reads on after
a short read until the bytes are in or the file ends, and any failure is unreadable.
The JSON of the header and sidecars is `serde_json`'s: UTF-8 only, no non-standard
constant, no number outside f64's range, no unpaired surrogate, and no nesting past 127.
`tests/test_load_kinds.py` holds that by difference against the Rust `read_header`
through the oracle's `header` operation.

The quantized step is traced through the Rust code rather than measured. The header
step reads the family from `model_type` and never `quantization_config`, and the native
`judge_family` judges the declared architecture alone, so a quantized qwen2 passes both
and its weights fail in `VarBuilder::from_mmaped_safetensors` at BF16 or
`ModelForCausalLM::new`, both `LoadFailed`. `tests/test_admission_steps.py` injects a
failure at each step and holds its kind, and holds the order where two steps would both
fail.

## 4. What is its own

**The engine.** The first version runs Hugging Face `transformers` in eager mode at
BF16, on one CUDA device, serving the Qwen2 family from a local safetensors directory.
It reads the weights only through the descriptors the admission pinned, with
`safetensors` on each pin, and builds the model with the class transformers maps the
config to, at `dtype=bfloat16`, so the model code, its tying and its cast are
transformers' own. It makes no download, runs no remote code, converts no quantization,
falls back to no other device, and shards nothing. Section 9 names the versions after
it.

**The kernel stack.** The kernels are PyTorch's and the CUDA libraries PyTorch brings
with it, its own cuBLAS and cuDNN among them, not the ones the Rust SPU links. They are
recorded beside the environment lock, since two runs on one card under two kernel
stacks are two deployments under the tuple, and the matrix's engine-library reader
does not reach them until section 5's second precondition lands.

**The runtime.** A CPython interpreter, one exact version per build, named by the lock
of section 8.

**The cache.** It owns the hot cache on the same terms `weaver-spu-PRD` section 2 sets:
the harness decides the flush and never touches the cache, and the cache ends with the
residency. How the cache is held is the engine's. The first version recomputes the
retained sequence after a truncation rather than rolling a cache back, which changes the
time a flush costs and not what it leaves resident.

## 5. The sampler

**The draws match exactly the sampler of the Rust engine the comparison is made
against.** The Rust SPU has two engines and they sample differently, per
`weaver-spu-Spec` section 4.1's two backends. Both build their sampler per generation
from the derived seed of section 8.5, and they differ in the order of the chain and in
the generator that draws. A comparison that let the two generators differ would measure
the generators at every sampled position, whatever the engines did, so `python-spu`
carries the partner's sampler and never an approximation of it.

**The first version's partner is the Rust native engine**, candle on the same
safetensors directory, which maps the weights at BF16 (`native.rs`, the single-device
load's `DType::BF16`), on the Planner's election of 2026-09-28 recorded on #25. It is
the one pairing that holds both the weights and the dtype fixed without changing
`crates/`, since llama.cpp computes on quantized blocks that no dequantized forward
reproduces. Its chain, from `crates/weaver-spu/src/decoder/native.rs` and candle's
`LogitsProcessor` at the pinned revision, is this:

1. The repetition penalty over the last `repetition_window` resident tokens, as candle's
   `apply_repeat_penalty` computes it, skipped where the penalty is 1 or the window 0.
2. Temperature zero or below is the argmax, and nothing below applies. The cutoff is
   `native.rs`'s own, which builds the sampler by `from_sampling`, and not the 1e-7 of
   candle's `LogitsProcessor::new`, which the engine does not call, so a temperature
   just above zero divides the logits and samples.
3. The logits divided by the temperature, then the softmax.
4. Top-k where k is above zero, then top-p. A top-k of zero disables that gate.
5. The draw, `WeightedIndex` over the remaining weights, from `rand`'s `StdRng` seeded
   by `seed_from_u64` with the whole 64-bit derived seed, at the `rand` version the lock
   pins, 0.9.5, whose `StdRng` is ChaCha12 by `rand_chacha` 0.9.0.

**The GGUF engine's chain is never ported to Python.** It runs penalties, the truncating
filters, then temperature, then llama.cpp's own draw from a seed folded to 32 bits. The
later control version, `llama-cpp-python` against the Rust GGUF cells, uses llama.cpp's
sampler natively and inherits that chain rather than reimplementing it.

**The sampler reads more than the logits, and the port is proven over whole
generations.** Within one generation the native engine's draw at a position reads five
things: that position's logits, the knobs, the derived seed the generation's sampler
was built from, the state of `StdRng` after every draw before it in the generation,
and the resident tail the penalty reads, which grows by each drawn token. The penalty
visits each distinct token of the last `repetition_window` resident tokens once,
dividing a non-negative logit by the penalty and multiplying a negative one, per
candle's `apply_repeat_penalty`. A single draw over fixed logits tests none of the
stream, so the oracle gains an operation over a whole generation: it takes the seed,
the knobs, the resident tail at the generation's start and a sequence of logits
vectors, one per position, and returns the sequence of draws candle's own
`LogitsProcessor` and the engine's penalty produce, appending each draw to the tail
before the next. The suite requires Python's sequence to equal it over many seeds,
knob sets and sequences. The edges are named cases: near-ties, a temperature at zero
and one just above it, a top-k of zero and one at or past the vocabulary, a top-p of
one and one at or past the mass top-k kept, a tail holding repeated and unrepeated
tokens, a token entering and leaving the window at its edge, a penalty of one and a
window of zero, each of which skips the penalty, and sequences long enough that the
generator's state carries across many draws. The arithmetic is ported in candle's
order and precision, since identical inputs give identical draws only where the
arithmetic is the same. **The order the filters leave the candidates in is part of the
draw.** Candle keeps the top-k by `select_nth_unstable_by`, whose partition decides
the order `WeightedIndex` walks, so the port reproduces that order and not only the
set.

**The arithmetic is candle's CPU path on the partner's host, so bit for bit holds on one
box.** The native engine builds the sampler's input as a host tensor (`native.rs`,
`Tensor::new(logits, &Device::Cpu)` in `sample`), so the softmax runs candle's CPU f32
code and never a device kernel. Three facts of that code bind the port. The softmax's
sum is `cpu::vec_sum`, which takes an AVX2 order of four accumulators of eight lanes
where `std::is_x86_feature_detected!("avx2")` holds and a sequential order where it does
not, so the port detects AVX2 the same way and reproduces whichever order the host
takes. The exponential is Rust's `f32::exp`, which on Linux is glibc's `expf`, so the
port calls that `expf` through `ctypes`, keeping the core in the standard library, where
a vectorised `exp` would differ in the last bit. And `&logits / temperature` is candle's
`affine(1 / temperature, 0)`, the reciprocal taken in f64 and cast to f32, then a
multiply, so the port never divides. The two SPUs therefore agree draw for draw where
they share one glibc and one CPU feature set, which the comparison's cells already
assume, and the box facts record the host's `avx2` flag and its glibc version for both.

**The proof reads the bits, not only the draws.** A one-ulp slip in a probability moves
a draw only when the uniform lands on the boundary it moved, which a run of draws rarely
shows. So beside `generation`, the oracle's `probs` returns the probability vector's
bits for a supplied penalty, temperature and logits, `select` returns the whole index
order the partition leaves, including inputs an adversary built to drive it into
`median_of_medians`, `weighted` samples `WeightedIndex` from scripted words so a choice
can land on a cumulative weight on purpose, and `rng` returns the generator's words.
Core's partition is read at the rustc commit `rust-toolchain.toml` pins,
47611e16044c68ef27bac31c35fda2ba1dc20b73: `library/core/src/slice/sort/select.rs`,
`shared/pivot.rs`, `shared/smallsort.rs` and `unstable/quicksort.rs`, whose
`partition_lomuto_branchless_cyclic` a `usize` slice takes.

**Where exact draws prove impossible, the comparison falls back to distributions and
says so.** If a draw cannot be reproduced exactly, the suite records which operation
departs, the comparison of section 7 is made at the distribution level alone, and every
cell it produces states that token streams are not comparable.

**Two preconditions before any comparison run.** The first: the native engine is
confirmed to load the safetensors directory the Python SPU serves, and the declaration
that selects the native engine for it is recorded, since which backend serves is a
property of the artifact decided at admit, per `weaver-spu-Spec` section 4.1.

**The second: the determinism matrix's provenance readers are extended, as their own act
under `determinism-matrix-Spec`, because three of them assume the Rust GGUF deployment
and no comparison run could exit 0 on either side without them.** The weights reader
hashes one file, so every safetensors directory reads unreadable, the Rust native
partner's included, and it becomes a directory digest over the sorted relative paths and
each file's sha256, a symbolic link refused. The engine-library reader runs `ldd` on the
SPU binary and looks for llama.cpp's libraries, which names nothing a Python engine
loads, since PyTorch opens its kernels at run time. It reads the libraries the loaded
SPU holds instead, and how is that act's to elect: the obvious source, the process's own
mappings under `/proc`, is closed to an unprivileged reader by the dumpable flag both
SPUs clear at entry, per `weaver-spu-Spec` section 2, and reopening it would undo that
section's claim. The toolchain reader reads `rustc` alone, and it reads per
implementation: `rustc` for the Rust SPU, and the interpreter's version with the lock's
hash for this one. Epic #25 carries the act as a stage C item, and no document here
changes the matrix.

## 6. What conforms means

This section is the statement of the conformance suite's pass, made before the suite is
hardened, per Working Process section 6.

**A pass certifies that `python-spu` answers every question the Rust code can answer
without a model the way the Rust code answers it**, at the commit the oracle pins. Those
questions are every row of section 3.1 that names an oracle operation, the sampler's
sequence of draws over a whole generation among them, per section 5. It also certifies
every check that section 8 of `weaver-harness-spu-decode-contract` lists, and the
ordering and failure cases of `weaver-harness-spu-contract` sections 3 and 5, run
against `python-spu` itself.

**The evidence is the Rust code's own answer, read by execution.** The oracle links the
workspace crates at the pinned commit and computes each expected value from them, so the
suite compares two implementations rather than one implementation against a
transcription of the other. The seed also meets `weaver-spu-Spec` section 8.5's test
vectors, which that section states independently of any implementation.

**It guards against drift.** A change on either side to anything the table of section 3
proves by the oracle fails the suite once the pin reaches it.

**It does not guard against, and does not certify:**

- That the two SPUs produce the same tokens from a model. The engines compute different
  logits, and section 7 is where that difference is measured.
- A Rust change the pin has not reached. Conformance is to the pinned commit, and moving
  the pin is its own act, carrying the suite's result at the new commit.
- Behaviour under a real device: admission against a live occupant, headroom and
  injected device faults. These are stage B's, on thinkpad, and not the suite's.
- A turn completed end to end through the harness, the gate and admin, and a trace
  verifying under the diagnostic replay. That is epic #25's deployment item.
- Families, precisions and engines beyond the version served.
- The label seam, which the classify process serves and this implementation does not.
- Speed.

## 7. What a substantive difference is

Stated before any comparison run, per the same rule, so that no result chooses its own
threshold. The comparison holds the weights, the dtype, the knobs, the declared seed and
the prompts fixed, and serves them from each SPU in turn. **Fixing the dtype does not
fix the arithmetic.** Both sides compute at BF16, and each engine's accumulation and
reduction order, which BF16 rounding makes visible, is part of the implementation being
measured rather than a variable held apart from it. A difference therefore has one cause
in the sense the comparison needs, the implementation, and the implementation includes
its kernels.

**At the distribution level**, both SPUs are driven along one token path, the partner's
recorded path, with the re-feed drive of `weaver-spu-Spec` section 4.6, so each position
is read against the same context. The re-feed fixes the output path and not the input,
which each SPU tokenizes for itself, so the same context rests on the `tokenize` row of
section 3.1 and is checked in every run: each generation's token identifiers in are
equal on both sides, and a run where they differ is not a comparison. The run elects one
probability-field depth for both, at least two and never below the effective top-k,
which `weaver-spu-Spec` section 7.5 already refuses at admit. A position differs
substantively when any of these holds:

- The two fields name different top-1 tokens, and each SPU's own margin between its
  top-1 and top-2 log-probabilities exceeds 0.01 nats. A disagreement where either
  margin is smaller is a near-tie and is numerical.
- The two fields' top sets share no token.
- The divergence over their shared tokens exceeds 0.001 nats. Each field carries only
  its own top candidates, so the divergence is taken over the tokens both fields hold,
  each at its own probability, with each SPU's remaining mass held as one further
  outcome. That is a coarsening of the full distributions, so the figure is a lower
  bound on the full Kullback-Leibler divergence of the partner's distribution from
  `python-spu`'s, and a position under the line may still differ beyond it where the
  mass lies outside the shared set. The report carries the shared set's size beside
  the figure for that reason.

Each measure is read from a member the record already carries, so the comparison adds
nothing to either SPU's record but the elected depth:

| Measure | Read from | `weaver-spu-Spec` |
| --- | --- | --- |
| Entropy per position, over the whole vocabulary | the signal vector each generation carries | 6 |
| Top-1 token, and the margin to top-2 in log-probability | the probability field's first two ranks, as probabilities | 7.5 |
| The shared set, its size, and the divergence over it | both fields at the position, to the elected depth | 7.5 |
| The first departing position under the shared sampler | the token identifiers out of each generation | 6 |
| Whether the departure is substantive | both fields at that position, by the three tests above | 7.5 |

**At the token level**, both SPUs sample with the shared sampler of section 5 from the
same derived seeds, so their streams agree until a draw departs. Both runs elect the
same field depth, and up to the first departure their contexts are one context, so the
first departing position is recorded with both fields at it, read against the same
context, and the departure is substantive exactly
when that position differs substantively by the distribution level. A departure at a
position that does not is a draw falling between two fields that agree within the
numbers above, and it is numerical.

**Every position is reported, and substantive is a label on it, never a filter.** A
report carries every position's measures, so a reader can apply a different line to the
same record without a second run.

**The two numbers are elections, and they are revised only before a run.** 0.01 nats of
margin is a probability ratio of about 1.01 between the top two tokens, and a top-1 flip
inside it is a near-tie any rounding can move. 0.001 nats of divergence is chosen as the
line for a distribution's shape, and at BF16 the rounding of two kernel stacks may reach
it, which is the point of the next sentence: a label above the line says the two
implementations differ at that position, kernels included, and not why. Both numbers
are the operator's to confirm on this document. A run that finds many positions near
either line raises that as a finding before the next run, and a later run that wants
different numbers states them in its own charter before it runs, never in the report of
the run that prompted them.

## 8. The environment

**Every package is pinned to an exact version and a hash.**
`python-spu/requirements.lock` carries a hash for every file, compiled from
`requirements.in` against `constraints.txt`, the set the prototype's GPU smoke ran on
thinkpad on 2026-09-28, so every transitive version is one that has run and none is
chosen fresh. It installs with `pip install --require-hashes --no-deps
--only-binary=:all:`, which refuses a file without a hash, resolves nothing and builds
nothing from source, so no package's build code runs as root at the install.
`requirements-test.lock` is the same set and pytest. One package torch lists is left out
of both, by `--no-emit-package triton`: triton, which eager serving needs none of, and
which, where it is installed, has torch compile code at the first forward on a device,
as the rule below forbids. **An install recreates the prefix and then holds it to the
lock**, because `pip install -r` adds what a lock lists and removes nothing: a prefix
installed from the lock before triton left it keeps triton, and admission then faults
on the import set. `scripts/installed_set.py`, run by the prefix's interpreter after
pip, reads the installed distributions by `importlib.metadata` and refuses by name, exit
1, one the lock does not pin, one it pins that is absent and one at another version. The
interpreter's own pip, at the version its `ensurepip` bundles, is the one distribution
admitted beside the lock. The suite holds its venv to the test lock the same way.
torch 2.14.0's wheel is PyPI's own, and it brings its own CUDA runtime, cuBLAS and
cuDNN, distinct from the CUDA the Rust stack builds against, which the lock's header
records.

**The interpreter is pinned by release and digest.** python-build-standalone 20260924,
CPython 3.14.7, whose archive URL and sha256 the lock's header names, placed at
`/opt/weaver/python-spu`, where agent identities can read and run it. A build under
another interpreter is another build. It links the system glibc dynamically, and section
5's claim rests on that: `tests/test_environment.py` reads the `libm` the process maps
once the sampler has loaded `expf` and requires it to be the one `ldd` names for the
Rust SPU's binary.

**The installed tree is digested, because a lock states what should be installed and not
what is.** The packages install into the interpreter's own prefix, not a venv, and the
digest is taken over that whole prefix, the interpreter binary and its standard library
included, by `scripts/tree_digest.py`: every entry's path relative to the prefix,
sorted, a directory by its path, so an empty one is recorded, a file by its sha256 and a
symbolic link by the text it points to, never followed, anything else refused. The
prefix holds links of its own, `bin/python3` among them, so a link is recorded rather
than refused. A digest is never of nothing: a root that is not a directory by `lstat`,
an error anywhere in the walk, and a tree with no entries each exit non-zero, and so
does the zipapp's build over a package missing a file the process cannot start without.
The digest is recorded beside the lock's hash, and with the SPU binary's digest it is
what section 1 names as what served.

**The core imports the standard library alone, and the engine is the one addition.** The
channel ends, the wire, the session, the seed and the sampler's arithmetic are standard
library code. The engine is the package set the serving version requires, `torch`,
`transformers` and `tokenizers` for the first. `blake3` stays, because the Rust SPU's
weights hash is blake3 and admission computes it. `pydantic` is dropped once the oracle
proves the wire, since a second model of the payloads beside the oracle's would be a
transcription the suite exists to avoid, and until then it is in the lock.

**The import set is judged, and serving depends on it.** After admission and again after
the first generation, since torch loads some modules only in its first forward, every
module the process holds must be one the declared set names. Loaded is a subset of
declared and not equal to it, because torch imports different modules on the CPU and on
CUDA. **So the declared set is two halves, one per device, and the process is judged
against their union**: `src/python_spu/imports-cpu.txt` and `imports-cuda.txt`, each a
clean run on its device through the real serving path launched with `-m` and from the
zipapp, generated by `scripts/declare_imports.py` and reviewed. A regeneration writes
its device's half whole and never merges into it, so a module the new run no longer
records leaves the half, and stays allowed only while the other half records it. A list
that only grew would keep every module any run ever loaded. The CUDA run is of the real
model on a card, and torch loads some modules only in its first forward on a device, so
a set missing them faults there after the first generation and not at admission. The
zipapp carries both halves as they stood when it was built, and is rebuilt when either
changes. A module outside the set faults the process: one line on standard error naming
the modules and the stage, then an exit, which the harness meets as the SPU gone, since
the lifecycle vocabulary has no case for an environment that loaded what it did not
declare. The generator runs the server with `--declare-imports`, which records instead
of judging and exits once the first generation is recorded, so the mode never serves.
The suite proves the rule by perturbation: a module imported at startup through
`sitecustomize` faults the process at admission, naming the module. This is the Python
counterpart of the rule of 2026-09-23 that nothing enters a binary unless operations
require it.

**python-spu compiles and loads no code at runtime.** Every file the process maps
executable, judged at the same two stages as the import set and in the declaring mode
too, must lie in the interpreter's prefix, which the tree digest covers, or be one of
the system objects the interpreter and torch link against, by directory and name:
glibc's family, `libgcc_s`, `libstdc++`, `libz`, and the NVIDIA driver's `libcuda` and
`libnvidia-*`. Anything else, an object in a temporary directory or a cache, a deleted
file or a memfd, faults the process the way an undeclared module does, one line naming
the objects and the stage, then exit 3, so no half is ever recorded from a run that held
one. A data mapping, such as the model's safetensors, is not judged, and neither is an
anonymous executable mapping, which names no file: a ctypes trampoline, or the driver's
own translation of a kernel. The rule was found by stage B's first load on 2026-09-29,
where triton, imported by torch's dynamo and reached through torch's native DSL
registration for an outer-product `bmm`, Qwen2's rotary frequencies, compiled its CUDA
driver helper with the system's gcc at the first forward, and failed in the agent's unit
for want of a linker. Every earlier CUDA run had done the same silently in a shell with
one. So the lock leaves triton out, and the package sets `TORCH_DISABLE_NATIVE_JIT=1`
before torch is imported, so an environment that gained triton still serves torch's
eager kernels and faults on triton's import rather than compiling. The suite proves the
rule by perturbation: a shared object copied out of the environment and loaded at
startup faults the process at admission, naming it. A container replaced under its name
during admission stays mapped as the pinned inode, `model.safetensors (deleted)` in a
data mapping, which the rule does not judge, measured with a replacement made between
the pin and the load. So `python-spu` serves the pinned bytes, as the Rust SPU serves
its pin, rather than faulting.

**The determinism environment is the process's own.** The engine enables torch's
deterministic algorithms, which require `CUBLAS_WORKSPACE_CONFIG`, so the package sets
it to `:4096:8` at its first import, before torch can initialise CUDA, rather than
relying on a deployment line. One admin configuration may serve both SPUs, per
`weaver-admin-Spec` section 9, and the Rust SPU needs no such line. A value the
environment already carries that differs is refused at the entry by name: one
`bad_environment` line, then exit 1, before the channels are adopted. It is never
overwritten, since a deployment that set one meant it. The rule holds for every variable
the package sets for itself, each set at its first import before the library that reads
it: `TORCH_DISABLE_NATIVE_JIT=1` for the rule on loaded code above,
`HF_HUB_DISABLE_PROGRESS_BARS=1` for the rule that the SPU is one process below, and
`CUBLAS_WORKSPACE_CONFIG`. One `bad_environment` line names each that differs.
`LD_LIBRARY_PATH` needs no setting. torch loads its CUDA libraries from the prefix by
path, so a cuBLAS of the same name earlier on the path is never mapped. That was
measured on thinkpad at import with the Rust stack's `/opt/cuda/lib64` on the path, and
a suite test shows the same with an impostor library. The loaded-code rule above would
fault a process that mapped one.

**The SPU is one process: it starts no child, not even for a moment.** The import set
and the rule on loaded code judge the process they run in, so a second process would run
outside both, and outside what this document calls the SPU. Stage B2 found two, on
2026-09-30. transformers' loading progress bar made a tqdm whose default write lock is a
multiprocessing RLock, which launches Python's resource tracker, a second interpreter
that outlives the load. And `ctypes.util.find_library`, which cuda.pathfinder calls at
torch's import, runs `ldconfig`, then `gcc` or `ld`, in a subprocess. The package turns
the progress bars off before transformers is imported, under the environment rule above,
and answers `find_library` with nothing. Every caller in the lock falls back or never
runs: cuda.pathfinder falls back to the stable `libdl.so.2`, torch's inductor compiles
nothing here, and pip's macOS truststore never runs. The sampler loads glibc's libm by
its soname. Two tests hold it. The served process has no child after admission and after
the first generation. A fresh interpreter watching every spawn, through the audit hook
and multiprocessing's own spawn, sees none through the import, an admission and a
generation.

**The process counts its inherited descriptors before any module opens one of its own.**
python-build-standalone's `ctypes` opens the interpreter's own executable at import,
inheritable, where the system build opens nothing, and the sampler and the transport
import `ctypes` before adoption. So the package records the descriptors it holds when it
is first imported, and adoption judges that record, section 3's descriptors 3 and 4 and
nothing else. A test driver spawning a child marks its own descriptors close-on-exec
first, since the same descriptor in a Python parent would otherwise reach the child.

## 9. The versions

**The first version is `transformers` eager at BF16, one CUDA device, Qwen2
safetensors.**

**Each later version is a separate binary, with its own locked environment, and a
separate deployment under the tuple**, crossed one axis at a time against the one before
it. Epic #25 lists them: `llama-cpp-python` pinned to the Rust SPU's llama.cpp commit
and build flags, the control, first, then the attention kernels, the precision, which is
`transformers` at FP32 crossed against the first version at BF16, a minimal variant with
the forward written in plain torch, the device, and a classifier serving the label seam
with its own binding. None is chartered by this document, and each extends it when it is
built.

## 10. What this document does not carry

**The SPU's self-reported identity.** An SPU naming its implementation and its lock's
hash at admission would change the admission or measurement vocabulary in `weaver-types`
and what the harness records. It waits until it can be coordinated with the olympus
seat, on the operator's ruling of 2026-09-28, and section 1 states what names the
implementation until then.

**Which agents a deployment serves from which SPU.** That is the operator's choice in
admin's configuration, per `weaver-admin-Spec` section 9, and this document binds only
what `python-spu` must be to serve one.

**The comparison cells.** Their declarations, predictions and falsifiers are the
deployment-tuple charter's, per Document Format section 2, and this document supplies
only the line of section 7 they read against.

**The Rust SPU's behaviours themselves.** Those are `weaver-spu-Spec`'s, and this
document cites them.
