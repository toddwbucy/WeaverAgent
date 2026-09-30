"""The import set, per python-spu-Spec section 8.

Every module the process holds after admission, and again after its first
generation, must be one the declared set names. A module outside it faults the
process: one line on standard error naming the modules and the stage, then an exit,
which the harness meets as the SPU gone, since the lifecycle vocabulary has no case
for an environment that loaded what it did not declare and a refusal would need one.

**The declared set is two halves, one per device, and the process is judged against
their union.** torch imports different modules on the CPU and on CUDA, and some
lazily, so each half is a clean run on its device, imports-cpu.txt and
imports-cuda.txt beside this file. Each is written whole by its own regeneration,
never merged into, so a module a regeneration no longer records leaves its half,
and is allowed only while the other half still records it.
"""
import json
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))


HALVES = ("imports-cpu.txt", "imports-cuda.txt")


def parse(text):
    """A half's module names: every line that is not blank and not a comment."""
    return frozenset(
        line.strip() for line in text.splitlines()
        if line.strip() and not line.startswith("#")
    )


def declared(texts):
    """The declared set, the union of the halves' names."""
    return frozenset().union(*(parse(text) for text in texts))


def _read_declared():
    # Read through the loader, so the halves read the same from the source tree and
    # from inside the zipapp, and read once, at import, so reading them adds nothing
    # the check itself would then have to allow. A missing half fails the import,
    # never reads as an empty one.
    loader = __loader__
    return declared(loader.get_data(os.path.join(_HERE, half)).decode() for half in HALVES)


DECLARED = _read_declared()


def undeclared(loaded=None):
    loaded = set(sys.modules) if loaded is None else set(loaded)
    return sorted(loaded - DECLARED)


def enforce(stage, declare=None):
    """Judges the process's modules at a stage, or, where the server was started
    with --declare-imports, records them instead. Declaring never serves past the
    first generation: the process exits once that stage is recorded, so the mode
    cannot stand in for a judged process.

    **The code the process maps is judged first, in either mode**, per
    `loaded_code`: a run that loaded code from outside the environment faults
    rather than serving, and rather than recording a half from what it held."""
    from .loaded_code import foreign_now
    code = foreign_now()
    if code:
        print(json.dumps({"loaded_code_violation": stage, "foreign": code}),
              file=sys.stderr, flush=True)
        os._exit(3)
    if declare is not None:
        with open(declare, "a") as fh:
            fh.write("".join(f"{name}\n" for name in sorted(sys.modules)))
        if stage == "first generation":
            os._exit(0)
        return
    extra = undeclared()
    if extra:
        print(json.dumps({"import_set_violation": stage, "undeclared": extra}),
              file=sys.stderr, flush=True)
        os._exit(3)
