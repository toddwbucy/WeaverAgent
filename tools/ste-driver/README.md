# External STE driver

This tool builds outside the agent workspace. It has its own lock file and no
dependency on an agent organ. It implements the external driver's pinned masking,
language and literal checks. It does not yet send requests or assemble dataset rows.

The authority is `docs/crates/weaver-harness/Loops/ste-conversion-loop.md`:

- 4.04 and 4.11: mask protected spans with the pinned source rules.
- 6.02: refuse changed glued literals when the caller assembles a row.
- 6.06: restore the exact codes after the caller's checks pass.
- 6.01: apply the pinned word, sentence, placeholder and meaning guards.
- 6.08: render the pinned repair notes before the request wrapper.

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
The generators need no approved-words artifact, model, gate or network. They change
only this tool's fixture and character-class files. They do not modify WeaverTrain.
The fixture cases are tests, not the R0 evaluation kit or an R0 result.
