#!/usr/bin/env bash
# The lock gate: is the resolution cargo would use the one the repository records?
#
#   process/gates/lock.sh
#
# **Run it before the suite, not inside it.** `cargo test` resolves and repairs
# `Cargo.lock` before the first test binary is spawned, so an instrument that
# lives inside a test binary is answering after the subject has already been
# changed underneath it. Measured 2026-09-15 at 9caf02b with `hex = "0.4"` added
# to `weaver-admin/Cargo.toml` and the lock left alone: `cargo test -p
# weaver-internal --offline --test manifest` exits 0 reporting two passed and
# leaves `Cargo.lock` modified by one line, while this gate exits 1 on the same
# tree and leaves the lock untouched. That is issue #551's third ask.
#
# **This is not an H gate and no ruling names it.** No document in this corpus
# carries a rule about the lock, so it is build hygiene under the resolution the
# documented commands rest on rather than a claim about a document. Whether the
# lock deserves a sentence in the corpus is #551's last open question and the
# operator's to answer.
#
# **`--no-deps` does not gate anything and is the trap here.** It reports the
# workspace's own packages without resolving the dependency graph, so it exits 0
# on a tree this gate exits 1 on. Measured on the same perturbation.
# `deploy/update-stack.sh` asks `cargo metadata --no-deps --offline --locked` for
# the target directory and is not a lock gate for that reason. Its own build and
# test lines carry `--locked` and are.
#
# Exit status is the answer and is three-valued, after the reading
# `CLAUDE.md` takes of a clippy run that fails for a reason that is not a lint:
#
#   0  the resolution is in step with the lock
#   1  drift. The manifests ask for a resolution the lock does not record
#   2  the gate could not run, so the answer is unknown rather than passed
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 2

# Full resolution, because that is what reads the lock. `--offline` keeps the
# network out of a gate whose subject is a file already on disk.
STDERR=$(cargo metadata --locked --offline --format-version 1 2>&1 >/dev/null)
RC=$?

if [ "$RC" -ne 0 ]; then
  # **Cargo's own refusal is told apart from every other failure.** A cold
  # registry cache, an unreadable manifest and a missing toolchain all exit
  # non-zero here and none of them is drift, and a gate that called them drift
  # would send a reader to edit a lock that was never the problem.
  case "$STDERR" in
    *"because --locked was passed"*)
      printf 'DRIFT   the manifests resolve to something Cargo.lock does not record\n'
      printf '        run `cargo update --workspace --offline` or the plain\n'
      printf '        `cargo metadata --offline`, then commit the lock in this act\n'
      printf '%s\n' "$STDERR" >&2
      exit 1
      ;;
    *"no matching package"*|*"failed to load source"*|*"revspec"*|*"failed to get"*)
      # **Drift reaches here too, and the reader has to be told so.** Under
      # `--offline` a resolution needing a package or a fork rev the local cache
      # does not hold fails before cargo reaches the `--locked` refusal, so the
      # one act most likely to drift this tree - bumping one of the five fork
      # revs `weaver-spu` pins - lands in this arm rather than in DRIFT. Saying
      # only that the gate did not run would send the reader at their cache when
      # the lock is what moved.
      printf 'UNKNOWN the resolution could not complete offline, and drift is one\n'
      printf '        of the reasons it cannot: a manifest naming a package or a\n'
      printf '        fork rev the local cache does not hold reaches here rather\n'
      printf '        than DRIFT. Warm the cache and re-run, or compare the lock\n'
      printf '        against the manifests by hand before trusting this answer\n'
      printf '%s\n' "$STDERR" >&2
      exit 2
      ;;
    *)
      printf 'UNKNOWN the gate did not run, so the lock is unchecked rather than clean\n'
      printf '%s\n' "$STDERR" >&2
      exit 2
      ;;
  esac
fi

# **A lock cargo already repaired satisfies the check above trivially**, which is
# the second half of how the drift of 2026-09-04 and 2026-09-11 reached main: the
# repair is silent, it lands in the working tree, and every later `--locked` run
# passes against it. So the gate says whether the file it just certified is the
# one the commit carries. This is a report rather than a refusal, an act that
# adds a dependency having a changed lock for the whole of its life before it
# commits.
# **The comparison is three-valued like the one above.** `git diff --quiet` exits
# 0 for no difference and 1 for a difference, and 128 for no repository at all or
# an unborn HEAD. Treating every non-zero as a difference reports a lock change
# to a reader who has no git, from an export or a container build, and hides why.
git diff --quiet HEAD -- Cargo.lock 2>/dev/null
case $? in
  0)
    ;;
  1)
    printf 'OK      the resolution is in step with Cargo.lock\n'
    printf 'NOTE    Cargo.lock differs from HEAD, so this act carries a lock change\n'
    printf '        and commits it, or a cargo run repaired it and nobody noticed\n'
    exit 0
    ;;
  *)
    printf 'OK      the resolution is in step with Cargo.lock\n'
    printf 'NOTE    the comparison against HEAD could not be made, so whether this\n'
    printf '        act carries a lock change is unknown rather than no\n'
    exit 0
    ;;
esac

printf 'OK      the resolution is in step with Cargo.lock, which matches HEAD\n'
