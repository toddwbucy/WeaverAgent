# External STE driver

This tool builds outside the agent workspace. It has its own lock file and no
dependency on an agent organ. It implements the external driver's pinned masking,
language and literal checks. Its command sends requests through `deploy/turn.py`
and keeps the external results. A separate offline command collects trace evidence.
Agent composition remains owed.

The authority is `docs/crates/weaver-harness/Loops/ste-conversion-loop.md`:

- 4.04 and 4.11: mask protected spans with the pinned source rules.
- 6.02: refuse changed glued literals when the caller assembles a row.
- 6.06: restore the exact codes after the caller's checks pass.
- 6.01: apply the pinned word, sentence, placeholder and meaning guards.
- 6.08: render the pinned repair notes before the request wrapper.
- 4.01, 4.05 and 4.06: send one user block and retain the gate's exact reply.
- 6.03, 6.04 and 6.07: share the request allowance across both retry kinds.
- 7.01 and 7.07: persist attempts, losses and a void evaluation's record.

The tool compiles the pure request owner from the harness's development-driver
directory as source. It links no harness or other agent library.

`Masker::mask` returns the masked text and exact code spans in their original
insertion order. Some spans contain earlier placeholders. Restore them in reverse
order. The caller must persist the codes, check placeholder counts and refuse
source marker collisions before it sends a request. This module is the source
mask function, not the conversion or acceptance policy.

Pass the masked request text to `LiteralCheck`. It reports changed glued tokens
without changing output. Slash-joined placeholders can separate if the caller's
count check passes. Missing output changes every glued literal.
The source check uses substring presence, not an
occurrence count for glued tokens. The separate placeholder check owns counts.

The checks return regex execution errors. Callers must retain those failures
and must not treat them as a passed check.

Use `Lexicon::load_pinned` with the WeaverTrain root and the approved-words JSON
path. It verifies all five source hashes before building the lexicon. The caller
supplies the pinned examples to `Judge` and the sentence limits to `Checker`.
For this workflow, the limits are 20 instruction words and 25 description words.
Keep every hard and soft finding. A soft vocabulary finding alone does not fail
the mechanical check. Checker acceptance is not a semantic verdict.

The external checker preserves the pinned reference's noun-position rules and
known limits. This tool does not implement the agent's detection rule in 5.02.
It elects no classifier and creates no lookup or filing grant. `repair_note`
accepts results produced by this checker, with their typed finding values.
`repair_notes_with_guards` adds the copied-example and word-ratio notes and the
empty-note fallback. The request owner still supplies the 6.08 wrapper.

`source_examples` verifies the original system text and the source bundle hash.
It returns only the four assistant texts, in source order. Its bundle encoding
matches `ste_prompt.main` at the pinned revision. `verify_folded_prompt` checks
the approved seed file separately. Fold headings never enter the judge.

`Kit::read` accepts already-masked narration and final segments. It preserves
their locators and exact input-file bytes, and refuses repeated IDs or long text.
Kit selection, the 100-item R0 roster and its mask-table deposit remain caller
duties. `prepare_text` rejects raw source marker collisions before masking.
The preparation caller must record each exclusion and persist the returned codes.

`evaluate` needs explicit R0 parameters of 2 repairs, 1 offer and 0 stops.
It writes and syncs the parameters before the first request. Each request is
recorded before `TurnClient` calls the gate script. The complete stdout, stderr,
exit status, answer identity, checks and notes stay with the attempt. A stopped
or invalid gate close stops the text and voids this R0 evaluation. A completed
answer with no text or a lost placeholder occurrence uses the shape retry path.
Extra or repeated placeholders use language repair with the checker's notes.
Truncation and other hard language findings use the repair path. There is no
automatic restart of a stopped pass and no earlier-output fallback.

Each run uses a new directory. `run.json` records its inputs and parameters,
`journal.jsonl` records requests and replies, and `attempts.jsonl` keeps each
completed segment's full history. `first-gate-answers.jsonl` keeps attempt zero.
`summary.json` holds all terminal counts and no kill-line verdict. Only a complete,
nonvoid run publishes `output.jsonl` for the pinned scorer. Final shape and
language failures keep their available last outputs and their refusal records.
Literal checks mark assembly eligibility and never start another request.

The `evaluate` command runs one frozen kit. Its config requires every field below.
Unknown, repeated, null or missing fields refuse the command. Input paths resolve
from the config file's directory. The output path resolves from the current directory.
The kit digest must match the exact file bytes. The command verifies the folded prompt,
source examples and lexicon before it creates a run or calls the gate. It records the
config digest, resolved inputs and source provenance in `run.json` before the
first call.
The config names the operator's agent and admin config root explicitly.

```json
{
  "repo": "/path/to/WeaverAgent",
  "agent": "ste-r0",
  "config_root": "/path/to/admin-config",
  "train_root": "/path/to/pinned/WeaverTrain",
  "approved_words": "/path/to/approved-words.json",
  "prompt_bundle": "/path/to/source-prompt-bundle.json",
  "kit": "/path/to/frozen-kit.jsonl",
  "kit_sha256": "REPLACE_WITH_THE_FROZEN_KIT_SHA256",
  "driver_repair_pass_cap": 2,
  "offer_pass_cap": 1,
  "transport_stop_bound": 0
}
```

Exit 0 means the complete evaluation was recorded. It can contain shape drops and
language failures. Exit 2 means the evaluation is void. Exit 1 means the command failed.
A void evaluation keeps its journal and counts, publishes no scorer output and starts
no automatic retry. Each invocation requires a new output directory.

