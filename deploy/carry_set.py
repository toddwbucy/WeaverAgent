#!/usr/bin/env python3
"""The carry set's manifest (weaver-admin-PRD section 7.2).

    carry_set.py write SET COMMIT    write SET/MANIFEST for the set laid out in SET

A carry set is a directory holding `build-tuple` and the three directories
`bin/`, `lib/` and `deploy/`, each holding files and links only. Its manifest
names every entry: one row per entry, tab-separated, sorted by path in C
order, after two header lines.

    # weaver carry set 1
    # commit <40 hex>
    file<TAB><sha256><TAB><0755|0644><TAB><path>
    link<TAB><target><TAB>-<TAB><path>

A path is relative, printable ASCII with no blank, and lies directly under one
of the three directories, or is `build-tuple`. A file's mode is 0755 or 0644.
A link's target is one name in the same directory, which the set holds. The
manifest's sha256 is the set's identity. Writing refuses, naming the entry,
where the set holds anything these rules do not admit, so a manifest is never
written that its reader would refuse. Every refusal exits 1 with one line on
standard error.
"""

import hashlib
import os
import re
import stat
import sys

HEADER = "# weaver carry set 1"
DIRECTORIES = ("bin", "lib", "deploy")
TUPLE = "build-tuple"
MODES = {0o755: "0755", 0o644: "0644"}
NAME = re.compile(r"[\x21-\x7e]+")
COMMIT = re.compile(r"[0-9a-f]{40}")


class Refused(Exception):
    pass


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def plain_name(name):
    return bool(NAME.fullmatch(name)) and "/" not in name and name not in (".", "..")


def entry_row(root, path):
    """The manifest row of the entry at PATH, relative to ROOT."""
    full = os.path.join(root, path)
    st = os.lstat(full)
    if stat.S_ISLNK(st.st_mode):
        target = os.readlink(full)
        if not plain_name(target):
            raise Refused(f"{path} is a link to {target!r}, not to one name in its own directory")
        if not os.path.lexists(os.path.join(os.path.dirname(full), target)):
            raise Refused(f"{path} is a link to {target}, which the set does not hold")
        return f"link\t{target}\t-\t{path}"
    if not stat.S_ISREG(st.st_mode):
        raise Refused(f"{path} is neither a file nor a link")
    mode = MODES.get(stat.S_IMODE(st.st_mode))
    if mode is None:
        raise Refused(f"{path} has mode {stat.S_IMODE(st.st_mode):04o}, not 0755 or 0644")
    return f"file\t{digest(full)}\t{mode}\t{path}"


def rows(root):
    """Every row of the set at ROOT, sorted by path in C order."""
    found = []
    for name in sorted(os.listdir(root)):
        full = os.path.join(root, name)
        if name == TUPLE:
            if not stat.S_ISREG(os.lstat(full).st_mode):
                raise Refused(f"{TUPLE} is not a file")
            found.append(TUPLE)
        elif name in DIRECTORIES and stat.S_ISDIR(os.lstat(full).st_mode):
            for entry in sorted(os.listdir(full)):
                if not plain_name(entry):
                    raise Refused(f"{name}/{entry!r} is not a name of printable ASCII with no blank")
                found.append(f"{name}/{entry}")
        else:
            raise Refused(f"{name} is not {TUPLE} or one of the directories {', '.join(DIRECTORIES)}")
    if TUPLE not in found:
        raise Refused(f"the set holds no {TUPLE}")
    found.sort(key=lambda p: p.encode("ascii"))
    return [entry_row(root, path) for path in found]


def write(root, commit):
    if not COMMIT.fullmatch(commit):
        raise Refused(f"the commit {commit!r} is not 40 hex digits")
    if os.path.lexists(os.path.join(root, "MANIFEST")):
        raise Refused("the set already holds a MANIFEST")
    body = [HEADER, f"# commit {commit}", *rows(root)]
    fd = os.open(os.path.join(root, "MANIFEST"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    with os.fdopen(fd, "w", encoding="ascii") as f:
        f.write("\n".join(body) + "\n")
    os.chmod(os.path.join(root, "MANIFEST"), 0o644)


def main(argv):
    if len(argv) == 4 and argv[1] == "write":
        write(argv[2], argv[3])
        return 0
    print(f"usage: {argv[0]} write SET COMMIT", file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (Refused, OSError) as e:
        print(f"carry_set.py: {e}", file=sys.stderr)
        sys.exit(1)
