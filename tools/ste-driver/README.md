# External STE driver

This tool builds outside the agent workspace. It has its own lock file and no
dependency on an agent organ. It implements the external driver's pinned masking
and literal checks. It does not yet send requests or assemble dataset rows.

The authority is `docs/crates/weaver-harness/Loops/ste-conversion-loop.md`:

- 4.04 and 4.11: mask protected spans with the pinned source rules.
- 6.02: refuse changed glued literals when the caller assembles a row.
- 6.06: restore the exact codes after the caller's checks pass.

`Masker::mask` returns the masked text and exact code spans in their original
insertion order. Some spans contain earlier placeholders. Restore them in reverse
order. The caller must persist the codes, check placeholder counts and refuse
source marker collisions before it sends a request. This module is the source
mask function, not the conversion or acceptance policy.

`LiteralCheck` reports changed glued tokens without changing output. Slash-joined
placeholders can separate if the caller's count check passes. Missing output
changes every glued literal. The source check uses substring presence, not an
occurrence count for glued tokens. The separate placeholder check owns counts.

Both checks return regex execution errors. Callers must retain those failures
and must not treat them as a passed check.

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
hashes before it imports `ste_convert` and `ste_experiment`. It regenerates 74
fixture cases and the Unicode character classes with Python Unicode 16.0.0.
The measured reference host used Python 3.14.8. Rust's default word and whitespace
classes differ from Python's, so the tool uses the generated ranges instead.
This also fixes the class version independently of the Rust regex dependency.

To reproduce the fixtures, supply a local checkout at the pinned revision:

```sh
python3 tools/ste-driver/tests/reference.py /path/to/pinned/WeaverTrain
```

The generator needs no approved-words artifact, model, gate or network. It changes
only this tool's fixture and character-class files. It does not modify WeaverTrain.
The fixture cases are tests, not the R0 evaluation kit or an R0 result.