After the agent and frozen kit qualify, the operator runs the two evaluations with
the same config. The source bundle is the original `ste_prompt` JSON output, not the
folded file. Its system and bundle hashes must match the workflow pins.

```sh
export CARGO_TARGET_DIR=/tmp/weaver-ste-driver-target
export TMPDIR=/tmp
cargo build --manifest-path tools/ste-driver/Cargo.toml --bin weaver-ste-driver --locked
"$CARGO_TARGET_DIR/debug/weaver-ste-driver" evaluate \
  --config /path/to/r0-config.json --output /path/to/r0-pass-1
"$CARGO_TARGET_DIR/debug/weaver-ste-driver" evaluate \
  --config /path/to/r0-config.json --output /path/to/r0-pass-2
```

The command consumes a prepared kit. It does not select the roster or attest its
qualification. The 100-item R0 roster, mask tables and selection evidence remain
required.

This library does not seed or load an agent. Before a live run, the composition
owner must verify the loaded prompt, model, caps, boot cut and frozen kit.
The report's prompt hash is the expected approved identity, not a measurement
of a live agent. The evaluation's trace fields stay null and marked incomplete.
Use the offline command below to collect trace evidence after capture.
The library makes no estimate from a gate-request count.
It does not assemble dataset trajectories or call the scorer or meaning reviewer.

The `trace-report` command joins a captured trace to `journal.jsonl`. It opens no
gate, agent or model. Its output directory must be new. It preserves both input
files byte for byte as `trace.jsonl` and `journal.jsonl`, with their SHA256 values
in `trace-report.json`. It changes no evaluation result or scorer output.

```sh
"$CARGO_TARGET_DIR/debug/weaver-ste-driver" trace-report \
  --trace /path/to/captured-trace.jsonl \
  --journal /path/to/r0-pass-1/journal.jsonl \
  --output /path/to/r0-pass-1-trace
```

Each report entry keeps the segment ID, request ordinal, request text and checked
attempt. The checked reply supplies its run and external turn. The entry keeps
every event in that turn, including thought blocks and change spans. It records
the first raw model output and counts only `model.output` events. It never counts
an `emission.change` as a generation. Offer entries retain the source Text block,
instruction sequence and response sequences from the recorded causal parents.

Token counts come from each `model.measurement` token vector. The report keeps
the measurement sequence and original timing values, and the full events retain
all other producer fields. It does not pair measurements to outcomes by adjacency
or treat resident context size as token use.

Exit 0 means the captured evidence passed this command's completeness checks.
It requires a gapless run from load at sequence zero, a complete turn bracket,
valid outcome fields, one measurement per outcome, and complete offer parent
links. Exit 2 means evidence is incomplete. That entry's generation count is null
and its reasons are recorded. An unfinished request or one without checked run
and turn identities remains incomplete. A cancelled offer without a recorded
assistant response also remains incomplete. Broken JSON lines or a missing final
trace LF make all counts incomplete. Trace gaps in a run make that run's counts
incomplete. An unload after the captured turns is not required.

Exit 1 means collection failed. An ambiguous journal refuses before creating the
output directory. This report is an external measurement of supplied files. It
does not certify custody, qualify R0, reconstruct effective output, prove filing
or verify loaded artifacts. The original trace remains the byte authority.

Run these commands from the repository root. Use a target owned by this checkout.

```sh
export CARGO_TARGET_DIR=/tmp/weaver-ste-driver-target
export TMPDIR=/tmp
cargo metadata --manifest-path tools/ste-driver/Cargo.toml --offline --locked
cargo fmt --manifest-path tools/ste-driver/Cargo.toml --all -- --check
cargo test --manifest-path tools/ste-driver/Cargo.toml --locked
cargo clippy --manifest-path tools/ste-driver/Cargo.toml --all-targets --locked \
  -- -D warnings
```

The reference revision is WeaverTrain
`9881da6cfbc92b08b31264883aea63b73aa7b694`. `tests/reference.py` checks the source
hashes before it imports `ste_convert` and `ste_experiment`. It regenerates 76
fixture cases and the Unicode character classes with Python Unicode 16.0.0.
The measured reference host used Python 3.14.8. Rust's default word and whitespace
classes differ from Python's, so the tool uses the generated ranges instead.
This also fixes the class version independently of the Rust regex dependency.

To reproduce the fixtures, supply a local checkout at the pinned revision:

```sh
python3 tools/ste-driver/tests/reference.py /path/to/pinned/WeaverTrain
python3 tools/ste-driver/tests/language_reference.py /path/to/pinned/WeaverTrain
```

The language generator records 75 synthetic source-test cases and 11 guard cases.
It also checks the pinned test module and manifest hashes before importing them.
It pins `ste_kit_run.py` and extracts the guard notes and fallback from its AST.
It does not run that module's model client.
The generators need no approved-words artifact, model, gate or network. They change
only this tool's fixture and character-class files. They do not modify WeaverTrain.
The fixture cases are tests, not the R0 evaluation kit or an R0 result.

The command's pinned-input test uses the real binary and gate client with a synthetic
Unix socket. It needs operator-supplied reference artifacts and is explicitly ignored
without them. Run it separately with those paths set. It starts no agent or model.
It covers a completed evaluation, output-directory reuse and a separate void evaluation.

```sh
export STE_TEST_TRAIN_ROOT=/path/to/pinned/WeaverTrain
export STE_TEST_APPROVED_WORDS=/path/to/approved-words.json
export STE_TEST_PROMPT_BUNDLE=/path/to/source-prompt-bundle.json
cargo test --manifest-path tools/ste-driver/Cargo.toml --locked --test command \
  command_runs_the_pinned_checker_through_a_synthetic_gate -- --ignored --exact
```
