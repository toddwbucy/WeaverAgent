# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace contains eleven packages under `crates/weaver-*`. Each crate keeps implementation in `src/` and integration tests in `tests/`, with fixtures where needed. `weaver-traits` and `weaver-types` provide shared foundations; the harness connects components through contracted seams.

`docs/crates/` holds crate PRDs, Specs, and contracts. `process/` holds working rules and validation scripts; `deploy/` contains deployment scripts. Read `process/WeaverTools-Working-Process.md` before changing behavior: implementation must follow merged Specs, and where code and a Spec disagree the change decides which is wrong and fixes it in the same PR.

## Build, Test, and Development Commands

Use the toolchain pinned in `rust-toolchain.toml` (`nightly-2026-02-13`). Run from the repository root:

- `cargo build --workspace --locked`: build all packages.
- `cargo test --workspace --locked`: run workspace tests and doctests.
- `cargo test -p weaver-harness --locked`: test one package.
- `cargo clippy -p <crate> --all-targets --locked -- -D warnings`: lint each changed crate.
- `cargo fmt --all -- --check`: check formatting.

Keep `--locked` on Cargo commands that resolve dependencies. The SPU defaults to GGUF and builds llama.cpp; CUDA is optional.

## Coding Style & Naming Conventions

Use rustfmt, four-space Rust indentation, `snake_case` functions/modules, and `PascalCase` types. Existing `conforms:` headers stay in place unmaintained, and new code owes none until release. Reuse contract-defined types across seams. Write documentation in ASCII with absolute dates and canonical terminology (`trace`, `reflection`, `substrate-state`).

## Testing Guidelines

Use Rust tests and doctests, with descriptive `snake_case` test names and integration files such as `tests/identity.rs`. Exercise refusal paths alongside success paths. Behavioral invariant tests must fail when the property is deliberately removed; use compile-time checks for type properties. Coverage is evidence, not a percentage target.

## Commit & Pull Request Guidelines

Follow history's descriptive prefixes: `code:`, `docs:`, or `process:`. Open PRs as drafts. Every PR body carries `Implements: <Spec> <sections>`, graded per section as conforms, drifted, better way, or Spec gap. Describe behavior changes, relevant issues, and validation results. The gates and the Planner's verification complete before leaving draft, and leaving draft fires the Codex pass. Answer every finding, and the review repeats after substantive rework until a pass leaves nothing to push. `CLAUDE.md` carries the sequence.
