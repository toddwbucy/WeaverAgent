# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace holds ten packages under `crates/weaver-*`, the agent of the
WeaverTools suite. Each crate keeps implementation in `src/` and integration tests in
`tests/`, with fixtures where needed. `weaver-traits` and `weaver-types` are the floor;
the harness connects the organs through contracted seams, every one a Unix socket where
a process line is crossed. `python-spu/` is a Python prototype of the SPU's decode role
with its own README and lock files.

`docs/crates/` holds crate PRDs, Specs, and contracts. `process/` holds the working
rules and the lock gate; `deploy/` holds the install scripts and their two guides. Read
`process/WeaverTools-Working-Process.md` before changing behavior: implementation must
follow merged Specs, and where code and a Spec disagree the change decides which is
wrong and fixes it in the same PR. `CLAUDE.md` carries the rest and is the fuller guide.

The analytical tools and the web frontend live in sibling repositories
(`toddwbucy/WeaverAnalysis`, `toddwbucy/WeaverWeb`) and never link a crate here.

## Build, Test, and Development Commands

Use the toolchain pinned in `rust-toolchain.toml` (`nightly-2026-02-13`). Run from the
repository root:

- `process/gates/lock.sh`: first; 0 means the lock is in step, 1 drift, 2 unchecked.
- `cargo build --workspace --locked`: build all packages.
- `cargo test --workspace --locked`: test everything; `cargo test -p <crate> --locked`
  tests one package.
- `cargo test -p <crate> --locked <name_fragment>`: one test by substring.
- `cargo clippy -p <crate> --all-targets --locked -- -D warnings`: lint each changed
  crate. `weaver-spu` adds `--features cuda,gguf` and is gated on the olympus lane.
- `cargo fmt --all -- --check`: check formatting.

Keep `--locked` on Cargo commands that resolve dependencies. The SPU defaults to GGUF
and builds llama.cpp; CUDA is optional.

## Coding Style & Naming Conventions

Use rustfmt, four-space Rust indentation, `snake_case` functions/modules, and
`PascalCase` types. Existing `conforms:` headers stay in place unmaintained, and new
code owes none until release. Reuse contract-defined types across seams. Write
documentation in ASCII with absolute dates and canonical terminology (`trace`,
`state management`, `memory`; see CLAUDE.md).

## Testing Guidelines

Use Rust tests and doctests, with descriptive `snake_case` test names and integration
files such as `tests/identity.rs`. Exercise refusal paths alongside success paths.
Behavioral invariant tests must fail when the property is deliberately removed; use
compile-time checks for type properties. Coverage is evidence, not a percentage target.

## Commit & Pull Request Guidelines

Follow history's descriptive prefixes: `code:`, `docs:`, or `process:`. Open PRs as
drafts from a branch off `main`; nobody pushes to `main` directly. Every PR body
carries `Implements: <Spec> <sections>`, graded per section as conforms, drifted,
better way, or Spec gap, and names every issue and epic item it answers. Describe
behavior changes, relevant issues, and validation results. The gates and the Planner's
verification complete before leaving draft, and leaving draft fires the Codex pass.
Answer every finding, and the review repeats after every push until a pass leaves
nothing to push. Only the operator merges. `CLAUDE.md` carries the sequence.
