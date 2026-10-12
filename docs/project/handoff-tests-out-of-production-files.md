# Handoff: tests out of production files (#118)

**Version:** v0.2, 2026-10-12. From the Planner seat to the Executor2 seat, per the
Handoff Format. Governs the act that answers issue #118, the operator's decision of
2026-10-12: move the inline Rust unit-test suites out of the production source files,
crate by crate, behaviour-preserving, with no API widened to make a test fit.

The act is structural. It implements no Spec section and changes no behaviour: a test
that passed before the move passes after it, under the same name, and a production file
loses its test module and gains one line. The act lands this handoff in
`docs/project/handoff-tests-out-of-production-files.md` in its first pull request, and
posts the baseline inventory below as a comment on #118.

## 1. The standard, one location per tier

| tier | access | location |
|---|---|---|
| unit tests | the module's private items | `src/<module>/tests.rs`, a child module declared by one line in the production file, `#[cfg(test)] mod tests;` |
| integration tests | the crate's public surface | `crates/<crate>/tests/*.rs`, as now |
| end to end | a built agent | `deploy/test_plans.py`, `deploy/verify-lifecycle.sh`, python-spu's pytest, as now |

Rust resolves `mod tests;` to the file beside the module, in its directory: for
`src/lifecycle.rs` it is `src/lifecycle/tests.rs`; for `src/decoder/session.rs` it is
`src/decoder/session/tests.rs`; for `src/family/mod.rs` it is `src/family/tests.rs`; for
`src/main.rs` or `src/lib.rs` it is `src/tests.rs`. A child module sees its parent's
private items, so `use super::*` and every private call keep working, and no `pub` is
added anywhere. Nothing moves to `crates/<crate>/tests/` unless it already used only the
public surface, and then only with the reason in the body.

## 2. The baseline inventory, main at c70c992 (ddbfd0e carries the same shape plus #119's harness tests)

Embedded lines are counted from the last `#[cfg(test)] mod` declaration to the end of
the file. Hooks are `#[cfg(test)]` items that are not modules: test-only functions,
`thread_local!` counters, statements inside production functions.

| crate | embedded lines | files | largest |
|---|---|---|---|
| weaver-harness | 9,332 | 10 | `src/lifecycle.rs` 3,945 of 6,766 |
| weaver-admin | 8,890 | 12 | `src/main.rs` 3,561 of 6,584 |
| weaver-spu | 6,145 | 9 | `src/decoder/session.rs` 1,384 |
| weaver-state | 2,289 | 7 | `src/engine/sqlite.rs` 937 |
| weaver-gate | 1,808 | 5 | `src/tools.rs` 659 |
| weaver-trace | 300 | 2 | |
| weaver-internal | 74 | 1 | |
| weaver-types | 34 | 1 | |

Total about 28,900 embedded lines in 53 files, beside 14,446 lines in 36 dedicated
`tests/` files; 36 hooks in 20 files. The script that produced it, run from the
repository root before and after each crate, its output kept for the body:

    for f in $(git ls-files 'crates/<crate>/src/**/*.rs' 'crates/<crate>/src/*.rs'); do
      t=$(wc -l < "$f")
      start=$(grep -n -A1 '^\s*#\[cfg(test)\]' "$f" \
        | grep -E '^[0-9]+-\s*(pub(\(crate\))? )?mod [a-z_]+ \{' | tail -1 | cut -d- -f1)
      hooks=$(( $(grep -c '^\s*#\[cfg(test)\]' "$f") - $(grep -n -A1 '^\s*#\[cfg(test)\]' "$f" | grep -cE '^[0-9]+-\s*(pub(\(crate\))? )?mod [a-z_]+ \{') ))
      [ -n "$start" ] && printf '%-50s %6d %6d %3d\n' "$f" "$t" $((t - start + 1)) "$hooks"
    done

## 3. The method, per crate

1. Branch from `main` in a worktree of its own, a real directory and never a symlink:
   `git worktree add ../WeaverAgent-wt-118-<crate> -b code/tests-out-<crate> main`.
   A private cargo target outside the home directory, `CARGO_TARGET_DIR=<private dir>`, for
   every cargo command.
