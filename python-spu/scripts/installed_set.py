"""Holds the installed distributions equal to a lock, per python-spu-Spec section 8.

`pip install -r` adds what a lock lists and removes nothing, so a prefix installed
from an earlier lock keeps what the new one dropped, triton from the lock before
section 8's rule among them. The lock states what should be installed. This reads
what is: every distribution the running interpreter's path holds, by
`importlib.metadata`, against the lock's pins. A distribution the lock does not pin,
one it pins that is absent, one at another version, one found twice and one without a
name are each refused by name. The one distribution admitted beyond the lock is the
interpreter's own pip, at the version its `ensurepip` bundles, the release shipping it.

Run it with the interpreter whose environment it judges, isolated, so neither the
caller's PYTHONPATH nor a user site directory is read:

    /opt/weaver/python-spu/bin/python3.14 -I scripts/installed_set.py requirements.lock

Exit 0 when the installed set equals the lock, 1 naming each difference, 2 when the
lock cannot be read or pins nothing, so nothing was judged. Standard library only.
"""
import ensurepip
import importlib.metadata
import re
import sys

PIN = re.compile(r"([A-Za-z0-9][A-Za-z0-9._-]*)==([^\s;\\]+)")


def normal(name):
    return re.sub(r"[-_.]+", "-", name).lower()


def pins(text):
    """The lock's pins by normalised name. Every line that is not a comment, a hash
    continuation or blank must be a pin, so a line of another form is refused rather
    than skipped."""
    found = {}
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith(("#", "--hash")):
            continue
        match = PIN.match(stripped)
        if match is None:
            raise ValueError(f"line {number} is not a pin: {stripped}")
        found[normal(match[1])] = match[2]
    if not found:
        raise ValueError("the lock pins nothing")
    return found


def differences(locked, path=None):
    """Each way the distributions on `path`, sys.path where None, differ from the
    lock's pins, sorted."""
    installed = {}
    problems = []
    for dist in importlib.metadata.distributions(path=path or sys.path):
        name = dist.metadata["Name"]
        if not name:
            problems.append(f"nameless distribution at {dist._path}")
            continue
        installed.setdefault(normal(name), []).append(dist.version)
    # Admitted where present, never required: a venv made without pip is not short.
    admitted = {"pip": ensurepip.version()}
    expected = {**admitted, **locked}
    for name, versions in installed.items():
        if len(versions) > 1:
            problems.append(f"found twice: {name} {', '.join(versions)}")
        elif name not in expected:
            problems.append(f"not in the lock: {name}=={versions[0]}")
        elif versions[0] != expected[name]:
            problems.append(f"another version: {name}=={versions[0]}, "
                            f"the lock pins {expected[name]}")
    problems.extend(f"missing: {name}=={version}" for name, version in locked.items()
                    if name not in installed)
    return sorted(problems)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) != 1:
        print("usage: installed_set.py <lock>", file=sys.stderr)
        return 2
    try:
        with open(argv[0]) as fh:
            locked = pins(fh.read())
    except (OSError, ValueError) as error:
        print(f"installed_set: {error}", file=sys.stderr)
        return 2
    problems = differences(locked)
    for problem in problems:
        print(f"installed_set: {problem}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
