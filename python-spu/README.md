# python-spu

**A prototype, carried into the tree as it stood.** This is the second implementation
of the SPU organ's decode role that `docs/crates/weaver-spu/python-spu-Spec.md`
governs, landed as a lab instrument under Working Process section 6's rule that a carry
is not a hardening. The Spec says what the implementation must become, and its section
2.1 names where this prototype falls short of it. Nothing here is relied on for a
verdict until the acts epic #25 lists have closed those gaps.

It serves Qwen2 causal models from a local Hugging Face directory with PyTorch in eager
mode, on the CPU for the tests and on one CUDA device for the smoke test. It carries an
experimental ModernBERT classifier, which the Spec scopes out of this implementation.

## Tests

The oracle links the workspace crates at the commit its manifest pins, so the suite
compares Python's answers with the Rust code's own. Build it first, then run the suite
from this directory:

```sh
cargo build --manifest-path oracle/Cargo.toml --locked
pytest -q
```

The suite looks for the oracle at `oracle/target/debug/`, so build it with
`CARGO_TARGET_DIR` unset where the environment sets one.

The suite runs in the locked test environment, `requirements-test.lock` installed with
`--require-hashes --no-deps --only-binary=:all:` into a fresh venv of the pinned
interpreter, never over an older one, since pip removes nothing a newer lock drops. The
suite holds the venv to that lock by `scripts/installed_set.py`, below.

The suite runs on the CPU. It covers Rust wire comparisons, seed derivation, rendering,
measurement arithmetic, state transitions, tiny-model forwards, rollback, classifier
scoring, Unix packet segmentation, Rust and Python transport, and the child process's
lifecycle. The socket tests need local Unix IPC, so a sandbox that refuses packet sends
cannot run them.

## Model smoke test

The smoke test launches three fresh SPU child processes, comparing two ordinary runs
and one with residual readout enabled. Each loads the model, opens a session,
generates, flushes, releases and exits. It exercises the SPU protocol through a local
test driver, not the WeaverTools harness, admin or trace replay.

```sh
python scripts/smoke.py <model-dir>
python scripts/smoke.py <model-dir> --device cuda --output <report.json>
```

CUDA mode refuses if CUDA is unavailable, and neither mode downloads a model. GPU runs
set a deterministic cuBLAS workspace configuration before the child starts. Name the
card with `CUDA_VISIBLE_DEVICES`. A report goes to the run's deposit on the shared bulk
store, not into this tree.

## Installing it on a box

The operator's steps, from this directory, each one that writes under `/opt/weaver`
taken through `sudo`. The interpreter is the one `requirements.lock`'s header names, by
release, URL and sha256, and the download is checked against that sha256 before it is
unpacked. **The block is the upgrade as well as the install, and it recreates the
prefix.** `pip install -r` adds what a lock lists and removes nothing, so a prefix
installed from an earlier lock keeps what the new one dropped, as a prefix installed
before triton left the lock keeps triton and then faults at admission. Run it with no
agent serving from the prefix:

```sh
T=cpython-3.14.7+20260924-x86_64-unknown-linux-gnu-install_only.tar.gz
curl -fLO https://github.com/astral-sh/python-build-standalone/releases/download/20260924/$T
echo "5539eaf1de20bd9b5f43ea11c3c1f84cbac74fe927ac050318a9210c022618cb  $T" | sha256sum -c
sudo rm -rf /opt/weaver/python-spu
sudo mkdir -p /opt/weaver/python-spu
sudo tar -xzf $T -C /opt/weaver/python-spu --strip-components=1
sudo /opt/weaver/python-spu/bin/python3.14 -m pip install --require-hashes --no-deps \
    --only-binary=:all: -r requirements.lock
/opt/weaver/python-spu/bin/python3.14 -I -B scripts/installed_set.py requirements.lock
sudo /opt/weaver/python-spu/bin/python3.14 scripts/build_zipapp.py \
    --output /opt/weaver/python-spu/python-spu.pyz
python3 scripts/tree_digest.py /opt/weaver/python-spu
```

`scripts/installed_set.py` reads what is installed, by `importlib.metadata`, and holds it
equal to the lock: a distribution the lock does not pin, one it pins that is absent and
one at another version each exit 1, named, and the interpreter's own pip is the one
distribution admitted beside the lock. It runs isolated, so neither `PYTHONPATH` nor a
user site directory is read, and it writes nothing.

The zipapp's first line names `/opt/weaver/python-spu/bin/python3.14`. Admin serves it
to an agent through `spu-implementations` and `agent-spu`, per
`docs/crates/weaver-admin/weaver-admin-Spec.md` section 9. The box facts record the
lock's sha256 and the tree digest, which covers the interpreter, its standard library
and every installed package. The packages are installed into the interpreter's own
prefix rather than a venv, so the prefix is the whole of what runs.

The serving process is judged against the union of two halves,
`src/python_spu/imports-cpu.txt` and `imports-cuda.txt`, each generated on its device
with the real model: `scripts/declare_imports.py --device cpu|cuda --zipapp <file>
--model <dir>`, the CUDA one on a card. A regeneration replaces its device's half whole,
so a module the new run does not load leaves the half. The zipapp carries both halves as
they stood when it was built, so rebuild it after either changes. A zipapp older than
its halves faults where a module the old list lacks is loaded, exit 3. The process also
faults, exit 3, on any code mapped from outside the environment, per python-spu-Spec
section 8: the lock carries no triton, and torch's native kernel compilation is turned
off.

## Files

- `src/python_spu/`: serving, the protocol, the model engine, sampling and the local
  test driver.
- `tests/`: the compatibility and behaviour tests.
- `oracle/`: the Rust program that answers the suite with the Rust code's results.
- `scripts/smoke.py`: the trained-model process test.
- `scripts/build_zipapp.py`, `scripts/tree_digest.py`, `scripts/declare_imports.py`,
  `scripts/installed_set.py`: the one file admin serves, the installed tree's digest,
  the import set's generator, and the check that the installed set is the lock's.
- `requirements*.in`, `requirements*.lock`, `constraints.txt`: the hash-locked
  environments and the proven set they are compiled against.

## Not yet shown

Integration with the Rust harness end to end, and a trace verifying under the
diagnostic replay, beyond the one turn of stage B, whose result lives in the suite
repository, `toddwbucy/WeaverTools`, at
`experiments/deployment-tuple/kernel-stack/python-spu/results/2026-09-29-thinkpad-stage-b/`.
The full wire vocabulary. GPU admission against a live occupant, and
injected device faults. The headroom's room judgment has been shown refusing and
admitting on an unoccupied card (toddwbucy/WeaverTools-old2#754). Other families,
precisions and sharding. The classifier's equivalence on a trained artifact. Speed. A
small model's smoke test passing establishes none of these.