2. Baseline: the gates of section 5 on the untouched crate, and the test list saved:
   `cargo test -p <crate> --locked -- --list > before.txt` (for weaver-spu with
   `--features cuda,gguf`).
3. For each production file carrying a test module, in the order of the inventory,
   largest first: cut the body of `mod tests { ... }` into `src/<module>/tests.rs`,
   leave `#[cfg(test)] mod tests;` in its place, keep the doc comment above the module
   where it has one. A production file with several test modules moves each to its own
   file beside the module, declared `#[cfg(test)] mod <name>;`, so `crate::<name>` paths
   and the test list stay identical and no test body is edited (admin's `scratch`,
   `gid_tests` and `channel/reference_tests` set the convention in B3); each such file
   opens with a doc comment saying it is test-only (#118), and the body lists them.
4. Hooks stay where they are. A hook that is a helper called only from tests may move
   into the new `tests.rs` when nothing in production reaches it; a `thread_local!`
   counter, a statement inside a production function or a helper a production path
   calls under `cfg(test)` stays, with a one-line comment naming #118, and is listed
   in the body as an exception. An existing `#[path = ...]` include stays as it is.
5. After: the same test list, `after.txt`, and `diff before.txt after.txt` empty. A
   nested module (step 3) changes the path of its tests and is the one admitted
   difference, each renamed test listed in the body before and after. Then the gates of
   section 5, and the inventory script again for the after count.
6. A draft pull request per crate, its body per section 6, a commit subject `code:
   <crate>: inline tests moved to src/<module>/tests.rs (#118)`. The Planner grades by
   the scripted check and undrafts; the operator merges. After the merge, remove the
   worktree and branch, and start the next crate from the new `main`. No branch waits
   on another: a long-lived branch is what the move must not become.

Order: weaver-types, weaver-trace and weaver-internal first, as one pull request if
that is simpler (408 lines in all, the pattern proven on them); then weaver-gate,
weaver-state, weaver-spu, one pull request each. **Not weaver-harness and not
weaver-admin**: the lifecycle act (B3) holds admin and moves its own suites as its
first commit; the harness moves after B3, as its own small pull request, by whichever
seat is free. Nothing under `deploy/`, `docs/` (this handoff aside) or `python-spu/` is
touched.

## 4. What the act may not do

Weaken, consolidate, rename or delete a test; change an assertion; widen a visibility;
change a feature gate, a manifest or the release configuration; fix a defect found on
the way. A defect found is an issue carrying what was measured, named in the body, and
never a fix in the move (the police-call line of CLAUDE.md stops at the move's own
litter: a stale doc comment on the moved module is fixed, a wrong test is not).

## 5. Gates, per crate, private target

`process/gates/lock.sh` (0); `cargo fmt --all -- --check`; `cargo clippy -p <crate>
--all-targets --locked -- -D warnings` (weaver-spu with `--features cuda,gguf`);
`cargo test -p <crate> --locked`; `cargo test --workspace --locked` once before the
draft; the `--list` diff of section 3. Each line in the body with its result and the
target directory used.

## 6. The review line, agreed before review

Stated in each body before the undraft, and held: **a finding that the move itself
changed behaviour, broke a test, renamed one unlisted, or widened an API is fixed in
the pull request. A finding about a moved test's substance, whatever its severity, is
recorded on one audit issue per crate (`#118 audit: <crate>`, opened with the first
such finding) and declined in the pull request with that cite.** Codex reads each
extracted suite as new code, since a partial extract is not a rename, so every crate
gets its first end-to-end read; that read is the input to the test-efficiency audit
#118 names as its follow-up, and it is banked there, not litigated in the move.

    Base            ddbfd0e, #119's merge on main
    Authority       issue #118 (the operator, 2026-10-12); CLAUDE.md's pull request path
                    and gates; no Spec section is implemented
    Crates          weaver-types, weaver-trace, weaver-internal, weaver-gate,
                    weaver-state, weaver-spu, in that order
    Not this seat's weaver-admin (B3 moves it), weaver-harness (after B3)
    Closes          #118 closes when the last crate, the harness, has moved; each pull
                    request names the crate it answers for
    Asked           a draft pull request per crate: the before and after inventory
                    lines, the files moved, the exceptions with their reasons, the
                    `--list` diff, the gates, and the review line of section 6
